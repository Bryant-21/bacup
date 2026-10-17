use super::*;
use crate::phase::animations::{
    FnvKfBindingCompatibility, FnvKfBindingEvidence, FnvKfCycle, FnvKfRootMotionEvidence,
    FnvKfTextKeyEvidence, ParsedFnvCreatureKf,
    fnv_creature_automatic::{AutomaticAttackSegment, plan},
};
use crate::source_rig::ContinuousAttackPhase;

#[derive(Clone)]
pub(super) struct SelectedAutomaticAttack {
    pub(super) source_weapon: StableFormKey,
    pub(super) segment: AutomaticAttackSegment,
    pub(super) generator: CapabilityRoleGenerator,
}

pub(super) fn is_automatic(family: &CreatureFamilyRecipeJob, candidate: &MotionCandidate) -> bool {
    family
        .indexed_motion_evidence
        .as_ref()
        .is_some_and(|motion| {
            motion.weapon_attacks.iter().any(|weapon| {
                weapon.is_ranged()
                    && weapon.animation.automatic
                    && weapon.matches_clip(&candidate.source_kf, &candidate.sequence_name)
            })
        })
}

pub(super) fn expand(
    family: &CreatureFamilyRecipeJob,
    clips: &mut Vec<SelectedClip>,
) -> Result<(), String> {
    let Some(motion) = &family.indexed_motion_evidence else {
        return Ok(());
    };
    let mut expanded = Vec::new();
    for clip in clips.iter() {
        let weapons = motion
            .weapon_attacks
            .iter()
            .filter(|weapon| {
                ((clip.role == CreatureClipRole::ProjectileAttack && weapon.is_ranged())
                    || (clip.role == CreatureClipRole::MeleeAttack && !weapon.is_ranged()))
                    && weapon.matches_clip(&clip.candidate.source_kf, &clip.candidate.sequence_name)
            })
            .collect::<Vec<_>>();
        if weapons.is_empty() {
            expanded.push(clip.clone());
            continue;
        }
        if clip.role == CreatureClipRole::MeleeAttack && clip.candidate.source_kf.rsplit(['/', '\\']).next().is_some_and(|leaf| leaf.to_ascii_lowercase().starts_with("h2h")) {
            expanded.push(clip.clone());
        }
        let sequence = motion
            .kf_evidence
            .iter()
            .find_map(|entry| {
                if canonical_mesh_path(&entry.source_kf)
                    != canonical_mesh_path(&clip.candidate.source_kf)
                {
                    return None;
                }
                match &entry.sequence {
                    KfParseEvidence::Parsed(sequence)
                        if sequence.sequence_index == clip.candidate.sequence_index =>
                    {
                        Some(sequence)
                    }
                    _ => None,
                }
            })
            .ok_or_else(|| {
                format!(
                    "ranged creature clip {} lacks exact KF timing",
                    clip.candidate.source_kf
                )
            })?;
        let sequence = timing_evidence(&clip.candidate, sequence);
        for weapon in weapons {
            if matches!(weapon.animation.animation_type, 11 | 12) {
                return Err(format!("source mine {} requires a verified native placement action", weapon.source));
            }
            if matches!(weapon.animation.animation_type, 10 | 13) && !sequence.text_keys.iter().any(|key| key.text.trim().eq_ignore_ascii_case("release")) {
                return Err(format!("source thrown weapon {} clip {} has no Release key", weapon.source, clip.candidate.source_kf));
            }
            let plugin_hash = blake3::hash(weapon.source.plugin.as_bytes())
                .to_hex()
                .to_string();
            let suffix = format!("W{:06X}_{}", weapon.source.local, &plugin_hash[..8]);
            if !weapon.is_ranged() || !weapon.animation.automatic {
                let mut selected = clip.clone();
                selected.name = format!("{}_{}", clip.name, suffix);
                selected.state_name = format!("{}_{}", clip.state_name, suffix);
                selected.trigger_event = Some(format!(
                    "{}_{}",
                    clip.trigger_event
                        .as_deref()
                        .ok_or("ranged attack has no source trigger")?,
                    suffix
                ));
                selected.source_weapon = Some(weapon.source.clone());
                selected.thrown_attack = matches!(weapon.animation.animation_type, 10 | 13);
                expanded.push(selected);
                continue;
            }
            let segments = plan(&weapon.animation, &sequence)?;
            let state = |phase| format!("{}_{}_{phase:?}", clip.state_name, suffix);
            let start_state = segments
                .iter()
                .any(|segment| segment.phase == ContinuousAttackPhase::Start)
                .then(|| state(ContinuousAttackPhase::Start));
            let loop_state = state(ContinuousAttackPhase::Loop);
            let stop_state = segments
                .iter()
                .any(|segment| segment.phase == ContinuousAttackPhase::Stop)
                .then(|| state(ContinuousAttackPhase::Stop));
            for segment in segments {
                let mut selected = clip.clone();
                selected.name = format!("{}_{}_{:?}", clip.name, suffix, segment.phase);
                selected.state_name = state(segment.phase);
                selected.target_looping = segment.phase == ContinuousAttackPhase::Loop;
                let generator = CapabilityRoleGenerator::ContinuousAttackPhase {
                    phase: segment.phase,
                    start_state: start_state.clone(),
                    loop_state: loop_state.clone(),
                    stop_state: stop_state.clone(),
                };
                selected.trigger_event = Some(if generator.is_attack_entry() {
                    format!(
                        "{}_{}",
                        clip.trigger_event
                            .as_deref()
                            .ok_or("automatic attack has no source trigger")?,
                        suffix
                    )
                } else {
                    format!("B21_{}_Phase", selected.state_name)
                });
                selected.automatic_attack = Some(SelectedAutomaticAttack {
                    source_weapon: weapon.source.clone(),
                    segment,
                    generator,
                });
                selected.source_weapon = Some(weapon.source.clone());
                expanded.push(selected);
            }
        }
    }
    *clips = expanded;
    Ok(())
}

