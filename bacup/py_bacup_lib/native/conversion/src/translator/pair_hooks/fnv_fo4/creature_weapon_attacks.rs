use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ids::FormKey;
use crate::phase::animations::weapon_events::{
    WeaponAnimationFields, WeaponPerspective, decode_weapon_animation_fields,
    source_weapon_clip_candidates,
};
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

use super::creature_attack_chains::{first_source_next_attack, source_default_attack_group};
use super::creature_catalog::{
    CreatureCorpusPlan, CreatureDisposition, LegacyRecordSource, StableFormKey,
};
use super::creature_dependencies::{
    DependencyReferenceResolution, FnvFo3CreatureDependencyLedger, RuntimeDependencyRecordReceipt,
};
use super::creature_motion::CreatureMotionFamilyEvidence;

#[path = "creature_animation_channels.rs"]
mod animation_channels;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureWeaponAttackEvidence {
    pub source: StableFormKey,
    pub animation: WeaponAnimationFields,
    pub embedded_weapon_node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held_weapon_node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipment_group: Option<StableFormKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipment_index: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attack_animation_groups: Vec<u8>,
}

impl CreatureWeaponAttackEvidence {
    pub fn is_ranged(&self) -> bool {
        self.animation.animation_type >= 3
    }

    pub fn matches_clip(&self, source_kf: &str, sequence_name: &str) -> bool {
        let leaf = source_kf
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(source_kf)
            .to_ascii_lowercase();
        let stem = leaf.strip_suffix(".kf").unwrap_or(&leaf);
        if self.animation.animation_type <= 2
            && let Some((group @ 92..=96, _)) = exact_source_attack_group(source_kf, sequence_name)
            && let Some(prefix) = source_weapon_prefix(self.animation.animation_type)
        {
            return attack_variant_matches(
                stem,
                &format!("{prefix}{}", source_attack_group_name(group).unwrap()),
            );
        }
        let default_group = if self.animation.attack_animation == 255 {
            32
        } else {
            self.animation.attack_animation
        };
        let groups = if self.attack_animation_groups.is_empty() {
            std::slice::from_ref(&default_group)
        } else {
            &self.attack_animation_groups
        };
        groups.iter().any(|group| {
            let mut animation = self.animation.clone();
            animation.attack_animation = *group;
            if let Some(prefix) = match animation.animation_type {
                0 => Some("h2h"),
                1 => Some("1hm"),
                2 => Some("2hm"),
                10 | 13 => Some("1gt"),
                11 => Some("1lm"),
                12 => Some("1md"),
                _ => None,
            } {
                let Some(attack) = source_attack_group_name(*group) else {
                    return false;
                };
                return attack_variant_matches(stem, &format!("{prefix}{attack}"));
            }
            source_weapon_clip_candidates(&animation, WeaponPerspective::Third).is_ok_and(|clips| {
                clips.iter().any(|clip| {
                    clip.action.is_attack()
                        && clip.source_path.rsplit('/').next().is_some_and(|name| {
                            name.eq_ignore_ascii_case(&leaf)
                                || name.strip_suffix(".kf").is_some_and(|name| {
                                    attack_variant_matches(stem, name)
                                        || name.eq_ignore_ascii_case(sequence_name)
                                })
                        })
                })
            })
        })
    }
}

fn attack_variant_matches(stem: &str, expected: &str) -> bool {
    // The source KF loader strips named variants at the first underscore (FNV 0x43B851).
    let stem = stem.split('_').next().unwrap_or(stem);
    let Some(suffix) = stem.strip_prefix(expected) else {
        return false;
    };
    matches!(suffix, "" | "up" | "down")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceAttackAim {
    Center,
    Up,
    Down,
}

pub(crate) fn exact_source_attack_group(
    source_kf: &str,
    sequence_name: &str,
) -> Option<(u8, SourceAttackAim)> {
    [source_kf, sequence_name].into_iter().find_map(|name| {
        let leaf = name.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
        let stem = leaf.strip_suffix(".kf").unwrap_or(&leaf);
        let stem = stem.split('_').next()?;
        let attack = [
            "h2h", "1hm", "2hm", "1hp", "2hr", "2ha", "2hh", "2hl", "1gt", "1lm", "1md",
        ]
        .into_iter()
        .find_map(|prefix| stem.strip_prefix(prefix))?;
        (0..=u8::MAX).find_map(|group| {
            let name = source_attack_group_name(group)?;
            let aim = match attack.strip_prefix(name)? {
                "" => SourceAttackAim::Center,
                "up" => SourceAttackAim::Up,
                "down" => SourceAttackAim::Down,
                _ => return None,
            };
            Some((group, aim))
        })
    })
}

fn source_attack_group_name(group: u8) -> Option<&'static str> {
    match group {
        26 => Some("attackleft"),
        32 => Some("attackright"),
        38 => Some("attack3"),
        44 => Some("attack4"),
        50 => Some("attack5"),
        56 => Some("attack6"),
        62 => Some("attack7"),
        68 => Some("attack8"),
        74 => Some("attackloop"),
        80 => Some("attackspin"),
        86 => Some("attackspin2"),
        92 => Some("attackpower"),
        93 => Some("attackforwardpower"),
        94 => Some("attackbackpower"),
        95 => Some("attackleftpower"),
        96 => Some("attackrightpower"),
        97 => Some("attackcustom1power"),
        98 => Some("attackcustom2power"),
        99 => Some("attackcustom3power"),
        100 => Some("attackcustom4power"),
        101 => Some("attackcustom5power"),
        102 => Some("placemine"),
        108 => Some("placemine2"),
        114 => Some("attackthrow"),
        120 => Some("attackthrow2"),
        126 => Some("attackthrow3"),
        132 => Some("attackthrow4"),
        138 => Some("attackthrow5"),
        144 => Some("attack9"),
        150 => Some("attackthrow6"),
        156 => Some("attackthrow7"),
        162 => Some("attackthrow8"),
        _ => None,
    }
}

