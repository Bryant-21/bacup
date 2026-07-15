//! Shared output-plugin reference index + struct-field FK walking helpers.
//!
//! Several fixups need to answer "given an output-plugin FormKey, what record
//! signature does it resolve to?" — reference-type validation
//! (`validate_reference_target_types`) and the struct-internal FK passes. This
//! module hosts the single shared builder so the `(local, plugin) → SigCode`
//! index is defined once.
//!
//! It also hosts `validate_struct_fk_fields` — the shared struct-internal FK
//! validator that drives off `struct_field_layout`, keyed by
//! `"<SUB>.<field_id>"`, so the fix side and the detect side (validator) agree
//! on offsets + path-keys by construction.

use rustc_hash::FxHashMap;

use esp_authoring_core::plugin_runtime::ensure_core_section;

use crate::fixups::FixupError;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

/// Build a map from `(local, plugin_sym)` → `SigCode` for every record in the
/// output plugin, using the index (no record decode required).
///
/// One `form_keys_of_sig` call per signature present in the target plugin.
pub fn build_target_fk_sig_map(
    session: &mut PluginSession,
    interner: &StringInterner,
) -> Result<FxHashMap<(u32, Sym), SigCode>, FixupError> {
    let sigs = {
        let core = ensure_core_section(session.target_slot_mut());
        core.by_signature_form_keys
            .keys()
            .filter_map(|sig| SigCode::from_str(sig.as_str()).ok())
            .collect::<Vec<_>>()
    };

    let mut map: FxHashMap<(u32, Sym), SigCode> = FxHashMap::default();
    for sig in sigs {
        let fks = session
            .form_keys_of_sig(sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        for fk in fks {
            map.insert((fk.local, fk.plugin), sig);
        }
    }
    Ok(map)
}

/// Convenience: resolve a `FormKey` to its output-plugin record signature.
///
/// Returns `None` for a null FK or one absent from the index (dangling /
/// external master).
pub fn target_record_sig(map: &FxHashMap<(u32, Sym), SigCode>, fk: &FormKey) -> Option<SigCode> {
    if fk.local == 0 {
        return None;
    }
    map.get(&(fk.local, fk.plugin)).copied()
}

/// Outcome of a struct-internal FK validation pass over one record.
#[derive(Debug, Default, Clone, Copy)]
pub struct StructFkValidateReport {
    /// Struct-internal FK slots zeroed (illegal type, NULL allowed).
    pub nulled: u32,
    /// Illegal-type FK slots left in place because NULL is not allowed and a
    /// struct member can't be removed without corrupting the row (warned).
    pub left_unfixable: u32,
}

impl StructFkValidateReport {
    pub fn changed(&self) -> bool {
        self.nulled > 0
    }
}

/// Validate every struct-internal FK field of `record` against the schema's
/// per-field `formlink_targets`, nulling illegal-type refs where NULL is
/// allowed. Operates on raw `struct:`-codec subrecord bytes via schema-lead's
/// `struct_field_layout` (offsets/widths keyed by `"<SUB>.<field_id>"`), so it
/// reaches the FKs that the decoded-FieldValue walk misses (RACE DATA, MGEF
/// DATA, IDLE ANAM, COBJ FVPA components, etc.).
///
/// `encoded_sig_of(raw)` resolves an encoded (master-byte<<24 | local) FormID
/// to its target record signature, or `None` when it doesn't resolve (dangling
/// — left to the invalid-target / sweep fixups, not nulled here).
///
/// Action per illegal-type struct FK (a struct member cannot be structurally
/// removed, only nulled):
/// - `formlink_targets` empty ⇒ unconstrained, skip.
/// - resolved sig in `formlink_targets` ⇒ legal, skip.
/// - illegal AND `null_allowed` ⇒ zero the 4 bytes (NULL).
/// - illegal AND NOT `null_allowed` ⇒ leave + count `left_unfixable` (cannot
///   null without producing a NULL-where-required error, cannot strip a struct
///   member; only a retarget would fix it — out of scope here).
pub fn validate_struct_fk_fields(
    record: &mut Record,
    schema: &AuthoringSchema,
    form_version: Option<u16>,
    encoded_sig_of: &dyn Fn(u32) -> Option<SigCode>,
) -> StructFkValidateReport {
    let mut report = StructFkValidateReport::default();
    let record_sig = record.sig.as_str().to_string();

    // Occurrence-agnostic: validate the first struct layout for each subrecord
    // sig. Multi-occurrence struct subrecords share one layout.
    let mut handled_sigs: rustc_hash::FxHashSet<String> = rustc_hash::FxHashSet::default();

    for entry in record.fields.iter_mut() {
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        let sub_sig = entry.sig.as_str().to_string();
        if !handled_sigs.insert(sub_sig.clone()) {
            // Still validate each occurrence's bytes, but only fetch layout once
            // per sig below; we re-fetch cheaply since it's a Vec lookup.
        }
        let layout = schema.struct_field_layout_versioned(&record_sig, &sub_sig, form_version);
        if layout.is_empty() {
            continue;
        }
        for field in &layout {
            if field.formlink_targets.is_empty() || field.width != 4 {
                continue;
            }
            let off = field.offset;
            let Some(slot) = bytes.get(off..off + 4) else {
                continue;
            };
            let raw = u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]);
            if raw == 0 {
                continue; // already null
            }
            let Some(sig) = encoded_sig_of(raw) else {
                continue; // dangling — not ours (sweep/invalid-target owns it)
            };
            if field
                .formlink_targets
                .iter()
                .any(|t| t.as_str() == sig.as_str())
            {
                continue; // legal type
            }
            // Illegal type.
            if field.null_allowed {
                bytes[off..off + 4].copy_from_slice(&0u32.to_le_bytes());
                report.nulled += 1;
            } else {
                report.left_unfixable += 1;
            }
        }
    }

    report
}