pub(crate) fn validate_automatic_phase_projection(
    family: &CreatureFamilyRecipeJob,
    candidate: &MotionCandidate,
    role: &CapabilityClipRole,
    clip: &ClipDecl,
) -> Result<bool, String> {
    let CapabilityRoleGenerator::ContinuousAttackPhase {
        phase,
        start_state,
        loop_state,
        stop_state,
    } = role.generator.base_generator()
    else {
        return Ok(false);
    };
    let motion = family
        .indexed_motion_evidence
        .as_ref()
        .ok_or("automatic attack has no source motion evidence")?;
    let sequence = motion
        .kf_evidence
        .iter()
        .find_map(|entry| {
            if canonical_mesh_path(&entry.source_kf) != canonical_mesh_path(&candidate.source_kf) {
                return None;
            }
            match &entry.sequence {
                KfParseEvidence::Parsed(sequence)
                    if sequence.sequence_index == candidate.sequence_index =>
                {
                    Some(sequence)
                }
                _ => None,
            }
        })
        .ok_or("automatic attack has no exact source sequence")?;
    let sequence = timing_evidence(candidate, sequence);
    for weapon in &motion.weapon_attacks {
        if !weapon.is_ranged()
            || !weapon.animation.automatic
            || !weapon.matches_clip(&candidate.source_kf, &candidate.sequence_name)
        {
            continue;
        }
        let hash = blake3::hash(weapon.source.plugin.as_bytes())
            .to_hex()
            .to_string();
        let suffix = format!("_W{:06X}_{}", weapon.source.local, &hash[..8]);
        let Some(base) = loop_state.strip_suffix(&format!("{suffix}_Loop")) else {
            continue;
        };
        let segments = plan(&weapon.animation, &sequence)?;
        let state = |phase| format!("{base}{suffix}_{phase:?}");
        let expected_start = segments
            .iter()
            .any(|part| part.phase == ContinuousAttackPhase::Start)
            .then(|| state(ContinuousAttackPhase::Start));
        let expected_stop = segments
            .iter()
            .any(|part| part.phase == ContinuousAttackPhase::Stop)
            .then(|| state(ContinuousAttackPhase::Stop));
        if start_state == &expected_start
            && stop_state == &expected_stop
            && role.state_name == state(*phase)
            && clip.looping == (*phase == ContinuousAttackPhase::Loop)
            && segments.iter().any(|part| part.phase == *phase)
        {
            return Ok(true);
        }
    }
    Err(format!(
        "automatic attack state {:?} differs from exact source WEAP/KF phases",
        role.state_name
    ))
}

