use std::collections::BTreeSet;

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

use super::legacy_ammo::source_form_key_for_raw_form_id;

const LEGACY_EXPLOSION_DATA_LEN: usize = 52;
const FO4_EXPLOSION_DATA_LEN: usize = 84;
const LEGACY_REFERENCE_SLOTS: [(&str, usize); 4] = [
    ("light", 12),
    ("sound_1", 16),
    ("impact_dataset", 28),
    ("sound_2", 32),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyExplosionSupport {
    pub reason_codes: Vec<String>,
}

impl LegacyExplosionSupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

pub(crate) fn legacy_explosion_fallback_warnings(record: &Record) -> Vec<String> {
    if !classify_legacy_explosion(record).is_ready() {
        return Vec::new();
    }
    let mut warnings = BTreeSet::new();
    for field in &record.fields {
        match field.sig.as_str() {
            "MNAM" => {
                warnings.insert("legacy_explosion_imagespace_modifier_omitted".to_string());
            }
            "DATA" if matches!(&field.value, FieldValue::Bytes(bytes) if bytes.len() == LEGACY_EXPLOSION_DATA_LEN && bytes[36..48].iter().any(|byte| *byte != 0)) =>
            {
                warnings.insert("legacy_explosion_radiation_scalars_omitted".to_string());
            }
            _ => {}
        }
    }
    warnings.into_iter().collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyExplosionEmbeddedReference {
    pub form_key: FormKey,
    pub locator: String,
}

pub(crate) fn legacy_explosion_embedded_references(
    record: &Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<Vec<LegacyExplosionEmbeddedReference>, String> {
    if record.sig.as_str() != "EXPL" {
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
        if bytes.len() != LEGACY_EXPLOSION_DATA_LEN {
            return Err(format!(
                "legacy_explosion_data_length_{}_unverified",
                bytes.len()
            ));
        }
        for (slot, offset) in LEGACY_REFERENCE_SLOTS {
            let raw = u32::from_le_bytes(
                bytes[offset..offset + 4]
                    .try_into()
                    .expect("validated explosion FormID slot"),
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
            .ok_or_else(|| format!("legacy_explosion_unresolved_formid_{raw:08x}"))?;
            references.push(LegacyExplosionEmbeddedReference {
                form_key,
                locator: format!("DATA[{data_index}].{slot}@{offset}"),
            });
        }
        data_index += 1;
    }
    Ok(references)
}

pub(crate) fn classify_legacy_explosion(record: &Record) -> LegacyExplosionSupport {
    let mut reasons = BTreeSet::new();
    if record.sig.as_str() != "EXPL" {
        reasons.insert("legacy_explosion_wrong_signature".to_string());
    }

    let mut editor_ids = 0usize;
    let mut bounds = 0usize;
    let mut names = 0usize;
    let mut models = 0usize;
    let mut model_info = 0usize;
    let mut image_space_modifiers = 0usize;
    let mut data_rows = 0usize;
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => editor_ids += 1,
            "OBND" if matches!(&field.value, FieldValue::Bytes(bytes) if bytes.len() == 12) => {
                bounds += 1;
            }
            "FULL" if matches!(field.value, FieldValue::String(_)) => names += 1,
            "MODL" if matches!(field.value, FieldValue::String(_)) => models += 1,
            "MODT" if matches!(field.value, FieldValue::Bytes(_)) => model_info += 1,
            "MNAM" if matches!(field.value, FieldValue::FormKey(_)) => {
                image_space_modifiers += 1;
            }
            "DATA" => {
                data_rows += 1;
                match &field.value {
                    FieldValue::Bytes(bytes) if bytes.len() == LEGACY_EXPLOSION_DATA_LEN => {
                        let flags = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
                        let sound_level = u32::from_le_bytes(bytes[48..52].try_into().unwrap());
                        if flags & !0x7f != 0 {
                            reasons.insert("legacy_explosion_flags_unverified".to_string());
                        }
                        if sound_level > 2 {
                            reasons.insert("legacy_explosion_sound_level_unverified".to_string());
                        }
                    }
                    _ => {
                        reasons.insert("legacy_explosion_data_shape_unverified".to_string());
                    }
                }
            }
            "EDID" | "OBND" | "FULL" | "MODL" | "MODT" | "MNAM" => {
                reasons.insert("legacy_explosion_subrecord_shape_unverified".to_string());
            }
            _ => {
                reasons.insert("legacy_explosion_subrecord_unverified".to_string());
            }
        }
    }

    if editor_ids != 1
        || bounds != 1
        || names > 1
        || models != 1
        || model_info > 1
        || image_space_modifiers > 1
        || data_rows != 1
    {
        reasons.insert("legacy_explosion_subrecord_multiplicity_unverified".to_string());
    }

    LegacyExplosionSupport {
        reason_codes: reasons.into_iter().collect(),
    }
}

pub(crate) fn lower_legacy_explosion(record: &mut Record) -> bool {
    if record.sig.as_str() != "EXPL" {
        return false;
    }
    let mut lowered = false;
    for field in &mut record.fields {
        if field.sig.as_str() != "DATA" {
            continue;
        }
        let FieldValue::Bytes(source) = &field.value else {
            continue;
        };
        if source.len() != LEGACY_EXPLOSION_DATA_LEN {
            continue;
        }
        let mut target = vec![0_u8; FO4_EXPLOSION_DATA_LEN];
        target[0..4].copy_from_slice(&source[12..16]);
        target[4..8].copy_from_slice(&source[16..20]);
        target[8..12].copy_from_slice(&source[32..36]);
        target[12..16].copy_from_slice(&source[28..32]);
        target[24..28].copy_from_slice(&source[0..4]);
        target[28..32].copy_from_slice(&source[4..8]);
        target[36..40].copy_from_slice(&source[8..12]);
        target[40..44].copy_from_slice(&source[24..28]);
        target[48..52].copy_from_slice(&source[20..24]);
        target[52..56].copy_from_slice(&source[48..52]);
        field.value = FieldValue::Bytes(target.into());
        lowered = true;
    }
    record
        .fields
        .retain(|field| !matches!(field.sig.as_str(), "MODT" | "MNAM"));
    lowered
}

#[cfg(test)]
mod tests {
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    use super::*;

    fn source_record(interner: &StringInterner, with_imagespace: bool) -> Record {
        let mut record = Record::new(
            SigCode::from_str("EXPL").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("Owner.esp"),
            },
        );
        let mut data = vec![0_u8; LEGACY_EXPLOSION_DATA_LEN];
        data[0..4].copy_from_slice(&8.0_f32.to_le_bytes());
        data[4..8].copy_from_slice(&25.0_f32.to_le_bytes());
        data[8..12].copy_from_slice(&55.0_f32.to_le_bytes());
        data[12..16].copy_from_slice(&0x0100_0100_u32.to_le_bytes());
        data[16..20].copy_from_slice(&0x0000_0200_u32.to_le_bytes());
        data[20..24].copy_from_slice(&0x43_u32.to_le_bytes());
        data[24..28].copy_from_slice(&154.0_f32.to_le_bytes());
        data[28..32].copy_from_slice(&0x0100_0300_u32.to_le_bytes());
        data[32..36].copy_from_slice(&0x0000_0400_u32.to_le_bytes());
        data[48..52].copy_from_slice(&1_u32.to_le_bytes());
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(interner.intern("LimbExplosion")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"OBND"),
                value: FieldValue::Bytes(vec![0; 12].into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"FULL"),
                value: FieldValue::String(interner.intern("Limb Explosion")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"MODL"),
                value: FieldValue::String(interner.intern("Gore\\LimbExplosion.nif")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"MODT"),
                value: FieldValue::Bytes(vec![1, 2, 3].into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(data.into()),
            },
        ]);
        if with_imagespace {
            record.fields.insert(
                5,
                FieldEntry {
                    sig: SubrecordSig(*b"MNAM"),
                    value: FieldValue::FormKey(FormKey {
                        local: 0x500,
                        plugin: interner.intern("Owner.esp"),
                    }),
                },
            );
        }
        record
    }

    #[test]
    fn exposes_all_four_embedded_runtime_references_with_exact_locators() {
        let interner = StringInterner::new();
        let record = source_record(&interner, false);
        let references = legacy_explosion_embedded_references(
            &record,
            "Owner.esp",
            &["FalloutNV.esm".to_string()],
            &interner,
        )
        .unwrap();
        assert_eq!(
            references
                .iter()
                .map(|reference| reference.locator.as_str())
                .collect::<Vec<_>>(),
            [
                "DATA[0].light@12",
                "DATA[0].sound_1@16",
                "DATA[0].impact_dataset@28",
                "DATA[0].sound_2@32",
            ]
        );
        assert_eq!(
            references
                .iter()
                .map(|reference| reference.form_key.local)
                .collect::<Vec<_>>(),
            [0x100, 0x200, 0x300, 0x400]
        );
    }

    #[test]
    fn source_only_imagespace_and_radiation_use_explicit_omission_warnings() {
        let interner = StringInterner::new();
        assert!(classify_legacy_explosion(&source_record(&interner, false)).is_ready());
        let imagespace = source_record(&interner, true);
        assert!(classify_legacy_explosion(&imagespace).is_ready());
        assert_eq!(
            legacy_explosion_fallback_warnings(&imagespace),
            ["legacy_explosion_imagespace_modifier_omitted"]
        );
        let mut radiation = source_record(&interner, false);
        let FieldValue::Bytes(data) = &mut radiation
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "DATA")
            .unwrap()
            .value
        else {
            panic!("fixture DATA must be bytes")
        };
        data[36..40].copy_from_slice(&1.0_f32.to_le_bytes());
        assert!(classify_legacy_explosion(&radiation).is_ready());
        assert_eq!(
            legacy_explosion_fallback_warnings(&radiation),
            ["legacy_explosion_radiation_scalars_omitted"]
        );
    }

    #[test]
    fn projects_exact_legacy_data_slots_into_fo4_v131_layout() {
        let interner = StringInterner::new();
        let mut record = source_record(&interner, false);
        assert!(lower_legacy_explosion(&mut record));
        assert!(
            record
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "MODT")
        );
        let FieldValue::Bytes(data) = &record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .unwrap()
            .value
        else {
            panic!("lowered DATA must be bytes")
        };
        assert_eq!(data.len(), FO4_EXPLOSION_DATA_LEN);
        assert_eq!(
            u32::from_le_bytes(data[0..4].try_into().unwrap()),
            0x0100_0100
        );
        assert_eq!(
            u32::from_le_bytes(data[4..8].try_into().unwrap()),
            0x0000_0200
        );
        assert_eq!(
            u32::from_le_bytes(data[8..12].try_into().unwrap()),
            0x0000_0400
        );
        assert_eq!(
            u32::from_le_bytes(data[12..16].try_into().unwrap()),
            0x0100_0300
        );
        assert_eq!(f32::from_le_bytes(data[24..28].try_into().unwrap()), 8.0);
        assert_eq!(f32::from_le_bytes(data[28..32].try_into().unwrap()), 25.0);
        assert_eq!(f32::from_le_bytes(data[32..36].try_into().unwrap()), 0.0);
        assert_eq!(f32::from_le_bytes(data[36..40].try_into().unwrap()), 55.0);
        assert_eq!(f32::from_le_bytes(data[40..44].try_into().unwrap()), 154.0);
        assert_eq!(u32::from_le_bytes(data[48..52].try_into().unwrap()), 0x43);
        assert_eq!(u32::from_le_bytes(data[52..56].try_into().unwrap()), 1);
        assert!(data[56..].iter().all(|byte| *byte == 0));
    }
}
