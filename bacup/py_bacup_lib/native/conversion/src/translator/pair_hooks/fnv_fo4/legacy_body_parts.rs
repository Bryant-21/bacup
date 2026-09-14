use std::collections::BTreeSet;

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::struct_relayout::{StructRelayoutCtx, relayout_struct_bytes};
use crate::sym::StringInterner;

use super::legacy_ammo::source_form_key_for_raw_form_id;

const LEGACY_BPND_LEN: usize = 84;
const FO4_BPND_LEN: usize = 101;

const LEGACY_BPND_REFERENCE_SLOTS: [(&str, usize); 6] = [
    ("explodable_debris", 12),
    ("explodable_explosion", 16),
    ("severable_debris", 32),
    ("severable_explosion", 36),
    ("severable_impact_dataset", 68),
    ("explodable_impact_dataset", 72),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LegacyBodyPartSourceFamily {
    Fnv,
    Fo3,
}

impl LegacyBodyPartSourceFamily {
    fn game(self) -> &'static str {
        match self {
            Self::Fnv => "fnv",
            Self::Fo3 => "fo3",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyBodyPartSupport {
    pub reason_codes: Vec<String>,
}

impl LegacyBodyPartSupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

pub(crate) fn legacy_body_part_fallback_warnings(record: &Record) -> Vec<String> {
    if !classify_legacy_body_part_data(record).is_ready() {
        return Vec::new();
    }
    let mut warnings = BTreeSet::new();
    for field in &record.fields {
        let FieldValue::Bytes(bytes) = &field.value else {
            continue;
        };
        if field.sig.as_str() != "BPND" || bytes.len() != LEGACY_BPND_LEN {
            continue;
        }
        if bytes[4] & 0x36 != 0 {
            warnings.insert("legacy_body_part_ik_flags_omitted".to_string());
        }
        if bytes[4] & 0x40 != 0 {
            warnings.insert("legacy_body_part_absolute_hit_chance_flag_omitted".to_string());
        }
        if bytes[78] != 0 || bytes[79] != 0 {
            warnings.insert("legacy_body_part_unknown_trailing_fields_omitted".to_string());
        }
    }
    warnings.into_iter().collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyBodyPartEmbeddedReference {
    pub form_key: FormKey,
    pub locator: String,
}

pub(crate) fn legacy_body_part_embedded_references(
    record: &Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<Vec<LegacyBodyPartEmbeddedReference>, String> {
    if record.sig.as_str() != "BPTD" {
        return Ok(Vec::new());
    }
    let mut references = Vec::new();
    let mut row_index = 0usize;
    for field in &record.fields {
        if field.sig.as_str() != "BPND" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &field.value else {
            row_index += 1;
            continue;
        };
        if bytes.len() != LEGACY_BPND_LEN {
            return Err(format!(
                "legacy_body_part_bpnd_length_{}_unverified",
                bytes.len()
            ));
        }
        for (slot, offset) in LEGACY_BPND_REFERENCE_SLOTS {
            let raw = u32::from_le_bytes(
                bytes[offset..offset + 4]
                    .try_into()
                    .expect("validated four-byte BPND reference"),
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
            .ok_or_else(|| format!("legacy_body_part_unresolved_formid_{raw:08x}"))?;
            references.push(LegacyBodyPartEmbeddedReference {
                form_key,
                locator: format!("BPND[{row_index}].{slot}@{offset}"),
            });
        }
        row_index += 1;
    }
    Ok(references)
}

pub(crate) fn classify_legacy_body_part_data(record: &Record) -> LegacyBodyPartSupport {
    let mut reasons = BTreeSet::new();
    if record.sig.as_str() != "BPTD" {
        reasons.insert("legacy_body_part_wrong_signature".to_string());
    }

    let mut edid_count = 0usize;
    let mut model_count = 0usize;
    let mut row_counts = [0usize; 8];
    let mut ragdoll_count = 0usize;
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => edid_count += 1,
            "MODL" if matches!(field.value, FieldValue::String(_)) => model_count += 1,
            "BPTN" if matches!(field.value, FieldValue::String(_)) => row_counts[0] += 1,
            "BPNN" if matches!(field.value, FieldValue::String(_)) => row_counts[1] += 1,
            "BPNT" if matches!(field.value, FieldValue::String(_)) => row_counts[2] += 1,
            "BPNI" if matches!(field.value, FieldValue::String(_)) => row_counts[3] += 1,
            "BPND" => {
                row_counts[4] += 1;
                match &field.value {
                    FieldValue::Bytes(bytes) if bytes.len() == LEGACY_BPND_LEN => {
                        if !matches!(bytes[5], 0..=14) {
                            reasons.insert("legacy_body_part_part_type_unverified".to_string());
                        }
                        if !matches!(bytes[7], 0xff | 25..=31) {
                            reasons.insert("legacy_body_part_actor_value_unverified".to_string());
                        }
                    }
                    FieldValue::Bytes(bytes) if bytes.len() == FO4_BPND_LEN => {}
                    FieldValue::Struct(_) => {}
                    FieldValue::Bytes(bytes) => {
                        reasons.insert(format!(
                            "legacy_body_part_bpnd_length_{}_unverified",
                            bytes.len()
                        ));
                    }
                    _ => {
                        reasons.insert("legacy_body_part_bpnd_shape_unverified".to_string());
                    }
                }
            }
            "NAM1" if matches!(field.value, FieldValue::String(_)) => row_counts[5] += 1,
            "NAM4" if matches!(field.value, FieldValue::String(_)) => row_counts[6] += 1,
            "NAM5" if matches!(field.value, FieldValue::Bytes(_)) => row_counts[7] += 1,
            "RAGA" if matches!(field.value, FieldValue::FormKey(_)) => ragdoll_count += 1,
            "EDID" | "MODL" | "BPTN" | "BPNN" | "BPNT" | "BPNI" | "NAM1" | "NAM4" | "NAM5"
            | "RAGA" => {
                reasons.insert("legacy_body_part_subrecord_shape_unverified".to_string());
            }
            _ => {
                reasons.insert("legacy_body_part_subrecord_unverified".to_string());
            }
        }
    }

    if edid_count != 1 || model_count != 1 {
        reasons.insert("legacy_body_part_header_multiplicity_unverified".to_string());
    }
    let row_count = row_counts[4];
    if row_count == 0
        || row_counts[1..]
            .iter()
            .enumerate()
            .any(|(index, count)| index != 3 && *count != row_count)
        || row_counts[0] > row_count
    {
        reasons.insert("legacy_body_part_row_multiplicity_unverified".to_string());
    }
    if ragdoll_count > 1 {
        reasons.insert("legacy_body_part_ragdoll_multiplicity_unverified".to_string());
    }

    LegacyBodyPartSupport {
        reason_codes: reasons.into_iter().collect(),
    }
}

pub(crate) fn lower_legacy_body_part_data(
    record: &mut Record,
    source: LegacyBodyPartSourceFamily,
) -> bool {
    if record.sig.as_str() != "BPTD" {
        return false;
    }
    let Ok(source_schema) = AuthoringSchema::for_game(source.game()) else {
        return false;
    };
    let Ok(target_schema) = AuthoringSchema::for_game("fo4") else {
        return false;
    };
    let ctx = StructRelayoutCtx {
        target_schema: &target_schema,
        target_form_version: 131,
        legacy_bptd_only: true,
    };
    for field in &mut record.fields {
        if field.sig.as_str() != "BPND" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &field.value else {
            continue;
        };
        if bytes.len() == LEGACY_BPND_LEN {
            let mut source = bytes.to_vec();
            source[4] &= !(0x36 | 0x40);
            source[78] = 0;
            source[79] = 0;
            let Some(lowered) =
                relayout_struct_bytes("BPTD", "BPND", &source, &source_schema, Some(15), &ctx)
            else {
                return false;
            };
            field.value = FieldValue::Bytes(lowered.into());
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    use super::*;

    fn record_with_row(interner: &StringInterner, flags: u8, ragdoll: bool) -> Record {
        let mut record = Record::new(
            SigCode::from_str("BPTD").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("Owner.esp"),
            },
        );
        let string = |signature: &[u8; 4], value: &str| FieldEntry {
            sig: SubrecordSig(*signature),
            value: FieldValue::String(interner.intern(value)),
        };
        record.fields.extend([
            string(b"EDID", "BodyParts"),
            string(b"MODL", "Creatures\\Test\\Skeleton.nif"),
            string(b"BPTN", "Head"),
            string(b"BPNN", "Head"),
            string(b"BPNT", "Head"),
            string(b"BPNI", "Head"),
        ]);
        let mut bpnd = vec![0_u8; LEGACY_BPND_LEN];
        bpnd[0..4].copy_from_slice(&1.0_f32.to_le_bytes());
        bpnd[4] = flags;
        bpnd[5] = 1;
        bpnd[6] = 100;
        bpnd[7] = 25;
        bpnd[24..28].copy_from_slice(&60.0_f32.to_le_bytes());
        bpnd[12..16].copy_from_slice(&0x0100_0100_u32.to_le_bytes());
        bpnd[36..40].copy_from_slice(&0x0000_0200_u32.to_le_bytes());
        bpnd[68..72].copy_from_slice(&0x0100_0300_u32.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"BPND"),
            value: FieldValue::Bytes(bpnd.into()),
        });
        record.fields.extend([
            string(b"NAM1", "Gore\\Head.nif"),
            string(b"NAM4", "Head"),
            FieldEntry {
                sig: SubrecordSig(*b"NAM5"),
                value: FieldValue::Bytes(Vec::new().into()),
            },
        ]);
        if ragdoll {
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"RAGA"),
                value: FieldValue::FormKey(FormKey {
                    local: 0x400,
                    plugin: interner.intern("Owner.esp"),
                }),
            });
        }
        record
    }

    #[test]
    fn decodes_all_six_embedded_runtime_references_with_exact_locators() {
        let interner = StringInterner::new();
        let record = record_with_row(&interner, 0x09, false);
        let references = legacy_body_part_embedded_references(
            &record,
            "Owner.esp",
            &["FalloutNV.esm".to_string()],
            &interner,
        )
        .unwrap();
        assert_eq!(references.len(), 3);
        assert_eq!(
            references
                .iter()
                .map(|reference| reference.locator.as_str())
                .collect::<Vec<_>>(),
            [
                "BPND[0].explodable_debris@12",
                "BPND[0].severable_explosion@36",
                "BPND[0].severable_impact_dataset@68",
            ]
        );
        assert_eq!(references[0].form_key.local, 0x100);
        assert_eq!(references[1].form_key.local, 0x200);
        assert_eq!(references[2].form_key.local, 0x300);
    }

    #[test]
    fn source_only_body_part_flags_are_omitted_with_warnings() {
        let interner = StringInterner::new();
        assert!(
            classify_legacy_body_part_data(&record_with_row(&interner, 0x09, false)).is_ready()
        );
        let ik = record_with_row(&interner, 0x0b, false);
        assert!(classify_legacy_body_part_data(&ik).is_ready());
        assert_eq!(
            legacy_body_part_fallback_warnings(&ik),
            ["legacy_body_part_ik_flags_omitted"]
        );
        let absolute = record_with_row(&interner, 0x49, false);
        assert!(classify_legacy_body_part_data(&absolute).is_ready());
        assert_eq!(
            legacy_body_part_fallback_warnings(&absolute),
            ["legacy_body_part_absolute_hit_chance_flag_omitted"]
        );
        assert!(classify_legacy_body_part_data(&record_with_row(&interner, 0x09, true)).is_ready());

        let mut unknown = record_with_row(&interner, 0x09, false);
        let FieldValue::Bytes(bytes) = &mut unknown
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "BPND")
            .unwrap()
            .value
        else {
            panic!("fixture BPND must be bytes")
        };
        bytes[78] = 1;
        assert!(classify_legacy_body_part_data(&unknown).is_ready());
        assert_eq!(
            legacy_body_part_fallback_warnings(&unknown),
            ["legacy_body_part_unknown_trailing_fields_omitted"]
        );
    }

    #[test]
    fn lowers_legacy_node_data_to_exact_fo4_layout() {
        let interner = StringInterner::new();
        let mut record = record_with_row(&interner, 0x09, false);
        assert!(lower_legacy_body_part_data(
            &mut record,
            LegacyBodyPartSourceFamily::Fnv,
        ));
        let FieldValue::Bytes(bytes) = &record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "BPND")
            .unwrap()
            .value
        else {
            panic!("lowered BPND must remain bytes")
        };
        assert_eq!(bytes.len(), FO4_BPND_LEN);
        let target = AuthoringSchema::for_game("fo4").unwrap();
        let layout = target.struct_field_layout_versioned("BPTD", "BPND", Some(131));
        let actor_value = layout
            .iter()
            .find(|field| field.field_id == "actor_value")
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(
                bytes[actor_value.offset..actor_value.offset + 4]
                    .try_into()
                    .unwrap()
            ),
            0x36c
        );
    }
}
