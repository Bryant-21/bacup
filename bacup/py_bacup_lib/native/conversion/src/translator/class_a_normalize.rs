//! Class A normalizer — schema-driven flag/enum domain pass (FO76 → FO4).
//!
//! Catch-all floor that guarantees every flag/enum value position in a
//! translated record is valid in the FO4 target domain. It is the systemic
//! replacement for the scattered per-record literals in
//! `translator::target_hooks::fo4` and the per-field YAML `remap_enum`
//! invocations — neither schema-driven, which is why ~15k FO76-only flag/enum
//! values reached the FO4 output (xEdit Class A errors).
//!
//! # Pipeline placement
//!
//! Runs as an explicit stage in `run::Run::translate_fks_with_mode`, **after**
//! `TargetHook::run` (FO4 record-specific semantic hooks) and **before**
//! `TargetRecordNormalizer::normalize`. Gated to `source == Fo76 && target ==
//! Fo4`. The caller pushes the returned decisions/warnings onto the run's sinks.
//!
//! # Policy (applied per value position)
//!
//! 1. value already valid in FO4 → keep (no-op).
//! 2. flag bitfield → clear bits not in the FO4 valid mask.
//! 3. scalar enum value not in the FO4 set → clamp to the schema `fallback_value`
//!    (typically the `0`/`none` value). Whole-subrecord removal is NOT done here
//!    (that risks cascading "missing subrecord" errors — Class D's domain).
//!
//! Header-flag masking only ever *clears* invalid bits; it never *sets*
//! required-set bits (e.g. NAVM bit18). When the FO4 schema has no flag metadata
//! for a record (`record_flags()` is `None`), header flags are left untouched —
//! a valid-but-unmodeled FO4 bit must never be silently dropped.
//!
//! Every clear/clamp is recorded in `ClassANormalizeReport` so the Class A error
//! count is auditable.

use crate::record::{FieldValue, Record, RecordFlags};
use crate::schema::AuthoringSchema;
use crate::sym::StringInterner;
use esp_authoring_core::plugin_runtime::{
    SchemaEnumJson, SchemaFieldJson, SchemaRecordJson, SchemaSubrecordJson,
};

const RECORD_FLAG_DENY_MASKS: &[(&str, u32)] = &[("FLOR", 0x0000_8000)];
// QUST.FNAM is scope-overloaded; the compiled schema resolves its first FNAM
// definition (objective flags), so alias rows need the runtime alias mask.
const FO4_QUST_ALIAS_FLAG_MASK: u32 = 0x017F_FFFF;

/// Outcome of a Class A normalization pass over one record.
///
/// Carries human-readable strings rather than interned `Sym`s so the caller can
/// decide whether to push them as decisions or warnings against its own
/// interner (the pass does not own the run's sinks).
#[derive(Debug, Default)]
pub struct ClassANormalizeReport {
    /// Header-flag bits cleared because they have no FO4 meaning.
    pub header_flag_bits_cleared: u32,
    /// Per-value clear / clamp actions, for auditing the error drop.
    pub decisions: Vec<String>,
    /// Non-fatal notices (e.g. FO4 schema unavailable).
    pub warnings: Vec<String>,
}

