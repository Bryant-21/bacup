use std::collections::BTreeSet;

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

use super::legacy_ammo::source_form_key_for_raw_form_id;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyLeveledItemSupport {
    pub reason_codes: Vec<String>,
}

impl LegacyLeveledItemSupport {
    pub fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyLeveledItemEmbeddedReference {
    pub form_key: FormKey,
    pub locator: String,
}

pub fn classify_legacy_leveled_item(record: &Record) -> LegacyLeveledItemSupport {
    let mut reasons = BTreeSet::new();
    if record.sig.as_str() != "LVLI" {
        reasons.insert("legacy_leveled_item_wrong_signature".to_string());
    }
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => {}
            "OBND" if matches!(&field.value, FieldValue::Bytes(bytes) if bytes.len() == 12) => {}
            "OBND" if matches!(field.value, FieldValue::Struct(_)) => {}
            "LVLD" | "LVLF" if matches!(field.value, FieldValue::Uint(_) | FieldValue::Int(_)) => {}
            "LVLG" if matches!(field.value, FieldValue::FormKey(_)) => {}
            "LVLO" => match lvlo_support(&field.value) {
                LegacyLvloSupport::Ready => {}
                LegacyLvloSupport::InvalidShape => {
                    reasons.insert("legacy_leveled_item_lvlo_shape_unverified".to_string());
                }
            },
            "COED" if legacy_null_owner_coed_is_target_valid(&field.value) => {}
            "COED" => {
                reasons
                    .insert("legacy_leveled_item_coed_union_requires_owner_signature".to_string());
            }
            "EDID" | "OBND" | "LVLD" | "LVLF" | "LVLG" => {
                reasons.insert(format!(
                    "legacy_leveled_item_{}_shape_unverified",
                    field.sig.as_str().to_ascii_lowercase()
                ));
            }
            _ => {
                reasons.insert("legacy_leveled_item_subrecord_unverified".to_string());
            }
        }
    }
    LegacyLeveledItemSupport {
        reason_codes: reasons.into_iter().collect(),
    }
}