/// Outcome of a struct-internal FK remap pass over one record.
#[derive(Debug, Default, Clone, Copy)]
pub struct StructFkRemapReport {
    /// Struct-internal FK slots rewritten source-local → target-encoded.
    pub remapped: u32,
}

impl StructFkRemapReport {
    pub fn changed(&self) -> bool {
        self.remapped > 0
    }
}

/// Remap every struct-internal FK field of `record` from a source-local id
/// (master byte 0) to its target-encoded id via `encoded_targets`
/// (`source_object_id → target-encoded u32`). This is the REMAP counterpart of
/// `validate_struct_fk_fields`: it fixes FKs the `FormKeyMapper` never reached
/// because they live in raw `struct:`/`array_struct:` codec bytes (LCEP/ACEP/
/// LCUN actor/ref, RACE DATA, MGEF DATA, etc.) and so kept their FO76 00-prefix.
///
/// Two paths, by how the subrecord decoded:
/// - **Bytes path** (opaque `struct:`/`array_struct:` blob): drives off
///   schema-lead's `struct_field_layout`, iterating each row and remapping the
///   FK offsets in place. GUARDED: when `source_schema` is supplied and the
///   FO76 source struct layout for the subrecord DIFFERS from the FO4 target
///   layout, the byte-offset remap is SKIPPED — remapping at FO4 offsets over a
///   FO76-laid-out blob smears bytes into wrong fields (the RACE DATA / MGEF
///   DATA corruption). Leaving a dangling 00-prefix FK is strictly
///   safer than smearing.
/// - **List/Struct path** (schema-decoded, e.g. LCEP/LCUN `array_struct` with
///   `formid` fields → `FieldValue::List` of structs): remaps `FormKey` leaves
///   directly via `target_by_source_local` (`source local id → target FormKey`).
///   This is layout-SAFE — no byte-offset math — and covers the FKs that the
///   Bytes path silently skipped because the value isn't `Bytes`.
pub fn remap_struct_fk_fields(
    record: &mut Record,
    schema: &AuthoringSchema,
    source_schema: Option<&AuthoringSchema>,
    form_version: Option<u16>,
    encoded_targets: &FxHashMap<u32, u32>,
    target_by_source_local: &FxHashMap<u32, FormKey>,
) -> StructFkRemapReport {
    let mut report = StructFkRemapReport::default();
    if encoded_targets.is_empty() && target_by_source_local.is_empty() {
        return report;
    }
    let record_sig = record.sig.as_str().to_string();

    for entry in record.fields.iter_mut() {
        let sub_sig = entry.sig.as_str().to_string();
        match &mut entry.value {
            FieldValue::Bytes(bytes) => {
                let layout =
                    schema.struct_field_layout_versioned(&record_sig, &sub_sig, form_version);
                if layout.is_empty() {
                    continue;
                }
                // GUARD: skip the byte-offset remap when the FO76 source
                // struct layout diverges from the FO4 target layout, else we'd
                // rewrite FKs at the wrong byte positions over a source-laid-out
                // blob and smear bytes into wrong-typed slots.
                if source_struct_layout_diverges(
                    source_schema,
                    &record_sig,
                    &sub_sig,
                    form_version,
                    &layout,
                ) {
                    continue;
                }
                // FK fields (width 4) + the row stride = span of the whole layout row.
                let row_size = layout.iter().map(|f| f.offset + f.width).max().unwrap_or(0);
                if row_size == 0 || bytes.len() % row_size != 0 {
                    continue; // not a clean row layout — skip (don't corrupt)
                }
                let fk_offsets: Vec<usize> = layout
                    .iter()
                    .filter(|f| f.width == 4 && !f.formlink_targets.is_empty())
                    .map(|f| f.offset)
                    .collect();
                if fk_offsets.is_empty() {
                    continue;
                }
                let row_count = bytes.len() / row_size;
                for row in 0..row_count {
                    let base = row * row_size;
                    for off in &fk_offsets {
                        if crate::fixups::rewrite_raw_object_template_formids::rewrite_formid_at(
                            bytes,
                            base + off,
                            encoded_targets,
                        ) {
                            report.remapped += 1;
                        }
                    }
                }
            }
            // List/Struct-decoded subrecord (e.g. LCEP/LCUN `array_struct` →
            // List of Structs): remap nested FormKey leaves the Bytes path can't
            // reach. RESTRICTED to List/Struct only — a top-level scalar
            // `FieldValue::FormKey` subrecord (RNAM/ATKR/TPLT/...) was ALREADY
            // remapped by `mapper.rewrite_record` during translate, which walks
            // every top-level FormKey field. Re-walking it here with the
            // plugin-BLIND `target_by_source_local` (keyed by object-id only)
            // wrongly overwrites a correctly-remapped foreign-master leaf when a
            // SOURCE record shares that object-id — e.g. an NPC RNAM correctly
            // pointing at DLCCoast.esm:0247C1 (Gulper race) gets clobbered to
            // Fallout4.esm:0247C1 (FO76 STAT IndSilo32Top01, a source object at
            // the same local) → "no race → HumanRace" creature CTD.
            list_or_struct @ (FieldValue::List(_) | FieldValue::Struct(_)) => {
                if !target_by_source_local.is_empty() {
                    remap_formkey_leaves(list_or_struct, target_by_source_local, &mut report);
                }
            }
            _ => {}
        }
    }

    report
}

