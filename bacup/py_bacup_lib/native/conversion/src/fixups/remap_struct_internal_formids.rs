//! Fixup: remap source-local FormIDs that live INSIDE struct/array_struct codec
//! subrecords, which the `FormKeyMapper` never reached.
//!
//! # Root cause (the systemic struct-FK remap gap)
//! `FormKeyMapper::rewrite_record` remaps only (a) VMAD bytes and (b) typed
//! `FieldValue::FormKey` leaves. A `struct:`/`array_struct:` codec subrecord
//! decodes to a single `FieldValue::Bytes` blob, so every FormID inside it is
//! invisible to the mapper and keeps its FO76 source-local (master byte 0)
//! value → it serializes as a non-existent `Fallout4.esm:00xxxxxx` and dangles.
//!
//! Affected subrecords include OMOD `DATA`, LCTN `LCEP`/`ACEP` enable-parent
//! Actor/Ref and `LCUN`, and RACE/MGEF `DATA`. The LCEP enable-parent ref is
//! also a cell-load crash vector — it is dereferenced on cell load.
//!
//! # The general fix
//! Descend EVERY struct-codec subrecord generically via `struct_field_layout`
//! (per-field byte offset/width/formlink_targets) and remap each source-local
//! FK to its target-encoded id — the same encoded rewrite
//! `rewrite_raw_object_template_formids` does for OMOD/OBTS, applied uniformly
//! to all struct subrecords.
//!
//! Runs UPSTREAM of the type validators (`validate_reference_target_types`,
//! placed-record normalization): remap first so the FKs resolve to real
//! converted records, THEN validate type. Shares `struct_field_layout` offsets
//! with the validators, so they agree by construction.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::ref_index::remap_struct_fk_fields;
use crate::fixups::rewrite_raw_object_template_formids::{
    encoded_targets_by_source_object_id, rewrite_formid_at,
};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::record::{FieldValue, Record};
use crate::schema::{AuthoringSchema, FieldDef, SubrecordDef};
use crate::session::PluginSession;

/// FO4 record form version. Every record in the FO4 conversion output carries
/// form_version 131, so the struct-FK layout and the layout-divergence guard
/// must be resolved at this version to honor wbFromVersion-gated fields
/// (e.g. RACE.DATA's FV-143/188 fields are absent at 131 → 200B, not 216B).
///
/// Assumes a single-FV source: schema verified the FO76 source is uniformly
/// form_version 131 across every affected record type (RACE/WEAP/MGEF/ALCH/
/// BPTD/QUST/WTHR/EFSH). If a future source carries records at FV ≥ 143, this
/// constant would wrongly drop their real gte-143 fields and re-skew offsets —
/// thread the per-record ParsedRecord.form_version (needs a session accessor)
/// instead of this constant if multi-version sources appear.
pub const FO4_TARGET_FORM_VERSION: u16 = 131;

pub struct RemapStructInternalFormIdsFixup;

pub(crate) fn candidate_signatures(
    session: &mut PluginSession,
    target_schema: &AuthoringSchema,
) -> Result<Vec<crate::ids::SigCode>, FixupError> {
    let present = session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut seen = FxHashSet::default();
    let mut candidates = Vec::new();
    for sig in present {
        if record_may_have_struct_internal_formids(target_schema, sig.as_str()) && seen.insert(sig)
        {
            candidates.push(sig);
        }
    }
    Ok(candidates)
}

fn record_may_have_struct_internal_formids(
    target_schema: &AuthoringSchema,
    record_sig: &str,
) -> bool {
    if matches!(record_sig, "PACK" | "FACT") {
        return true;
    }
    let Some(record_def) = target_schema.record_def(record_sig) else {
        return false;
    };
    record_def.subrecords.iter().any(|sub| {
        let layout = target_schema.struct_field_layout_versioned(
            record_sig,
            &sub.id,
            Some(FO4_TARGET_FORM_VERSION),
        );
        layout
            .iter()
            .any(|field| field.width == 4 && !field.formlink_targets.is_empty())
            || subrecord_may_decode_nested_formkeys(sub)
    })
}