pub fn decode_legacy_leveled_item_entries(
    record: &mut Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<(), String> {
    if record.sig.as_str() != "LVLI" {
        return Ok(());
    }
    for field in &mut record.fields {
        if field.sig.as_str() != "LVLO" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &field.value else {
            continue;
        };
        let row =
            parse_lvlo(bytes).ok_or_else(|| "legacy_leveled_item_invalid_lvlo".to_string())?;
        let item = source_form_key_for_raw_form_id(
            row.item,
            source_plugin_name,
            source_master_names,
            interner,
        )
        .ok_or_else(|| {
            format!(
                "legacy_leveled_item_unresolved_entry_formid_{:08x}",
                row.item
            )
        })?;
        // FO3/FNV define both two-byte gaps as unused; FO4 assigns byte 10 to Chance None.
        field.value = FieldValue::Struct(vec![
            (
                interner.intern("level"),
                bytes_value(&row.level.to_le_bytes()),
            ),
            (interner.intern("unknown_u8_1"), bytes_value(&[0])),
            (interner.intern("unknown_u8_2"), bytes_value(&[0])),
            (interner.intern("item"), FieldValue::FormKey(item)),
            (
                interner.intern("count"),
                bytes_value(&row.count.to_le_bytes()),
            ),
            (interner.intern("chance_none"), bytes_value(&[0])),
            (interner.intern("unknown_u8_6"), bytes_value(&[0])),
        ]);
    }
    Ok(())
}

fn bytes_value(bytes: &[u8]) -> FieldValue {
    FieldValue::Bytes(bytes.into())
}

pub fn legacy_leveled_item_embedded_references(
    record: &Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<Vec<LegacyLeveledItemEmbeddedReference>, String> {
    if record.sig.as_str() != "LVLI" {
        return Ok(Vec::new());
    }
    let mut references = Vec::new();
    let mut index = 0usize;
    for field in &record.fields {
        if field.sig.as_str() != "LVLO" {
            continue;
        }
        if let FieldValue::Bytes(bytes) = &field.value {
            let row =
                parse_lvlo(bytes).ok_or_else(|| "legacy_leveled_item_invalid_lvlo".to_string())?;
            let form_key = source_form_key_for_raw_form_id(
                row.item,
                source_plugin_name,
                source_master_names,
                interner,
            )
            .ok_or_else(|| {
                format!(
                    "legacy_leveled_item_unresolved_entry_formid_{:08x}",
                    row.item
                )
            })?;
            references.push(LegacyLeveledItemEmbeddedReference {
                form_key,
                locator: format!("LVLO[{index}].item@4"),
            });
        }
        index += 1;
    }
    Ok(references)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LegacyLvloSupport {
    Ready,
    InvalidShape,
}

fn lvlo_support(value: &FieldValue) -> LegacyLvloSupport {
    match value {
        FieldValue::Bytes(bytes) => match parse_lvlo(bytes) {
            Some(_) => LegacyLvloSupport::Ready,
            None => LegacyLvloSupport::InvalidShape,
        },
        FieldValue::Struct(fields)
            if fields
                .iter()
                .any(|(_, value)| matches!(value, FieldValue::FormKey(_))) =>
        {
            LegacyLvloSupport::Ready
        }
        _ => LegacyLvloSupport::InvalidShape,
    }
}

fn legacy_null_owner_coed_is_target_valid(value: &FieldValue) -> bool {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 12 => {
            bytes[..8] == [0; 8]
                && f32::from_le_bytes(bytes[8..12].try_into().expect("COED size checked"))
                    .is_finite()
        }
        _ => false,
    }
}

#[derive(Clone, Copy)]
struct LegacyLvliRow {
    level: u16,
    item: u32,
    count: u16,
}

fn parse_lvlo(bytes: &[u8]) -> Option<LegacyLvliRow> {
    if bytes.len() != 12 {
        return None;
    }
    let item = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    if item & 0x00ff_ffff == 0 {
        return None;
    }
    Some(LegacyLvliRow {
        level: u16::from_le_bytes(bytes[0..2].try_into().ok()?),
        item,
        count: u16::from_le_bytes(bytes[8..10].try_into().ok()?),
    })
}

#[cfg(test)]
mod tests {
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::schema::AuthoringSchema;

    use super::*;

    #[test]
    fn exact_legacy_entry_decodes_to_mapper_visible_fo4_struct() {
        let interner = StringInterner::new();
        let mut row = Vec::new();
        row.extend_from_slice(&7u16.to_le_bytes());
        row.extend_from_slice(&[1, 2]);
        row.extend_from_slice(&0x0100_1234u32.to_le_bytes());
        row.extend_from_slice(&3u16.to_le_bytes());
        row.extend_from_slice(&[0, 5]);
        let mut record = Record::new(
            SigCode::from_str("LVLI").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("Owner.esp"),
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"LVLO"),
            value: FieldValue::Bytes(row.into()),
        });

        decode_legacy_leveled_item_entries(
            &mut record,
            "Owner.esp",
            &["FalloutNV.esm".to_string()],
            &interner,
        )
        .unwrap();

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("LVLO was not decoded");
        };
        assert!(fields.iter().any(|(name, value)| {
            interner.resolve(*name) == Some("item")
                && matches!(value, FieldValue::FormKey(key) if key.local == 0x1234 && interner.resolve(key.plugin) == Some("Owner.esp"))
        }));
        assert_eq!(
            legacy_leveled_item_embedded_references(
                &Record {
                    fields: smallvec::smallvec![FieldEntry {
                        sig: SubrecordSig(*b"LVLO"),
                        value: FieldValue::Bytes(
                            [7, 0, 1, 2, 0x34, 0x12, 0x00, 0x01, 3, 0, 4, 5,]
                                .as_slice()
                                .into(),
                        ),
                    }],
                    ..record.clone()
                },
                "Owner.esp",
                &["FalloutNV.esm".to_string()],
                &interner,
            )
            .unwrap()[0]
                .locator,
            "LVLO[0].item@4"
        );

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let encoded = crate::target_write::encode_field_pub(
            &record.fields[0],
            schema.record_def("LVLI"),
            &interner,
        )
        .expect("decoded LVLO encodes against the FO4 schema");
        assert_eq!(encoded, [7, 0, 0, 0, 0x34, 0x12, 0, 0, 3, 0, 0, 0]);
    }

    #[test]
    fn nonnull_coed_union_remains_typed_blocked() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("LVLI").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"LVLO"),
                value: FieldValue::Bytes([1, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0].as_slice().into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"COED"),
                value: FieldValue::Bytes(
                    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x80, 0x3f].as_slice().into(),
                ),
            },
        ]);
        assert_eq!(
            classify_legacy_leveled_item(&record).reason_codes,
            ["legacy_leveled_item_coed_union_requires_owner_signature"]
        );
    }

    #[test]
    fn null_owner_coed_and_empty_lists_are_target_valid() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("LVLI").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"LVLG"),
            value: FieldValue::FormKey(FormKey {
                local: 0x123,
                plugin: interner.intern("FalloutNV.esm"),
            }),
        });
        assert!(classify_legacy_leveled_item(&record).is_ready());

        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"LVLO"),
                value: FieldValue::Bytes([1, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0].as_slice().into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"COED"),
                value: FieldValue::Bytes(
                    [0, 0, 0, 0, 0, 0, 0, 0, 0xcd, 0xcc, 0x4c, 0x3f]
                        .as_slice()
                        .into(),
                ),
            },
        ]);
        assert!(classify_legacy_leveled_item(&record).is_ready());
        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let encoded = crate::target_write::encode_field_pub(
            &record.fields[2],
            schema.record_def("LVLI"),
            &interner,
        )
        .expect("legacy null-owner COED encodes against the identical FO4 layout");
        assert_eq!(encoded, [0, 0, 0, 0, 0, 0, 0, 0, 0xcd, 0xcc, 0x4c, 0x3f]);
    }

    #[test]
    fn legacy_unused_entry_bytes_are_zeroed_instead_of_becoming_fo4_chance() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("LVLI").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"LVLO"),
            value: FieldValue::Bytes([1, 0, 0, 0, 2, 0, 0, 0, 1, 0, 25, 0].as_slice().into()),
        });
        assert!(classify_legacy_leveled_item(&record).is_ready());
        decode_legacy_leveled_item_entries(&mut record, "FalloutNV.esm", &[], &interner).unwrap();
        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let encoded = crate::target_write::encode_field_pub(
            &record.fields[0],
            schema.record_def("LVLI"),
            &interner,
        )
        .expect("decoded LVLO encodes against the FO4 schema");
        assert_eq!(encoded, [1, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0]);
    }
}