/// Whether the FO76 source struct layout for `(record_sig, sub_sig)` differs
/// from the FO4 `target_layout` such that a byte-offset FK remap is unsafe
/// (it would rewrite FormIDs at FO4 offsets over source-laid-out bytes and smear
/// them into wrong fields — the RACE/MGEF DATA corruption).
///
/// PRIMARY, self-healing test: compare the ordered (offset, width) of the FK
/// fields (width-4 fields with a non-empty `formlink_targets`); a mismatch ⇒
/// skip. This auto-corrects: while the FO76 RACE.DATA codec is wrong (it claims
/// extra fields vs the real 200-byte disk layout, a schema_forge bug) the FK
/// offsets differ from FO4's, so we skip and avoid the smear; once schema-lead
/// fixes the FO76 codec to the true layout, source==target and the remap is
/// allowed to proceed and fix the legit 00-prefix FKs — no code change here.
///
/// FALLBACK deny-list: only consulted when the source layout is UNAVAILABLE
/// (no source schema, or the source doesn't model this struct), so the known
/// dangerous structs are still protected when we can't compare.
fn source_struct_layout_diverges(
    source_schema: Option<&AuthoringSchema>,
    record_sig: &str,
    sub_sig: &str,
    form_version: Option<u16>,
    target_layout: &[esp_authoring_core::plugin_runtime::StructFieldInfo<'_>],
) -> bool {
    // Known dangerous structs — used ONLY as a fallback when we can't compare
    // layouts (see below). Not an unconditional skip, so a corrected source
    // codec lets the remap proceed.
    const KNOWN_DANGEROUS: &[(&str, &str)] = &[("RACE", "DATA"), ("MGEF", "DATA")];
    let is_known_dangerous = KNOWN_DANGEROUS
        .iter()
        .any(|(r, s)| *r == record_sig && *s == sub_sig);

    let Some(source_schema) = source_schema else {
        // No source layout to compare against — protect the known cases only.
        return is_known_dangerous;
    };
    // Compare at the SAME form_version the target layout was computed at, so a
    // version-gated struct (RACE.DATA: FV-gated 143/188 fields) is measured at
    // the record's actual layout on both sides — see #35.
    let source_layout =
        source_schema.struct_field_layout_versioned(record_sig, sub_sig, form_version);
    if source_layout.is_empty() {
        // Source doesn't model this struct (e.g. an FO4-only subrecord): can't
        // prove equivalence. Protect the known-dangerous cases; treat everything
        // else as non-divergent rather than over-skip every subrecord.
        return is_known_dangerous;
    }
    let fk_sig = |layout: &[esp_authoring_core::plugin_runtime::StructFieldInfo<'_>]| {
        layout
            .iter()
            .filter(|f| f.width == 4 && !f.formlink_targets.is_empty())
            .map(|f| (f.offset, f.width))
            .collect::<Vec<_>>()
    };
    fk_sig(&source_layout) != fk_sig(target_layout)
}

/// Recursively remap `FormKey` leaves inside a List/Struct value: a FK whose
/// local id is a converted source id (`target_by_source_local`) is rewritten to
/// its target FormKey. Only source-local ids are touched; FKs already pointing
/// at converted/foreign records are left alone (their local isn't in the map).
fn remap_formkey_leaves(
    value: &mut FieldValue,
    target_by_source_local: &FxHashMap<u32, FormKey>,
    report: &mut StructFkRemapReport,
) {
    match value {
        FieldValue::FormKey(fk) => {
            if fk.local != 0 {
                if let Some(target) = target_by_source_local.get(&fk.local) {
                    if *fk != *target {
                        *fk = *target;
                        report.remapped += 1;
                    }
                }
            }
        }
        FieldValue::List(items) => {
            for item in items.iter_mut() {
                remap_formkey_leaves(item, target_by_source_local, report);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, v) in fields.iter_mut() {
                remap_formkey_leaves(v, target_by_source_local, report);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;

    fn fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    #[test]
    fn remap_formkey_leaves_rewrites_nested_source_local_fks() {
        let interner = StringInterner::new();
        // LCEP-shaped: a List of Structs each with two FormKey leaves.
        let src_ref = fk(0x18116E, "Fallout4.esm", &interner); // 00-prefix mislabel
        let src_parent = fk(0x3CBFFD, "Fallout4.esm", &interner); // dropped — no target
        let tgt_ref = fk(0x18116E, "SeventySix.esm", &interner);

        let mut value = FieldValue::List(vec![FieldValue::Struct(vec![
            (interner.intern("Ref"), FieldValue::FormKey(src_ref)),
            (
                interner.intern("EnableParent"),
                FieldValue::FormKey(src_parent),
            ),
        ])]);

        let mut map = FxHashMap::default();
        map.insert(0x18116E_u32, tgt_ref); // only the recoverable one has a target

        let mut report = StructFkRemapReport::default();
        remap_formkey_leaves(&mut value, &map, &mut report);

        assert_eq!(report.remapped, 1, "only the FK with a target is remapped");
        let FieldValue::List(items) = &value else {
            panic!()
        };
        let FieldValue::Struct(fields) = &items[0] else {
            panic!()
        };
        assert_eq!(
            fields[0].1,
            FieldValue::FormKey(tgt_ref),
            "Ref -> 07 target"
        );
        assert_eq!(
            fields[1].1,
            FieldValue::FormKey(src_parent),
            "dropped EnableParent left as-is (no target; null/strip is a separate decision)"
        );
    }

    #[test]
    fn remap_struct_fk_fields_does_not_clobber_top_level_scalar_formkey() {
        // A top-level scalar `formid` subrecord
        // (e.g. NPC_ RNAM) already correctly remapped to a foreign DLC master
        // must NOT be re-walked by the plugin-blind `target_by_source_local`,
        // even when a SOURCE record shares its object-id. Here RNAM points at
        // DLCCoast.esm:0247C1 (the Gulper race); the source had a STAT at the
        // same local 0247C1 → Fallout4.esm:0247C1. The fix restricts the
        // List/Struct leaf-walk to List/Struct values, leaving scalars alone.
        let interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("fo4").expect("fo4 schema");

        let correct = fk(0x0247C1, "DLCCoast.esm", &interner); // already remapped
        let wrong = fk(0x0247C1, "Fallout4.esm", &interner); // source-local collision

        let mut record = Record::new(
            crate::ids::SigCode::from_str("NPC_").unwrap(),
            fk(0x000800, "Output.esm", &interner),
        );
        record.fields = smallvec::smallvec![crate::record::FieldEntry {
            sig: crate::ids::SubrecordSig::from_str("RNAM").unwrap(),
            value: FieldValue::FormKey(correct),
        }];

        let mut target_by_source_local = FxHashMap::default();
        target_by_source_local.insert(0x0247C1_u32, wrong);
        let encoded: FxHashMap<u32, u32> = FxHashMap::default();

        let report = remap_struct_fk_fields(
            &mut record,
            &schema,
            None,
            Some(131),
            &encoded,
            &target_by_source_local,
        );

        assert_eq!(
            report.remapped, 0,
            "top-level scalar RNAM must be left alone"
        );
        assert_eq!(
            record.fields[0].value,
            FieldValue::FormKey(correct),
            "RNAM must keep DLCCoast.esm:0247C1, not be clobbered to Fallout4.esm:0247C1",
        );
    }

    #[test]
    fn remap_formkey_leaves_skips_null_and_unmapped() {
        let interner = StringInterner::new();
        let mut value = FieldValue::FormKey(fk(0, "Fallout4.esm", &interner));
        let map: FxHashMap<u32, FormKey> = FxHashMap::default();
        let mut report = StructFkRemapReport::default();
        remap_formkey_leaves(&mut value, &map, &mut report);
        assert_eq!(report.remapped, 0);
    }

    #[test]
    fn layout_guard_denylists_race_and_mgef_data_without_source_schema() {
        // Deny-list branch: no source schema available, but RACE.DATA / MGEF.DATA
        // are known FO76↔FO4 layout-divergent → must report divergent (skip).
        assert!(source_struct_layout_diverges(
            None,
            "RACE",
            "DATA",
            Some(131),
            &[]
        ));
        assert!(source_struct_layout_diverges(
            None,
            "MGEF",
            "DATA",
            Some(131),
            &[]
        ));
        // A subrecord not on the deny-list, with no source schema, is treated as
        // non-divergent (the byte remap proceeds — deny-list covers the danger).
        assert!(!source_struct_layout_diverges(
            None,
            "LCTN",
            "LCSR",
            Some(131),
            &[]
        ));
    }
}
