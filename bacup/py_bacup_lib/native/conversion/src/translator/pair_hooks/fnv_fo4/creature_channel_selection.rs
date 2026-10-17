use super::*;
use crate::source_rig::SourceAnimationClip;

pub(super) fn collect(
    family: &CreatureFamilyRecipeJob,
    clips: &[SelectedClip],
    overlays: &[SelectedOverlay],
) -> Result<Vec<SourceAnimationClip>, String> {
    let motion = family
        .indexed_motion_evidence
        .as_ref()
        .ok_or("source channels require KF evidence")?;
    let mut channels = Vec::new();
    for (name, candidate, override_group) in clips
        .iter()
        .map(|clip| {
            (
                &clip.name,
                &clip.candidate,
                clip.source_idle.as_ref().and_then(|generator| {
                    if let CapabilityRoleGenerator::SourceIdle { source_group, .. } = generator {
                        Some(*source_group)
                    } else {
                        None
                    }
                }),
            )
        })
        .chain(
            overlays
                .iter()
                .map(|clip| (&clip.clip_name, &clip.candidate, None)),
        )
    {
        let channel = override_group
            .map(|group| match group {
                20 => 1,
                21 => 4,
                other => other,
            })
            .or_else(|| {
                super::super::creature_weapon_attacks::exact_source_animation_channel(
                    &candidate.source_kf,
                    &candidate.sequence_name,
                )
            })
            .ok_or_else(|| {
                format!(
                    "{} ({}) has no native source animation channel",
                    candidate.source_kf, candidate.sequence_name
                )
            })?;
        let sequence = motion
            .kf_evidence
            .iter()
            .find_map(|entry| match &entry.sequence {
                KfParseEvidence::Parsed(sequence)
                    if canonical_mesh_path(&entry.source_kf)
                        == canonical_mesh_path(&candidate.source_kf)
                        && sequence.sequence_index == candidate.sequence_index =>
                {
                    Some(sequence)
                }
                _ => None,
            })
            .ok_or_else(|| {
                format!(
                    "{} has no exact source sequence priorities",
                    candidate.source_kf
                )
            })?;
        if sequence
            .binding
            .transform_priorities
            .values()
            .any(|priority| *priority == 255)
        {
            return Err(format!(
                "{} has an unresolved source transform priority",
                candidate.source_kf
            ));
        }
        channels.push(SourceAnimationClip {
            clip_name: name.clone(),
            channel,
            accumulation_root: match &sequence.root_motion {
                RootMotionEvidence::Planar { accum_root, .. } => Some(accum_root.clone()),
                RootMotionEvidence::Stationary { accum_root } => accum_root.clone(),
                _ => None,
            },
            transform_priorities: sequence.binding.transform_priorities.clone(),
        });
    }
    Ok(channels)
}