fn subrecord_may_decode_nested_formkeys(sub: &SubrecordDef) -> bool {
    let codec = sub.codec.as_deref().unwrap_or("");
    let structish = codec.starts_with("struct:")
        || codec.starts_with("array_struct:")
        || !sub.fields.is_empty()
        || !sub.union_variants.is_empty();
    structish
        && (fields_may_contain_formids(&sub.fields)
            || fields_may_contain_formids(&sub.union_variants))
}

fn fields_may_contain_formids(fields: &[FieldDef]) -> bool {
    fields.iter().any(field_may_contain_formids)
}

fn field_may_contain_formids(field: &FieldDef) -> bool {
    let kind_has_formid = field.kind.to_ascii_lowercase().contains("formid");
    let codec_has_formid = field
        .codec
        .as_deref()
        .map(|codec| codec.to_ascii_lowercase().contains("formid"))
        .unwrap_or(false);
    kind_has_formid
        || codec_has_formid
        || fields_may_contain_formids(&field.fields)
        || fields_may_contain_formids(&field.union_variants)
}

impl Fixup for RemapStructInternalFormIdsFixup {
    fn name(&self) -> &'static str {
        "remap_struct_internal_formids"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();

        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        // Post-relayout, every `struct:` subrecord in the TARGET plugin is
        // already laid out per the FO4 target schema (the FO76→FO4 byte relayout
        // runs at decode — see `crate::struct_relayout`). So the bytes in hand
        // here match the TARGET layout, not the FO76 source layout. Feed the
        // target schema as the divergence-guard's "source" so the guard compares
        // target-vs-target (identical ⇒ not divergent ⇒ the FK remap proceeds and
        // bumps the now-correctly-positioned gore/owner FKs 00→07). Passing the
        // real FO76 source_schema here would make the guard see FO76≠FO4 and skip
        // the remap over already-relaid-out bytes — re-stranding the gore FKs.
        let source_schema = Some(target_schema);

        let target_masters = session.target_masters().to_vec();
        let encoded_targets = encoded_targets_by_source_object_id(mapper, &target_masters);
        // Source-local id → target FormKey, for remapping FormKey leaves inside
        // List/Struct-decoded subrecords (LCEP/LCUN) the Bytes path can't reach.
        let target_by_source_local: rustc_hash::FxHashMap<u32, crate::ids::FormKey> = mapper
            .source_to_target_iter()
            .map(|(source, target)| (source.local, target))
            .collect();
        if encoded_targets.is_empty() && target_by_source_local.is_empty() {
            return Ok(report);
        }

        let sigs = candidate_signatures(session, target_schema)?;
        if sigs.is_empty() {
            return Ok(report);
        }

        for sig in sigs {
            let fks = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            let mut changed_records = Vec::new();
            for fk in fks {
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                // The conversion target is FO4, whose records use form_version
                // 131. Pass it so struct_field_layout_versioned computes the
                // FV-gated layout (RACE.DATA: 200B at FV-131, not the maximal
                // 216B) — both for the remap offsets and the divergence guard.
                // Without this, version-gated FK offsets skew.
                let mut remap = remap_struct_fk_fields(
                    &mut record,
                    target_schema,
                    source_schema,
                    Some(FO4_TARGET_FORM_VERSION),
                    &encoded_targets,
                    &target_by_source_local,
                );
                // PACK PTDA/PLDT/PDTO and FACT PLVD hide their FK inside a
                // VALUE-selected union the generic struct-field layout can't
                // surface (the union field has no formlink_targets). Remap them
                // explicitly, gated on the per-subrecord type selector so scalar
                // payloads (alias indices, subtype fourCC, packed bytes) aren't
                // corrupted.
                if matches!(record.sig.as_str(), "PACK" | "FACT") {
                    remap.remapped +=
                        remap_value_selected_union_formids(&mut record, &encoded_targets);
                }
                if remap.changed() {
                    changed_records.push(record);
                }
            }
            let expected = changed_records.len();
            let replaced = session
                .replace_records_contents(changed_records, target_schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if replaced != expected {
                return Err(FixupError::HandleError(format!(
                    "remap_struct_internal_formids replaced {replaced} of {expected} expected records for {}",
                    sig.as_str()
                )));
            }
            report.records_changed = report
                .records_changed
                .saturating_add(replaced.try_into().unwrap_or(u32::MAX));
        }

        Ok(report)
    }
}

