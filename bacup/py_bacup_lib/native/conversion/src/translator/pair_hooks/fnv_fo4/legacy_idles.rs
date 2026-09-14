use std::collections::BTreeSet;

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;
use crate::translator::pair_hooks::fnv_conditions::{
    LegacyConditionFamily, legacy_condition_reference_slots,
};

use super::legacy_ammo::source_form_key_for_raw_form_id;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyIdleSupport {
    pub reason_codes: Vec<String>,
}

impl LegacyIdleSupport {
    pub(crate) fn is_ready(&self) -> bool {
        self.reason_codes.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyIdleEmbeddedReference {
    pub form_key: FormKey,
    pub locator: String,
}

pub(crate) fn classify_legacy_idle(record: &Record) -> LegacyIdleSupport {
    let mut reasons = BTreeSet::new();
    if record.sig.as_str() != "IDLE" {
        reasons.insert("legacy_idle_wrong_signature".to_string());
    }

    let mut editor_ids = 0usize;
    let mut model_paths = 0usize;
    let mut animation_links = 0usize;
    let mut data_rows = 0usize;
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" if matches!(field.value, FieldValue::String(_)) => editor_ids += 1,
            "MODL" if matches!(field.value, FieldValue::String(_)) => model_paths += 1,
            "MODB" if matches!(&field.value, FieldValue::Bytes(bytes) if bytes.len() == 4) => {}
            "ANAM" if matches!(&field.value, FieldValue::Bytes(bytes) if bytes.len() == 8) => {
                animation_links += 1;
            }
            "DATA" if matches!(&field.value, FieldValue::Bytes(bytes) if matches!(bytes.len(), 6 | 8)) =>
            {
                data_rows += 1;
            }
            "CTDA" | "CTDT" if matches!(&field.value, FieldValue::Bytes(bytes) if matches!(bytes.len(), 20 | 28)) =>
                {}
            "EDID" | "MODL" | "MODB" | "ANAM" | "DATA" | "CTDA" | "CTDT" => {
                reasons.insert("legacy_idle_field_shape_unverified".to_string());
            }
            _ => {
                reasons.insert("legacy_idle_subrecord_unverified".to_string());
            }
        }
    }
    if editor_ids != 1 || model_paths != 1 || animation_links != 1 || data_rows != 1 {
        reasons.insert("legacy_idle_field_multiplicity_unverified".to_string());
    }

    LegacyIdleSupport {
        reason_codes: reasons.into_iter().collect(),
    }
}

pub(crate) fn legacy_idle_fallback_warnings(record: &Record) -> Vec<String> {
    if !classify_legacy_idle(record).is_ready() {
        return Vec::new();
    }
    let mut warnings = vec![
        "legacy_idle_source_kf_replaced_by_generated_behavior".to_string(),
        "legacy_idle_source_data_semantics_omitted".to_string(),
    ];
    if record
        .fields
        .iter()
        .any(|field| matches!(field.sig.as_str(), "CTDA" | "CTDT"))
    {
        warnings.push("legacy_idle_conditions_omitted".to_string());
    }
    warnings
}

pub(crate) fn legacy_idle_embedded_references(
    record: &Record,
    condition_family: LegacyConditionFamily,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<Vec<LegacyIdleEmbeddedReference>, String> {
    if record.sig.as_str() != "IDLE" {
        return Ok(Vec::new());
    }
    let mut references = Vec::new();
    let mut animation_index = 0usize;
    let mut condition_indices = std::collections::BTreeMap::<String, usize>::new();
    for field in &record.fields {
        match field.sig.as_str() {
            "ANAM" => {
                let FieldValue::Bytes(bytes) = &field.value else {
                    return Err("legacy_idle_anam_shape_unverified".to_string());
                };
                if bytes.len() != 8 {
                    return Err("legacy_idle_anam_shape_unverified".to_string());
                }
                for (offset, name) in [(0usize, "parent"), (4usize, "previous")] {
                    let raw = read_u32(bytes, offset);
                    if raw == 0 {
                        continue;
                    }
                    let form_key = source_form_key_for_raw_form_id(
                        raw,
                        source_plugin_name,
                        source_master_names,
                        interner,
                    )
                    .ok_or_else(|| {
                        format!("legacy_idle_unresolved_anam_{name}_formid_{raw:08x}")
                    })?;
                    references.push(LegacyIdleEmbeddedReference {
                        form_key,
                        locator: format!("ANAM[{animation_index}].{name}@{offset}"),
                    });
                }
                animation_index += 1;
            }
            signature @ ("CTDA" | "CTDT") => {
                let index = condition_indices.entry(signature.to_string()).or_default();
                let condition_index = *index;
                *index += 1;
                let slots = legacy_condition_reference_slots(&field.value, condition_family)
                    .map_err(|_| {
                        format!(
                            "legacy_idle_{}_shape_unverified",
                            signature.to_ascii_lowercase()
                        )
                    })?;
                for slot in slots {
                    if slot.source_raw == 0 {
                        continue;
                    }
                    let form_key = source_form_key_for_raw_form_id(
                        slot.source_raw,
                        source_plugin_name,
                        source_master_names,
                        interner,
                    )
                    .ok_or_else(|| {
                        format!(
                            "legacy_idle_unresolved_ctda_{}_formid_{:08x}",
                            slot.field, slot.source_raw
                        )
                    })?;
                    references.push(LegacyIdleEmbeddedReference {
                        form_key,
                        locator: format!(
                            "{signature}[{condition_index}].{}@{}",
                            slot.field, slot.offset
                        ),
                    });
                }
            }
            _ => {}
        }
    }
    if animation_index != 1 {
        return Err("legacy_idle_anam_multiplicity_unverified".to_string());
    }
    Ok(references)
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated IDLE ANAM row"),
    )
}

