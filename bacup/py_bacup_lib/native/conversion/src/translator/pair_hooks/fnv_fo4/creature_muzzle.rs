use std::collections::BTreeMap;

use nif_core_native::model::{NifFile, NifValue};

use crate::formkey_mapper::{FormKeyMapper, MapperState};
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::phase::animations::weapon_events::decode_weapon_animation_fields;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::source_rig::{
    CreatureEquipmentSlotBinding, CreatureTargetRecordReference, TargetFormKey,
};
use crate::sym::StringInterner;

use super::creature_weapon_attacks::CreatureWeaponAttackEvidence;

pub(crate) fn embedded_weapon_node(record: &Record, interner: &StringInterner) -> Option<String> {
    if record.sig.0 != *b"WEAP" {
        return None;
    }
    decode_weapon_animation_fields(record, interner).ok()?;
    let node = record.fields.iter().find(|field| field.sig.0 == *b"NNAM")?;
    let node = match &node.value {
        FieldValue::String(value) => interner.resolve(*value)?,
        FieldValue::Bytes(value) => {
            std::str::from_utf8(value.split(|byte| *byte == 0).next()?).ok()?
        }
        _ => return None,
    };
    (!node.is_empty()).then(|| node.to_string())
}

pub(crate) fn embedded_slot_source_key(weapon: &str, interner: &StringInterner) -> FormKey {
    let encoded = weapon
        .to_ascii_lowercase()
        .bytes()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    FormKey {
        local: 1,
        plugin: interner.intern(&format!("B21_LegacyEmbeddedSlot_{encoded}.esm")),
    }
}

pub(crate) fn allocate_embedded_weapon_slots(
    mapper: &mut FormKeyMapper<'_>,
    cached: &mut BTreeMap<String, FormKey>,
    weapons: impl IntoIterator<Item = String>,
    interner: &StringInterner,
) -> Vec<Record> {
    for (node, target) in cached.iter() {
        mapper.add_mapping(embedded_slot_source_key(node, interner), *target);
    }
    let mut records = Vec::new();
    for weapon in weapons {
        let canonical = weapon.to_ascii_lowercase();
        if cached.contains_key(&canonical) {
            continue;
        }
        let target = mapper.allocate_generated();
        mapper.add_mapping(embedded_slot_source_key(&weapon, interner), target);
        cached.insert(canonical, target);
        let mut record = Record::new(SigCode(*b"EQUP"), target);
        record.eid = Some(interner.intern(&format!(
            "B21_LegacyEmbeddedSlot_{}",
            blake3::hash(weapon.to_ascii_lowercase().as_bytes()).to_hex()
        )));
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Uint(0),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"ANAM"),
            value: FieldValue::Uint(0),
        });
        records.push(record);
    }
    records
}

pub(crate) fn bind_embedded_weapon_slot(record: &mut Record, interner: &StringInterner) {
    if embedded_weapon_node(record, interner).is_some() {
        let weapon = format!(
            "{:06X}:{}",
            record.form_key.local,
            interner.resolve(record.form_key.plugin).unwrap()
        );
        let value = FieldValue::FormKey(embedded_slot_source_key(&weapon, interner));
        if let Some(field) = record
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"ETYP")
        {
            field.value = value;
        } else {
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"ETYP"),
                value,
            });
        }
    }
}

pub(crate) fn family_muzzle_nodes(weapons: &[CreatureWeaponAttackEvidence]) -> Vec<String> {
    weapons
        .iter()
        .filter(|weapon| weapon.is_ranged())
        .filter(|weapon| weapon.equipment_group.is_some())
        .filter_map(|weapon| weapon.embedded_weapon_node.as_ref())
        .map(|node| (node.to_ascii_lowercase(), node.clone()))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect()
}

pub(crate) fn validate_weapon_attachment_nodes(
    nif: &NifFile,
    weapons: &[CreatureWeaponAttackEvidence],
) -> Result<(), String> {
    for weapon in weapons.iter().filter(|weapon| weapon.equipment_index.is_some()) {
        let Some(node) = weapon.embedded_weapon_node.as_ref().or(weapon.held_weapon_node.as_ref()) else {
            return Err(format!("equipped creature weapon {} has no source attachment node", weapon.source));
        };
        let count = nif.blocks.iter().filter(|block| block.fields.get("Name") == Some(&NifValue::String(node.clone()))).count();
        if count != 1 {
            return Err(format!("source weapon attachment node {node} must occur exactly once, found {count}"));
        }
    }
    Ok(())
}

