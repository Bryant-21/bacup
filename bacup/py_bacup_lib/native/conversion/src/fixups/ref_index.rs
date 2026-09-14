//! Shared output-plugin reference index + struct-field FK walking helpers.
//!
//! Hosts the one `(local, plugin) → SigCode` index builder used by
//! `validate_reference_target_types` and the struct-internal FK passes, and
//! `validate_struct_fk_fields`, which reads `struct_field_layout` (keyed by
//! `"<SUB>.<field_id>"`) so the fix and detect sides agree on offsets and path-keys.

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

/// Null illegal-type struct-internal FKs of `record` where NULL is allowed,
/// checked against the schema's per-field `formlink_targets`. Works on raw
/// `struct:`-codec bytes via `struct_field_layout` (keyed by `"<SUB>.<field_id>"`),
/// reaching FKs the decoded-FieldValue walk misses (RACE DATA, MGEF DATA, IDLE ANAM,
/// COBJ FVPA components, ...).
///
/// `encoded_sig_of(raw)` maps an encoded (master-byte<<24 | local) FormID to its
/// record signature; `None` (dangling) is left to the invalid-target / sweep fixups.
/// A struct member can only be nulled, never removed, so an illegal FK in a field
/// that is not `null_allowed` is left and counted in `left_unfixable`.
pub fn validate_struct_fk_fields(
    record: &mut Record,
    schema: &AuthoringSchema,
    form_version: Option<u16>,
    encoded_sig_of: &dyn Fn(u32) -> Option<SigCode>,
) -> StructFkValidateReport {
    let mut report = StructFkValidateReport::default();
    let record_sig = record.sig.as_str();

    for entry in record.fields.iter_mut() {
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        let sub_sig = entry.sig.as_str();
        let layout = schema.struct_field_layout_versioned(record_sig, sub_sig, form_version);
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

/// Remap struct-internal FKs of `record` from source-local ids (master byte 0) to
/// target-encoded ids via `encoded_targets` (`source_object_id → target-encoded`).
/// Counterpart of `validate_struct_fk_fields` for FKs the `FormKeyMapper` never
/// reached in raw `struct:`/`array_struct:` bytes (LCEP/ACEP/LCUN actor/ref, RACE
/// DATA, MGEF DATA, ...), which kept their FO76 00-prefix.
///
/// - **Bytes path**: walks each `struct_field_layout` row and remaps FK offsets in
///   place. Skipped when `source_schema` shows the FO76 layout differs from FO4's:
///   remapping at FO4 offsets over a FO76 blob smears bytes into wrong fields (the
///   RACE/MGEF DATA corruption), and a dangling 00-prefix FK is safer. Raw index
///   zero is ambiguous after encoding: a compatible first-master signature proves
///   target ownership; without master evidence index zero is preserved.
/// - **List/Struct path** (schema-decoded, e.g. LCEP/LCUN `array_struct`): remaps
///   `FormKey` leaves via `target_by_source_form_key`, with no byte-offset math.
pub fn remap_struct_fk_fields(
    record: &mut Record,
    schema: &AuthoringSchema,
    source_schema: Option<&AuthoringSchema>,
    form_version: Option<u16>,
    first_target_master_sigs: Option<&FxHashMap<u32, SigCode>>,
    encoded_targets: &FxHashMap<u32, u32>,
    target_by_source_form_key: &FxHashMap<FormKey, FormKey>,
) -> StructFkRemapReport {
    let mut report = StructFkRemapReport::default();
    if encoded_targets.is_empty() && target_by_source_form_key.is_empty() {
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
                let fk_fields = layout
                    .iter()
                    .filter(|f| f.width == 4 && !f.formlink_targets.is_empty())
                    .collect::<Vec<_>>();
                if fk_fields.is_empty() {
                    continue;
                }
                let row_count = bytes.len() / row_size;
                for row in 0..row_count {
                    let base = row * row_size;
                    for field in &fk_fields {
                        let absolute_offset = base + field.offset;
                        let Some(slot) = bytes.get(absolute_offset..absolute_offset + 4) else {
                            continue;
                        };
                        let raw = u32::from_le_bytes(slot.try_into().unwrap());
                        if raw >> 24 == 0 {
                            let preserve = first_target_master_sigs.is_none_or(|signatures| {
                                signatures
                                    .get(&(raw & 0x00FF_FFFF))
                                    .is_some_and(|signature| {
                                        field.formlink_targets.iter().any(|target| {
                                            target.as_str().eq_ignore_ascii_case(signature.as_str())
                                        })
                                    })
                            });
                            if preserve {
                                continue;
                            }
                        }
                        if crate::fixups::rewrite_raw_object_template_formids::rewrite_formid_at(
                            bytes,
                            absolute_offset,
                            encoded_targets,
                        ) {
                            report.remapped += 1;
                        }
                    }
                }
            }
            // List/Struct-decoded subrecord (e.g. LCEP/LCUN `array_struct`): remap
            // nested FormKey leaves the Bytes path can't reach. Top-level scalar
            // `FieldValue::FormKey` subrecords (RNAM/ATKR/TPLT/...) were already
            // remapped by `mapper.rewrite_record` during translate and are skipped,
            // so a foreign-master leaf is not clobbered when a source record
            // shares its local.
            list_or_struct @ (FieldValue::List(_) | FieldValue::Struct(_)) => {
                if !target_by_source_form_key.is_empty() {
                    remap_formkey_leaves(list_or_struct, target_by_source_form_key, &mut report);
                }
            }
            _ => {}
        }
    }

    report
}

