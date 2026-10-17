use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

use super::{FnvKfCycle, ParsedFnvCreatureKf};

pub(crate) const SOURCE_SPIN_START_EVENT: &str = "B21LegacyWeaponSpinStart";
pub(crate) const SOURCE_SPIN_STOP_EVENT: &str = "B21LegacyWeaponSpinStop";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct WeaponAnimationFields {
    pub(crate) animation_type: u32,
    pub(crate) grip_animation: u8,
    pub(crate) attack_animation: u8,
    pub(crate) reload_animation: u8,
    pub(crate) ammo_use: u8,
    pub(crate) automatic: bool,
    pub(crate) per_round_reload: bool,
    pub(crate) animation_multiplier: f64,
    pub(crate) attack_multiplier: f64,
    pub(crate) fire_rate: f64,
    pub(crate) attack_shots_sec: f64,
    pub(crate) reload_time: f64,
}

pub(crate) fn decode_weapon_animation_fields(
    record: &Record,
    interner: &StringInterner,
) -> Result<WeaponAnimationFields, String> {
    let value = &record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"DNAM")
        .ok_or("weapon is missing source DNAM")?
        .value;
    let (
        animation_type,
        grip,
        attack,
        reload,
        ammo_use,
        flags_1,
        _flags_2,
        animation_multiplier,
        attack_multiplier,
        fire_rate,
        attack_shots_sec,
        reload_time,
    ) = match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 96 => {
            let uint = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            let float = |offset| {
                f64::from(f32::from_le_bytes(
                    bytes[offset..offset + 4].try_into().unwrap(),
                ))
            };
            (
                uint(0),
                bytes[13],
                bytes[41],
                bytes[15],
                bytes[14],
                u32::from(bytes[12]),
                uint(56),
                float(4),
                float(60),
                float(64),
                float(88),
                float(92),
            )
        }
        FieldValue::Struct(fields) => {
            let number = |name: &str| -> Result<f64, String> {
                let value = fields
                    .iter()
                    .find(|(key, _)| interner.resolve(*key) == Some(name))
                    .map(|(_, value)| value)
                    .ok_or_else(|| format!("source DNAM is missing {name}"))?;
                match value {
                    FieldValue::Float(value) => Ok(f64::from(*value)),
                    FieldValue::Uint(value) => Ok(*value as f64),
                    FieldValue::Int(value) => Ok(*value as f64),
                    _ => Err(format!("source DNAM {name} is not numeric")),
                }
            };
            (
                number("animation_type")? as u32,
                number("grip_animation")? as u8,
                number("attack_animation")? as u8,
                number("reload_animation")? as u8,
                number("ammo_use")? as u8,
                number("flags_1")? as u32,
                number("flags_2")? as u32,
                number("animation_multiplier")?,
                number("animation_attack_multiplier")?,
                number("fire_rate")?,
                number("attack_shots_sec")?,
                number("reload_time")?,
            )
        }
        _ => return Err("source weapon DNAM is truncated or unsupported".into()),
    };
    for (name, value) in [
        ("animation_multiplier", animation_multiplier),
        ("attack_multiplier", attack_multiplier),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(format!("source {name} must be finite and positive"));
        }
    }
    for (name, value) in [
        ("fire_rate", fire_rate),
        ("attack_shots_sec", attack_shots_sec),
        ("reload_time", reload_time),
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(format!("source {name} must be finite and nonnegative"));
        }
    }
    Ok(WeaponAnimationFields {
        animation_type,
        grip_animation: grip,
        attack_animation: attack,
        reload_animation: reload,
        ammo_use,
        automatic: flags_1 & 2 != 0,
        // Reload W/X/Y/Z use Hit to end each loading cycle; flags2 mask 0x4 is NoJamAfterReload.
        per_round_reload: matches!(reload, 19..=22),
        animation_multiplier,
        attack_multiplier,
        fire_rate,
        attack_shots_sec,
        reload_time,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum WeaponClipAction {
    Aim,
    AimIronSights,
    Equip,
    Unequip,
    Attack,
    AttackIronSights,
    AttackLoop,
    AttackLoopIronSights,
    Reload,
    ReloadStart,
    ReloadEnd,
    HandGrip,
    SneakAim,
    SneakAimIronSights,
    SneakEquip,
    SneakUnequip,
    SneakAttack,
    SneakAttackIronSights,
    SneakAttackLoop,
    SneakAttackLoopIronSights,
    SneakReload,
}

impl WeaponClipAction {
    pub(crate) fn is_attack(self) -> bool {
        matches!(
            self,
            Self::Attack
                | Self::AttackIronSights
                | Self::AttackLoop
                | Self::AttackLoopIronSights
                | Self::SneakAttack
                | Self::SneakAttackIronSights
                | Self::SneakAttackLoop
                | Self::SneakAttackLoopIronSights
        )
    }

    pub(crate) fn is_reload(self) -> bool {
        matches!(self, Self::Reload | Self::SneakReload)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum WeaponPerspective {
    First,
    Third,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct WeaponClipCandidate {
    pub(crate) source_path: String,
    pub(crate) action: WeaponClipAction,
    pub(crate) playback_speed: f64,
    pub(crate) optional: bool,
}

pub(crate) fn source_weapon_clip_candidates(
    fields: &WeaponAnimationFields,
    perspective: WeaponPerspective,
) -> Result<Vec<WeaponClipCandidate>, String> {
    let prefix = match fields.animation_type {
        3 | 4 => "1hp",
        5 | 7 => "2hr",
        6 => "2ha",
        8 => "2hh",
        9 => "2hl",
        value => {
            return Err(format!(
                "gun KF selection does not support animation_type {value}"
            ));
        }
    };
    let attack = match fields.attack_animation {
        26 => "attackleft",
        32 => "attackright",
        38 => "attack3",
        44 => "attack4",
        50 => "attack5",
        56 => "attack6",
        62 => "attack7",
        68 => "attack8",
        74 => "attackloop",
        80 => "attackspin",
        86 => "attackspin2",
        144 => "attack9",
        value => {
            return Err(format!(
                "gun KF selection does not support attack_animation {value}"
            ));
        }
    };
    let root = match perspective {
        WeaponPerspective::First => "_1stperson",
        WeaponPerspective::Third => "_male",
    };
    let mut clips = Vec::new();
    let mut add = |stem: &str, action: WeaponClipAction, optional| {
        let speed = fields.animation_multiplier
            * if action.is_attack() {
                fields.attack_multiplier
            } else {
                1.0
            };
        clips.push(WeaponClipCandidate {
            source_path: format!("meshes/characters/{root}/{stem}.kf"),
            action,
            playback_speed: speed,
            optional,
        });
    };
    add(&format!("{prefix}aim"), WeaponClipAction::Aim, false);
    add(
        &format!("{prefix}aimis"),
        WeaponClipAction::AimIronSights,
        true,
    );
    add(&format!("{prefix}equip"), WeaponClipAction::Equip, false);
    add(
        &format!("{prefix}unequip"),
        WeaponClipAction::Unequip,
        false,
    );
    add(
        &format!("{prefix}{attack}"),
        if attack == "attackloop" {
            WeaponClipAction::AttackLoop
        } else {
            WeaponClipAction::Attack
        },
        false,
    );
    add(
        &format!("{prefix}{attack}is"),
        if attack == "attackloop" {
            WeaponClipAction::AttackLoopIronSights
        } else {
            WeaponClipAction::AttackIronSights
        },
        true,
    );
    if fields.automatic && attack != "attackloop" {
        add(
            &format!("{prefix}attackloop"),
            WeaponClipAction::AttackLoop,
            true,
        );
        add(
            &format!("{prefix}attackloopis"),
            WeaponClipAction::AttackLoopIronSights,
            true,
        );
    }
    if perspective == WeaponPerspective::Third {
        add(
            &format!("sneak{prefix}aim"),
            WeaponClipAction::SneakAim,
            true,
        );
        add(
            &format!("sneak{prefix}aimis"),
            WeaponClipAction::SneakAimIronSights,
            true,
        );
        add(
            &format!("sneak{prefix}equip"),
            WeaponClipAction::SneakEquip,
            true,
        );
        add(
            &format!("sneak{prefix}unequip"),
            WeaponClipAction::SneakUnequip,
            true,
        );
        add(
            &format!("sneak{prefix}{attack}"),
            if attack == "attackloop" {
                WeaponClipAction::SneakAttackLoop
            } else {
                WeaponClipAction::SneakAttack
            },
            true,
        );
        add(
            &format!("sneak{prefix}{attack}is"),
            if attack == "attackloop" {
                WeaponClipAction::SneakAttackLoopIronSights
            } else {
                WeaponClipAction::SneakAttackIronSights
            },
            true,
        );
        if fields.automatic && attack != "attackloop" {
            add(
                &format!("sneak{prefix}attackloop"),
                WeaponClipAction::SneakAttackLoop,
                true,
            );
            add(
                &format!("sneak{prefix}attackloopis"),
                WeaponClipAction::SneakAttackLoopIronSights,
                true,
            );
        }
    }
    if fields.ammo_use != 0 {
        let letter = match fields.reload_animation {
            0..=18 => char::from(b'a' + fields.reload_animation),
            19 => 'w',
            20 => 'x',
            21 => 'y',
            22 => 'z',
            value => {
                return Err(format!(
                    "gun KF selection does not support reload_animation {value}"
                ));
            }
        };
        let reload = format!("{prefix}reload{letter}");
        add(&reload, WeaponClipAction::Reload, false);
        if perspective == WeaponPerspective::Third {
            add(
                &format!("sneak{reload}"),
                WeaponClipAction::SneakReload,
                true,
            );
        }
        if fields.per_round_reload {
            add(
                &format!("{reload}start"),
                WeaponClipAction::ReloadStart,
                true,
            );
            add(&format!("{reload}end"), WeaponClipAction::ReloadEnd, true);
        }
    }
    // xNVSE's HandGrip getter treats unrecognized 172 as DEFAULT; retain authored finger tracks.
    if !matches!(fields.grip_animation, 255 | 172) {
        let grip = fields
            .grip_animation
            .checked_sub(229)
            .filter(|value| (1..=6).contains(value))
            .ok_or_else(|| format!("unsupported source hand grip {}", fields.grip_animation))?;
        add(
            &format!("{prefix}handgrip{grip}"),
            WeaponClipAction::HandGrip,
            false,
        );
    }
    Ok(clips)
}

pub(crate) fn resolve_weapon_clip_candidate(
    candidate: &WeaponClipCandidate,
    source_paths: &[String],
) -> Result<Option<WeaponClipCandidate>, String> {
    let wanted = candidate
        .source_path
        .replace('\\', "/")
        .to_ascii_lowercase();
    let stem = wanted.strip_suffix(".kf").unwrap_or(&wanted);
    let mut matches: Vec<_> = source_paths
        .iter()
        .filter(|path| {
            let path = path.replace('\\', "/").to_ascii_lowercase();
            path == wanted
                || (candidate.action == WeaponClipAction::HandGrip
                    && path.starts_with(&format!("{stem}_"))
                    && path.ends_with(".kf"))
        })
        .collect();
    if let Some(exact) = matches
        .iter()
        .find(|path| path.replace('\\', "/").eq_ignore_ascii_case(&wanted))
    {
        matches = vec![*exact];
    }
    match matches.as_slice() {
        [] if candidate.optional => Ok(None),
        [] => Err(format!(
            "required source weapon KF is missing: {}",
            candidate.source_path
        )),
        [path] => Ok(Some(WeaponClipCandidate {
            source_path: path.replace('\\', "/"),
            ..candidate.clone()
        })),
        _ => Err(format!(
            "source weapon KF selector is ambiguous: {}",
            candidate.source_path
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct WeaponAnnotation {
    pub(crate) time: f64,
    pub(crate) text: String,
    pub(crate) source_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct WeaponAttackTiming {
    pub(crate) attack_delay_seconds: f64,
    pub(crate) animation_attack_seconds: f64,
    pub(crate) refire_authority: String,
    pub(crate) combined_source_bolt_cycle: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ContinuousFireContract {
    pub(crate) shot_interval_seconds: f64,
    pub(crate) first_shot_seconds: Option<f64>,
    pub(crate) cadence_start_seconds: f64,
    pub(crate) loop_start_seconds: Option<f64>,
    pub(crate) loop_end_seconds: f64,
    pub(crate) source_internal_loop: bool,
    pub(crate) loop_repeat_count: usize,
    pub(crate) engine_cadence_required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ReloadContract {
    pub(crate) transfer_seconds: Vec<f64>,
    pub(crate) transfer_authority: String,
    pub(crate) per_round: bool,
    pub(crate) repeat_until_seconds: Option<f64>,
    pub(crate) exit_tail_start_seconds: Option<f64>,
    pub(crate) end_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct WeaponEventContract {
    pub(crate) annotations: Vec<WeaponAnnotation>,
    pub(crate) duration_seconds: f64,
    pub(crate) playback_speed: f64,
    pub(crate) fire_seconds: Vec<f64>,
    pub(crate) eject_seconds: Vec<f64>,
    pub(crate) refire_seconds: Option<f64>,
    pub(crate) attack_timing: Option<WeaponAttackTiming>,
    pub(crate) continuous_fire: Option<ContinuousFireContract>,
    pub(crate) reload: Option<ReloadContract>,
    pub(crate) unresolved_sounds: Vec<String>,
    pub(crate) unmapped_keys: Vec<String>,
}

pub(crate) fn build_weapon_event_contract(
    fields: &WeaponAnimationFields,
    action: WeaponClipAction,
    sequence: &ParsedFnvCreatureKf,
    sound_editor_ids: &HashMap<String, String>,
) -> Result<WeaponEventContract, String> {
    if !sequence.start_time.is_finite()
        || !sequence.stop_time.is_finite()
        || sequence.stop_time <= sequence.start_time
        || !sequence.frequency.is_finite()
        || sequence.frequency <= 0.0
    {
        return Err("weapon KF has invalid timing".into());
    }
    let attack = action.is_attack();
    let reload = action.is_reload();
    let duration = (sequence.stop_time - sequence.start_time) / sequence.frequency;
    let mut result = WeaponEventContract {
        annotations: Vec::new(),
        duration_seconds: duration,
        playback_speed: fields.animation_multiplier
            * if attack {
                fields.attack_multiplier
            } else {
                1.0
            },
        fire_seconds: Vec::new(),
        eject_seconds: Vec::new(),
        refire_seconds: None,
        attack_timing: None,
        continuous_fire: None,
        reload: None,
        unresolved_sounds: Vec::new(),
        unmapped_keys: Vec::new(),
    };
    let mut transfer_seconds = Vec::new();
    let mut loop_start = None;
    let mut internal_fire = None;
    for key in &sequence.text_keys {
        let time = (key.time - sequence.start_time) / sequence.frequency;
        if !time.is_finite() || time < -0.0001 || time > duration + 0.0001 {
            return Err(format!(
                "weapon text key {:?} falls outside the selected sequence",
                key.text
            ));
        }
        let time = time.clamp(0.0, duration);
        for text in key
            .text
            .lines()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            let mut emit = |mapped: &str| {
                result.annotations.push(WeaponAnnotation {
                    time,
                    text: mapped.to_string(),
                    source_text: Some(text.to_string()),
                })
            };
            let lower = text.to_ascii_lowercase();
            if let Some(source_sound) = lower.strip_prefix("sound:") {
                let source_sound = source_sound.trim();
                let target = sound_editor_ids
                    .iter()
                    .find(|(source, _)| source.eq_ignore_ascii_case(source_sound))
                    .map(|(_, target)| target)
                    .filter(|target| !target.trim().is_empty());
                if let Some(target) = target {
                    emit(&format!("SoundPlay.{target}"));
                } else {
                    result.unresolved_sounds.push(text[6..].trim().to_string());
                }
            } else if matches!(lower.as_str(), "hit" | "fire") && attack {
                if lower == "fire" {
                    internal_fire = Some(time);
                }
                result.fire_seconds.push(time);
                emit("weaponFire");
            } else if lower == "hit" && reload {
                transfer_seconds.push(time);
                emit("reloadComplete");
            } else if lower == "eject" {
                result.eject_seconds.push(time);
                emit("EjectShellCasing");
            } else if lower == "attach"
                && matches!(
                    action,
                    WeaponClipAction::Equip | WeaponClipAction::SneakEquip
                )
            {
                emit("weaponDraw");
            } else if lower == "detach"
                && matches!(
                    action,
                    WeaponClipAction::Unequip | WeaponClipAction::SneakUnequip
                )
            {
                emit("weaponSheathe");
            } else if lower == "loop" && attack {
                loop_start = Some(time);
            } else if lower.starts_with("a:") && attack {
                result.refire_seconds = Some(time);
            } else if lower == "end" && (reload || action == WeaponClipAction::ReloadEnd) {
                emit("reloadEnd");
            } else if lower != "start" && lower != "end" {
                result.unmapped_keys.push(text.to_string());
            }
        }
    }
    if attack && fields.automatic {
        let rate = if fields.fire_rate > 0.0 {
            fields.fire_rate
        } else {
            fields.attack_shots_sec
        };
        if !rate.is_finite() || rate <= 0.0 {
            return Err(
                "automatic weapon requires a positive source fire_rate or attack_shots_sec".into(),
            );
        }
        let first_shot = result.fire_seconds.first().copied();
        let internal_loop = if sequence.cycle == FnvKfCycle::Clamp {
            internal_fire.zip(loop_start)
        } else {
            None
        };
        if internal_loop.is_some_and(|(start, end)| start <= 0.0 || end <= start || end >= duration)
        {
            return Err(
                "source Fire/Loop keys do not define startup, repeating fire and release segments"
                    .into(),
            );
        }
        let loop_end = internal_loop.map_or(duration, |(_, end)| end);
        let loop_begin = internal_loop.map_or(0.0, |(start, _)| start);
        let loop_period = (loop_end - loop_begin) / result.playback_speed;
        let loop_repeat_count = (1..=300)
            .find(|count| {
                let period = loop_period * *count as f64;
                let shots = period * rate;
                period <= 10.0 + 1e-4 && (shots - shots.round()).abs() < 1e-4
            })
            .ok_or(
                "source fire loop cannot preserve WEAP cadence within a ten-second target clip",
            )?;
        let cadence_start = first_shot.or(loop_start).unwrap_or(0.0);
        let interval = rate.recip() * result.playback_speed;
        if !interval.is_finite() || interval <= 0.0 {
            return Err("automatic weapon cadence has invalid playback speed".into());
        }
        let count = ((loop_end - cadence_start) / interval).ceil().max(0.0) as usize;
        if count > 100_000 {
            return Err("automatic weapon cadence exceeds the animation event limit".into());
        }
        // FO4 master clips need repeated weaponFire annotations; FNV loops use WEAP cadence.
        for index in 0..count {
            let time = cadence_start + index as f64 * interval;
            if time >= loop_end
                || result
                    .fire_seconds
                    .iter()
                    .any(|source| (source - time).abs() < 1e-5)
            {
                continue;
            }
            result.annotations.push(WeaponAnnotation {
                time: time.max(0.00001).min(duration),
                text: "weaponFire".into(),
                source_text: None,
            });
        }
        result.continuous_fire = Some(ContinuousFireContract {
            shot_interval_seconds: rate.recip(),
            first_shot_seconds: first_shot,
            cadence_start_seconds: cadence_start,
            loop_start_seconds: internal_loop
                .map(|(start, _)| start)
                .or(loop_start)
                .or_else(|| (sequence.cycle == FnvKfCycle::Loop).then_some(0.0)),
            loop_end_seconds: loop_end,
            source_internal_loop: internal_loop.is_some(),
            loop_repeat_count,
            engine_cadence_required: false,
        });
    } else if attack && result.fire_seconds.is_empty() {
        return Err("nonautomatic attack clip has no source Hit or Fire key".into());
    }
    if attack && !fields.automatic {
        let (delay, authority) = if let Some(refire) = result.refire_seconds {
            if refire <= result.fire_seconds[0] {
                return Err("source refire marker does not follow the shot".into());
            }
            (refire / result.playback_speed, "source_refire_text_key")
        } else if fields.attack_shots_sec > 0.0 {
            (fields.attack_shots_sec.recip(), "source_attack_shots_sec")
        } else {
            (duration / result.playback_speed, "source_sequence_duration")
        };
        // FO4's nonbolt refire timer uses AttackDelay/Speed and requires positive AttackSeconds.
        result.attack_timing = Some(WeaponAttackTiming {
            attack_delay_seconds: delay,
            animation_attack_seconds: duration / result.playback_speed,
            refire_authority: authority.into(),
            combined_source_bolt_cycle: matches!(fields.animation_type, 5 | 7)
                && matches!(fields.attack_animation, 38 | 144),
        });
    }
    if reload {
        let transfer_authority;
        if transfer_seconds.is_empty() {
            if fields.per_round_reload {
                return Err("per-round reload requires a source Hit transfer marker".into());
            }
            // Cached WEAP reload_time exceeds the authored clip on SawedOff/Varmint/Incinerator.
            let time = duration;
            transfer_authority = "source_reload_sequence_end";
            transfer_seconds.push(time);
            result.annotations.push(WeaponAnnotation {
                time,
                text: "reloadComplete".into(),
                source_text: None,
            });
        } else {
            transfer_authority = "source_reload_hit_text_key";
        }
        let boundary = fields
            .per_round_reload
            .then(|| *transfer_seconds.last().unwrap());
        result.reload = Some(ReloadContract {
            transfer_seconds,
            transfer_authority: transfer_authority.into(),
            per_round: fields.per_round_reload,
            repeat_until_seconds: boundary,
            exit_tail_start_seconds: boundary,
            end_seconds: duration,
        });
    }
    result.annotations.sort_by(|left, right| {
        left.time.total_cmp(&right.time).then_with(|| {
            // Transfer must precede the state-exit event when both are at end.
            (left.text == "reloadEnd").cmp(&(right.text == "reloadEnd"))
        })
    });
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::phase::animations::{
        FnvKfBindingCompatibility, FnvKfBindingEvidence, FnvKfRootMotionEvidence,
        FnvKfTextKeyEvidence, parse_fnv_creature_kf,
    };
    use crate::record::FieldEntry;

    fn pistol() -> WeaponAnimationFields {
        WeaponAnimationFields {
            animation_type: 3,
            grip_animation: 255,
            attack_animation: 32,
            reload_animation: 11,
            ammo_use: 1,
            automatic: false,
            per_round_reload: false,
            animation_multiplier: 1.0,
            attack_multiplier: 1.25,
            fire_rate: 1.0,
            attack_shots_sec: 3.125,
            reload_time: 1.6667,
        }
    }

    fn sequence(duration: f64, keys: &[(f64, &str)]) -> ParsedFnvCreatureKf {
        ParsedFnvCreatureKf {
            source_kf: "test.kf".into(),
            sequence_index: 0,
            sequence_name: "test".into(),
            cycle: FnvKfCycle::Clamp,
            start_time: 0.0,
            stop_time: duration,
            frequency: 1.0,
            text_keys: keys
                .iter()
                .map(|(time, text)| FnvKfTextKeyEvidence {
                    time: *time,
                    text: text.to_string(),
                })
                .collect(),
            binding: FnvKfBindingEvidence {
                skeleton_path: None,
                transform_track_count: 0,
                float_track_count: 0,
                controller_types: vec![],
                interpolator_types: vec![],
                target_names: vec![],
                transform_priorities: Default::default(),
                transform_components: Default::default(),
                required_float_slots: vec![],
                compatibility: FnvKfBindingCompatibility::Unverified {
                    detail: "test".into(),
                },
            },
            root_motion: FnvKfRootMotionEvidence::None,
        }
    }

    #[test]
    fn source_dnam_offsets_decode_animation_and_reload_contract() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode(*b"WEAP"),
            FormKey::parse("000810@FalloutNV.esm", &interner).unwrap(),
        );
        let mut bytes = vec![0_u8; 96];
        bytes[0..4].copy_from_slice(&3_u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&1_f32.to_le_bytes());
        bytes[12..16].copy_from_slice(&[2, 255, 1, 20]);
        bytes[41] = 50;
        bytes[56..60].copy_from_slice(&12_u32.to_le_bytes());
        for (offset, value) in [(60, 0.7_f32), (64, 20.0), (88, 1.75), (92, 0.6333)] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::Bytes(bytes.into()),
        });
        let fields = decode_weapon_animation_fields(&record, &interner).unwrap();
        assert_eq!(
            (
                fields.animation_type,
                fields.attack_animation,
                fields.reload_animation
            ),
            (3, 50, 20)
        );
        assert!(fields.automatic && fields.per_round_reload);
        assert_eq!(fields.grip_animation, 255);
        assert!((fields.attack_multiplier - 0.7).abs() < 1e-6);
        assert!((fields.reload_time - 0.6333).abs() < 1e-6);
        if let FieldValue::Bytes(bytes) = &mut record.fields[0].value {
            bytes[15] = 0;
        }
        assert!(
            !decode_weapon_animation_fields(&record, &interner)
                .unwrap()
                .per_round_reload
        );
        if let FieldValue::Bytes(bytes) = &mut record.fields[0].value {
            bytes[15] = 21;
            bytes[56..60].copy_from_slice(&8_u32.to_le_bytes());
        }
        assert!(
            decode_weapon_animation_fields(&record, &interner)
                .unwrap()
                .per_round_reload
        );
    }

    #[test]
    fn source_enums_select_verified_clip_names_and_playback_speeds() {
        let cases = [
            (3, 32, 11, "1hpattackright", "1hpreloadl"),
            (5, 38, 0, "2hrattack3", "2hrreloada"),
            (8, 80, 4, "2hhattackspin", "2hhreloade"),
        ];
        for (family, attack, reload, attack_stem, reload_stem) in cases {
            let mut fields = pistol();
            fields.animation_type = family;
            fields.attack_animation = attack;
            fields.reload_animation = reload;
            for (perspective, root) in [
                (WeaponPerspective::First, "_1stperson"),
                (WeaponPerspective::Third, "_male"),
            ] {
                let clips = source_weapon_clip_candidates(&fields, perspective).unwrap();
                let attack = clips
                    .iter()
                    .find(|clip| clip.action == WeaponClipAction::Attack)
                    .unwrap();
                assert_eq!(
                    attack.source_path,
                    format!("meshes/characters/{root}/{attack_stem}.kf")
                );
                assert_eq!(attack.playback_speed, 1.25);
                let reload = clips
                    .iter()
                    .find(|clip| clip.action == WeaponClipAction::Reload)
                    .unwrap();
                assert_eq!(
                    reload.source_path,
                    format!("meshes/characters/{root}/{reload_stem}.kf")
                );
                assert_eq!(reload.playback_speed, 1.0);
            }
        }
    }

    #[test]
    fn grip_selector_resolves_real_suffixed_names_and_reports_ambiguity() {
        let mut fields = pistol();
        fields.animation_type = 5;
        fields.grip_animation = 230;
        let clips = source_weapon_clip_candidates(&fields, WeaponPerspective::Third).unwrap();
        let grip = clips
            .iter()
            .find(|clip| clip.action == WeaponClipAction::HandGrip)
            .unwrap();
        let inventory = vec!["meshes\\characters\\_male\\2hrhandgrip1_railwayrifle.kf".into()];
        let resolved = resolve_weapon_clip_candidate(grip, &inventory)
            .unwrap()
            .unwrap();
        assert!(
            resolved
                .source_path
                .ends_with("2hrhandgrip1_railwayrifle.kf")
        );
        let mut ambiguous = inventory;
        ambiguous.push("meshes/characters/_male/2hrhandgrip1_other.kf".into());
        assert!(
            resolve_weapon_clip_candidate(grip, &ambiguous)
                .unwrap_err()
                .contains("ambiguous")
        );
        assert!(resolve_weapon_clip_candidate(grip, &[]).is_err());
        let optional = clips.iter().find(|clip| clip.optional).unwrap();
        assert_eq!(resolve_weapon_clip_candidate(optional, &[]).unwrap(), None);
    }

    #[test]
    fn legacy_tutorial_grip_preserves_source_byte_and_authored_finger_pose() {
        let mut fields = pistol();
        fields.animation_type = 5;
        fields.grip_animation = 172;
        for perspective in [WeaponPerspective::First, WeaponPerspective::Third] {
            let clips = source_weapon_clip_candidates(&fields, perspective).unwrap();
            assert!(
                clips
                    .iter()
                    .all(|clip| clip.action != WeaponClipAction::HandGrip)
            );
            assert!(
                clips
                    .iter()
                    .any(|clip| clip.action == WeaponClipAction::Aim)
            );
        }
        assert_eq!(fields.grip_animation, 172);
    }

    #[test]
    fn semiautomatic_refire_uses_baked_source_cycle_once_including_bolt_motion() {
        for (family, attack, speed, duration, refire) in [
            (3, 32, 1.25, 0.5, 0.4),
            (5, 38, 1.2, 1.3333333, 1.2666667),
            (5, 144, 1.0, 2.3333333, 2.2333333),
        ] {
            let mut fields = pistol();
            fields.animation_type = family;
            fields.attack_animation = attack;
            fields.attack_multiplier = speed;
            let contract = build_weapon_event_contract(
                &fields,
                WeaponClipAction::Attack,
                &sequence(duration, &[(0.0333333, "Hit"), (refire, "a:R")]),
                &HashMap::new(),
            )
            .unwrap();
            let timing = contract.attack_timing.unwrap();
            assert!((timing.attack_delay_seconds - refire / speed).abs() < 1e-9);
            assert!((timing.animation_attack_seconds - duration / speed).abs() < 1e-9);
            assert_eq!(timing.refire_authority, "source_refire_text_key");
            assert_eq!(timing.combined_source_bolt_cycle, family == 5);
        }
    }

    #[test]
    fn semiautomatic_refire_fallback_keeps_record_cadence_separate_from_clip_speed() {
        let mut fields = pistol();
        let clip = sequence(0.5, &[(0.0333333, "Hit")]);
        let timing =
            build_weapon_event_contract(&fields, WeaponClipAction::Attack, &clip, &HashMap::new())
                .unwrap()
                .attack_timing
                .unwrap();
        assert_eq!(timing.attack_delay_seconds, 1.0 / fields.attack_shots_sec);
        assert_eq!(timing.refire_authority, "source_attack_shots_sec");
        fields.attack_shots_sec = 0.0;
        let timing =
            build_weapon_event_contract(&fields, WeaponClipAction::Attack, &clip, &HashMap::new())
                .unwrap()
                .attack_timing
                .unwrap();
        assert_eq!(timing.attack_delay_seconds, 0.5 / 1.25);
        assert_eq!(timing.refire_authority, "source_sequence_duration");
        assert!(
            build_weapon_event_contract(
                &fields,
                WeaponClipAction::Attack,
                &sequence(0.5, &[(0.1, "Hit"), (0.05, "a:R")]),
                &HashMap::new()
            )
            .is_err()
        );
        fields.automatic = true;
        assert!(
            build_weapon_event_contract(&fields, WeaponClipAction::Attack, &clip, &HashMap::new())
                .unwrap()
                .attack_timing
                .is_none()
        );
    }

    #[test]
    fn pistol_fire_marker_and_reload_end_have_distinct_meanings() {
        let fields = pistol();
        let attack = sequence(
            0.5,
            &[
                (0.0333333, "Hit"),
                (0.4, "a:R"),
                (0.466667, "BlendIn:1"),
                (0.5, "end"),
            ],
        );
        let fire = build_weapon_event_contract(
            &fields,
            WeaponClipAction::Attack,
            &attack,
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(fire.fire_seconds, vec![0.0333333]);
        assert_eq!(fire.refire_seconds, Some(0.4));
        assert_eq!(fire.annotations[0].text, "weaponFire");
        assert_eq!(fire.playback_speed, 1.25);
        let reload = sequence(
            1.6666665,
            &[
                (0.1999998, "Sound: WPNPistol9mmRReload1"),
                (1.6666665, "end"),
            ],
        );
        let sounds = HashMap::from([(
            "WPNPistol9mmRReload1".into(),
            "B21_WPNPistol9mmRReload1".into(),
        )]);
        let reload =
            build_weapon_event_contract(&fields, WeaponClipAction::Reload, &reload, &sounds)
                .unwrap();
        assert_eq!(
            reload
                .annotations
                .iter()
                .map(|event| event.text.as_str())
                .collect::<Vec<_>>(),
            vec![
                "SoundPlay.B21_WPNPistol9mmRReload1",
                "reloadComplete",
                "reloadEnd"
            ]
        );
        assert_eq!(reload.reload.unwrap().transfer_seconds, vec![1.6666665]);
        assert_eq!(reload.annotations[1].source_text, None);
    }

    #[test]
    fn cached_reload_time_cannot_override_authored_magazine_completion() {
        for (cached, duration) in [(3.125, 2.5), (2.1633, 1.9666667), (2.5, 1.5), (0.0, 3.0)] {
            let mut fields = pistol();
            fields.reload_time = cached;
            let contract = build_weapon_event_contract(
                &fields,
                WeaponClipAction::Reload,
                &sequence(duration, &[(duration, "end")]),
                &HashMap::new(),
            )
            .unwrap();
            let reload = contract.reload.unwrap();
            assert_eq!(reload.transfer_seconds, vec![duration]);
            assert_eq!(reload.transfer_authority, "source_reload_sequence_end");
            assert!(!reload.per_round);
            assert_eq!(contract.annotations[0].text, "reloadComplete");
            assert_eq!(contract.annotations[1].text, "reloadEnd");
        }
    }

    #[test]
    fn revolver_source_hit_defines_repeated_reload_prefix_and_cocking_tail() {
        let mut fields = pistol();
        fields.per_round_reload = true;
        fields.reload_time = 0.6333;
        let clip = sequence(
            1.7666674,
            &[
                (0.0000095, "Sound: WPN357RevolverReloadLoop"),
                (0.6333332, "hit"),
                (1.466667, "Sound: WPN357RevolverCock"),
                (1.7666674, "end"),
            ],
        );
        let contract =
            build_weapon_event_contract(&fields, WeaponClipAction::Reload, &clip, &HashMap::new())
                .unwrap();
        assert!(contract.fire_seconds.is_empty());
        assert_eq!(contract.annotations[0].text, "reloadComplete");
        let reload = contract.reload.unwrap();
        assert!(reload.per_round);
        assert_eq!(reload.repeat_until_seconds, Some(0.6333332));
        assert_eq!(reload.exit_tail_start_seconds, Some(0.6333332));
        assert_eq!(reload.end_seconds, 1.7666674);
        assert_eq!(
            contract.unresolved_sounds,
            vec!["WPN357RevolverReloadLoop", "WPN357RevolverCock"]
        );
    }

    #[test]
    fn annotation_free_automatic_loop_emits_record_cadence_without_inventing_source_hit() {
        let mut fields = pistol();
        fields.automatic = true;
        fields.fire_rate = 20.0;
        fields.attack_multiplier = 1.0;
        let mut clip = sequence(
            0.2,
            &[(0.0, "start"), (0.0333333, "BlendIn:1"), (0.2, "end")],
        );
        clip.cycle = FnvKfCycle::Loop;
        let contract = build_weapon_event_contract(
            &fields,
            WeaponClipAction::AttackLoop,
            &clip,
            &HashMap::new(),
        )
        .unwrap();
        assert!(contract.fire_seconds.is_empty());
        assert_eq!(contract.annotations.len(), 4);
        assert!(
            contract
                .annotations
                .iter()
                .all(|event| event.text == "weaponFire" && event.source_text.is_none())
        );
        assert_eq!(contract.annotations[0].time, 0.00001);
        assert!((contract.annotations[3].time - 0.15).abs() < 1e-9);
        let automatic = contract.continuous_fire.unwrap();
        assert_eq!(automatic.shot_interval_seconds, 0.05);
        assert_eq!(automatic.first_shot_seconds, None);
        assert_eq!(automatic.cadence_start_seconds, 0.0);
        assert_eq!(automatic.loop_start_seconds, Some(0.0));
        assert!(!automatic.engine_cadence_required);
    }

    #[test]
    fn contextual_selectors_keep_sneak_and_sighted_source_actions_separate() {
        let fields = pistol();
        let third = source_weapon_clip_candidates(&fields, WeaponPerspective::Third).unwrap();
        for (action, stem) in [
            (WeaponClipAction::AttackIronSights, "1hpattackrightis"),
            (WeaponClipAction::SneakAttack, "sneak1hpattackright"),
            (
                WeaponClipAction::SneakAttackIronSights,
                "sneak1hpattackrightis",
            ),
            (WeaponClipAction::SneakReload, "sneak1hpreloadl"),
        ] {
            assert_eq!(
                third
                    .iter()
                    .find(|clip| clip.action == action)
                    .unwrap()
                    .source_path,
                format!("meshes/characters/_male/{stem}.kf")
            );
        }
        let first = source_weapon_clip_candidates(&fields, WeaponPerspective::First).unwrap();
        assert!(
            first
                .iter()
                .any(|clip| clip.action == WeaponClipAction::AttackIronSights)
        );
        assert!(
            !first
                .iter()
                .any(|clip| clip.action == WeaponClipAction::SneakAttack)
        );
    }

    #[test]
    fn generated_automatic_annotations_preserve_record_cadence_at_nonunit_speed() {
        let mut fields = pistol();
        fields.automatic = true;
        fields.fire_rate = 10.0;
        fields.attack_multiplier = 2.0;
        let clip = sequence(1.0, &[(0.1, "Fire")]);
        let contract =
            build_weapon_event_contract(&fields, WeaponClipAction::Attack, &clip, &HashMap::new())
                .unwrap();
        assert_eq!(contract.annotations.len(), 5);
        assert_eq!(contract.annotations[0].source_text.as_deref(), Some("Fire"));
        assert!(
            contract.annotations[1..]
                .iter()
                .all(|event| event.source_text.is_none())
        );
        assert!((contract.annotations[1].time - 0.3).abs() < 1e-9);
        assert!(
            ((contract.annotations[1].time - contract.annotations[0].time)
                / contract.playback_speed
                - 0.1)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn minigun_spin_preserves_source_fire_and_loop_entry() {
        let mut fields = pistol();
        fields.automatic = true;
        fields.fire_rate = 20.0;
        let clip = sequence(
            1.73333335,
            &[(0.2, "Fire"), (0.8666667, "Loop"), (1.73333335, "end")],
        );
        let contract =
            build_weapon_event_contract(&fields, WeaponClipAction::Attack, &clip, &HashMap::new())
                .unwrap();
        let automatic = contract.continuous_fire.unwrap();
        assert_eq!(automatic.first_shot_seconds, Some(0.2));
        assert_eq!(automatic.cadence_start_seconds, 0.2);
        assert_eq!(automatic.loop_start_seconds, Some(0.2));
        assert_eq!(automatic.loop_end_seconds, 0.8666667);
        assert!(automatic.source_internal_loop);
        assert_eq!(automatic.loop_repeat_count, 3);
        assert!(
            contract
                .annotations
                .iter()
                .filter(|event| event.text == "weaponFire")
                .all(|event| event.time < 0.8666667)
        );
        assert_eq!(contract.annotations[0].source_text.as_deref(), Some("Fire"));
    }

    #[test]
    fn f32_grenade_machinegun_rate_closes_at_the_ten_second_boundary() {
        let mut fields = pistol();
        fields.automatic = true;
        fields.attack_multiplier = 1.0;
        fields.fire_rate = f64::from(3.1f32);
        let mut clip = sequence(f64::from(0.2f32), &[]);
        clip.cycle = FnvKfCycle::Loop;
        let contract = build_weapon_event_contract(
            &fields,
            WeaponClipAction::AttackLoop,
            &clip,
            &HashMap::new(),
        )
        .unwrap();
        let fire = contract.continuous_fire.unwrap();
        assert_eq!(fire.loop_repeat_count, 50);
        let loop_seconds = clip.stop_time * fire.loop_repeat_count as f64;
        assert!((loop_seconds - 31.0 * fire.shot_interval_seconds).abs() < 1e-6);
    }

    #[test]
    fn hunting_rifle_eject_and_chamber_sound_remain_at_source_times() {
        let clip = sequence(
            1.333333,
            &[
                (0.0333333, "Hit"),
                (0.6, "Sound: WPNRifleHuntingReloadChamber"),
                (0.7999997, "Eject"),
                (1.266666, "a:R"),
            ],
        );
        let sound_map = HashMap::from([(
            "WPNRifleHuntingReloadChamber".into(),
            "B21_HuntingChamber".into(),
        )]);
        let contract =
            build_weapon_event_contract(&pistol(), WeaponClipAction::Attack, &clip, &sound_map)
                .unwrap();
        assert_eq!(contract.eject_seconds, vec![0.7999997]);
        assert_eq!(
            contract
                .annotations
                .iter()
                .map(|event| event.text.as_str())
                .collect::<Vec<_>>(),
            vec![
                "weaponFire",
                "SoundPlay.B21_HuntingChamber",
                "EjectShellCasing"
            ]
        );
    }

    #[test]
    fn sequence_frequency_normalizes_keys_without_applying_graph_playback_twice() {
        let mut clip = sequence(12.0, &[(10.4, "Hit\nEject")]);
        clip.start_time = 10.0;
        clip.frequency = 2.0;
        let contract = build_weapon_event_contract(
            &pistol(),
            WeaponClipAction::Attack,
            &clip,
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(contract.duration_seconds, 1.0);
        assert_eq!(contract.playback_speed, 1.25);
        assert!((contract.fire_seconds[0] - 0.2).abs() < 1e-9);
        assert_eq!(contract.fire_seconds, contract.eject_seconds);
    }

    #[test]
    fn attachment_markers_preserve_source_timing_and_do_not_invent_reload_transfers() {
        let mut fields = pistol();
        fields.per_round_reload = true;
        for (action, key, mapped) in [
            (WeaponClipAction::Equip, "Attach", "weaponDraw"),
            (WeaponClipAction::SneakEquip, "attach", "weaponDraw"),
            (WeaponClipAction::Unequip, "Detach", "weaponSheathe"),
            (WeaponClipAction::SneakUnequip, "detach", "weaponSheathe"),
        ] {
            let clip = sequence(0.5, &[(0.1666665, key)]);
            let contract =
                build_weapon_event_contract(&fields, action, &clip, &HashMap::new()).unwrap();
            assert_eq!(contract.annotations.len(), 1);
            assert_eq!(contract.annotations[0].text, mapped);
            assert_eq!(contract.annotations[0].time, 0.1666665);
            assert_eq!(contract.annotations[0].source_text.as_deref(), Some(key));
            assert!(contract.reload.is_none());
        }
        for action in [WeaponClipAction::ReloadStart, WeaponClipAction::ReloadEnd] {
            let contract = build_weapon_event_contract(
                &fields,
                action,
                &sequence(0.5, &[(0.5, "end")]),
                &HashMap::new(),
            )
            .unwrap();
            assert!(contract.reload.is_none());
            assert!(
                contract
                    .annotations
                    .iter()
                    .all(|event| event.text != "reloadComplete")
            );
        }
    }

    #[test]
    #[ignore = "requires extracted Fallout New Vegas actor KF assets"]
    fn real_source_equip_and_unequip_markers_become_attachment_events() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../extracted/fnv/meshes/characters");
        for perspective in ["_male", "_1stperson"] {
            for (stem, action, event, time) in [
                ("1hpequip", WeaponClipAction::Equip, "weaponDraw", 0.1666665),
                (
                    "1hpunequip",
                    WeaponClipAction::Unequip,
                    "weaponSheathe",
                    0.2333333,
                ),
                ("2hrequip", WeaponClipAction::Equip, "weaponDraw", 0.3),
                (
                    "2hrunequip",
                    WeaponClipAction::Unequip,
                    "weaponSheathe",
                    0.4666667,
                ),
            ] {
                let path = source.join(perspective).join(format!("{stem}.kf"));
                let bytes = std::fs::read(&path).unwrap();
                let clip =
                    parse_fnv_creature_kf(&bytes, &path.to_string_lossy(), None, None).unwrap();
                let contract =
                    build_weapon_event_contract(&pistol(), action, &clip, &HashMap::new()).unwrap();
                let annotations: Vec<_> = contract
                    .annotations
                    .iter()
                    .filter(|annotation| annotation.text == event)
                    .collect();
                assert_eq!(annotations.len(), 1, "{}", path.display());
                assert!((annotations[0].time - time).abs() < 1e-6);
                assert!(annotations[0].source_text.is_some());
            }
        }
    }

    #[test]
    fn unsupported_timing_is_reported_instead_of_silently_creating_events() {
        let empty = sequence(0.5, &[]);
        assert!(
            build_weapon_event_contract(
                &pistol(),
                WeaponClipAction::Attack,
                &empty,
                &HashMap::new()
            )
            .is_err()
        );
        let mut fields = pistol();
        fields.per_round_reload = true;
        assert!(
            build_weapon_event_contract(&fields, WeaponClipAction::Reload, &empty, &HashMap::new())
                .is_err()
        );
        fields.per_round_reload = false;
        assert!(
            build_weapon_event_contract(&fields, WeaponClipAction::Reload, &empty, &HashMap::new())
                .is_ok()
        );
        let outside = sequence(0.5, &[(0.6, "Hit")]);
        assert!(
            build_weapon_event_contract(
                &fields,
                WeaponClipAction::Attack,
                &outside,
                &HashMap::new()
            )
            .is_err()
        );
        fields.automatic = true;
        fields.fire_rate = 0.0;
        fields.attack_shots_sec = 0.0;
        assert!(
            build_weapon_event_contract(
                &fields,
                WeaponClipAction::AttackLoop,
                &empty,
                &HashMap::new()
            )
            .is_err()
        );
    }

    #[test]
    #[ignore = "requires extracted Fallout New Vegas actor KF assets"]
    fn real_source_weapon_kfs_produce_fire_reload_and_automatic_contracts() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../extracted/fnv/meshes/characters");
        for perspective in ["_male", "_1stperson"] {
            for (stem, action) in [
                ("1hpattackright", WeaponClipAction::Attack),
                ("1hpreloadl", WeaponClipAction::Reload),
                ("1hpreloadx", WeaponClipAction::Reload),
                ("2hrattack3", WeaponClipAction::Attack),
                ("2hhattackspin", WeaponClipAction::Attack),
                ("2hhattackloop", WeaponClipAction::AttackLoop),
            ] {
                let path = source.join(perspective).join(format!("{stem}.kf"));
                let bytes = std::fs::read(&path)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                let clip =
                    parse_fnv_creature_kf(&bytes, &path.to_string_lossy(), None, None).unwrap();
                let mut fields = pistol();
                if stem == "1hpreloadx" {
                    fields.per_round_reload = true;
                    fields.reload_time = 0.6333;
                }
                if stem.starts_with("2hh") {
                    fields.automatic = true;
                    fields.fire_rate = 20.0;
                }
                let contract = build_weapon_event_contract(&fields, action, &clip, &HashMap::new())
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                if action == WeaponClipAction::Reload {
                    assert!(contract.reload.is_some());
                } else if !fields.automatic {
                    assert!(!contract.fire_seconds.is_empty());
                } else {
                    assert_eq!(
                        contract.continuous_fire.unwrap().shot_interval_seconds,
                        0.05
                    );
                }
            }
        }
    }
}