/// Normalize all flag/enum value positions in `record` to the FO4 target
/// domain, in place. `schema` is the FO4-target `AuthoringSchema`.
pub fn normalize_flags_and_enums(
    record: &mut Record,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> ClassANormalizeReport {
    let mut report = ClassANormalizeReport::default();
    let sig = record.sig.as_str().to_string();

    // ── 1. Record header flags ───────────────────────────────────────────
    // Mask the raw 32-bit word (preserved via `from_bits_retain` at read)
    // against the FO4 per-record valid mask. `record_flags() == None` ⇒ no
    // metadata captured ⇒ do NOT strip.
    if let Some(rf) = schema.record_flags(&sig) {
        let raw = record.flags.bits();
        let stripped = rf.strip(raw) & !record_flag_deny_mask(&sig);
        let cleared = raw & !stripped;
        if cleared != 0 {
            record.flags = RecordFlags::from_bits_retain(stripped);
            report.header_flag_bits_cleared = cleared;
            report
                .decisions
                .push(format!("class_a:{sig}:header_flags cleared {cleared:#x}"));
        }
    }

    // ── 2. Subrecord flag bitfields + enum scalars ───────────────────────
    // Drive from the authoritative compiled record def (which carries the
    // enum_refs the conversion crate's minimal view drops). For each enum-
    // bearing schema path, locate the matching decoded value(s) and normalize.
    let Some(rec_def) = schema.compiled_record_def(&sig) else {
        return report;
    };
    normalize_record_against_def(record, rec_def, schema, &sig, interner, &mut report);

    report
}

fn record_flag_deny_mask(sig: &str) -> u32 {
    RECORD_FLAG_DENY_MASKS
        .iter()
        .find_map(|(record_sig, mask)| (*record_sig == sig).then_some(*mask))
        .unwrap_or(0)
}

/// Walk the compiled record def's subrecords; for each one that has a decoded
/// counterpart in `record`, normalize its enum-bearing positions.
fn normalize_record_against_def(
    record: &mut Record,
    rec_def: &SchemaRecordJson,
    schema: &AuthoringSchema,
    rec_sig: &str,
    interner: &StringInterner,
    report: &mut ClassANormalizeReport,
) {
    let mut in_qust_aliases = false;
    for entry in record.fields.iter_mut() {
        let sub_sig = entry.sig.as_str();
        if rec_sig == "QUST" && sub_sig == "ANAM" {
            in_qust_aliases = true;
        }
        if in_qust_aliases && sub_sig == "FNAM" {
            normalize_qust_alias_flags(&mut entry.value, rec_sig, report);
            continue;
        }
        let Some(sub_def) = rec_def.subrecords.iter().find(|s| s.id == sub_sig) else {
            continue;
        };
        normalize_subrecord(
            &mut entry.value,
            sub_def,
            schema,
            rec_sig,
            sub_sig,
            interner,
            report,
        );
    }
}

fn normalize_qust_alias_flags(
    value: &mut FieldValue,
    rec_sig: &str,
    report: &mut ClassANormalizeReport,
) {
    let raw = match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            u32::from_le_bytes(bytes[..4].try_into().expect("four-byte FNAM prefix"))
        }
        FieldValue::Uint(value) => *value as u32,
        FieldValue::Int(value) => *value as u32,
        _ => return,
    };
    let masked = raw & FO4_QUST_ALIAS_FLAG_MASK;
    if masked == raw {
        return;
    }

    match value {
        FieldValue::Bytes(bytes) => bytes[..4].copy_from_slice(&masked.to_le_bytes()),
        FieldValue::Uint(value) => *value = masked as u64,
        FieldValue::Int(value) => *value = masked as i64,
        _ => unreachable!("matched QUST alias FNAM representation"),
    }
    report.decisions.push(format!(
        "class_a:{rec_sig}.FNAM:alias_flags {raw:#x}->{masked:#x}"
    ));
}

/// Normalize one decoded subrecord value against its schema def.
fn normalize_subrecord(
    value: &mut FieldValue,
    sub_def: &SchemaSubrecordJson,
    schema: &AuthoringSchema,
    rec_sig: &str,
    sub_sig: &str,
    interner: &StringInterner,
    report: &mut ClassANormalizeReport,
) {
    // Raw-bytes subrecords (struct: / union codecs decode to FieldValue::Bytes
    // in source_read — "full struct decode" never landed). The enum/flag fields
    // live INSIDE the blob at byte offsets the schema layout util provides; mask
    // them in place. This is the high-volume path (BOOK.DNAM, LIGH.DATA, DSTD,
    // RACE.DATA, EXPL.DATA, SNDR.LNAM, NPC_.AIDT, PROJ/HAZD.DNAM, REFR.XRDO,
    // LVLN/LVLI.LVLF, ...).
    if let FieldValue::Bytes(buf) = value {
        mask_struct_bytes(buf, schema, rec_sig, sub_sig, report);
        return;
    }

    // Subrecord-level enum (e.g. KYWD.TNAM — the whole subrecord is an enum).
    if let Some(enum_ref) = sub_def.enum_ref.as_deref() {
        if let Some(edef) = schema.enum_def(enum_ref) {
            normalize_scalar_value(value, edef, rec_sig, sub_sig, interner, report);
        }
    }

    // Struct/list field positions carrying enum_refs (e.g. OMOD DATA props).
    if !sub_def.fields.is_empty() {
        normalize_against_fields(
            value,
            &sub_def.fields,
            schema,
            rec_sig,
            sub_sig,
            interner,
            report,
        );
    }
}