fn source_weapon_prefix(animation_type: u32) -> Option<&'static str> {
    match animation_type {
        0 => Some("h2h"),
        1 => Some("1hm"),
        2 => Some("2hm"),
        3 | 4 => Some("1hp"),
        5 | 7 => Some("2hr"),
        6 => Some("2ha"),
        8 => Some("2hh"),
        9 => Some("2hl"),
        10 | 13 => Some("1gt"),
        11 => Some("1lm"),
        12 => Some("1md"),
        _ => None,
    }
}

pub(super) fn weapon_attack_evidence(
    record: &Record,
    interner: &StringInterner,
) -> Result<CreatureWeaponAttackEvidence, String> {
    if record.sig.as_str() != "WEAP" {
        return Err("creature attack weapon is not WEAP".to_string());
    }
    let animation = decode_weapon_animation_fields(record, interner)?;
    if animation.animation_type > 13 {
        return Err(format!(
            "creature attack weapon has unknown animation type {}",
            animation.animation_type
        ));
    }
    Ok(CreatureWeaponAttackEvidence {
        source: stable(record.form_key, interner)?,
        animation,
        equipment_group: None,
        equipment_index: None,
        held_weapon_node: None,
        attack_animation_groups: Vec::new(),
        embedded_weapon_node: record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "NNAM")
            .and_then(|entry| match &entry.value {
                FieldValue::String(value) => interner.resolve(*value).map(str::to_string),
                FieldValue::Bytes(bytes) => {
                    std::str::from_utf8(bytes.split(|byte| *byte == 0).next().unwrap_or_default())
                        .ok()
                        .map(str::to_string)
                }
                _ => None,
            })
            .filter(|name| !name.is_empty()),
    })
}