/// Whether the FO76 source struct layout for `(record_sig, sub_sig)` differs from
/// the FO4 `target_layout` enough that a byte-offset FK remap would smear FormIDs
/// into wrong fields (the RACE/MGEF DATA corruption).
///
/// Primary test: compare the ordered (offset, width) of the FK fields (width 4,
/// non-empty `formlink_targets`). This self-corrects: the FO76 RACE.DATA codec
/// claims more fields than the real 200-byte disk layout (a schema_forge bug), so
/// its FK offsets differ and the remap is skipped; once the codec matches, the remap
/// runs with no change here. A deny-list protects known dangerous structs when the
/// source layout is unavailable.
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
/// exact source identity is present in `target_by_source_form_key` is rewritten
/// to its target FormKey. Already-target-master leaves are not source keys.
fn remap_formkey_leaves(
    value: &mut FieldValue,
    target_by_source_form_key: &FxHashMap<FormKey, FormKey>,
    report: &mut StructFkRemapReport,
) {
    match value {
        FieldValue::FormKey(fk) => {
            if fk.local != 0 {
                if let Some(target) = target_by_source_form_key.get(fk) {
                    if *fk != *target {
                        *fk = *target;
                        report.remapped += 1;
                    }
                }
            }
        }
        FieldValue::List(items) => {
            for item in items.iter_mut() {
                remap_formkey_leaves(item, target_by_source_form_key, report);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, v) in fields.iter_mut() {
                remap_formkey_leaves(v, target_by_source_form_key, report);
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
    fn struct_validation_checks_each_raw_occurrence_and_skips_typed_fields() {
        let interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        let mut placed = Record::new(
            SigCode(*b"REFR"),
            fk(0x0015_781F, "SeventySix.esm", &interner),
        );
        for _ in 0..2 {
            let mut xlkr = vec![0; 8];
            xlkr[0..4].copy_from_slice(&0x0700_1000_u32.to_le_bytes());
            xlkr[4..8].copy_from_slice(&0x0700_2000_u32.to_le_bytes());
            placed.fields.push(crate::record::FieldEntry {
                sig: crate::ids::SubrecordSig(*b"XLKR"),
                value: FieldValue::Bytes(smallvec::SmallVec::from_vec(xlkr)),
            });
        }
        placed.fields.push(crate::record::FieldEntry {
            sig: crate::ids::SubrecordSig(*b"EDID"),
            value: FieldValue::String(interner.intern("Fixture")),
        });
        let resolver = |raw| match raw {
            0x0700_1000 => Some(SigCode(*b"STAT")),
            0x0700_2000 => Some(SigCode(*b"ACHR")),
            _ => None,
        };

        let report = validate_struct_fk_fields(&mut placed, &schema, None, &resolver);

        assert_eq!(report.nulled, 2);
        assert_eq!(report.left_unfixable, 0);
        for entry in &placed.fields[..2] {
            let FieldValue::Bytes(bytes) = &entry.value else {
                panic!("XLKR must remain raw bytes")
            };
            assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 0);
            assert_eq!(
                u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
                0x0700_2000
            );
        }
        assert!(matches!(&placed.fields[2].value, FieldValue::String(_)));
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
        map.insert(src_ref, tgt_ref); // only the recoverable one has a target

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
    fn struct_remap_preserves_compatible_first_master_refs_on_local_collision() {
        let interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        let mut pack = Record::new(SigCode(*b"PACK"), fk(0x1231B6, "FalloutNV.esm", &interner));
        let mut pkcu = Vec::new();
        pkcu.extend_from_slice(&7_u32.to_le_bytes());
        pkcu.extend_from_slice(&0x0000_2CE0_u32.to_le_bytes());
        pkcu.extend_from_slice(&2_u32.to_le_bytes());
        pack.fields.push(crate::record::FieldEntry {
            sig: crate::ids::SubrecordSig(*b"PKCU"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(pkcu)),
        });

        let mut info = Record::new(SigCode(*b"INFO"), fk(0x130161, "FalloutNV.esm", &interner));
        let mut trda = vec![0_u8; 20];
        trda[0..4].copy_from_slice(&0x000F_A847_u32.to_le_bytes());
        info.fields.push(crate::record::FieldEntry {
            sig: crate::ids::SubrecordSig(*b"TRDA"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(trda)),
        });

        let encoded_targets =
            FxHashMap::from_iter([(0x0000_2CE0, 0x0700_2CE0), (0x000F_A847, 0x070F_A847)]);
        let first_master_sigs = FxHashMap::from_iter([
            (0x0000_2CE0, SigCode(*b"PACK")),
            (0x000F_A847, SigCode(*b"KYWD")),
        ]);
        let source_to_target = FxHashMap::default();

        for record in [&mut pack, &mut info] {
            let report = remap_struct_fk_fields(
                record,
                &schema,
                Some(&schema),
                Some(131),
                Some(&first_master_sigs),
                &encoded_targets,
                &source_to_target,
            );
            assert_eq!(report.remapped, 0);
        }
        let FieldValue::Bytes(pkcu) = &pack.fields[0].value else {
            panic!("PKCU must remain raw bytes")
        };
        assert_eq!(
            u32::from_le_bytes(pkcu[4..8].try_into().unwrap()),
            0x0000_2CE0
        );
        let FieldValue::Bytes(trda) = &info.fields[0].value else {
            panic!("TRDA must remain raw bytes")
        };
        assert_eq!(
            u32::from_le_bytes(trda[0..4].try_into().unwrap()),
            0x000F_A847
        );
    }

    #[test]
    fn struct_remap_still_rewrites_first_master_local_with_incompatible_type() {
        let interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        let mut record = Record::new(SigCode(*b"INFO"), fk(0x130161, "FalloutNV.esm", &interner));
        let mut trda = vec![0_u8; 20];
        trda[0..4].copy_from_slice(&0x000F_A847_u32.to_le_bytes());
        record.fields.push(crate::record::FieldEntry {
            sig: crate::ids::SubrecordSig(*b"TRDA"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(trda)),
        });
        let report = remap_struct_fk_fields(
            &mut record,
            &schema,
            Some(&schema),
            Some(131),
            Some(&FxHashMap::from_iter([(0x000F_A847, SigCode(*b"REFR"))])),
            &FxHashMap::from_iter([(0x000F_A847, 0x070F_A847)]),
            &FxHashMap::default(),
        );

        assert_eq!(report.remapped, 1);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("TRDA must remain raw bytes")
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x070F_A847
        );

        let FieldValue::Bytes(bytes) = &mut record.fields[0].value else {
            unreachable!()
        };
        bytes[0..4].copy_from_slice(&0x000F_A847_u32.to_le_bytes());
        let report = remap_struct_fk_fields(
            &mut record,
            &schema,
            Some(&schema),
            Some(131),
            None,
            &FxHashMap::from_iter([(0x000F_A847, 0x070F_A847)]),
            &FxHashMap::default(),
        );
        assert_eq!(report.remapped, 0);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            unreachable!()
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x000F_A847
        );
    }

    #[test]
    fn remap_struct_fk_fields_does_not_clobber_top_level_scalar_formkey() {
        // A top-level scalar `formid` subrecord (e.g. NPC_ RNAM) already remapped
        // to a foreign DLC master must not be re-walked by the source-keyed map,
        // even when a source record shares its object-id. Here RNAM points at
        // DLCCoast.esm:0247C1 (the Gulper race); the source had a STAT at the
        // same local 0247C1 → Fallout4.esm:0247C1.
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

        let mut target_by_source_form_key = FxHashMap::default();
        target_by_source_form_key.insert(fk(0x0247C1, "Source.esm", &interner), wrong);
        let encoded: FxHashMap<u32, u32> = FxHashMap::default();

        let report = remap_struct_fk_fields(
            &mut record,
            &schema,
            None,
            Some(131),
            None,
            &encoded,
            &target_by_source_form_key,
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
        let map: FxHashMap<FormKey, FormKey> = FxHashMap::default();
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
