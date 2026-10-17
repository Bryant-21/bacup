use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::source_rig::{CreatureEquipmentSlotBinding, CreatureTargetRecordReference};
use crate::sym::{StringInterner, Sym};

const NATIVE_SLOTS: &[(&str, u32)] = &[
    ("RightHand", 0x013F42),
    ("LeftHand", 0x013F43),
    ("EitherHand", 0x013F44),
    ("BothHands", 0x013F45),
    ("Shield", 0x0141E8),
    ("Voice", 0x025BEE),
    ("Potion", 0x035698),
];

pub(crate) fn native_slot_mappings(
    source_entries: &[(Sym, FormKey, SigCode)],
    interner: &StringInterner,
) -> Vec<(FormKey, FormKey)> {
    let target_plugin = interner.intern("Fallout4.esm");
    source_entries
        .iter()
        .filter_map(|(editor_id, source, signature)| {
            if signature.0 != *b"EQUP" {
                return None;
            }
            let name = interner.resolve(*editor_id)?;
            let (_, local) = NATIVE_SLOTS.iter().find(|(expected, local)| {
                source.local == *local && name.eq_ignore_ascii_case(expected)
            })?;
            Some((
                *source,
                FormKey {
                    local: *local,
                    plugin: target_plugin,
                },
            ))
        })
        .collect()
}

pub(crate) fn project_race_slots(
    race: &Record,
    slots: Vec<CreatureTargetRecordReference>,
    interner: &StringInterner,
) -> Result<Vec<CreatureEquipmentSlotBinding>, String> {
    if slots.is_empty() {
        return Ok(Vec::new());
    }
    let flags = race
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"VNAM")
        .and_then(|field| equipment_flags(&field.value, interner))
        .ok_or_else(|| {
            format!(
                "source race {} has equipment slots without decoded VNAM flags",
                race.form_key.format(interner)
            )
        })?;
    let weapon_types = (flags & 0x01ff) as u16
        | ((flags & 0x0e00) << 3) as u16
        | if flags & 0x1000 != 0 { 0x0080 } else { 0 };
    Ok(slots
        .into_iter()
        .map(|equip_slot| CreatureEquipmentSlotBinding {
            equip_slot,
            node: String::new(),
            weapon_types,
            preserve_weapon_slots: true,
            weapons: Vec::new(),
        })
        .collect())
}

fn equipment_flags(value: &FieldValue, interner: &StringInterner) -> Option<u32> {
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => Some(*value as u32),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            Some(u32::from_le_bytes(bytes.as_slice().try_into().ok()?))
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("equipment_flags"))
            .and_then(|(_, value)| equipment_flags(value, interner)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::source_rig::TargetFormKey;
    use esp_authoring_core::plugin_runtime::{ParsedRecord, ParsedSubrecord};

    #[test]
    fn native_equipment_slot_identity_does_not_depend_on_asset_reuse() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("Skyrim_Merged.esm");
        let mut entries = NATIVE_SLOTS
            .iter()
            .map(|(name, local)| {
                (
                    interner.intern(name),
                    FormKey {
                        local: *local,
                        plugin: source_plugin,
                    },
                    SigCode(*b"EQUP"),
                )
            })
            .collect::<Vec<_>>();
        entries.push((
            interner.intern("LeftHand"),
            FormKey {
                local: 0x123456,
                plugin: source_plugin,
            },
            SigCode(*b"EQUP"),
        ));
        entries.push((
            interner.intern("LeftHand"),
            FormKey {
                local: 0x013F43,
                plugin: source_plugin,
            },
            SigCode(*b"KYWD"),
        ));
        let mappings = native_slot_mappings(&entries, &interner);
        assert_eq!(mappings.len(), 7);
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                use_base_game_assets: false,
                output_plugin_name: "Output.esm".into(),
                ..Default::default()
            },
            &interner,
        );
        for &(source, target) in &mappings {
            mapper.add_mapping(source, target);
        }
        for &(eid, source, signature) in &entries[..7] {
            let target = mapper.allocate_or_resolve(source, Some(eid), signature);
            assert_eq!(target.local, source.local);
            assert_eq!(interner.resolve(target.plugin), Some("Fallout4.esm"));
        }
        let (eid, source, signature) = entries[7];
        let custom = mapper.allocate_or_resolve(source, Some(eid), signature);
        assert_eq!(interner.resolve(custom.plugin), Some("Output.esm"));
    }

    #[test]
    fn source_atronach_slots_preserve_order_and_lower_spell_equipment_bit() {
        let interner = StringInterner::new();
        let key = FormKey {
            local: 0x0131F5,
            plugin: interner.intern("Skyrim.esm"),
        };
        let source = ParsedRecord {
            signature: "RACE".into(),
            form_id: key.local,
            flags: 0,
            version_control: 0,
            form_version: Some(44),
            version2: Some(0),
            raw_payload: None,
            parse_error: None,
            subrecords: vec![
                ParsedSubrecord {
                    signature: "VNAM".into(),
                    data: 0xffff_e201u32.to_le_bytes().to_vec().into(),
                    semantic_type: None,
                },
                ParsedSubrecord {
                    signature: "QNAM".into(),
                    data: 0x013F43u32.to_le_bytes().to_vec().into(),
                    semantic_type: None,
                },
                ParsedSubrecord {
                    signature: "QNAM".into(),
                    data: 0x035698u32.to_le_bytes().to_vec().into(),
                    semantic_type: None,
                },
                ParsedSubrecord {
                    signature: "UNES".into(),
                    data: 0x013F43u32.to_le_bytes().to_vec().into(),
                    semantic_type: None,
                },
            ],
        };
        let schema = crate::schema::AuthoringSchema::for_game("skyrimse").unwrap();
        let race = crate::source_read::decode_record_from_parsed(
            &source,
            &key,
            &schema,
            &[],
            "Skyrim.esm",
            None,
            false,
            &interner,
        )
        .unwrap();
        let slots = race
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"QNAM")
            .map(|field| {
                let FieldValue::FormKey(source) = field.value else {
                    panic!("slot reference was not decoded")
                };
                CreatureTargetRecordReference::new(
                    "EQUP",
                    TargetFormKey::new(source.local, "Fallout4.esm"),
                )
            })
            .collect();
        let projected = project_race_slots(&race, slots, &interner).unwrap();
        assert_eq!(
            projected
                .iter()
                .map(|slot| slot.equip_slot.form_key.local)
                .collect::<Vec<_>>(),
            [0x013F43, 0x035698]
        );
        assert!(projected.iter().all(|slot| slot.weapon_types == 0x1001
            && slot.preserve_weapon_slots
            && slot.weapons.is_empty()
            && slot.node.is_empty()));
    }
}