pub(super) fn load_weapon_attack_evidence(
    catalog: &CreatureCorpusPlan,
    sources: &[LegacyRecordSource<'_>],
    dependency: &FnvFo3CreatureDependencyLedger,
    families: &mut [CreatureMotionFamilyEvidence],
    interner: &StringInterner,
) -> Result<(), String> {
    let mut winners = BTreeMap::new();
    for source in sources {
        let key = (
            source.provenance.game,
            stable(source.record.form_key, interner)?,
        );
        let current = winners.entry(key).or_insert((0, source.record));
        if source.provenance.precedence >= current.0 {
            *current = (source.provenance.precedence, source.record);
        }
    }
    let mut records_by_game = BTreeMap::<_, BTreeMap<_, _>>::new();
    for ((game, key), (_, record)) in winners {
        records_by_game.entry(game).or_default().insert(key, record);
    }
    let receipts = dependency
        .records
        .iter()
        .map(|receipt| {
            (
                (receipt.provenance.game, canonical_key(&receipt.source)),
                receipt,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut candidate_weapons = Vec::new();
    for family in families.iter_mut() {
        let records = records_by_game
            .get(&family.rig.game)
            .ok_or_else(|| "creature motion family has no source records".to_string())?;
        let creatures = records
            .iter()
            .filter(|(_, record)| record.sig.as_str() == "CREA")
            .map(|(key, record)| (key.clone(), *record))
            .collect::<BTreeMap<_, _>>();
        let owners = catalog
            .rig_families
            .iter()
            .find(|rig| rig.key == family.rig)
            .ok_or_else(|| "motion family has no source rig".to_string())?;
        let mut family_weapons = BTreeSet::new();
        let game_receipts = receipts
            .iter()
            .filter(|((game, _), _)| *game == family.rig.game)
            .map(|((_, key), receipt)| (key.clone(), *receipt))
            .collect::<BTreeMap<_, _>>();
        for creature in &catalog.records {
            let owner = match &creature.disposition {
                CreatureDisposition::VisualOwner { rig, .. } if rig == &family.rig => None,
                CreatureDisposition::Proxy {
                    terminal_visual_owners,
                    ..
                } if terminal_visual_owners
                    .iter()
                    .any(|owner| owners.members.contains(owner)) =>
                {
                    terminal_visual_owners
                        .first()
                        .filter(|_| terminal_visual_owners.len() == 1)
                }
                _ => continue,
            };
            let (inventory_source, _) =
                super::creature_live_preparations::resolve_template_group_record(
                    &creature.source,
                    0x0100,
                    &creatures,
                    owner,
                    interner,
                )?;
            let (traits_source, _) =
                super::creature_live_preparations::resolve_template_group_record(
                    &creature.source,
                    0x0001,
                    &creatures,
                    owner,
                    interner,
                )?;
            if let (Some(inventory), Some(traits)) = (
                game_receipts.get(&canonical_key(&inventory_source)),
                game_receipts.get(&canonical_key(&traits_source)),
            ) {
                let direct = weapon_references(inventory, "CNTO")
                    .into_iter()
                    .filter(|key| {
                        records
                            .get(key)
                            .is_some_and(|record| record.sig.as_str() == "WEAP")
                    })
                    .collect::<Vec<_>>();
                let inventory_weapons =
                    super::creature_live_preparations::exact_inventory_weapon_sources(
                        inventory,
                        &direct,
                        &game_receipts,
                    )?;
                let usable = if family.rig.game == super::creature_catalog::LegacyCreatureGame::Fnv
                {
                    super::creature_live_preparations::exact_usable_inventory_weapon_sources(
                        &inventory_weapons,
                        traits,
                        &game_receipts,
                    )?
                } else {
                    inventory_weapons
                }
                .into_iter()
                .map(|key| canonical_key(&key))
                .collect::<BTreeSet<_>>();
                family_weapons.extend(usable.iter().cloned());
                candidate_weapons.push(usable);
            }
        }
        let mut evidence = Vec::new();
        for key in family_weapons {
            let record = records
                .get(&key)
                .ok_or_else(|| format!("usable source creature weapon {key} is absent"))?;
            evidence.push(weapon_attack_evidence(record, interner)?);
        }
        for weapon in &mut evidence {
            source_default_attack_groups(weapon, family);
            weapon.held_weapon_node = source_held_weapon_node(weapon, family)?;
        }
        evidence.sort_by(|left, right| left.source.cmp(&right.source));
        family.weapon_attacks = evidence;
    }
    assign_equipment_groups(families, &candidate_weapons)?;
    Ok(())
}

fn assign_equipment_groups(
    families: &mut [CreatureMotionFamilyEvidence],
    candidate_weapons: &[BTreeSet<StableFormKey>],
) -> Result<(), String> {
    let mut compatible = BTreeMap::<_, BTreeSet<StableFormKey>>::new();
    for weapon in families.iter().flat_map(|family| {
        family
            .weapon_attacks
            .iter()
            .filter(|weapon| family_uses_weapon(family, weapon))
    }) {
        if let Some(node) = &weapon.embedded_weapon_node {
            compatible
                .entry((
                    node.to_ascii_lowercase(),
                    weapon.animation.animation_type,
                    weapon.animation.attack_animation,
                ))
                .or_default()
                .insert(canonical_key(&weapon.source));
        }
    }
    let mut assigned = BTreeMap::new();
    for weapons in compatible.into_values() {
        let weapons = weapons.into_iter().collect::<Vec<_>>();
        let conflicts = weapons
            .iter()
            .map(|weapon| {
                weapons
                    .iter()
                    .enumerate()
                    .filter_map(|(index, other)| {
                        (weapon != other
                            && candidate_weapons
                                .iter()
                                .any(|owned| owned.contains(weapon) && owned.contains(other)))
                        .then_some(index)
                    })
                    .collect::<BTreeSet<_>>()
            })
            .collect::<Vec<_>>();
        let mut order = (0..weapons.len()).collect::<Vec<_>>();
        order.sort_by_key(|index| (std::cmp::Reverse(conflicts[*index].len()), *index));
        let colors = (1..=5)
            .find_map(|limit| {
                let mut colors = vec![None; weapons.len()];
                color_equipment_conflicts(&conflicts, &order, &mut colors, 0, limit)
                    .then_some(colors)
            })
            .ok_or_else(|| {
                "source creature equipment needs more than five concurrent embedded weapon slots"
                    .to_string()
            })?;
        let mut groups = BTreeMap::<usize, StableFormKey>::new();
        for (index, weapon) in weapons.iter().enumerate() {
            let group = groups
                .entry(colors[index].unwrap())
                .or_insert_with(|| weapon.clone());
            assigned.insert(weapon.clone(), group.clone());
        }
    }
    for family in families {
        let groups = family
            .weapon_attacks
            .iter()
            .filter(|weapon| family_uses_weapon(family, weapon))
            .filter_map(|weapon| assigned.get(&canonical_key(&weapon.source)).cloned())
            .collect::<BTreeSet<_>>();
        if groups.len() > 5 {
            return Err(format!(
                "creature rig {} needs {} embedded weapon equipment slots; FO4 supports five",
                family.rig.skeleton_path,
                groups.len()
            ));
        }
        let indexes = groups
            .into_iter()
            .enumerate()
            .map(|(index, group)| (group, index as u8))
            .collect::<BTreeMap<_, _>>();
        let active_weapons = family
            .weapon_attacks
            .iter()
            .filter(|weapon| family_uses_weapon(family, weapon))
            .map(|weapon| canonical_key(&weapon.source))
            .collect::<BTreeSet<_>>();
        for weapon in &mut family.weapon_attacks {
            if let Some(group) = assigned
                .get(&canonical_key(&weapon.source))
                .filter(|_| active_weapons.contains(&canonical_key(&weapon.source)))
            {
                weapon.equipment_group = Some(group.clone());
                weapon.equipment_index = Some(indexes[group]);
            }
        }
        assign_held_equipment_indices(family, indexes.len())?;
    }
    Ok(())
}

fn source_held_weapon_node(
    weapon: &CreatureWeaponAttackEvidence,
    family: &CreatureMotionFamilyEvidence,
) -> Result<Option<String>, String> {
    if weapon.embedded_weapon_node.is_some() {
        return Ok(None);
    }
    let Some(prefix) = source_weapon_prefix(weapon.animation.animation_type) else {
        return Ok(None);
    };
    let expected = format!("{prefix}equip.kf");
    let mut nodes = BTreeMap::new();
    for evidence in &family.kf_evidence {
        if !evidence
            .source_kf
            .rsplit(['/', '\\'])
            .next()
            .is_some_and(|leaf| leaf.eq_ignore_ascii_case(&expected))
        {
            continue;
        }
        let super::creature_motion::KfParseEvidence::Parsed(sequence) = &evidence.sequence else {
            continue;
        };
        for key in &sequence.text_keys {
            if let Some((kind, node)) = key.text.split_once(':')
                && kind.trim().eq_ignore_ascii_case("prn")
                && !node.trim().is_empty()
            {
                nodes.insert(node.trim().to_ascii_lowercase(), node.trim().to_string());
            }
        }
    }
    if nodes.len() > 1 {
        return Err(format!(
            "source held weapon {} has conflicting equip PRN nodes in {expected}",
            weapon.source
        ));
    }
    Ok(nodes.into_values().next())
}

fn assign_held_equipment_indices(
    family: &mut CreatureMotionFamilyEvidence,
    embedded_count: usize,
) -> Result<(), String> {
    let active = family
        .weapon_attacks
        .iter()
        .filter(|weapon| family_uses_weapon(family, weapon))
        .map(|weapon| canonical_key(&weapon.source))
        .collect::<BTreeSet<_>>();
    let mut ordinary = BTreeSet::new();
    let mut thrown = BTreeSet::new();
    for weapon in &family.weapon_attacks {
        if !active.contains(&canonical_key(&weapon.source)) {
            continue;
        }
        if let Some(node) = &weapon.held_weapon_node {
            if matches!(weapon.animation.animation_type, 10 | 13) {
                thrown.insert(node.to_ascii_lowercase());
            } else {
                ordinary.insert(node.to_ascii_lowercase());
            }
        }
    }
    let ordinary_slots = usize::from(!ordinary.is_empty()) * 2;
    let count = embedded_count + ordinary_slots + usize::from(!thrown.is_empty());
    if count > 5 {
        return Err(format!(
            "source rig {} requires {count} native equipment slots",
            family.rig.skeleton_path
        ));
    }
    for weapon in &mut family.weapon_attacks {
        if active.contains(&canonical_key(&weapon.source)) && weapon.held_weapon_node.is_some() {
            weapon.equipment_index = Some(
                (embedded_count
                    + if matches!(weapon.animation.animation_type, 10 | 13) {
                        ordinary_slots
                    } else {
                        0
                    }) as u8,
            );
        }
    }
    Ok(())
}

fn family_uses_weapon(
    family: &CreatureMotionFamilyEvidence,
    weapon: &CreatureWeaponAttackEvidence,
) -> bool {
    family
        .referenced_kfs
        .iter()
        .any(|path| weapon.matches_clip(path, ""))
}

fn color_equipment_conflicts(
    conflicts: &[BTreeSet<usize>],
    order: &[usize],
    colors: &mut [Option<usize>],
    next: usize,
    limit: usize,
) -> bool {
    let Some(&index) = order.get(next) else {
        return true;
    };
    let next_color = colors.iter().flatten().max().map_or(0, |color| color + 1);
    for color in 0..limit.min(next_color + 1) {
        if conflicts[index]
            .iter()
            .any(|other| colors[*other] == Some(color))
        {
            continue;
        }
        colors[index] = Some(color);
        if color_equipment_conflicts(conflicts, order, colors, next + 1, limit) {
            return true;
        }
        colors[index] = None;
    }
    false
}

fn source_default_attack_groups(
    weapon: &mut CreatureWeaponAttackEvidence,
    family: &CreatureMotionFamilyEvidence,
) {
    let configured = weapon.animation.attack_animation;
    let mut groups = BTreeSet::from([source_default_attack_group(configured)]);
    if configured == 255 && source_movement_opens_left(family) {
        groups.insert(26);
    }
    loop {
        weapon.attack_animation_groups = groups.iter().copied().collect();
        let before = groups.len();
        for clip in &family.kf_evidence {
            let super::creature_motion::KfParseEvidence::Parsed(sequence) = &clip.sequence else {
                continue;
            };
            if !weapon.matches_clip(&clip.source_kf, &sequence.name) {
                continue;
            }
            if let Some(next) = first_source_next_attack(&sequence.text_keys, configured) {
                groups.insert(next.group);
            }
        }
        if groups.len() == before {
            break;
        }
    }
}

fn source_movement_opens_left(family: &CreatureMotionFamilyEvidence) -> bool {
    family.kf_evidence.iter().any(|clip| {
        let super::creature_motion::KfParseEvidence::Parsed(sequence) = &clip.sequence else {
            return false;
        };
        is_source_movement_sequence(&clip.source_kf)
            && sequence.text_keys.iter().any(|key| {
                super::creature_attack_chains::next_source_movement_attack(
                    &sequence.text_keys,
                    key.time,
                    255,
                ) == 26
            })
    })
}

pub(super) fn is_source_movement_sequence(path: &str) -> bool {
    exact_source_animation_channel(path, "") == Some(1)
}

pub(super) fn exact_source_animation_channel(path: &str, sequence_name: &str) -> Option<u8> {
    [path, sequence_name].into_iter().find_map(|name| {
        let leaf = name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let stem = leaf
            .strip_suffix(".kf")
            .unwrap_or(&leaf)
            .split('_')
            .next()
            .unwrap_or_default();
        let stem = stem.strip_prefix("pa").unwrap_or(stem);
        let stem = ["sneak", "swim", "fly"]
            .into_iter()
            .find_map(|prefix| stem.strip_prefix(prefix))
            .unwrap_or(stem);
        let group = stem
            .strip_prefix("mt")
            .or_else(|| {
                (0..=13)
                    .filter_map(source_weapon_prefix)
                    .find_map(|prefix| stem.strip_prefix(prefix))
            })
            .unwrap_or(stem);
        animation_channels::SOURCE_ANIMATION_CHANNELS
            .iter()
            .find_map(|(channel, names)| {
                names
                    .split('|')
                    .any(|name| name == group)
                    .then_some(*channel)
            })
    })
}

#[cfg(test)]
fn exact_weapon_animation_for_clip<'a>(
    family: &'a CreatureMotionFamilyEvidence,
    source_kf: &str,
    sequence_name: &str,
) -> Result<Option<&'a WeaponAnimationFields>, String> {
    let mut matches = family
        .weapon_attacks
        .iter()
        .filter(|weapon| weapon.is_ranged() && weapon.matches_clip(source_kf, sequence_name));
    let Some(first) = matches.next() else {
        return Ok(None);
    };
    for other in matches {
        let left = &first.animation;
        let right = &other.animation;
        if left.automatic != right.automatic
            || left.animation_multiplier != right.animation_multiplier
            || left.attack_multiplier != right.attack_multiplier
            || (left.automatic
                && (left.fire_rate != right.fire_rate
                    || left.attack_shots_sec != right.attack_shots_sec))
        {
            return Err(format!(
                "creature clip {source_kf} has incompatible source weapon timing: {} and {}",
                first.source, other.source,
            ));
        }
    }
    Ok(Some(&first.animation))
}

fn stable(key: FormKey, interner: &StringInterner) -> Result<StableFormKey, String> {
    Ok(StableFormKey {
        local: key.local,
        plugin: interner
            .resolve(key.plugin)
            .ok_or("unresolved creature weapon plugin")?
            .to_ascii_lowercase(),
    })
}

fn canonical_key(key: &StableFormKey) -> StableFormKey {
    StableFormKey {
        local: key.local,
        plugin: key.plugin.to_ascii_lowercase(),
    }
}

fn weapon_references(receipt: &RuntimeDependencyRecordReceipt, field: &str) -> Vec<StableFormKey> {
    receipt
        .references
        .iter()
        .filter_map(|reference| {
            let DependencyReferenceResolution::Followed { signature } = &reference.resolution
            else {
                return None;
            };
            (matches!(signature.as_str(), "WEAP" | "FLST" | "LVLI")
                && reference
                    .source_locators
                    .iter()
                    .any(|locator| locator.starts_with(&format!("{field}["))))
            .then(|| canonical_key(&reference.target))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::creature_catalog::{LegacyCreatureGame, RigFamilyKey};
    use super::super::creature_motion::{
        MotionRole, build_creature_motion_catalog, parse_creature_kf_evidence,
    };
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    pub(super) fn weapon(interner: &StringInterner, local: u32, kind: u32, attack: u8) -> Record {
        let mut record = Record::new(
            SigCode(*b"WEAP"),
            FormKey {
                local,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        let mut dnam = vec![0; 204];
        dnam[..4].copy_from_slice(&kind.to_le_bytes());
        dnam[4..8].copy_from_slice(&1f32.to_le_bytes());
        dnam[41] = attack;
        dnam[60..64].copy_from_slice(&1f32.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::Bytes(dnam.into()),
        });
        record
    }

    #[test]
    fn creature_weapon_type_overrides_shared_attack_left_name() {
        let interner = StringInterner::new();
        let spit = weapon_attack_evidence(&weapon(&interner, 1, 9, 26), &interner).unwrap();
        let melee = weapon_attack_evidence(&weapon(&interner, 2, 0, 26), &interner).unwrap();
        assert!(spit.is_ranged());
        assert!(!melee.is_ranged());
        assert!(spit.matches_clip("creatures/centaur/2hlattackleft.kf", "AttackLeft"));
        assert!(!spit.matches_clip("creatures/centaur/h2hattackleft.kf", "AttackLeft"));
        assert!(!spit.matches_clip("creatures/centaur/2hlattackright.kf", "AttackRight"));
    }

    #[test]
    fn exact_attack_group_preserves_aim_and_owned_power_alternatives() {
        assert_eq!(
            exact_source_attack_group("special/2hrattackleftup_variant.kf", "AttackLeftUp"),
            Some((26, SourceAttackAim::Up))
        );
        assert_eq!(
            exact_source_attack_group("2hrattackleftdown.kf", "AttackLeftDown"),
            Some((26, SourceAttackAim::Down))
        );
        assert_eq!(
            exact_source_attack_group("2hrattackleftunknown.kf", "AttackLeftUnknown"),
            None
        );
        let interner = StringInterner::new();
        let melee = weapon_attack_evidence(&weapon(&interner, 1, 2, 32), &interner).unwrap();
        assert!(melee.matches_clip("2hmattackforwardpower.kf", "AttackForwardPower"));
        assert!(!melee.matches_clip("1hmattackforwardpower.kf", "AttackForwardPower"));
        assert!(!melee.matches_clip("2hmattackcustom1power.kf", "AttackCustom1Power"));
        let ranged = weapon_attack_evidence(&weapon(&interner, 2, 5, 32), &interner).unwrap();
        assert!(!ranged.matches_clip("2hrattackforwardpower.kf", "AttackForwardPower"));
    }

    #[test]
    fn source_movement_channel_accepts_native_stances_and_rejects_attack_names() {
        for path in [
            "mtfastforward_hurt.kf",
            "sneak2hhleft.kf",
            "swimmtturnright.kf",
            "pa2hrjumpland.kf",
            "FastForward.kf",
        ] {
            assert!(is_source_movement_sequence(path), "{path}");
        }
        for path in [
            "2hrattackforwardpower.kf",
            "mtidle.kf",
            "mt_hurtfastforward.kf",
            "forwardwalk.kf",
        ] {
            assert!(!is_source_movement_sequence(path), "{path}");
        }
    }

    #[test]
    #[ignore = "requires modkit CLI WEAP corpus and extracted Super Mutant power KFs"]
    fn real_supermutant_sledgehammer_admits_source_power_hit_timing() {
        let interner = StringInterner::new();
        let corpus: Vec<serde_json::Value> = serde_json::from_slice(
            &std::fs::read(std::env::temp_dir().join("b21_fnv_weapon_corpus.json")).unwrap(),
        )
        .unwrap();
        let source = corpus
            .iter()
            .find(|record| record["editor_id"] == "WeapSledgehammer")
            .unwrap();
        let bytes = source["dnam_hex"]
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        let mut record = weapon(&interner, 0x004351, 2, 32);
        record.fields[0].value = FieldValue::Bytes(bytes.into());
        let weapon = weapon_attack_evidence(&record, &interner).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../extracted/fnv/meshes");
        for suffix in ["power", "forwardpower", "leftpower", "rightpower"] {
            let path = format!("creatures/smspinebreaker/2hmattack{suffix}.kf");
            let parsed = parse_creature_kf_evidence(
                &std::fs::read(root.join(&path)).unwrap(),
                &path,
                LegacyCreatureGame::Fnv,
                None,
                None,
            )
            .unwrap();
            let super::super::creature_motion::KfParseEvidence::Parsed(sequence) = parsed.sequence
            else {
                panic!("source power sequence failed");
            };
            assert!(weapon.matches_clip(&path, &sequence.name), "{path}");
            assert!(
                sequence
                    .text_keys
                    .iter()
                    .any(|key| key.text.eq_ignore_ascii_case("hit")
                        && key.time >= sequence.start_time
                        && key.time <= sequence.stop_time),
                "{path}"
            );
            assert!(matches!(
                exact_source_attack_group(&path, &sequence.name),
                Some((92..=96, _))
            ));
        }
    }

    #[test]
    fn source_melee_and_thrown_weapon_groups_match_only_their_exact_kfs() {
        let interner = StringInterner::new();
        for (kind, group, prefix, attack) in [
            (0, 32, "h2h", "attackright"),
            (1, 26, "1hm", "attackleft"),
            (2, 32, "2hm", "attackright"),
            (10, 114, "1gt", "attackthrow"),
            (11, 102, "1lm", "placemine"),
            (12, 102, "1md", "placemine"),
            (13, 162, "1gt", "attackthrow8"),
        ] {
            let weapon =
                weapon_attack_evidence(&weapon(&interner, 1, kind, group), &interner).unwrap();
            for suffix in [
                "",
                "up",
                "down",
                "_a",
                "_b",
                "up_a",
                "down_b",
                "_chop",
                "_stab",
                "_uppercut",
            ] {
                assert!(weapon.matches_clip(
                    &format!("creatures/source/{prefix}{attack}{suffix}.kf"),
                    attack
                ));
            }
            assert!(!weapon.matches_clip(
                &format!("creatures/source/{prefix}{attack}power.kf"),
                attack
            ));
            assert!(!weapon.matches_clip(
                &format!("creatures/source/{prefix}{attack}custom.kf"),
                attack
            ));
            assert!(
                !weapon.matches_clip(&format!("creatures/source/unrelated{attack}.kf"), attack)
            );
        }
    }

    #[test]
    fn creature_weapon_timing_conflicts_cannot_silently_pick_first_variant() {
        let interner = StringInterner::new();
        let first = weapon_attack_evidence(&weapon(&interner, 1, 9, 26), &interner).unwrap();
        let mut other = first.clone();
        other.source.local = 2;
        let mut family = CreatureMotionFamilyEvidence {
            rig: RigFamilyKey {
                game: LegacyCreatureGame::Fnv,
                skeleton_path: "creatures/centaur/skeleton.nif".to_string(),
            },
            referenced_kfs: Vec::new(),
            kf_evidence: Vec::new(),
            creature_traits: Vec::new(),
            weapon_attacks: vec![first, other],
        };
        let path = "creatures/centaur/2hlattackleft.kf";
        assert!(
            exact_weapon_animation_for_clip(&family, path, "AttackLeft")
                .unwrap()
                .is_some()
        );
        family.weapon_attacks[1].animation.fire_rate = 4.0;
        family.weapon_attacks[1].animation.attack_shots_sec = 3.0;
        assert!(
            exact_weapon_animation_for_clip(&family, path, "AttackLeft")
                .unwrap()
                .is_some()
        );
        family.weapon_attacks[0].animation.automatic = true;
        family.weapon_attacks[1].animation.automatic = true;
        assert!(
            exact_weapon_animation_for_clip(&family, path, "AttackLeft")
                .unwrap_err()
                .contains("incompatible source weapon timing")
        );
        family.weapon_attacks[0].animation.automatic = false;
        family.weapon_attacks[1].animation.automatic = false;
        family.weapon_attacks[1].animation.attack_multiplier = 2.0;
        assert!(exact_weapon_animation_for_clip(&family, path, "AttackLeft").is_err());
    }

    #[test]
    fn embedded_equipment_groups_preserve_concurrency_and_merge_exclusive_variants() {
        let interner = StringInterner::new();
        let mut first = weapon_attack_evidence(&weapon(&interner, 1, 5, 80), &interner).unwrap();
        first.embedded_weapon_node = Some("ProjectileNode_RGatling".into());
        let mut exclusive_variant = first.clone();
        exclusive_variant.source.local = 2;
        let mut concurrent_variant = first.clone();
        concurrent_variant.source.local = 3;
        let mut missile = first.clone();
        missile.source.local = 4;
        missile.animation.animation_type = 9;
        missile.animation.attack_animation = 26;
        let mut unused_list_weapon = missile.clone();
        unused_list_weapon.source.local = 5;
        unused_list_weapon.animation.attack_animation = 32;
        unused_list_weapon.embedded_weapon_node = Some("ProjectileNode_HeadLaser".into());
        let mut families = vec![CreatureMotionFamilyEvidence {
            rig: RigFamilyKey {
                game: LegacyCreatureGame::Fnv,
                skeleton_path: "creatures/sentrybot/skeleton.nif".into(),
            },
            referenced_kfs: vec![
                "creatures/sentrybot/2hrattackspin.kf".into(),
                "creatures/sentrybot/2hlattackleft.kf".into(),
            ],
            kf_evidence: Vec::new(),
            creature_traits: Vec::new(),
            weapon_attacks: vec![
                first.clone(),
                exclusive_variant.clone(),
                concurrent_variant.clone(),
                missile.clone(),
                unused_list_weapon,
            ],
        }];
        let owned = vec![
            BTreeSet::from([
                first.source,
                concurrent_variant.source,
                missile.source.clone(),
            ]),
            BTreeSet::from([exclusive_variant.source, missile.source]),
        ];
        assign_equipment_groups(&mut families, &owned).unwrap();
        let weapons = &families[0].weapon_attacks;
        assert_eq!(weapons[0].equipment_group, weapons[1].equipment_group);
        assert_ne!(weapons[0].equipment_group, weapons[2].equipment_group);
        assert_ne!(weapons[0].equipment_group, weapons[3].equipment_group);
        assert_eq!(weapons[0].equipment_index, Some(0));
        assert_eq!(weapons[2].equipment_index, Some(1));
        assert_eq!(weapons[3].equipment_index, Some(2));
        assert_eq!(weapons[4].equipment_group, None);
        assert_eq!(weapons[4].equipment_index, None);
    }

    #[test]
    fn embedded_equipment_cannot_claim_support_for_six_concurrent_weapons() {
        let interner = StringInterner::new();
        let weapons = (1..=6)
            .map(|id| {
                let mut weapon =
                    weapon_attack_evidence(&weapon(&interner, id, 3, 32), &interner).unwrap();
                weapon.embedded_weapon_node = Some("ProjectileNode".into());
                weapon
            })
            .collect::<Vec<_>>();
        let owned = vec![weapons.iter().map(|weapon| weapon.source.clone()).collect()];
        let mut families = vec![CreatureMotionFamilyEvidence {
            rig: RigFamilyKey {
                game: LegacyCreatureGame::Fnv,
                skeleton_path: "creatures/test/skeleton.nif".into(),
            },
            referenced_kfs: vec!["creatures/test/1hpattackright.kf".into()],
            kf_evidence: Vec::new(),
            creature_traits: Vec::new(),
            weapon_attacks: weapons,
        }];
        assert!(
            assign_equipment_groups(&mut families, &owned)
                .unwrap_err()
                .contains("five")
        );
    }

    #[test]
    fn embedded_source_node_survives_weapon_evidence() {
        let interner = StringInterner::new();
        let mut record = weapon(&interner, 1, 9, 26);
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"NNAM"),
            value: FieldValue::String(interner.intern("ProjectileNode")),
        });
        assert_eq!(
            weapon_attack_evidence(&record, &interner)
                .unwrap()
                .embedded_weapon_node
                .as_deref(),
            Some("ProjectileNode")
        );
    }

    #[test]
    fn default_creature_attack_resolves_to_source_attack_right_group() {
        let interner = StringInterner::new();
        let attack = weapon_attack_evidence(&weapon(&interner, 1, 3, 255), &interner).unwrap();
        assert!(attack.matches_clip(
            "creatures/blowfly/specialanims/1hpattackright.kf",
            "AttackRight"
        ));
        assert!(attack.matches_clip(
            "creatures/blowfly/specialanims/1hpattackright_2.kf",
            "AttackRight_2"
        ));
        assert!(attack.matches_clip(
            "creatures/blowfly/specialanims/1hpattackrightup.kf",
            "AttackRightUp"
        ));
        assert!(!attack.matches_clip(
            "creatures/blowfly/specialanims/1hpattackleft.kf",
            "AttackLeft"
        ));
    }

    #[test]
    #[ignore = "requires extracted source Securitron KF corpus"]
    fn real_securitron_default_attack_preserves_only_reachable_source_groups() {
        let interner = StringInterner::new();
        let mut weapon =
            weapon_attack_evidence(&weapon(&interner, 0x10BA90, 8, 255), &interner).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../extracted/fnv/meshes");
        let paths = ["right", "left", "3", "4", "5", "6"]
            .map(|suffix| format!("creatures/nvsecuritron/2hhattack{suffix}.kf"));
        let family = CreatureMotionFamilyEvidence {
            rig: RigFamilyKey {
                game: LegacyCreatureGame::Fnv,
                skeleton_path: "creatures/nvsecuritron/skeleton.nif".into(),
            },
            referenced_kfs: paths.to_vec(),
            kf_evidence: paths
                .iter()
                .map(|path| {
                    parse_creature_kf_evidence(
                        &std::fs::read(root.join(path)).unwrap(),
                        path,
                        LegacyCreatureGame::Fnv,
                        None,
                        None,
                    )
                    .unwrap()
                })
                .collect(),
            creature_traits: Vec::new(),
            weapon_attacks: Vec::new(),
        };
        source_default_attack_groups(&mut weapon, &family);
        assert_eq!(weapon.attack_animation_groups, [26, 32, 38, 44, 50, 56]);
        assert!(!weapon.matches_clip("creatures/nvsecuritron/2hhattack7.kf", "Attack7"));
        assert!(!weapon.matches_clip("creatures/nvsecuritron/2hhattack8.kf", "Attack8"));
    }

    #[test]
    #[ignore = "requires the modkit CLI FNV weapon corpus and extracted Centaur KF"]
    fn real_centaur_spit_uses_ranged_motion_and_matching_source_weapon_timing() {
        let interner = StringInterner::new();
        let path = std::env::temp_dir().join("b21_fnv_weapon_corpus.json");
        let corpus: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let mut weapons = Vec::new();
        for source in corpus.iter().filter(|record| {
            matches!(
                record["editor_id"].as_str(),
                Some("CentaurSpitMissile" | "CentaurEvolvedSpitMissile")
            )
        }) {
            let mut record = weapon(&interner, weapons.len() as u32 + 1, 9, 26);
            let dnam = source["dnam_hex"]
                .as_str()
                .unwrap()
                .as_bytes()
                .chunks_exact(2)
                .map(|hex| u8::from_str_radix(std::str::from_utf8(hex).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            record.fields[0].value = FieldValue::Bytes(dnam.into());
            weapons.push(weapon_attack_evidence(&record, &interner).unwrap());
        }
        assert_eq!(weapons.len(), 2);
        let source_kf = "creatures/centaur/2hlattackleft.kf";
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let bytes = std::fs::read(root.join("extracted/fnv/meshes").join(source_kf)).unwrap();
        let evidence =
            parse_creature_kf_evidence(&bytes, source_kf, LegacyCreatureGame::Fnv, None, None)
                .unwrap();
        let family = CreatureMotionFamilyEvidence {
            rig: RigFamilyKey {
                game: LegacyCreatureGame::Fnv,
                skeleton_path: "creatures/centaur/skeleton.nif".to_string(),
            },
            referenced_kfs: vec![source_kf.to_string()],
            kf_evidence: vec![evidence],
            creature_traits: Vec::new(),
            weapon_attacks: weapons,
        };
        let motion = build_creature_motion_catalog(std::slice::from_ref(&family)).unwrap();
        assert!(
            motion.families[0]
                .role(MotionRole::MeleeAttack)
                .unwrap()
                .candidates
                .is_empty()
        );
        assert_eq!(
            motion.families[0]
                .role(MotionRole::RangedAttack)
                .unwrap()
                .candidates
                .len(),
            1
        );
        let fields = exact_weapon_animation_for_clip(&family, source_kf, "AttackLeft")
            .unwrap()
            .unwrap();
        assert!(!fields.automatic);
        assert_eq!(fields.attack_animation, 26);
        assert_eq!(fields.animation_type, 9);
        assert!((fields.attack_shots_sec - 1.5789474248886108).abs() < 1e-6);
    }

    #[test]
    fn held_equipment_indices_are_shared_across_creature_attachment_variants() {
        let interner = StringInterner::new();
        let mut family = CreatureMotionFamilyEvidence {
            rig: RigFamilyKey {
                game: LegacyCreatureGame::Fnv,
                skeleton_path: "nvdlc01/creatures/ghosts/skeleton.nif".into(),
            },
            referenced_kfs: ["1hmattackright", "2hmattackright", "1gtattackthrow"]
                .map(|name| format!("nvdlc01/creatures/ghosts/{name}.kf")).to_vec(),
            kf_evidence: Vec::new(),
            creature_traits: Vec::new(),
            weapon_attacks: Vec::new(),
        };
        for (id, kind, attack, node) in [
            (1, 1, 32, "Bip01 R Foretwist"),
            (2, 2, 32, "Bip01 R Hand"),
            (3, 10, 114, "Bip01 R Hand"),
        ] {
            let mut evidence = weapon_attack_evidence(
                &weapon(&interner, id, kind, attack), &interner,
            ).unwrap();
            evidence.held_weapon_node = Some(node.into());
            family.weapon_attacks.push(evidence);
        }
        assign_held_equipment_indices(&mut family, 0).unwrap();
        assert_eq!(family.weapon_attacks.iter().map(|weapon| weapon.equipment_index)
            .collect::<Vec<_>>(), [Some(0), Some(0), Some(2)]);
        assign_held_equipment_indices(&mut family, 2).unwrap();
        assert_eq!(family.weapon_attacks.iter().map(|weapon| weapon.equipment_index)
            .collect::<Vec<_>>(), [Some(2), Some(2), Some(4)]);
        assert!(assign_held_equipment_indices(&mut family, 3).unwrap_err()
            .contains("requires 6 native equipment slots"));
    }

    #[test]
    #[ignore = "requires extracted source Super Mutant equip KFs"]
    fn real_supermutant_equipment_uses_source_prn_and_distinct_grenade_slot() {
        let interner = StringInterner::new();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../extracted/fnv/meshes");
        let mut family = CreatureMotionFamilyEvidence {
            rig: RigFamilyKey {
                game: LegacyCreatureGame::Fnv,
                skeleton_path: "creatures/smspinebreaker/skeleton.nif".into(),
            },
            referenced_kfs: ["2hmattackright_a", "2hrattackright", "1gtattackthrow"]
                .map(|name| format!("creatures/smspinebreaker/{name}.kf"))
                .to_vec(),
            kf_evidence: Vec::new(),
            creature_traits: Vec::new(),
            weapon_attacks: Vec::new(),
        };
        for prefix in ["2hm", "2hr", "1gt"] {
            let path = format!("creatures/smspinebreaker/{prefix}equip.kf");
            family.kf_evidence.push(
                parse_creature_kf_evidence(
                    &std::fs::read(root.join(&path)).unwrap(),
                    &path,
                    LegacyCreatureGame::Fnv,
                    None,
                    None,
                )
                .unwrap(),
            );
        }
        for (id, kind, attack) in [(0x0CD50E, 2, 32), (0x004333, 5, 32), (0x004330, 10, 114)] {
            let mut evidence =
                weapon_attack_evidence(&weapon(&interner, id, kind, attack), &interner).unwrap();
            evidence.held_weapon_node = source_held_weapon_node(&evidence, &family).unwrap();
            assert_eq!(evidence.held_weapon_node.as_deref(), Some("Bip01 R Hand"));
            family.weapon_attacks.push(evidence);
        }
        assign_held_equipment_indices(&mut family, 0).unwrap();
        assert_eq!(
            family
                .weapon_attacks
                .iter()
                .map(|weapon| weapon.equipment_index)
                .collect::<Vec<_>>(),
            [Some(0), Some(0), Some(2)]
        );
    }
}
