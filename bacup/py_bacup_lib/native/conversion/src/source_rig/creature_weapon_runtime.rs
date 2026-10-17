use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{CreatureAttackRecordProjection, SourceRigExecutableRecipe, TargetFormKey};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct CreatureWeaponRuntimeCatalog {
    pub version: u32,
    pub actors: Vec<CreatureWeaponRuntimeActor>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct CreatureWeaponRuntimeActor {
    pub npc: TargetFormKey,
    pub race: TargetFormKey,
    pub attacks: Vec<CreatureWeaponRuntimeAttack>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub movement: Vec<CreatureMovementRuntimeClip>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct CreatureMovementRuntimeClip {
    pub animation: String,
    pub source_order: u32,
    pub keys: Vec<CreatureMovementRuntimeKey>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct CreatureMovementRuntimeKey {
    pub time: f32,
    pub group: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct CreatureWeaponRuntimeAttack {
    pub event: String,
    pub kind: CreatureWeaponRuntimeKind,
    pub equip_index: u8,
    pub weapons: Vec<TargetFormKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aim_bank: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_gate: Option<CreatureWeaponGraphGate>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct CreatureWeaponGraphGate {
    pub name: String,
    pub value: i32,
    #[serde(default)]
    pub movement_opening: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CreatureWeaponRuntimeKind {
    Natural,
    Melee,
    Ranged,
}

pub(crate) fn build_creature_weapon_runtime_catalog(
    recipes: &[SourceRigExecutableRecipe],
) -> Result<CreatureWeaponRuntimeCatalog, String> {
    let mut actors = BTreeMap::new();
    let mut races = BTreeMap::new();
    for recipe in recipes {
        let projection = &recipe.projection;
        let mut movement = BTreeMap::new();
        for source in recipe
            .graph
            .roles
            .iter()
            .flat_map(|role| role.generator.source_movement())
        {
            let clip = recipe
                .rig
                .clips
                .iter()
                .find(|clip| clip.name == source.clip_name)
                .ok_or_else(|| {
                    format!("source movement {} has no emitted clip", source.clip_name)
                })?;
            let keys = source
                .keys
                .iter()
                .map(|key| {
                    let time = f32::from_bits(key.time_bits);
                    if !time.is_finite() || !matches!(key.group, 26 | 32) {
                        return Err(format!(
                            "source movement {} has an invalid key",
                            source.clip_name
                        ));
                    }
                    Ok(CreatureMovementRuntimeKey {
                        time,
                        group: key.group,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let entry = CreatureMovementRuntimeClip {
                animation: clip.path.clone(),
                source_order: source.source_order,
                keys,
            };
            if let Some(previous) = movement.insert(source.clip_name.clone(), entry.clone())
                && previous != entry
            {
                return Err(format!(
                    "source movement {} has conflicting runtime descriptions",
                    source.clip_name
                ));
            }
        }
        let movement = movement.into_values().collect::<Vec<_>>();
        let mut attacks = Vec::new();
        for attack in &projection.attacks {
            let (kind, equipment) = match &attack.projection {
                CreatureAttackRecordProjection::MeleeUnarmed { .. } => {
                    (CreatureWeaponRuntimeKind::Natural, &[][..])
                }
                CreatureAttackRecordProjection::MeleeEquipment { equipment } => {
                    (CreatureWeaponRuntimeKind::Melee, equipment.as_slice())
                }
                CreatureAttackRecordProjection::RangedEquipment { equipment, .. } => {
                    (CreatureWeaponRuntimeKind::Ranged, equipment.as_slice())
                }
                _ => {
                    return Err(format!(
                        "legacy weapon catalog cannot represent attack {}",
                        attack.event
                    ));
                }
            };
            let equip_index = if kind == CreatureWeaponRuntimeKind::Natural {
                projection
                    .equipment_slots
                    .iter()
                    .position(|slot| {
                        slot.equip_slot.form_key.local == 0x013F42
                            && slot
                                .equip_slot
                                .form_key
                                .plugin
                                .eq_ignore_ascii_case("Fallout4.esm")
                    })
                    .unwrap_or(projection.equipment_slots.len())
            } else {
                let indexes = equipment
                    .iter()
                    .map(|weapon| {
                        let indexes = projection
                            .equipment_slots
                            .iter()
                            .enumerate()
                            .filter_map(|(index, slot)| {
                                slot.weapons.contains(weapon).then_some(index)
                            })
                            .collect::<Vec<_>>();
                        match indexes.as_slice() {
                            [index] => Ok(*index),
                            _ => Err(format!(
                                "attack {} weapon {:?} has {} equipment bindings",
                                attack.event,
                                weapon.form_key,
                                indexes.len()
                            )),
                        }
                    })
                    .collect::<Result<BTreeSet<_>, _>>()?;
                if indexes.len() != 1 {
                    return Err(format!(
                        "attack {} spans {} equipment indexes",
                        attack.event,
                        indexes.len()
                    ));
                }
                *indexes.first().unwrap()
            };
            let maximum = if kind == CreatureWeaponRuntimeKind::Natural {
                15
            } else {
                4
            };
            if equip_index > maximum {
                return Err(format!(
                    "attack {} uses unsupported equipment index {equip_index}",
                    attack.event
                ));
            }
            let mut weapons = equipment
                .iter()
                .map(|weapon| weapon.form_key.clone())
                .collect::<Vec<_>>();
            weapons.sort();
            weapons.dedup();
            let role = recipe
                .graph
                .roles
                .iter()
                .find(|role| {
                    role.trigger_event.as_deref() == Some(attack.event.as_str())
                        || role.trigger_aliases.contains(&attack.event)
                })
                .ok_or_else(|| format!("attack {} has no generated graph role", attack.event))?;
            let graph_gate =
                role.generator
                    .source_attack()
                    .map(|contract| CreatureWeaponGraphGate {
                        name: contract.gate_variable.clone(),
                        value: contract.group,
                        movement_opening: contract.opening_variable.is_some(),
                    });
            let aim_banks = recipe.graph.roles.iter().filter_map(|role| {
                match &role.generator {
                    super::CapabilityRoleGenerator::SourceAim { bank_index, attack_events, .. }
                        if attack_events.contains(&attack.event) => Some(*bank_index),
                    _ => None,
                }
            }).collect::<BTreeSet<_>>();
            if aim_banks.len() > 1 {
                return Err(format!("attack {} belongs to multiple source aim banks", attack.event));
            }
            attacks.push(CreatureWeaponRuntimeAttack {
                event: attack.event.clone(),
                kind,
                equip_index: equip_index as u8,
                weapons,
                aim_bank: aim_banks.first().copied(),
                graph_gate,
            });
        }
        attacks.sort_by(|left, right| left.event.cmp(&right.event));
        let race = projection.base.form_keys.race.clone();
        if let Some(previous) = races.insert(race.clone(), (attacks.clone(), movement.clone()))
            && previous != (attacks.clone(), movement.clone())
        {
            return Err(format!(
                "race {race:?} has conflicting native weapon attack profiles"
            ));
        }
        let npcs = std::iter::once(projection.base.form_keys.npc.clone()).chain(
            projection
                .variants
                .iter()
                .map(|variant| variant.form_key.clone()),
        );
        for npc in npcs {
            let actor = CreatureWeaponRuntimeActor {
                npc: npc.clone(),
                race: race.clone(),
                attacks: attacks.clone(),
                movement: movement.clone(),
            };
            if let Some(previous) = actors.insert(npc.clone(), actor.clone())
                && previous != actor
            {
                return Err(format!(
                    "NPC {npc:?} has conflicting native weapon attack profiles"
                ));
            }
        }
    }
    Ok(CreatureWeaponRuntimeCatalog {
        version: 1,
        actors: actors.into_values().collect(),
    })
}