pub(crate) fn family_equipment_slots(
    weapons: &[CreatureWeaponAttackEvidence],
    owned_weapons: &[super::creature_catalog::StableFormKey],
    mapper: &MapperState,
    interner: &StringInterner,
) -> Result<Vec<CreatureEquipmentSlotBinding>, String> {
    let mut slots = BTreeMap::new();
    for weapon in weapons {
        let (Some(group), Some(index), Some(node)) = (
            &weapon.equipment_group,
            weapon.equipment_index,
            &weapon.embedded_weapon_node,
        ) else {
            continue;
        };
        if index > 4 {
            return Err(format!("invalid embedded weapon equipment index {index}"));
        }
        if let Some((previous_group, previous_node)) = slots.insert(index, (group, node)) {
            if previous_group != group || previous_node != node {
                return Err(format!(
                    "conflicting embedded weapon equipment index {index}"
                ));
            }
        }
    }
    let mut bindings = slots
        .into_iter()
        .enumerate()
        .map(|(expected, (index, (group, node)))| {
            if usize::from(index) != expected {
                return Err(format!(
                    "missing embedded weapon equipment index {expected}"
                ));
            }
            let source = embedded_slot_source_key(
                &format!("{:06X}:{}", group.local, group.plugin),
                interner,
            );
            let target = mapper
                .source_to_target
                .get(&source)
                .ok_or_else(|| format!("unallocated embedded weapon equip slot for {node}"))?;
            Ok(CreatureEquipmentSlotBinding {
                equip_slot: CreatureTargetRecordReference {
                    signature: "EQUP".to_string(),
                    form_key: TargetFormKey::new(
                        target.local,
                        interner.resolve(target.plugin).unwrap(),
                    ),
                },
                node: node.clone(),
                preserve_weapon_slots: false,
                weapon_types: weapons.iter().filter(|weapon| {
                    weapon.equipment_group.as_ref() == Some(group)
                        && owned_weapons.iter().any(|owned| {
                            owned.local == weapon.source.local
                                && owned.plugin.eq_ignore_ascii_case(&weapon.source.plugin)
                        })
                }).fold(0, |types, weapon| {
                    types | (1u16 << crate::fixups::synthesize_weap_data_blocks::legacy_animation_type(weapon.animation.animation_type))
                }),
                weapons: weapons
                    .iter()
                    .filter(|weapon| {
                        weapon.equipment_group.as_ref() == Some(group)
                            && owned_weapons.iter().any(|owned| {
                                owned.local == weapon.source.local
                                    && owned.plugin.eq_ignore_ascii_case(&weapon.source.plugin)
                            })
                    })
                    .map(|weapon| {
                        let source = FormKey {
                            local: weapon.source.local,
                            plugin: interner.intern(&weapon.source.plugin),
                        };
                        let target = mapper
                            .source_to_target
                            .get(&source)
                            .or_else(|| {
                                mapper.source_to_target.iter().find_map(|(key, target)| {
                                    (key.local == source.local
                                        && interner.resolve(key.plugin).is_some_and(|plugin| {
                                            plugin.eq_ignore_ascii_case(&weapon.source.plugin)
                                        }))
                                    .then_some(target)
                                })
                            })
                            .ok_or_else(|| format!("unmapped embedded weapon {}", weapon.source))?;
                        Ok(CreatureTargetRecordReference {
                            signature: "WEAP".to_string(),
                            form_key: TargetFormKey::new(
                                target.local,
                                interner.resolve(target.plugin).unwrap(),
                            ),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut held = BTreeMap::<u8, Vec<&CreatureWeaponAttackEvidence>>::new();
    for weapon in weapons {
        if weapon.held_weapon_node.is_some() && let Some(index) = weapon.equipment_index {
            held.entry(index).or_default().push(weapon);
        }
    }
    for (index, group) in held {
        if usize::from(index) != bindings.len() || index > 4 {
            return Err(format!("inconsistent held weapon equipment index {index}"));
        }
        let thrown = matches!(group[0].animation.animation_type, 10 | 13);
        if group.iter().any(|weapon| {
            matches!(weapon.animation.animation_type, 10 | 13) != thrown
        }) {
            return Err(format!("conflicting held weapon equipment index {index}"));
        }
        let owned = group.iter().copied().filter(|weapon| owned_weapons.iter().any(|owned| {
            owned.local == weapon.source.local && owned.plugin.eq_ignore_ascii_case(&weapon.source.plugin)
        })).collect::<Vec<_>>();
        let node = owned.first().copied().unwrap_or(group[0]).held_weapon_node.as_ref().unwrap();
        if owned.iter().any(|weapon| {
            !weapon.held_weapon_node.as_ref().unwrap().eq_ignore_ascii_case(node)
        }) {
            return Err(format!("conflicting owned held weapon attachments at equipment index {index}"));
        }
        let equip_slot = |local| CreatureTargetRecordReference {
            signature: "EQUP".to_string(),
            form_key: TargetFormKey::new(local, "Fallout4.esm"),
        };
        bindings.push(CreatureEquipmentSlotBinding {
            equip_slot: equip_slot(if thrown { 0x046AAC } else { 0x013F42 }),
            node: node.clone(),
            weapon_types: owned.iter().fold(0, |types, weapon| types | (1u16 << crate::fixups::synthesize_weap_data_blocks::legacy_animation_type(weapon.animation.animation_type))),
            preserve_weapon_slots: true,
            weapons: owned.iter().map(|weapon| mapped_weapon_reference(&weapon.source, mapper, interner)).collect::<Result<Vec<_>, _>>()?,
        });
        if !thrown {
            bindings.push(CreatureEquipmentSlotBinding {
                equip_slot: equip_slot(0x013F43),
                node: String::new(),
                weapon_types: 0,
                preserve_weapon_slots: true,
                weapons: Vec::new(),
            });
        }
    }
    Ok(bindings)
}

fn mapped_weapon_reference(
    source: &super::creature_catalog::StableFormKey,
    mapper: &MapperState,
    interner: &StringInterner,
) -> Result<CreatureTargetRecordReference, String> {
    let target = mapper.source_to_target.iter().find_map(|(key, target)| {
        (key.local == source.local && interner.resolve(key.plugin).is_some_and(|plugin| plugin.eq_ignore_ascii_case(&source.plugin))).then_some(target)
    }).ok_or_else(|| format!("unmapped held weapon {source}"))?;
    Ok(CreatureTargetRecordReference {
        signature: "WEAP".to_string(),
        form_key: TargetFormKey::new(target.local, interner.resolve(target.plugin).unwrap()),
    })
}

pub(crate) fn install_projectile_node_aliases(
    nif: &mut NifFile,
    nodes: &[String],
) -> Result<usize, String> {
    let identity = NifValue::Matrix33([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    let mut added = 0;
    for node in nodes {
        let matching = nif
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.fields.get("Name") == Some(&NifValue::String(node.clone())))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let [parent] = matching.as_slice() else {
            return Err(format!(
                "source embedded weapon node {node} must occur exactly once, found {}",
                matching.len()
            ));
        };
        if node == "ProjectileNode" {
            continue;
        }
        let children = match nif.blocks[*parent].fields.get("Children") {
            Some(NifValue::Array(children)) => children.clone(),
            _ => {
                return Err(format!(
                    "source embedded weapon node {node} has no NiNode children"
                ));
            }
        };
        let existing = children
            .iter()
            .filter_map(|value| match value {
                NifValue::Ref(index) => usize::try_from(*index)
                    .ok()
                    .and_then(|index| nif.blocks.get(index)),
                _ => None,
            })
            .find(|block| {
                block.fields.get("Name") == Some(&NifValue::String("ProjectileNode".into()))
            });
        if let Some(existing) = existing {
            if existing.fields.get("Translation") != Some(&NifValue::Vec3([0.0; 3]))
                || existing.fields.get("Rotation") != Some(&identity)
                || existing.fields.get("Scale") != Some(&NifValue::Float(1.0))
            {
                return Err(format!(
                    "source embedded weapon node {node} already has an offset ProjectileNode child"
                ));
            }
            continue;
        }
        let child = nif.add_block("NiNode", None);
        nif.blocks[child].set_field("Name", NifValue::String("ProjectileNode".to_string()));
        nif.blocks[child].set_field("Translation", NifValue::Vec3([0.0; 3]));
        nif.blocks[child].set_field("Rotation", identity.clone());
        nif.blocks[child].set_field("Scale", NifValue::Float(1.0));
        let mut children = children;
        children.push(NifValue::Ref(child as i32));
        nif.blocks[*parent].set_field("Num Children", NifValue::UInt(children.len() as u64));
        nif.blocks[*parent].set_field("Children", NifValue::Array(children));
        added += 1;
    }
    Ok(added)
}

#[cfg(test)]
mod tests;