fn timing_evidence(
    candidate: &MotionCandidate,
    sequence: &super::super::creature_motion::NiControllerSequenceEvidence,
) -> ParsedFnvCreatureKf {
    ParsedFnvCreatureKf {
        source_kf: candidate.source_kf.clone(),
        sequence_index: sequence.sequence_index,
        sequence_name: sequence.name.clone(),
        cycle: match sequence.cycle {
            super::super::creature_motion::SequenceCycle::Loop => FnvKfCycle::Loop,
            super::super::creature_motion::SequenceCycle::Reverse => FnvKfCycle::Reverse,
            super::super::creature_motion::SequenceCycle::Clamp => FnvKfCycle::Clamp,
        },
        start_time: sequence.start_time,
        stop_time: sequence.stop_time,
        frequency: sequence.frequency,
        text_keys: sequence
            .text_keys
            .iter()
            .map(|key| FnvKfTextKeyEvidence {
                time: key.time,
                text: key.text.clone(),
            })
            .collect(),
        binding: FnvKfBindingEvidence {
            skeleton_path: Some(sequence.binding.source_skeleton_path.clone()),
            transform_track_count: sequence.binding.transform_track_count,
            float_track_count: sequence.binding.float_track_count,
            controller_types: sequence.binding.controller_types.clone(),
            interpolator_types: sequence.binding.interpolator_types.clone(),
            target_names: sequence.binding.target_names.clone(),
            transform_priorities: sequence.binding.transform_priorities.clone(),
            transform_components: sequence.binding.transform_components.clone(),
            required_float_slots: sequence.binding.required_float_slots.clone(),
            compatibility: match sequence.binding.compatibility {
                super::super::creature_motion::BindingCompatibility::Verified => {
                    FnvKfBindingCompatibility::Verified
                }
                super::super::creature_motion::BindingCompatibility::Unverified => {
                    FnvKfBindingCompatibility::Unverified {
                        detail: sequence
                            .binding
                            .compatibility_detail
                            .clone()
                            .unwrap_or_default(),
                    }
                }
                super::super::creature_motion::BindingCompatibility::Incompatible => {
                    FnvKfBindingCompatibility::Incompatible {
                        detail: sequence
                            .binding
                            .compatibility_detail
                            .clone()
                            .unwrap_or_default(),
                    }
                }
            },
        },
        root_motion: match &sequence.root_motion {
            RootMotionEvidence::None => FnvKfRootMotionEvidence::None,
            RootMotionEvidence::Stationary { accum_root } => FnvKfRootMotionEvidence::Stationary {
                accum_root: accum_root.clone(),
            },
            RootMotionEvidence::Planar {
                accum_root,
                distance,
                yaw_radians,
            } => FnvKfRootMotionEvidence::Planar {
                accum_root: accum_root.clone(),
                distance: *distance,
                yaw_radians: *yaw_radians,
            },
            RootMotionEvidence::Unsupported { accum_root, detail } => {
                FnvKfRootMotionEvidence::Unsupported {
                    accum_root: accum_root.clone(),
                    detail: detail.clone(),
                }
            }
            RootMotionEvidence::Unknown => FnvKfRootMotionEvidence::Unsupported {
                accum_root: None,
                detail: "unknown source root motion".into(),
            },
        },
    }
}

#[cfg(test)]
#[path = "creature_automatic_selection_tests.rs"]
mod tests;
