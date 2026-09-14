use std::collections::BTreeSet;

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

use super::legacy_ammo::source_form_key_for_raw_form_id;

const TARGET_SPECIAL_COMBAT: u32 = 0x0002;
const TARGET_TRACK_CRIME: u32 = 0x0040;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyFactionSupport {
    pub reason_codes: Vec<String>,
}

impl LegacyFactionSupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

pub(crate) fn legacy_faction_fallback_warnings(record: &Record) -> Vec<String> {
    if !classify_legacy_faction(record).is_ready() {
        return Vec::new();
    }
    let mut warnings = BTreeSet::new();
    for field in &record.fields {
        match field.sig.as_str() {
            "DATA" => {
                if let FieldValue::Bytes(bytes) = &field.value {
                    warnings.extend(classify_data(bytes).1);
                }
            }
            "WMI1" => {
                warnings.insert("legacy_faction_reputation_omitted".to_string());
            }
            "CNAM" => {
                warnings.insert("legacy_faction_crime_multiplier_omitted".to_string());
            }
            _ => {}
        }
    }
    warnings.into_iter().collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyFactionEmbeddedReference {
    pub form_key: FormKey,
    pub locator: String,
}

pub(crate) fn classify_legacy_faction(record: &Record) -> LegacyFactionSupport {
    let mut reasons = BTreeSet::new();
    if record.sig.as_str() != "FACT" {
        reasons.insert("legacy_faction_wrong_signature".to_string());
    }

    let mut editor_ids = 0usize;
    let mut data_rows = 0usize;
    let mut rank_rows = 0usize;
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => editor_ids += 1,
            "FULL" if matches!(field.value, FieldValue::String(_)) => {}
            "XNAM" if matches!(&field.value, FieldValue::Bytes(bytes) if valid_xnam(bytes)) => {}
            "DATA" => {
                data_rows += 1;
                match &field.value {
                    FieldValue::Bytes(bytes) if matches!(bytes.len(), 1 | 4) => {}
                    _ => {
                        reasons.insert("legacy_faction_data_shape_unverified".to_string());
                    }
                }
            }
            "RNAM" => {
                rank_rows += 1;
                if !matches!(field.value, FieldValue::Int(value) if value >= 0) {
                    reasons.insert("legacy_faction_rank_shape_unverified".to_string());
                }
            }
            "MNAM" | "FNAM" if matches!(field.value, FieldValue::String(_)) => {}
            "WMI1" if matches!(field.value, FieldValue::FormKey(_)) => {}
            "CNAM" if matches!(field.value, FieldValue::Float(_)) => {}
            _ => {
                reasons.insert("legacy_faction_subrecord_shape_unverified".to_string());
            }
        }
    }
    if editor_ids != 1 || data_rows != 1 || rank_rows > 1 {
        reasons.insert("legacy_faction_field_multiplicity_unverified".to_string());
    }

    LegacyFactionSupport {
        reason_codes: reasons.into_iter().collect(),
    }
}