/// Mask/clamp every flag/enum field that lives inside a raw subrecord byte blob,
/// using the schema's struct-field layout for offsets/widths.
///
/// The layout is resolved at the FO4 TARGET form_version (131) — the version the
/// output records are written at. This matters for `record_form_version` union
/// subrecords (e.g. EFSH.DNAM): the version-less layout defaults to form_version
/// 0, which selects the OLD-format variant (a leading byte ⇒ enum fields shifted
/// to the wrong offsets) and masks the carried NEW-format bytes at the wrong
/// slots, zeroing valid enums (the EFSH `<Unknown:0>` blend-op/z-test bug).
/// Selecting the fv131 variant matches the real byte layout. For
/// non-`record_form_version` (width-selector) unions (LVLN.LVLF 1 vs 2;
/// IPCT/TXST.DODT 36 vs 28) variant selection is unaffected by form_version, and
/// every field is bounds-checked against `buf.len()` so a width mismatch skips
/// safely rather than corrupting.
fn mask_struct_bytes(
    buf: &mut [u8],
    schema: &AuthoringSchema,
    rec_sig: &str,
    sub_sig: &str,
    report: &mut ClassANormalizeReport,
) {
    let layout = schema.struct_field_layout_versioned(
        rec_sig,
        sub_sig,
        Some(crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION),
    );
    if layout.is_empty() {
        return;
    }
    // Single-field subrecords (incl. scalar/union-arm codecs like LVLN/LVLI.LVLF)
    // occupy the whole blob: the FO76 source arm may be wider than the variant
    // `struct_field_layout(None)` resolved (uint16 vs uint8), so use the actual
    // buffer width. Masking with the FO4 valid_flag_mask still zeroes any
    // high-byte invalid bits, so this is correct without threading form_version.
    let single_full_field = layout.len() == 1 && layout[0].offset == 0;
    for field in &layout {
        let Some(enum_ref) = field.enum_ref else {
            continue;
        };
        let Some(edef) = schema.enum_def(enum_ref) else {
            continue;
        };
        let off = field.offset;
        let width = if single_full_field {
            buf.len().min(8)
        } else {
            field.width
        };
        if width == 0 || width > 8 || off.saturating_add(width) > buf.len() {
            // Truncated/short FO76 struct or a variable-width field — skip, don't
            // corrupt. (Class D owns truncated-struct repair.)
            continue;
        }
        let cur = read_le(&buf[off..off + width]) as i128;
        let new = if edef.is_flags() {
            cur & edef.valid_flag_mask()
        } else if edef.contains_value(cur) {
            cur
        } else {
            edef.fallback_value().unwrap_or(0)
        };
        if new != cur {
            write_le(&mut buf[off..off + width], new as u128);
            report.decisions.push(format!(
                "class_a:{rec_sig}.{}:{} {cur:#x}->{new:#x}",
                field.field_path,
                if edef.is_flags() { "flags" } else { "enum" },
            ));
        }
    }
}

/// Read a little-endian unsigned integer of `bytes.len()` (1..=8) bytes.
fn read_le(bytes: &[u8]) -> u64 {
    let mut v = 0u64;
    for (i, &b) in bytes.iter().enumerate() {
        v |= (b as u64) << (8 * i);
    }
    v
}

/// Write the low `bytes.len()` bytes of `value` little-endian into `bytes`.
fn write_le(bytes: &mut [u8], value: u128) {
    for (i, slot) in bytes.iter_mut().enumerate() {
        *slot = (value >> (8 * i)) as u8;
    }
}

/// Match a decoded Struct/List value against a list of schema field defs and
/// normalize every enum-bearing position (recursing into nested structs/lists).
fn normalize_against_fields(
    value: &mut FieldValue,
    field_defs: &[SchemaFieldJson],
    schema: &AuthoringSchema,
    rec_sig: &str,
    path: &str,
    interner: &StringInterner,
    report: &mut ClassANormalizeReport,
) {
    match value {
        FieldValue::List(items) => {
            for item in items.iter_mut() {
                normalize_against_fields(item, field_defs, schema, rec_sig, path, interner, report);
            }
        }
        FieldValue::Struct(fields) => {
            for (key, val) in fields.iter_mut() {
                let key_str = interner.resolve(*key).unwrap_or("");
                let Some(fd) = field_defs.iter().find(|f| field_def_matches(f, key_str)) else {
                    continue;
                };
                let child_path = format!("{path}.{key_str}");
                if let Some(enum_ref) = fd.enum_ref.as_deref() {
                    if let Some(edef) = schema.enum_def(enum_ref) {
                        normalize_scalar_value(val, edef, rec_sig, &child_path, interner, report);
                    }
                }
                if !fd.fields.is_empty() {
                    normalize_against_fields(
                        val,
                        &fd.fields,
                        schema,
                        rec_sig,
                        &child_path,
                        interner,
                        report,
                    );
                }
            }
        }
        _ => {}
    }
}

