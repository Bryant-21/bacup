use std::collections::BTreeSet;

use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};

const LEGACY_CSTD_LEN: usize = 92;
const LEGACY_CSAD_LEN: usize = 84;
const LEGACY_CSSD_LEN: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyCombatStyleSupport {
    pub reason_codes: Vec<String>,
}

impl LegacyCombatStyleSupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

pub(crate) fn legacy_combat_style_fallback_warnings(record: &Record) -> Vec<String> {
    if !classify_legacy_combat_style(record).is_ready() {
        return Vec::new();
    }
    vec![
        "legacy_combat_style_source_timing_and_skill_scalars_omitted".to_string(),
        "legacy_combat_style_uses_standard_fo4_creature_defaults".to_string(),
    ]
}

pub(crate) fn classify_legacy_combat_style(record: &Record) -> LegacyCombatStyleSupport {
    let mut reasons = BTreeSet::new();
    if record.sig.as_str() != "CSTY" {
        reasons.insert("legacy_combat_style_wrong_signature".to_string());
    }

    let mut edid_count = 0usize;
    let mut cstd_count = 0usize;
    let mut csad_count = 0usize;
    let mut cssd_count = 0usize;
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => edid_count += 1,
            "CSTD" => {
                cstd_count += 1;
                match &field.value {
                    FieldValue::Bytes(bytes) if bytes.len() == LEGACY_CSTD_LEN => {}
                    FieldValue::Struct(_) => {}
                    FieldValue::Bytes(bytes) => {
                        reasons.insert(format!(
                            "legacy_combat_style_cstd_length_{}_unverified",
                            bytes.len()
                        ));
                    }
                    _ => {
                        reasons.insert("legacy_combat_style_cstd_shape_unverified".to_string());
                    }
                }
            }
            "CSAD" => {
                csad_count += 1;
                match &field.value {
                    FieldValue::Bytes(bytes) if bytes.len() == LEGACY_CSAD_LEN => {}
                    FieldValue::Struct(_) => {}
                    FieldValue::Bytes(bytes) => {
                        reasons.insert(format!(
                            "legacy_combat_style_csad_length_{}_unverified",
                            bytes.len()
                        ));
                    }
                    _ => {
                        reasons.insert("legacy_combat_style_csad_shape_unverified".to_string());
                    }
                }
            }
            "CSSD" => {
                cssd_count += 1;
                match &field.value {
                    FieldValue::Bytes(bytes) if bytes.len() == LEGACY_CSSD_LEN => {}
                    FieldValue::Struct(_) => {}
                    FieldValue::Bytes(bytes) => {
                        reasons.insert(format!(
                            "legacy_combat_style_cssd_length_{}_unverified",
                            bytes.len()
                        ));
                    }
                    _ => {
                        reasons.insert("legacy_combat_style_cssd_shape_unverified".to_string());
                    }
                }
            }
            "EDID" => {
                reasons.insert("legacy_combat_style_editor_id_shape_unverified".to_string());
            }
            _ => {
                reasons.insert("legacy_combat_style_subrecord_unverified".to_string());
            }
        }
    }
    if edid_count != 1 || cstd_count != 1 || csad_count != 1 || cssd_count != 1 {
        reasons.insert("legacy_combat_style_subrecord_multiplicity_unverified".to_string());
    }

    LegacyCombatStyleSupport {
        reason_codes: reasons.into_iter().collect(),
    }
}

pub(crate) fn lower_legacy_combat_style(record: &mut Record) -> bool {
    if !classify_legacy_combat_style(record).is_ready() {
        return false;
    }

    let editor_id = record.eid.or_else(|| {
        record.fields.iter().find_map(|field| match &field.value {
            FieldValue::String(value) if field.sig.as_str() == "EDID" => Some(*value),
            _ => None,
        })
    });
    let mut output = Record::new(record.sig, record.form_key);
    output.eid = editor_id;
    output.flags = record.flags;
    output.warnings = record.warnings.clone();
    if let Some(editor_id) = editor_id {
        output
            .fields
            .push(field(*b"EDID", FieldValue::String(editor_id)));
    }
    output.fields.extend([
        floats(
            *b"CSGD",
            &[0.5, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.8, 0.2, 0.2],
        ),
        floats(
            *b"CSME",
            &[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.1, 1.0, 1.0],
        ),
        floats(*b"CSRA", &[0.75]),
        close_range_defaults(),
        floats(*b"CSLR", &[0.5, 0.5, 0.5, 0.0, 0.0]),
        floats(*b"CSCV", &[0.5]),
        floats(*b"CSFL", &[0.5, 1.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.75]),
        field(*b"DATA", FieldValue::Uint(0)),
    ]);
    *record = output;
    true
}

fn floats(signature: [u8; 4], values: &[f32]) -> FieldEntry {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    field(signature, FieldValue::Bytes(bytes.into()))
}

fn close_range_defaults() -> FieldEntry {
    let mut bytes = Vec::with_capacity(44);
    for value in [0.2_f32, 0.2, 0.2, 0.2, 0.0, 0.0, 0.0, 0.0, 0.0] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0.5_f32.to_le_bytes());
    field(*b"CSCR", FieldValue::Bytes(bytes.into()))
}

fn field(signature: [u8; 4], value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(signature),
        value,
    }
}

#[cfg(test)]
mod tests {
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::sym::StringInterner;

    use super::*;

    fn official_shape(interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("CSTY").unwrap(),
            FormKey {
                local: 0x3d,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(interner.intern("DefaultCombatstyle")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"CSTD"),
                value: FieldValue::Bytes(vec![0_u8; LEGACY_CSTD_LEN].into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"CSAD"),
                value: FieldValue::Bytes(vec![0_u8; LEGACY_CSAD_LEN].into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"CSSD"),
                value: FieldValue::Bytes(vec![0_u8; LEGACY_CSSD_LEN].into()),
            },
        ]);
        record
    }

    #[test]
    fn exact_legacy_payloads_admit_a_standard_fo4_fallback() {
        let interner = StringInterner::new();
        let support = classify_legacy_combat_style(&official_shape(&interner));
        assert!(support.is_ready());
        assert_eq!(
            legacy_combat_style_fallback_warnings(&official_shape(&interner)),
            vec![
                "legacy_combat_style_source_timing_and_skill_scalars_omitted".to_string(),
                "legacy_combat_style_uses_standard_fo4_creature_defaults".to_string(),
            ]
        );
    }

    #[test]
    fn lowering_emits_only_target_csty_fields_with_target_widths() {
        let interner = StringInterner::new();
        let mut record = official_shape(&interner);
        assert!(lower_legacy_combat_style(&mut record));
        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            [
                "EDID", "CSGD", "CSME", "CSRA", "CSCR", "CSLR", "CSCV", "CSFL", "DATA"
            ]
        );

        let schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        let definition = schema.record_def("CSTY");
        for field in &record.fields {
            crate::target_write::encode_field_pub(field, definition, &interner)
                .unwrap_or_else(|error| panic!("{}: {error}", field.sig.as_str()));
        }
    }

    #[test]
    fn malformed_payload_is_not_hidden_by_the_semantic_blockers() {
        let interner = StringInterner::new();
        let mut record = official_shape(&interner);
        let cstd = record
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "CSTD")
            .unwrap();
        cstd.value = FieldValue::Bytes(vec![0_u8; 91].into());
        let support = classify_legacy_combat_style(&record);
        assert!(
            support
                .reason_codes
                .contains(&"legacy_combat_style_cstd_length_91_unverified".to_string())
        );
    }
}