/// Whether a value-selected-union subrecord's `type` selector designates a
/// FormID-bearing variant whose FK sits at offset 4. Returns `false` for scalar
/// / packed-byte / fourCC variants so their offset-4 word is never rewritten.
///
/// PTDA (Target): the FO4 `PACK.PTDA` union holds a FormID ONLY at kinds 0
///   `reference` (PLYR/ACHR/REFR…), 1 `object_id` (ACTI/STAT/NPC_…), 3 `keyword`
///   (KYWD) and 7 `keyword` (KYWD). The other kinds are scalars: 2 `object_type`
///   (u32 form-type code), 4 `alias` (i32), 5 `interrupt_data` (u32), 6/8 (u32).
///   So the gate admits exactly {0,1,3,7}. Kind 2 stays scalar — an xEdit "could
///   not resolve" on a kind-2 offset-4 is a false positive on a form-type code,
///   not a dangling FK.
/// PLDT / PLVD (Location, `location_enum`): type 0 = Reference FK, 1 = Cell FK,
///   4 = Object ID FK, 6 = Keyword FK; types 2/3/5/7/8/9/.. are packed bytes /
///   alias scalars.
/// PDTO (Topic Data, `PACK.PDTO.type`): type 0 = Topic Ref (DIAL FK); type 1 =
///   Topic Subtype, a 4-byte fourCC string (e.g. "CUST") — NOT a FormID.
pub(crate) fn union_type_holds_formid(sub_sig: &str, kind: i32) -> bool {
    match sub_sig {
        "PTDA" => matches!(kind, 0 | 1 | 3 | 7),
        "PLDT" | "PLVD" => matches!(kind, 0 | 1 | 4 | 6),
        "PDTO" => kind == 0,
        _ => false,
    }
}

