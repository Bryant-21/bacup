use serde::{Deserialize, Serialize};

use crate::record::Record;
use crate::source_rig::race_data::{Fo4RaceDataDerivation, derive_fo4_race_data};
use crate::source_rig::RaceDataMapping;
use crate::sym::StringInterner;

use super::creature_catalog::StableFormKey;
use super::creature_recipe::CreatureFamilyRecipeJob;
use super::creature_race_data::{scalar_f32, selected_cycle, struct_u16};

pub(crate) fn controller_scale_keyword() -> crate::source_rig::CreatureTargetRecordReference {
    crate::source_rig::CreatureTargetRecordReference::new(
        "KYWD",
        crate::source_rig::TargetFormKey::new(0x249F53, "Fallout4.esm"),
    )
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct CreatureProfileRaceData {
    pub stats_owner: StableFormKey,
    pub base_scale_bits: u32,
    pub speed_multiplier: u16,
    pub turning_speed_bits: u32,
    pub source_level: u16,
    pub derivation: Fo4RaceDataDerivation,
}

pub(crate) fn prepare(
    family: &CreatureFamilyRecipeJob,
    stats_owner: &StableFormKey,
    record: &Record,
    interner: &StringInterner,
) -> Result<CreatureProfileRaceData, String> {
    let base_scale = scalar_f32(record, "BNAM", "float32_0", interner).ok_or("Stats owner BNAM is absent")?;
    let speed_multiplier = struct_u16(record, "ACBS", "speed_multiplier", interner).ok_or("Stats owner ACBS.speed_multiplier is absent")?;
    let turning_speed = scalar_f32(record, "TNAM", "float32_0", interner).ok_or("Stats owner TNAM is absent")?;
    let source_level = struct_u16(record, "ACBS", "level", interner).ok_or("Stats owner ACBS.level is absent")?;
    let derivation = derive(family, base_scale, speed_multiplier, turning_speed, source_level)?;
    Ok(CreatureProfileRaceData { stats_owner:stats_owner.clone(), base_scale_bits:base_scale.to_bits(), speed_multiplier, turning_speed_bits:turning_speed.to_bits(), source_level, derivation })
}

pub(crate) fn validate(profile: &CreatureProfileRaceData, family: &CreatureFamilyRecipeJob) -> Result<(), String> {
    let expected = derive(family, f32::from_bits(profile.base_scale_bits), profile.speed_multiplier, f32::from_bits(profile.turning_speed_bits), profile.source_level)?;
    if profile.derivation != expected { return Err("profile RACE derivation differs from its source Stats evidence".into()); }
    Ok(())
}

fn derive(
    family: &CreatureFamilyRecipeJob,
    base_scale: f32,
    speed_multiplier: u16,
    turning_speed: f32,
    source_level: u16,
) -> Result<Fo4RaceDataDerivation, String> {
    if !base_scale.is_finite() || base_scale <= 0.0 || speed_multiplier == 0 || !turning_speed.is_finite() || turning_speed < 0.0 {
        return Err("profile BNAM/ACBS.speed_multiplier/TNAM contains invalid source movement values".into());
    }
    let baseline = family.race_data.as_ref().ok_or("family RACE evidence is absent")?;
    let mut measured = baseline.measured_evidence.clone();
    measured.male_actor_scale = Some(base_scale);
    measured.female_actor_scale = Some(base_scale);
    measured.xp_value = Some(i16::try_from(source_level.max(1)).unwrap_or(i16::MAX));
    let graph = family.graph_contract.as_ref().ok_or("family graph contract is absent")?;
    let motion = family.motion_evidence.as_ref().ok_or("family motion evidence is absent")?;
    let indexed = family.indexed_motion_evidence.as_ref().ok_or("family indexed motion is absent")?;
    let selection = super::creature_live_builder::build_movement_selections(std::slice::from_ref(graph), std::slice::from_ref(motion))
        .into_iter().next().ok_or("family movement selection is absent")?;
    if let Some(movement) = measured.movement.as_mut() {
        if let Some(linear) = movement.linear.as_mut() {
            let source_kf = selection.locomotion_source_kf.as_deref().ok_or("moving profile lacks source locomotion")?;
            let cycle = selected_cycle(indexed, source_kf).map_err(|error| format!("profile locomotion: {error:?}"))?;
            if selection.uses_controller_speed || cycle.planar_distance <= 0.0 {
                linear.max_speed = f32::from_bits(baseline.movement_settings.f_move_base_speed_bits)
                    * f32::from_bits(baseline.movement_settings.locomotion_multiplier_bits)
                    * f32::from(speed_multiplier) / 100.0;
            }
        }
        if let Some(angular) = movement.angular.as_mut() {
            let turn = selection.turn_source_kf.as_deref().map(|source_kf| selected_cycle(indexed, source_kf))
                .transpose().map_err(|error| format!("profile turn: {error:?}"))?;
            if movement.linear.is_some() && !turn.is_some_and(|cycle| cycle.yaw_degrees > 0.0) {
                angular.max_yaw_speed_degrees_per_second = turning_speed;
            }
        }
    }
    let derivation = derive_fo4_race_data(&measured, &baseline.scale_policy).map_err(|error| error.to_string())?;
    if !matches!(derivation.mapping, RaceDataMapping::Mapped { .. }) || !derivation.missing_evidence.is_empty() {
        return Err("profile RACE derivation has missing measured evidence".into());
    }
    Ok(derivation)
}
