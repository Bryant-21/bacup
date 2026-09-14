use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;
use crate::translator::pair_hooks::fnv_conditions::{
    LegacyConditionFamily, legacy_condition_reference_slots,
};

use super::legacy_ammo::source_form_key_for_raw_form_id;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyPackageEmbeddedReference {
    pub form_key: FormKey,
    pub locator: String,
}

pub(crate) fn legacy_package_embedded_references(
    record: &Record,
    condition_family: LegacyConditionFamily,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<Vec<LegacyPackageEmbeddedReference>, String> {
    if record.sig.as_str() != "PACK" {
        return Ok(Vec::new());
    }
    let mut references = Vec::new();
    let mut signature_indices = std::collections::BTreeMap::<String, usize>::new();
    for field in &record.fields {
        let signature = field.sig.as_str();
        if matches!(signature, "CTDA" | "CTDT") {
            let index = signature_indices.entry(signature.to_string()).or_default();
            let current_index = *index;
            *index += 1;
            let slots =
                legacy_condition_reference_slots(&field.value, condition_family).map_err(|_| {
                    format!(
                        "legacy_pack_{}_shape_unverified",
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
                        "legacy_pack_unresolved_ctda_{}_formid_{:08x}",
                        slot.field, slot.source_raw
                    )
                })?;
                references.push(LegacyPackageEmbeddedReference {
                    form_key,
                    locator: format!(
                        "{signature}[{current_index}].{}@{}",
                        slot.field, slot.offset
                    ),
                });
            }
            continue;
        }
        let Some((expected_len, payload_name, reference_types, valid_types)) =
            union_shape(signature)
        else {
            continue;
        };
        let index = signature_indices.entry(signature.to_string()).or_default();
        let current_index = *index;
        *index += 1;
        let FieldValue::Bytes(bytes) = &field.value else {
            if matches!(field.value, FieldValue::Struct(_)) {
                continue;
            }
            return Err(format!(
                "legacy_pack_{}_shape_unverified",
                signature.to_ascii_lowercase()
            ));
        };
        if bytes.len() != expected_len {
            return Err(format!(
                "legacy_pack_{}_shape_unverified",
                signature.to_ascii_lowercase()
            ));
        }
        let type_code = read_u32(bytes, 0);
        if !valid_types.contains(&type_code) {
            return Err(format!(
                "legacy_pack_{}_type_{type_code}_unverified",
                signature.to_ascii_lowercase()
            ));
        }
        if !reference_types.contains(&type_code) {
            continue;
        }
        let raw = read_u32(bytes, 4);
        if raw == 0 {
            continue;
        }
        let form_key =
            source_form_key_for_raw_form_id(raw, source_plugin_name, source_master_names, interner)
                .ok_or_else(|| {
                    format!(
                        "legacy_pack_unresolved_{}_{}_formid_{raw:08x}",
                        signature.to_ascii_lowercase(),
                        payload_name
                    )
                })?;
        references.push(LegacyPackageEmbeddedReference {
            form_key,
            locator: format!("{signature}[{current_index}].{payload_name}@4"),
        });
    }
    Ok(references)
}

fn union_shape(signature: &str) -> Option<(usize, &'static str, &'static [u32], &'static [u32])> {
    match signature {
        "PLDT" | "PLD2" => Some((12, "location", &[0, 1, 4], &[0, 1, 2, 3, 4, 5, 6, 7])),
        "PTDT" | "PTD2" => Some((16, "target", &[0, 1], &[0, 1, 2, 3])),
        _ => None,
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated PACK union row"),
    )
}

#[cfg(test)]
mod tests {
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    use super::*;