/// A schema field def matches a decoded struct key if the key equals its `id`
/// or its `display_label` (decoded keys use the authoring/display name).
fn field_def_matches(fd: &SchemaFieldJson, key: &str) -> bool {
    fd.id == key || fd.display_label.as_deref() == Some(key)
}

/// Apply the mask (flags) or clamp (scalar enum) policy to one decoded value.
fn normalize_scalar_value(
    value: &mut FieldValue,
    edef: &SchemaEnumJson,
    rec_sig: &str,
    path: &str,
    interner: &StringInterner,
    report: &mut ClassANormalizeReport,
) {
    // Coerce to i128 (matches SchemaEnumJson value type). A string label resolves
    // via the schema; an unresolvable label is a miss → clamp (value enums) or
    // leave (flags).
    let cur: i128 = match value {
        FieldValue::Int(i) => *i as i128,
        FieldValue::Uint(u) => *u as i128,
        FieldValue::Float(f) => *f as i128,
        FieldValue::String(s) => {
            match interner
                .resolve(*s)
                .and_then(|t| edef.value_for_token_or_label(t))
            {
                Some(v) => v,
                None => {
                    if !edef.is_flags() {
                        let fb = edef.fallback_value().unwrap_or(0);
                        write_int(value, fb);
                        report.decisions.push(format!(
                            "class_a:{rec_sig}.{path}:enum_label_unresolved->{fb}"
                        ));
                    }
                    return;
                }
            }
        }
        _ => return,
    };

    if edef.is_flags() {
        let mask = edef.valid_flag_mask();
        let masked = cur & mask;
        if masked != cur {
            write_int(value, masked);
            report.decisions.push(format!(
                "class_a:{rec_sig}.{path}:flags {cur:#x}->{masked:#x}"
            ));
        }
    } else if !edef.contains_value(cur) {
        // TODO(remap table): when a per-enum FO76->FO4 remap map is available,
        // look it up here before clamping. For now: clamp to the FO4 fallback.
        let fb = edef.fallback_value().unwrap_or(0);
        write_int(value, fb);
        report
            .decisions
            .push(format!("class_a:{rec_sig}.{path}:enum {cur}->{fb}"));
    }
}