/// Remap the FormID hidden in a value-selected union subrecord — PACK
/// `PTDA`/`PLDT`/`PDTO` and FACT `PLVD`.
///
/// Each subrecord lays out as `[i32 type][union value @offset 4][...]`. The
/// union's active variant (and thus whether offset 4 is a FormID at all) is
/// chosen by the `type` selector, which the schema-driven struct-field layout
/// can't evaluate — the union field carries no `formlink_targets`, so the
/// generic remap never sees the FK and it keeps its FO76 00-prefix master byte.
/// `union_type_holds_formid` gates the offset-4 rewrite per sig+type so scalar /
/// fourCC / packed payloads at other types are left untouched.
///
/// PLUGIN-AWARE: `rewrite_formid_at` rewrites ONLY a leaf whose
/// master byte is 0 (an un-remapped source-plugin FK) AND whose object-id is in
/// `encoded_targets`; a leaf already carrying a foreign master byte (DLCCoast
/// etc.) or absent from the map is left untouched. So this never clobbers an
/// already-finalized cross-master FK, and it is a no-op when the target record
/// was never emitted (those dangles are an emission gap, not a union-visibility
/// one).
///
/// CONTRACT / scope: this handles ONLY the *value-selected union* case — a
/// struct field whose FK presence depends on a sibling type selector and whose
/// field-level `formlink_targets` is therefore empty (invisible to the generic
/// `remap_struct_fk_fields` Bytes path). It is NOT a substitute for plain
/// struct FK fields that DO declare `formlink_targets` (e.g. RACE.DATA
/// severable/explodable EXPL/DEBR/IPDS gore slots): those are already surfaced
/// by `struct_field_layout` and remapped generically — if they dangle, the
/// cause is the `source_struct_layout_diverges` skip-guard (FO76 codec
/// divergence), NOT a union-visibility gap. Generalizing this
/// helper would require a schema-driven selector model (read the selector field
/// offset + each variant's per-type FK offsets from the union spec) rather than
/// the hardcoded offset-0 selector + offset-4 FK used here.
pub(crate) fn remap_value_selected_union_formids(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
) -> u32 {
    let mut remapped = 0u32;
    for entry in record.fields.iter_mut() {
        let sub_sig = entry.sig.as_str();
        if !matches!(sub_sig, "PTDA" | "PLDT" | "PLVD" | "PDTO") {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if bytes.len() < 8 {
            continue;
        }
        let kind = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if !union_type_holds_formid(sub_sig, kind) {
            continue;
        }
        if rewrite_formid_at(bytes, 4, encoded_targets) {
            remapped += 1;
        }
    }
    remapped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::sym::StringInterner;
    use smallvec::SmallVec;

    fn union_sub(sig: &[u8; 4], kind: i32, fk: u32) -> FieldEntry {
        let mut raw = Vec::new();
        raw.extend_from_slice(&kind.to_le_bytes());
        raw.extend_from_slice(&fk.to_le_bytes());
        raw.extend_from_slice(&0u32.to_le_bytes());
        FieldEntry {
            sig: SubrecordSig(*sig),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        }
    }

    fn ptda(kind: i32, fk: u32) -> FieldEntry {
        union_sub(b"PTDA", kind, fk)
    }

    fn record_of(sig: &str, fields: Vec<FieldEntry>) -> Record {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str(sig).unwrap(),
            FormKey::parse("8A4CF3@SeventySix.esm", &interner).unwrap(),
        );
        record.fields = SmallVec::from_vec(fields);
        record
    }

    fn pack(fields: Vec<FieldEntry>) -> Record {
        record_of("PACK", fields)
    }

    fn fk_bytes(entry: &FieldEntry) -> u32 {
        let FieldValue::Bytes(bytes) = &entry.value else {
            panic!("expected bytes");
        };
        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])
    }

    #[test]
    fn candidate_filter_includes_known_struct_internal_fk_records() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");

        assert!(record_may_have_struct_internal_formids(&schema, "RACE"));
        assert!(record_may_have_struct_internal_formids(&schema, "MGEF"));
        assert!(record_may_have_struct_internal_formids(&schema, "LCTN"));
        assert!(record_may_have_struct_internal_formids(&schema, "PACK"));
        assert!(record_may_have_struct_internal_formids(&schema, "FACT"));
    }

    #[test]
    fn remaps_ptda_reference_target_keeping_scalar_type2() {
        // type 0 (reference) → remapped to target-encoded; type 2 (object_type, a
        // u32 form-type code — NOT a FormID) left alone even though its offset-4
        // u32 collides with a map key.
        let mut encoded = FxHashMap::default();
        encoded.insert(0x008A483A, 0x078A483A); // source-local → 07-encoded
        encoded.insert(0x0000000F, 0x0700000F); // would corrupt the type-2 scalar
        let mut record = pack(vec![ptda(0, 0x008A483A), ptda(2, 0x0000000F)]);

        let n = remap_value_selected_union_formids(&mut record, &encoded);

        assert_eq!(n, 1, "only the type-0 reference FK is remapped");
        assert_eq!(fk_bytes(&record.fields[0]), 0x078A483A);
        assert_eq!(
            fk_bytes(&record.fields[1]),
            0x0000000F,
            "type-2 object_type scalar must be untouched"
        );
    }

    #[test]
    fn repairs_ptda_keyword_type3_master_byte_00_to_07() {
        // A FO76 PTDA keyword target at kind 3 carries
        // a truncated 00 master byte. The FO4 union schema models kind 3 as a
        // FormID-bearing `keyword` (→KYWD) variant, so the gate must let it through
        // and the remap rewrites 00793507 → 07793507 (the emitted output KYWD).
        let mut encoded = FxHashMap::default();
        encoded.insert(0x00793507, 0x07793507);
        let mut record = pack(vec![ptda(3, 0x00793507)]);

        let n = remap_value_selected_union_formids(&mut record, &encoded);

        assert_eq!(n, 1, "kind-3 keyword PTDA must be remapped 00→07");
        assert_eq!(fk_bytes(&record.fields[0]), 0x07793507);
    }

    #[test]
    fn repairs_ptda_type7_master_byte_00_to_07() {
        // kind 7 is also a FormID-bearing keyword variant in the FO4 union schema.
        let mut encoded = FxHashMap::default();
        encoded.insert(0x00796A01, 0x07796A01);
        let mut record = pack(vec![ptda(7, 0x00796A01)]);

        let n = remap_value_selected_union_formids(&mut record, &encoded);

        assert_eq!(n, 1, "kind-7 keyword PTDA must be remapped 00→07");
        assert_eq!(fk_bytes(&record.fields[0]), 0x07796A01);
    }

    #[test]
    fn leaves_already_encoded_and_unmapped_targets() {
        let mut encoded = FxHashMap::default();
        encoded.insert(0x008A483A, 0x078A483A);
        // type 0 but FK already carries a non-zero master byte (foreign/converted)
        // → rewrite_formid_at skips it; type 1 with an unmapped id → unchanged.
        let mut record = pack(vec![ptda(0, 0x07112233), ptda(1, 0x00999999)]);

        let n = remap_value_selected_union_formids(&mut record, &encoded);

        assert_eq!(n, 0);
        assert_eq!(fk_bytes(&record.fields[0]), 0x07112233);
        assert_eq!(fk_bytes(&record.fields[1]), 0x00999999);
    }

    #[test]
    fn remaps_pack_pdto_topic_ref_but_not_subtype_fourcc() {
        // PDTO type 0 = Topic Ref (DIAL FK) → remapped. type 1 = Topic Subtype,
        // a 4-byte fourCC ("CUST") at offset 4 — must be left intact even though
        // its little-endian u32 (0x54535543) is not in the map anyway. Use a
        // fourCC that DOES collide with a map key to prove the type gate, not the
        // map miss, is what protects it.
        let cust_le = u32::from_le_bytes(*b"CUST"); // 0x54535543
        let mut encoded = FxHashMap::default();
        encoded.insert(0x00548B7F, 0x07548B7F); // emitted DIAL → 07
        encoded.insert(cust_le, 0x07000001); // would corrupt the subtype fourCC
        let mut record = pack(vec![
            union_sub(b"PDTO", 0, 0x00548B7F),
            union_sub(b"PDTO", 1, cust_le),
        ]);

        let n = remap_value_selected_union_formids(&mut record, &encoded);

        assert_eq!(n, 1, "only the type-0 DIAL topic-ref FK is remapped");
        assert_eq!(fk_bytes(&record.fields[0]), 0x07548B7F);
        assert_eq!(
            fk_bytes(&record.fields[1]),
            cust_le,
            "type-1 subtype fourCC must be untouched"
        );
    }

    #[test]
    fn remaps_fact_plvd_location_reference() {
        // FACT PLVD shares PLDT's location_enum: type 0 = Reference FK. The
        // record-sig gate must visit FACT (not just PACK) for this to fire.
        let mut encoded = FxHashMap::default();
        encoded.insert(0x0037D921, 0x0737D921);
        let mut record = record_of("FACT", vec![union_sub(b"PLVD", 0, 0x0037D921)]);

        let n = remap_value_selected_union_formids(&mut record, &encoded);

        assert_eq!(n, 1);
        assert_eq!(fk_bytes(&record.fields[0]), 0x0737D921);
    }

    #[test]
    fn plvd_keyword_and_objectid_variants_remap_scalars_skip() {
        // location_enum: type 4 = Object ID FK, type 6 = Keyword FK (both
        // remapped); type 5 = Object Type enum scalar — skipped.
        let mut encoded = FxHashMap::default();
        encoded.insert(0x0005D5E6, 0x0705D5E6); // keyword
        encoded.insert(0x00112233, 0x07112233); // object_id
        encoded.insert(0x00000005, 0x07000005); // would corrupt the type-5 enum
        let mut record = record_of(
            "PACK",
            vec![
                union_sub(b"PLDT", 6, 0x0005D5E6),
                union_sub(b"PLDT", 4, 0x00112233),
                union_sub(b"PLDT", 5, 0x00000005),
            ],
        );

        let n = remap_value_selected_union_formids(&mut record, &encoded);

        assert_eq!(n, 2, "keyword + object_id remap; object_type enum skipped");
        assert_eq!(fk_bytes(&record.fields[0]), 0x0705D5E6);
        assert_eq!(fk_bytes(&record.fields[1]), 0x07112233);
        assert_eq!(
            fk_bytes(&record.fields[2]),
            0x00000005,
            "type-5 object-type enum must be untouched"
        );
    }
}