    fn fixture(interner: &StringInterner) -> Record {
        let mut location = Vec::new();
        location.extend_from_slice(&0_u32.to_le_bytes());
        location.extend_from_slice(&0x0000_1234_u32.to_le_bytes());
        location.extend_from_slice(&128_i32.to_le_bytes());
        let mut target = Vec::new();
        target.extend_from_slice(&1_u32.to_le_bytes());
        target.extend_from_slice(&0x0100_5678_u32.to_le_bytes());
        target.extend_from_slice(&0_i32.to_le_bytes());
        target.extend_from_slice(&0_u32.to_le_bytes());
        let mut implicit = Vec::new();
        implicit.extend_from_slice(&6_u32.to_le_bytes());
        implicit.extend_from_slice(&0_u32.to_le_bytes());
        implicit.extend_from_slice(&0_i32.to_le_bytes());
        let mut record = Record::new(
            SigCode::from_str("PACK").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("Owner.esp"),
            },
        );
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"PLDT"),
                value: FieldValue::Bytes(location.into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"PTDT"),
                value: FieldValue::Bytes(target.into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"PLD2"),
                value: FieldValue::Bytes(implicit.into()),
            },
        ]);
        record
    }

    #[test]
    fn exposes_reference_typed_unions_with_exact_load_order_aware_locators() {
        let interner = StringInterner::new();
        let references = legacy_package_embedded_references(
            &fixture(&interner),
            LegacyConditionFamily::Fnv,
            "Owner.esp",
            &["Master.esm".to_string()],
            &interner,
        )
        .unwrap();

        assert_eq!(references.len(), 2);
        assert_eq!(references[0].locator, "PLDT[0].location@4");
        assert_eq!(
            interner.resolve(references[0].form_key.plugin),
            Some("Master.esm")
        );
        assert_eq!(references[0].form_key.local, 0x1234);
        assert_eq!(references[1].locator, "PTDT[0].target@4");
        assert_eq!(
            interner.resolve(references[1].form_key.plugin),
            Some("Owner.esp")
        );
        assert_eq!(references[1].form_key.local, 0x5678);
    }

    #[test]
    fn implicit_union_does_not_create_a_phantom_dependency() {
        let interner = StringInterner::new();
        let mut record = fixture(&interner);
        record.fields.drain(..2);
        assert!(
            legacy_package_embedded_references(
                &record,
                LegacyConditionFamily::Fnv,
                "Owner.esp",
                &[],
                &interner,
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn malformed_or_unknown_union_shape_fails_closed() {
        let interner = StringInterner::new();
        let mut record = fixture(&interner);
        record.fields[0].value = FieldValue::Bytes(vec![0; 8].into());
        assert_eq!(
            legacy_package_embedded_references(
                &record,
                LegacyConditionFamily::Fnv,
                "Owner.esp",
                &[],
                &interner,
            ),
            Err("legacy_pack_pldt_shape_unverified".to_string())
        );
    }

    #[test]
    fn exposes_ctda_reference_slots_without_numeric_parameter_phantoms() {
        let interner = StringInterner::new();
        let mut record = fixture(&interner);
        record.fields.clear();
        let mut condition = vec![0; 28];
        condition[0] = 0x04;
        condition[4..8].copy_from_slice(&0x0000_1111_u32.to_le_bytes());
        condition[8..10].copy_from_slice(&60_u16.to_le_bytes());
        condition[12..16].copy_from_slice(&0x0100_2222_u32.to_le_bytes());
        condition[16..20].copy_from_slice(&0x0000_3333_u32.to_le_bytes());
        condition[20..24].copy_from_slice(&2_u32.to_le_bytes());
        condition[24..28].copy_from_slice(&0_u32.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"CTDA"),
            value: FieldValue::Bytes(condition.into()),
        });
        let mut old_condition = vec![0; 20];
        old_condition[8..10].copy_from_slice(&79_u16.to_le_bytes());
        old_condition[12..16].copy_from_slice(&0x0000_4444_u32.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"CTDT"),
            value: FieldValue::Bytes(old_condition.into()),
        });

        let references = legacy_package_embedded_references(
            &record,
            LegacyConditionFamily::Fnv,
            "Owner.esp",
            &["Master.esm".to_string()],
            &interner,
        )
        .unwrap();

        assert_eq!(references.len(), 4);
        assert_eq!(
            references
                .iter()
                .map(|reference| reference.locator.as_str())
                .collect::<Vec<_>>(),
            [
                "CTDA[0].condition_comparison_global@4",
                "CTDA[0].condition_parameter_1@12",
                "CTDA[0].condition_parameter_2@16",
                "CTDT[0].condition_parameter_1@12",
            ]
        );
        assert_eq!(references[0].form_key.local, 0x1111);
        assert_eq!(references[1].form_key.local, 0x2222);
        assert_eq!(references[2].form_key.local, 0x3333);
        assert_eq!(references[3].form_key.local, 0x4444);
    }
}