/// Write an integer back, preserving the existing Uint/Int variant where known.
fn write_int(value: &mut FieldValue, v: i128) {
    *value = match value {
        FieldValue::Uint(_) => FieldValue::Uint(v as u64),
        _ => FieldValue::Int(v as i64),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, RecordFlags};
    use smallvec::SmallVec;

    fn efsh_record(dnam: Vec<u8>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("EFSH").unwrap(),
            form_key: FormKey::parse("863000@SeventySix.esm", interner).unwrap(),
            eid: None,
            flags: RecordFlags::empty(),
            fields: SmallVec::from_vec(vec![FieldEntry {
                sig: SubrecordSig::from_str("DNAM").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(dnam)),
            }]),
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn clears_flor_visible_when_distant_header_flag() {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let mut record = Record {
            sig: SigCode::from_str("FLOR").unwrap(),
            form_key: FormKey::parse("3916F2@SeventySix.esm", &interner).unwrap(),
            eid: None,
            flags: RecordFlags::from_bits_retain(0x0000_8000),
            fields: SmallVec::new(),
            warnings: SmallVec::new(),
        };

        let report = normalize_flags_and_enums(&mut record, &schema, &interner);

        assert_eq!(report.header_flag_bits_cleared, 0x0000_8000);
        assert_eq!(record.flags.bits(), 0);
    }

    #[test]
    fn qust_alias_fnam_uses_alias_mask_after_anam() {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let mut record = Record {
            sig: SigCode::from_str("QUST").unwrap(),
            form_key: FormKey::parse("05A243@SeventySix.esm", &interner).unwrap(),
            eid: None,
            flags: RecordFlags::empty(),
            fields: SmallVec::from_vec(vec![
                FieldEntry {
                    sig: SubrecordSig::from_str("QOBJ").unwrap(),
                    value: FieldValue::Uint(11),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("FNAM").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(3_u32.to_le_bytes().to_vec())),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("ANAM").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(1_u32.to_le_bytes().to_vec())),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("ALST").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(0_u32.to_le_bytes().to_vec())),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("FNAM").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(
                        0x8008_8000_u32.to_le_bytes().to_vec(),
                    )),
                },
            ]),
            warnings: SmallVec::new(),
        };

        normalize_flags_and_enums(&mut record, &schema, &interner);

        let fnam_values: Vec<u32> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "FNAM")
            .map(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => u32::from_le_bytes(bytes[..4].try_into().unwrap()),
                other => panic!("FNAM must stay Bytes, got {other:?}"),
            })
            .collect();
        assert_eq!(fnam_values, vec![3, 0x0008_8000]);
    }

    /// EFSH.DNAM is a `record_form_version` union. The carried bytes are the FO4
    /// NEW format (fv>=106, no leading byte): source_blend@0, blend_op@4,
    /// z_test@8 — all valid FO4 enum values. Class A must mask them against the
    /// fv131 layout (the version the output is written at) so the valid values
    /// survive. The pre-fix version-less layout picked the OLD-format variant
    /// (leading byte ⇒ fields at offsets 1/5/9) and zeroed blend_op@4 + z_test@8,
    /// the `<Unknown:0>` bug. Ground-truthed against output EFSH 863000.
    #[test]
    fn efsh_dnam_membrane_enums_survive_at_fv131_layout() {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");

        // Real FO76 EFSH 863000 DNAM head: 05 (SourceAlpha) 01 (Add) 07 (GTE),
        // then color-key bytes. 157 bytes total. The Ambient Sound formlink lives
        // at offset 108 (an `I`); seed a recognizable SNDR-ish formid there to
        // prove the version-less (old-format, leading-byte) layout — which shifted
        // an enum field onto byte 108 and zeroed the formid's low byte
        // (0x2491AB -> 0x249100, the "Ambient Sound wrong type" errors) — no
        // longer corrupts it under the fv131 layout.
        let mut dnam = vec![0u8; 157];
        dnam[0..4].copy_from_slice(&5u32.to_le_bytes()); // membrane source blend mode
        dnam[4..8].copy_from_slice(&1u32.to_le_bytes()); // blend operation = Add
        dnam[8..12].copy_from_slice(&7u32.to_le_bytes()); // z test = GreaterThanOrEqualTo
        dnam[108..112].copy_from_slice(&0x0024_91ABu32.to_le_bytes()); // Ambient Sound formlink

        let mut record = efsh_record(dnam, &interner);
        normalize_flags_and_enums(&mut record, &schema, &interner);

        let out = match &record.fields[0].value {
            FieldValue::Bytes(b) => b.clone(),
            other => panic!("DNAM must stay Bytes, got {other:?}"),
        };
        assert_eq!(
            u32::from_le_bytes([out[0], out[1], out[2], out[3]]),
            5,
            "source_blend_mode@0 preserved"
        );
        assert_eq!(
            u32::from_le_bytes([out[4], out[5], out[6], out[7]]),
            1,
            "blend_operation@4 must NOT be zeroed (was the <Unknown:0> bug)"
        );
        assert_eq!(
            u32::from_le_bytes([out[8], out[9], out[10], out[11]]),
            7,
            "z_test_function@8 must NOT be zeroed (was the <Unknown:0> bug)"
        );
        assert_eq!(
            u32::from_le_bytes([out[108], out[109], out[110], out[111]]),
            0x0024_91AB,
            "Ambient Sound formlink@108 must survive intact (was corrupted to \
             0x249100 by the old-format-layout enum write — the wrong-type errors)"
        );
    }

    /// An out-of-domain enum value still clamps — the fix only changed WHICH
    /// offsets are read, not the clamp policy. A blend_operation of 99 (invalid
    /// in FO4's 1..=5 set) at the correct fv131 offset must clamp to fallback.
    #[test]
    fn efsh_dnam_invalid_enum_still_clamps_at_correct_offset() {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let mut dnam = vec![0u8; 157];
        dnam[0..4].copy_from_slice(&5u32.to_le_bytes());
        dnam[4..8].copy_from_slice(&99u32.to_le_bytes()); // invalid blend op
        dnam[8..12].copy_from_slice(&7u32.to_le_bytes());

        let mut record = efsh_record(dnam, &interner);
        normalize_flags_and_enums(&mut record, &schema, &interner);

        let out = match &record.fields[0].value {
            FieldValue::Bytes(b) => b.clone(),
            other => panic!("DNAM must stay Bytes, got {other:?}"),
        };
        assert_ne!(
            u32::from_le_bytes([out[4], out[5], out[6], out[7]]),
            99,
            "invalid blend_operation@4 must be clamped"
        );
        assert_eq!(
            u32::from_le_bytes([out[8], out[9], out[10], out[11]]),
            7,
            "valid z_test_function@8 untouched"
        );
    }
}