#[cfg(test)]
mod tests {
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    use super::*;

    fn fixture(interner: &StringInterner, parent: u32, previous: u32) -> Record {
        let mut links = Vec::new();
        links.extend_from_slice(&parent.to_le_bytes());
        links.extend_from_slice(&previous.to_le_bytes());
        let mut record = Record::new(
            SigCode::from_str("IDLE").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("Owner.esp"),
            },
        );
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(interner.intern("CreatureSpecialIdle")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"MODL"),
                value: FieldValue::String(interner.intern("Creatures\\Test\\IdleAnims\\Idle.kf")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"ANAM"),
                value: FieldValue::Bytes(links.into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(vec![7, 0, 0, 0xcd, 0, 0].into()),
            },
        ]);
        record
    }

    #[test]
    fn exposes_parent_and_previous_with_exact_load_order_aware_locators() {
        let interner = StringInterner::new();
        let record = fixture(&interner, 0x0000_1234, 0x0100_5678);
        let references = legacy_idle_embedded_references(
            &record,
            LegacyConditionFamily::Fnv,
            "Owner.esp",
            &["Master.esm".to_string()],
            &interner,
        )
        .unwrap();

        assert_eq!(references.len(), 2);
        assert_eq!(references[0].locator, "ANAM[0].parent@0");
        assert_eq!(
            interner.resolve(references[0].form_key.plugin),
            Some("Master.esm")
        );
        assert_eq!(references[0].form_key.local, 0x1234);
        assert_eq!(references[1].locator, "ANAM[0].previous@4");
        assert_eq!(
            interner.resolve(references[1].form_key.plugin),
            Some("Owner.esp")
        );
        assert_eq!(references[1].form_key.local, 0x5678);
    }

    #[test]
    fn null_links_are_not_phantom_dependencies_and_semantics_remain_typed_blockers() {
        let interner = StringInterner::new();
        let record = fixture(&interner, 0, 0);
        assert!(
            legacy_idle_embedded_references(
                &record,
                LegacyConditionFamily::Fnv,
                "Owner.esp",
                &[],
                &interner,
            )
            .unwrap()
            .is_empty()
        );
        assert!(classify_legacy_idle(&record).is_ready());
        assert_eq!(
            legacy_idle_fallback_warnings(&record),
            [
                "legacy_idle_source_kf_replaced_by_generated_behavior",
                "legacy_idle_source_data_semantics_omitted",
            ]
        );
    }

    #[test]
    fn malformed_anam_never_silently_drops_topology() {
        let interner = StringInterner::new();
        let mut record = fixture(&interner, 0, 0);
        record
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "ANAM")
            .unwrap()
            .value = FieldValue::Bytes(vec![0; 4].into());

        assert_eq!(
            legacy_idle_embedded_references(
                &record,
                LegacyConditionFamily::Fnv,
                "Owner.esp",
                &[],
                &interner,
            ),
            Err("legacy_idle_anam_shape_unverified".to_string())
        );
    }

    #[test]
    fn exposes_condition_references_alongside_idle_topology() {
        let interner = StringInterner::new();
        let mut record = fixture(&interner, 0, 0);
        let mut condition = vec![0; 28];
        condition[8..10].copy_from_slice(&60_u16.to_le_bytes());
        condition[12..16].copy_from_slice(&0x0000_1234_u32.to_le_bytes());
        condition[16..20].copy_from_slice(&0x0100_5678_u32.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"CTDA"),
            value: FieldValue::Bytes(condition.into()),
        });
        let mut old_condition = vec![0; 20];
        old_condition[8..10].copy_from_slice(&79_u16.to_le_bytes());
        old_condition[12..16].copy_from_slice(&0x0000_7777_u32.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"CTDT"),
            value: FieldValue::Bytes(old_condition.into()),
        });

        let references = legacy_idle_embedded_references(
            &record,
            LegacyConditionFamily::Fnv,
            "Owner.esp",
            &["Master.esm".to_string()],
            &interner,
        )
        .unwrap();

        assert_eq!(references.len(), 3);
        assert_eq!(references[0].locator, "CTDA[0].condition_parameter_1@12");
        assert_eq!(references[0].form_key.local, 0x1234);
        assert_eq!(references[1].locator, "CTDA[0].condition_parameter_2@16");
        assert_eq!(references[1].form_key.local, 0x5678);
        assert_eq!(references[2].locator, "CTDT[0].condition_parameter_1@12");
        assert_eq!(references[2].form_key.local, 0x7777);
        assert_eq!(
            legacy_idle_fallback_warnings(&record),
            [
                "legacy_idle_source_kf_replaced_by_generated_behavior",
                "legacy_idle_source_data_semantics_omitted",
                "legacy_idle_conditions_omitted",
            ]
        );
    }
}
