use std::collections::HashMap;

use crate::source_rig::ContinuousAttackPhase;

use super::legacy_weapons::{repeat_fire_clip, slice_clip};
use super::weapon_events::{WeaponAnimationFields, WeaponClipAction, build_weapon_event_contract};
use super::{AnimationClip, ParsedFnvCreatureKf};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AutomaticAttackSegment {
    pub(crate) phase: ContinuousAttackPhase,
    pub(crate) start_seconds: f64,
    pub(crate) end_seconds: f64,
    pub(crate) playback_speed: f64,
    pub(crate) repeat_count: usize,
    pub(crate) shot_interval_seconds: f64,
    pub(crate) equip_index: Option<u32>,
}

pub(crate) fn plan(
    fields: &WeaponAnimationFields,
    sequence: &ParsedFnvCreatureKf,
) -> Result<Vec<AutomaticAttackSegment>, String> {
    if !fields.automatic {
        return Err("creature continuous attack requires an automatic source weapon".into());
    }
    let contract =
        build_weapon_event_contract(fields, WeaponClipAction::Attack, sequence, &HashMap::new())?;
    let fire = contract
        .continuous_fire
        .ok_or("automatic source weapon has no fire contract")?;
    let duration = contract.duration_seconds / contract.playback_speed;
    let start = if fire.source_internal_loop {
        fire.loop_start_seconds
            .ok_or("internal creature fire loop has no start")?
            / contract.playback_speed
    } else {
        0.0
    };
    let end = fire.loop_end_seconds / contract.playback_speed;
    let segment = |phase, start_seconds, end_seconds, repeat_count| AutomaticAttackSegment {
        phase,
        start_seconds,
        end_seconds,
        repeat_count,
        playback_speed: contract.playback_speed,
        shot_interval_seconds: fire.shot_interval_seconds,
        equip_index: None,
    };
    let mut result = Vec::new();
    if start > 0.0 {
        result.push(segment(ContinuousAttackPhase::Start, 0.0, start, 1));
    }
    result.push(segment(
        ContinuousAttackPhase::Loop,
        start,
        end,
        fire.loop_repeat_count,
    ));
    if end < duration {
        result.push(segment(ContinuousAttackPhase::Stop, end, duration, 1));
    }
    Ok(result)
}

pub(super) fn apply(
    clip: &mut AnimationClip,
    segment: AutomaticAttackSegment,
) -> Result<(), String> {
    if segment.equip_index.is_some_and(|index| index > 4) {
        return Err(
            "creature weaponFire equipment index exceeds the supported attack slots".into(),
        );
    }
    scale_playback(clip, segment.playback_speed)?;
    *clip = slice_clip(
        clip,
        segment.start_seconds,
        segment.end_seconds,
        segment.phase == ContinuousAttackPhase::Stop,
    )?;
    clip.events.retain(|event| {
        let text = event.text.to_ascii_lowercase();
        !matches!(text.as_str(), "hit" | "fire" | "weaponfire") && !text.starts_with("weaponfire.")
    });
    if segment.phase == ContinuousAttackPhase::Loop {
        *clip = repeat_fire_clip(clip, segment.repeat_count, segment.shot_interval_seconds)?;
        if let Some(index) = segment.equip_index {
            for event in &mut clip.events {
                if event.text == "weaponFire" {
                    event.text = format!("weaponFire.{index}");
                }
            }
        }
        clip.cycle_type = "loop".into();
    } else {
        clip.cycle_type = "clamp".into();
    }
    Ok(())
}

pub(super) fn scale_playback(clip: &mut AnimationClip, speed: f64) -> Result<(), String> {
    if !speed.is_finite() || speed <= 0.0 {
        return Err("creature attack playback speed is invalid".into());
    }
    clip.duration /= speed;
    for channel in &mut clip.channels {
        for keys in [
            &mut channel.translations,
            &mut channel.rotations,
            &mut channel.scales,
        ] {
            for key in keys {
                key.time /= speed;
            }
        }
    }
    for channel in &mut clip.float_channels {
        for key in &mut channel.keyframes {
            key.time /= speed;
        }
    }
    for event in &mut clip.events {
        event.time /= speed;
    }
    Ok(())
}

#[cfg(test)]
#[path = "fnv_creature_automatic_tests.rs"]
mod tests;