pub(crate) fn legacy_faction_embedded_references(
    record: &Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<Vec<LegacyFactionEmbeddedReference>, String> {
    if record.sig.as_str() != "FACT" {
        return Ok(Vec::new());
    }
    let mut references = Vec::new();
    let mut index = 0usize;
    for field in &record.fields {
        if field.sig.as_str() != "XNAM" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &field.value else {
            index += 1;
            continue;
        };
        if !valid_xnam(bytes) {
            return Err("legacy_faction_xnam_shape_unverified".to_string());
        }
        let raw = read_u32(bytes, 0);
        let form_key =
            source_form_key_for_raw_form_id(raw, source_plugin_name, source_master_names, interner)
                .ok_or_else(|| format!("legacy_faction_unresolved_xnam_formid_{raw:08x}"))?;
        references.push(LegacyFactionEmbeddedReference {
            form_key,
            locator: format!("XNAM[{index}].faction@0"),
        });
        index += 1;
    }
    Ok(references)
}

pub(crate) fn lower_legacy_faction(
    record: &mut Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<bool, String> {
    if !classify_legacy_faction(record).is_ready() {
        return Ok(false);
    }
    for field in &mut record.fields {
        match field.sig.as_str() {
            "DATA" => {
                let FieldValue::Bytes(bytes) = &field.value else {
                    continue;
                };
                let (target_flags, _) = classify_data(bytes);
                field.value = FieldValue::Uint(target_flags.into());
            }
            "RNAM" => {
                let FieldValue::Int(rank) = field.value else {
                    continue;
                };
                field.value = FieldValue::Uint(rank as u64);
            }
            "XNAM" => {
                let FieldValue::Bytes(bytes) = &field.value else {
                    continue;
                };
                let raw = read_u32(bytes, 0);
                let faction = source_form_key_for_raw_form_id(
                    raw,
                    source_plugin_name,
                    source_master_names,
                    interner,
                )
                .ok_or_else(|| format!("legacy_faction_unresolved_xnam_formid_{raw:08x}"))?;
                field.value = FieldValue::Struct(vec![
                    (interner.intern("faction"), FieldValue::FormKey(faction)),
                    (
                        interner.intern("modifier"),
                        FieldValue::Bytes(bytes[4..8].into()),
                    ),
                    (
                        interner.intern("group_combat_reaction"),
                        FieldValue::Bytes(bytes[8..12].into()),
                    ),
                ]);
            }
            _ => {}
        }
    }
    record
        .fields
        .retain(|field| !matches!(field.sig.as_str(), "WMI1" | "CNAM"));
    Ok(true)
}

fn classify_data(bytes: &[u8]) -> (u32, BTreeSet<String>) {
    let mut reasons = BTreeSet::new();
    if !matches!(bytes.len(), 1 | 4) {
        reasons.insert("legacy_faction_data_shape_unverified".to_string());
        return (0, reasons);
    }
    let flags_1 = bytes[0];
    let flags_2 = bytes.get(1).copied().unwrap_or(0);
    if flags_1 & 0x01 != 0 {
        reasons.insert("legacy_faction_hidden_from_pc_has_no_exact_fo4_semantic".to_string());
    }
    if flags_1 & 0x02 != 0 {
        reasons.insert("legacy_faction_evil_flag_has_no_fo4_semantic".to_string());
    }
    if flags_1 & !0x07 != 0 {
        reasons.insert("legacy_faction_flags_1_unverified".to_string());
    }
    if flags_2 & 0x02 != 0 {
        reasons.insert("legacy_faction_allow_sell_has_no_exact_fo4_semantic".to_string());
    }
    if flags_2 & !0x03 != 0 {
        reasons.insert("legacy_faction_flags_2_unverified".to_string());
    }
    if bytes.get(2).copied().unwrap_or(0) != 0 || bytes.get(3).copied().unwrap_or(0) != 0 {
        reasons.insert("legacy_faction_unknown_flag_bytes_nonzero".to_string());
    }
    let target_flags = if flags_1 & 0x04 != 0 {
        TARGET_SPECIAL_COMBAT
    } else {
        0
    } | if flags_2 & 0x01 != 0 {
        TARGET_TRACK_CRIME
    } else {
        0
    };
    (target_flags, reasons)
}

fn valid_xnam(bytes: &[u8]) -> bool {
    bytes.len() == 12 && read_u32(bytes, 0) & 0x00ff_ffff != 0 && read_u32(bytes, 8) <= 3
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated FACT row"),
    )
}

#[cfg(test)]
mod tests {
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    use super::*;

    fn fixture(interner: &StringInterner, flags: [u8; 4]) -> Record {
        let mut record = Record::new(
            SigCode::from_str("FACT").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("Owner.esp"),
            },
        );
        let mut relation = Vec::new();
        relation.extend_from_slice(&0x0100_1234u32.to_le_bytes());
        relation.extend_from_slice(&(-10_i32).to_le_bytes());
        relation.extend_from_slice(&2_u32.to_le_bytes());
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(interner.intern("CreatureFaction")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"XNAM"),
                value: FieldValue::Bytes(relation.into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(flags.to_vec().into()),
            },
        ]);
        record
    }

    #[test]
    fn exact_flags_and_reaction_reference_lower_without_semantic_drift() {
        let interner = StringInterner::new();
        let mut record = fixture(&interner, [0x04, 0x01, 0, 0]);
        assert!(classify_legacy_faction(&record).is_ready());
        let refs = legacy_faction_embedded_references(
            &record,
            "Owner.esp",
            &["Master.esm".to_string()],
            &interner,
        )
        .unwrap();
        assert_eq!(refs[0].locator, "XNAM[0].faction@0");
        assert_eq!(refs[0].form_key.local, 0x1234);
        assert_eq!(interner.resolve(refs[0].form_key.plugin), Some("Owner.esp"));

        assert!(
            lower_legacy_faction(
                &mut record,
                "Owner.esp",
                &["Master.esm".to_string()],
                &interner,
            )
            .unwrap()
        );
        assert!(matches!(
            record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "DATA")
                .unwrap()
                .value,
            FieldValue::Uint(value)
                if value == u64::from(TARGET_SPECIAL_COMBAT | TARGET_TRACK_CRIME)
        ));
        assert!(matches!(
            record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "XNAM")
                .unwrap()
                .value,
            FieldValue::Struct(_)
        ));
    }

    #[test]
    fn source_only_flags_and_fields_are_omitted_with_warnings() {
        let interner = StringInterner::new();
        let mut record = fixture(&interner, [0x03, 0x02, 0, 0]);
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"CNAM"),
                value: FieldValue::Float(1.0),
            },
            FieldEntry {
                sig: SubrecordSig(*b"WMI1"),
                value: FieldValue::FormKey(FormKey {
                    local: 0x900,
                    plugin: interner.intern("Owner.esp"),
                }),
            },
        ]);
        assert!(classify_legacy_faction(&record).is_ready());
        assert_eq!(
            legacy_faction_fallback_warnings(&record),
            [
                "legacy_faction_allow_sell_has_no_exact_fo4_semantic".to_string(),
                "legacy_faction_crime_multiplier_omitted".to_string(),
                "legacy_faction_evil_flag_has_no_fo4_semantic".to_string(),
                "legacy_faction_hidden_from_pc_has_no_exact_fo4_semantic".to_string(),
                "legacy_faction_reputation_omitted".to_string(),
            ]
        );
        lower_legacy_faction(
            &mut record,
            "Owner.esp",
            &["Master.esm".to_string()],
            &interner,
        )
        .unwrap();
        assert!(
            record
                .fields
                .iter()
                .all(|field| !matches!(field.sig.as_str(), "CNAM" | "WMI1"))
        );
    }
}
