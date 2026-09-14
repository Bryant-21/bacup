use crate::ids::FormKey;
use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use smallvec::SmallVec;

use super::legacy_ammo::source_form_key_for_raw_form_id;

const LEGACY_IMPACT_MATERIAL_SLOTS: [&str; 12] = [
    "stone",
    "dirt",
    "grass",
    "glass",
    "metal",
    "wood",
    "organic",
    "cloth",
    "water",
    "hollow_metal",
    "organic_bug",
    "organic_glow",
];

const FO4_IMPACT_MATERIALS: [u32; 12] = [
    0x012F34, 0x012F38, 0x012F46, 0x012F39, 0x0223D1, 0x043DCC, 0x012F3D, 0x022957, 0x012F40,
    0x02295C, 0x10D5CC, 0x1C756F,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyImpactDatasetSupport {
    pub reason_codes: Vec<String>,
}

impl LegacyImpactDatasetSupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyImpactDatasetEmbeddedReference {
    pub form_key: FormKey,
    pub locator: String,
}

pub fn legacy_impact_dataset_embedded_references(
    record: &Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<Vec<LegacyImpactDatasetEmbeddedReference>, String> {
    if record.sig.as_str() != "IPDS" {
        return Ok(Vec::new());
    }
    let mut references = Vec::new();
    let mut data_index = 0usize;
    for field in &record.fields {
        if field.sig.as_str() != "DATA" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &field.value else {
            data_index += 1;
            continue;
        };
        let slot_count = match bytes.len() {
            36 => 9,
            40 => 10,
            48 => 12,
            length => {
                return Err(format!(
                    "legacy_impact_dataset_data_length_{length}_unverified"
                ));
            }
        };
        for (slot_index, slot_name) in LEGACY_IMPACT_MATERIAL_SLOTS
            .iter()
            .enumerate()
            .take(slot_count)
        {
            let offset = slot_index * 4;
            let raw = u32::from_le_bytes(
                bytes[offset..offset + 4]
                    .try_into()
                    .expect("validated four-byte slot"),
            );
            if raw & 0x00ff_ffff == 0 {
                continue;
            }
            let form_key = source_form_key_for_raw_form_id(
                raw,
                source_plugin_name,
                source_master_names,
                interner,
            )
            .ok_or_else(|| format!("legacy_impact_dataset_unresolved_formid_{raw:08x}"))?;
            references.push(LegacyImpactDatasetEmbeddedReference {
                form_key,
                locator: format!("DATA[{data_index}].{slot_name}@{offset}"),
            });
        }
        data_index += 1;
    }
    Ok(references)
}

pub(crate) fn classify_legacy_impact_dataset(
    record: &Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> LegacyImpactDatasetSupport {
    let reason_codes = impact_rows(record, source_plugin_name, source_master_names, interner)
        .map(|_| Vec::new())
        .unwrap_or_else(|reason| vec![reason]);
    LegacyImpactDatasetSupport { reason_codes }
}

pub(crate) fn lower_legacy_impact_dataset(
    record: &mut Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> bool {
    let Ok(rows) = impact_rows(record, source_plugin_name, source_master_names, interner) else {
        return false;
    };

    let mut fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "EDID")
        .cloned()
        .collect::<Vec<_>>();
    let target_plugin = interner.intern("Fallout4.esm");
    fields.extend(rows.into_iter().map(|row| FieldEntry {
        sig: SubrecordSig(*b"PNAM"),
        value: FieldValue::Struct(vec![
            (
                interner.intern("material"),
                FieldValue::FormKey(FormKey {
                    local: FO4_IMPACT_MATERIALS[row.slot_index],
                    plugin: target_plugin,
                }),
            ),
            (interner.intern("impact"), FieldValue::FormKey(row.impact)),
        ]),
    }));
    record.fields = SmallVec::from_vec(fields);
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ImpactRow {
    slot_index: usize,
    impact: FormKey,
}

fn impact_rows(
    record: &Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<Vec<ImpactRow>, String> {
    if record.sig.as_str() != "IPDS" {
        return Err("legacy_impact_dataset_wrong_signature".to_string());
    }
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => {}
            "DATA" => {}
            "EDID" => {
                return Err("legacy_impact_dataset_editor_id_shape_unverified".to_string());
            }
            _ => return Err("legacy_impact_dataset_subrecord_unverified".to_string()),
        }
    }
    let mut data_fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "DATA");
    let Some(data) = data_fields.next() else {
        return Err("legacy_impact_dataset_data_multiplicity_unverified".to_string());
    };
    if data_fields.next().is_some() {
        return Err("legacy_impact_dataset_data_multiplicity_unverified".to_string());
    }

    match &data.value {
        FieldValue::Bytes(bytes) => {
            let slot_count = match bytes.len() {
                36 => 9,
                40 => 10,
                48 => 12,
                length => {
                    return Err(format!(
                        "legacy_impact_dataset_data_length_{length}_unverified"
                    ));
                }
            };
            let mut rows = Vec::new();
            for slot_index in 0..slot_count {
                let offset = slot_index * 4;
                let raw = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                if raw & 0x00ff_ffff == 0 {
                    continue;
                }
                let impact = source_form_key_for_raw_form_id(
                    raw,
                    source_plugin_name,
                    source_master_names,
                    interner,
                )
                .ok_or_else(|| format!("legacy_impact_dataset_unresolved_formid_{raw:08x}"))?;
                rows.push(ImpactRow { slot_index, impact });
            }
            Ok(rows)
        }
        FieldValue::Struct(fields) => {
            let mut rows = Vec::new();
            for (slot_index, slot_name) in LEGACY_IMPACT_MATERIAL_SLOTS.iter().enumerate() {
                let value = fields
                    .iter()
                    .find(|(name, _)| interner.resolve(*name) == Some(*slot_name))
                    .map(|(_, value)| value)
                    .ok_or_else(|| "legacy_impact_dataset_data_shape_unverified".to_string())?;
                match value {
                    FieldValue::FormKey(impact) => rows.push(ImpactRow {
                        slot_index,
                        impact: *impact,
                    }),
                    FieldValue::Uint(0) | FieldValue::Int(0) | FieldValue::None => {}
                    _ => {
                        return Err("legacy_impact_dataset_data_shape_unverified".to_string());
                    }
                }
            }
            Ok(rows)
        }
        _ => Err("legacy_impact_dataset_data_shape_unverified".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::schema::AuthoringSchema;

    use super::*;

    #[test]
    fn resolves_legacy_material_slots_with_exact_offsets() {
        let interner = StringInterner::new();
        let mut data = vec![0_u8; 48];
        data[0..4].copy_from_slice(&0x0100_0100_u32.to_le_bytes());
        data[40..44].copy_from_slice(&0x0000_0200_u32.to_le_bytes());
        data[44..48].copy_from_slice(&0x0100_0300_u32.to_le_bytes());
        let mut record = Record::new(
            SigCode::from_str("IPDS").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("Owner.esp"),
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(data.into()),
        });

        let references = legacy_impact_dataset_embedded_references(
            &record,
            "Owner.esp",
            &["FalloutNV.esm".to_string()],
            &interner,
        )
        .unwrap();
        assert_eq!(references.len(), 3);
        assert_eq!(references[0].locator, "DATA[0].stone@0");
        assert_eq!(references[1].locator, "DATA[0].organic_bug@40");
        assert_eq!(references[2].locator, "DATA[0].organic_glow@44");
        assert_eq!(
            references
                .iter()
                .map(|reference| reference.form_key.local)
                .collect::<Vec<_>>(),
            [0x100, 0x200, 0x300]
        );
        assert_eq!(
            interner.resolve(references[0].form_key.plugin),
            Some("Owner.esp")
        );
        assert_eq!(
            interner.resolve(references[1].form_key.plugin),
            Some("FalloutNV.esm")
        );
    }

    #[test]
    fn accepts_nine_and_ten_slot_legacy_shapes_and_rejects_other_lengths() {
        let interner = StringInterner::new();
        let make = |length| {
            let mut record = Record::new(
                SigCode::from_str("IPDS").unwrap(),
                FormKey {
                    local: 0x800,
                    plugin: interner.intern("FalloutNV.esm"),
                },
            );
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(vec![0_u8; length].into()),
            });
            record
        };
        assert!(
            legacy_impact_dataset_embedded_references(&make(36), "FalloutNV.esm", &[], &interner,)
                .unwrap()
                .is_empty()
        );
        assert!(
            legacy_impact_dataset_embedded_references(&make(40), "FalloutNV.esm", &[], &interner,)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            legacy_impact_dataset_embedded_references(&make(44), "FalloutNV.esm", &[], &interner,)
                .unwrap_err(),
            "legacy_impact_dataset_data_length_44_unverified"
        );
    }

    #[test]
    fn lowers_slots_to_exact_fo4_material_impact_pairs() {
        let interner = StringInterner::new();
        let mut data = vec![0_u8; 48];
        data[0..4].copy_from_slice(&0x0000_0100_u32.to_le_bytes());
        data[40..44].copy_from_slice(&0x0000_0200_u32.to_le_bytes());
        data[44..48].copy_from_slice(&0x0000_0300_u32.to_le_bytes());
        let mut record = Record::new(
            SigCode::from_str("IPDS").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(data.into()),
        });

        assert!(
            classify_legacy_impact_dataset(&record, "FalloutNV.esm", &[], &interner).is_ready()
        );
        assert!(lower_legacy_impact_dataset(
            &mut record,
            "FalloutNV.esm",
            &[],
            &interner,
        ));
        assert_eq!(record.fields.len(), 3);

        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let definition = schema.record_def("IPDS");
        let packed = record
            .fields
            .iter()
            .map(|field| {
                crate::target_write::encode_field_pub(field, definition, &interner).unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            packed,
            [
                [0x34, 0x2F, 0x01, 0, 0x00, 0x01, 0, 0].to_vec(),
                [0xCC, 0xD5, 0x10, 0, 0x00, 0x02, 0, 0].to_vec(),
                [0x6F, 0x75, 0x1C, 0, 0x00, 0x03, 0, 0].to_vec(),
            ]
        );
    }

    #[test]
    fn lowers_nine_slot_blood_impact_without_phantom_materials() {
        let interner = StringInterner::new();
        let mut data = vec![0_u8; 36];
        data[24..28].copy_from_slice(&0x0002_ED48_u32.to_le_bytes());
        let mut record = Record::new(
            SigCode::from_str("IPDS").unwrap(),
            FormKey {
                local: 0x02ED47,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(data.into()),
        });

        let references =
            legacy_impact_dataset_embedded_references(&record, "FalloutNV.esm", &[], &interner)
                .unwrap();
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].locator, "DATA[0].organic@24");
        assert!(lower_legacy_impact_dataset(
            &mut record,
            "FalloutNV.esm",
            &[],
            &interner,
        ));

        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let definition = schema.record_def("IPDS");
        let packed = record
            .fields
            .iter()
            .map(|field| {
                crate::target_write::encode_field_pub(field, definition, &interner).unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            packed,
            [[0x3D, 0x2F, 0x01, 0, 0x48, 0xED, 0x02, 0].to_vec()]
        );
    }
}
