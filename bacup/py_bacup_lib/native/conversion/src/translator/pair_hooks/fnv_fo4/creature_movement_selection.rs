use super::*;
use crate::source_rig::{SourceMovementClip, SourceMovementKey};

pub(super) fn attach(
    family: &CreatureFamilyRecipeJob,
    clips: &mut [SelectedClip],
) -> Result<(), String> {
    if !clips.iter().any(|clip| clip.source_attack.as_ref().is_some_and(|attack| attack.opening_variable.is_some())) {
        return Ok(());
    }
    let Some(motion) = &family.indexed_motion_evidence else { return Ok(()); };
    for clip in clips {
        if !super::super::creature_weapon_attacks::is_source_movement_sequence(&clip.candidate.source_kf) {
            continue;
        }
        let (source_order, sequence) = motion.kf_evidence.iter().enumerate().find_map(|(index, entry)| {
            match &entry.sequence {
                KfParseEvidence::Parsed(sequence)
                    if canonical_mesh_path(&entry.source_kf) == canonical_mesh_path(&clip.candidate.source_kf)
                        && sequence.sequence_index == clip.candidate.sequence_index => Some((index, sequence)),
                _ => None,
            }
        }).ok_or_else(|| format!("{} has no exact source movement sequence", clip.candidate.source_kf))?;
        if !sequence.start_time.is_finite() || !sequence.frequency.is_finite() || sequence.frequency <= 0.0 {
            return Err(format!("{} has an invalid movement clock", clip.candidate.source_kf));
        }
        let keys = movement_keys(&sequence.text_keys, sequence.start_time, sequence.frequency)?;
        clip.source_movement = Some(SourceMovementClip {
            clip_name: clip.name.clone(),
            source_order: u32::try_from(source_order).map_err(|_| "source movement order exceeds u32")?,
            keys,
        });
    }
    Ok(())
}

fn movement_keys(
    keys: &[super::super::creature_motion::TextKeyEvidence],
    start: f64,
    frequency: f64,
) -> Result<Vec<SourceMovementKey>, String> {
    keys.iter().filter(|key| key.text.as_bytes().get(..2).is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"m:")))
        .map(|key| {
            let time = ((key.time - start) / frequency) as f32;
            if !time.is_finite() { return Err("source movement key has nonfinite local time".to_string()); }
            Ok(SourceMovementKey {
                time_bits: time.to_bits(),
                group: if key.text.as_bytes().get(2).is_some_and(|suffix| suffix.eq_ignore_ascii_case(&b'l')) { 26 } else { 32 },
            })
        }).collect()
}

pub(super) fn wrap(selected: &SelectedGraph, clip: &SelectedClip, generator: CapabilityRoleGenerator) -> CapabilityRoleGenerator {
    let names = generator.clip_names(&clip.name);
    let clips = selected.clips.iter().filter_map(|sample| sample.source_movement.as_ref())
        .filter(|sample| names.contains(&sample.clip_name.as_str())).cloned().collect::<Vec<_>>();
    if clips.is_empty() { generator } else { CapabilityRoleGenerator::SourceMovement { clips, generator:Box::new(generator) } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::creature_motion::TextKeyEvidence;

    #[test]
    fn movement_metadata_preserves_array_order_literal_suffix_and_source_clock() {
        let keys = [(3.0, "M:L"), (2.0, "m: L"), (4.0, "m:r"), (1.5, " m:l"), (1.6, "Hit\nm:l")]
            .map(|(time, text)| TextKeyEvidence { time, text:text.to_string() });
        let actual = movement_keys(&keys, 1.0, 2.0).unwrap();
        assert_eq!(actual.iter().map(|key| (f32::from_bits(key.time_bits), key.group)).collect::<Vec<_>>(), vec![(1.0,26), (0.5,32), (1.5,32)]);
        assert!(movement_keys(&[TextKeyEvidence {time:f64::INFINITY, text:"m:l".into()}], 0.0, 1.0).is_err());
    }
}
