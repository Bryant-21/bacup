use super::super::creature_attack_chains::{first_source_next_attack, source_default_attack_group};
use super::super::creature_weapon_attacks::{SourceAttackAim, exact_source_attack_group};
use super::*;
use crate::source_rig::{CapabilityBlenderChild, SourceAttackContract, SourceAttackWindow};

pub(super) fn attach(
    family: &CreatureFamilyRecipeJob,
    clips: &mut Vec<SelectedClip>,
) -> Result<(), String> {
    clips.retain(|clip| {
        !(clip.role == CreatureClipRole::MeleeAttack
            && clip.source_weapon.is_none()
            && exact_source_attack_group(&clip.candidate.source_kf, &clip.candidate.sequence_name)
                .is_some_and(|(group, _)| (97..=101).contains(&group)))
    });
    let Some(motion) = &family.indexed_motion_evidence else {
        return Ok(());
    };
    for clip in clips.iter_mut().filter(|clip| {
        matches!(
            clip.role,
            CreatureClipRole::MeleeAttack | CreatureClipRole::ProjectileAttack
        )
    }) {
        let Some((group, _)) =
            exact_source_attack_group(&clip.candidate.source_kf, &clip.candidate.sequence_name)
        else {
            continue;
        };
        let weapon = clip.source_weapon.as_ref().and_then(|source| {
            motion
                .weapon_attacks
                .iter()
                .find(|weapon| &weapon.source == source)
        });
        let configured = weapon.map_or(255, |weapon| weapon.animation.attack_animation);
        let power = (92..=96).contains(&group);
        let identity = format!(
            "{}:{:?}:{:?}:{}",
            family.rig.skeleton_path, family.rig.game, clip.source_weapon, power
        );
        let gate_variable = format!(
            "B21SourceAttack_{}",
            &blake3::hash(identity.as_bytes()).to_hex()[..20]
        );
        let sequence = motion
            .kf_evidence
            .iter()
            .find_map(|entry| match &entry.sequence {
                KfParseEvidence::Parsed(sequence)
                    if canonical_mesh_path(&entry.source_kf)
                        == canonical_mesh_path(&clip.candidate.source_kf)
                        && sequence.sequence_index == clip.candidate.sequence_index =>
                {
                    Some(sequence)
                }
                _ => None,
            })
            .ok_or_else(|| {
                format!(
                    "{} has no exact source attack sequence",
                    clip.candidate.source_kf
                )
            })?;
        let speed = weapon.map_or(1.0, |weapon| {
            weapon.animation.animation_multiplier * weapon.animation.attack_multiplier
        });
        let window = if power {
            None
        } else if let Some(next) = first_source_next_attack(&sequence.text_keys, configured) {
            let end = sequence
                .text_keys
                .iter()
                .find(|key| key.text.eq_ignore_ascii_case("End"))
                .map(|key| key.time)
                .ok_or_else(|| {
                    format!(
                        "{} has a continuation key without End",
                        clip.candidate.source_kf
                    )
                })?;
            let mut start = (next.time - sequence.start_time) / sequence.frequency / speed;
            let mut end = (end - sequence.start_time) / sequence.frequency / speed;
            if let Some(automatic) = &clip.automatic_attack {
                if automatic.segment.repeat_count != 1
                    || start < automatic.segment.start_seconds
                    || end > automatic.segment.end_seconds
                {
                    clip.source_attack = Some(SourceAttackContract {
                        gate_variable,
                        group: i32::from(group),
                        opening_group: i32::from(source_default_attack_group(configured)),
                        opening_variable: (configured == 255)
                            .then(|| "B21SourceMovementAttackGroup".to_string()),
                        window: None,
                        aim_children: Vec::new(),
                    });
                    continue;
                }
                start -= automatic.segment.start_seconds;
                end -= automatic.segment.start_seconds;
            }
            if !start.is_finite() || !end.is_finite() || start < 0.0 || end < 0.0 {
                return Err(format!(
                    "{} has an invalid source continuation window",
                    clip.candidate.source_kf
                ));
            }
            (end > start).then_some(SourceAttackWindow {
                start_bits: (start as f32).to_bits(),
                end_bits: (end as f32).to_bits(),
                next_group: i32::from(next.group),
            })
        } else {
            None
        };
        clip.source_attack = Some(SourceAttackContract {
            gate_variable,
            group: if power { 0 } else { i32::from(group) },
            opening_group: if power {
                0
            } else {
                i32::from(source_default_attack_group(configured))
            },
            opening_variable: (!power && configured == 255)
                .then(|| "B21SourceMovementAttackGroup".to_string()),
            window,
            aim_children: Vec::new(),
        });
    }
    let original = clips.clone();
    let mut children = Vec::new();
    for clip in clips.iter_mut() {
        let Some((group, aim)) =
            exact_source_attack_group(&clip.candidate.source_kf, &clip.candidate.sequence_name)
        else {
            continue;
        };
        if clip.source_attack.is_none() {
            continue;
        }
        if aim != SourceAttackAim::Center {
            if !original.iter().any(|candidate| {
                candidate.source_weapon == clip.source_weapon
                    && candidate
                        .automatic_attack
                        .as_ref()
                        .map(|attack| attack.segment.phase)
                        == clip
                            .automatic_attack
                            .as_ref()
                            .map(|attack| attack.segment.phase)
                    && exact_source_attack_group(
                        &candidate.candidate.source_kf,
                        &candidate.candidate.sequence_name,
                    ) == Some((group, SourceAttackAim::Center))
            }) {
                return Err(format!(
                    "{} has a directional weapon channel without its source center sequence",
                    clip.candidate.source_kf
                ));
            }
            clip.attack_blend_only = true;
            continue;
        }
        let phase = |clip: &SelectedClip| {
            clip.automatic_attack
                .as_ref()
                .map(|automatic| automatic.segment.phase)
        };
        let mut samples = Vec::new();
        for (direction, pitch) in [
            (SourceAttackAim::Up, -std::f32::consts::FRAC_PI_2),
            (SourceAttackAim::Center, 0.0),
            (SourceAttackAim::Down, std::f32::consts::FRAC_PI_2),
        ] {
            if direction == SourceAttackAim::Center {
                samples.push(sample(clip.name.clone(), pitch));
                continue;
            }
            let possible = original
                .iter()
                .filter(|candidate| {
                    candidate.source_weapon == clip.source_weapon
                        && phase(candidate) == phase(clip)
                        && exact_source_attack_group(
                            &candidate.candidate.source_kf,
                            &candidate.candidate.sequence_name,
                        ) == Some((group, direction))
                })
                .collect::<Vec<_>>();
            if possible.is_empty() {
                continue;
            }
            let matching = possible
                .iter()
                .copied()
                .filter(|candidate| {
                    variant(&candidate.candidate.source_kf) == variant(&clip.candidate.source_kf)
                })
                .collect::<Vec<_>>();
            let selected = if matching.len() == 1 {
                matching[0]
            } else if possible.len() == 1 {
                possible[0]
            } else {
                return Err(format!(
                    "{} has ambiguous source {direction:?} weapon channels",
                    clip.candidate.source_kf
                ));
            };
            let mut child = selected.clone();
            child.name = format!("{}_{direction:?}", clip.name);
            child.attack_blend_only = true;
            child.source_attack = None;
            samples.push(sample(child.name.clone(), pitch));
            children.push(child);
        }
        if samples.len() > 1 {
            clip.source_attack.as_mut().unwrap().aim_children = samples;
        }
    }
    clips.retain(|clip| !clip.attack_blend_only);
    clips.extend(children);
    Ok(())
}

fn sample(clip_name: String, pitch: f32) -> CapabilityBlenderChild {
    CapabilityBlenderChild {
        clip_name,
        weight_bits: pitch.to_bits(),
        world_from_model_weight_bits: 1.0_f32.to_bits(),
    }
}

fn variant(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .split_once('_')
        .map_or("", |(_, suffix)| suffix)
}
