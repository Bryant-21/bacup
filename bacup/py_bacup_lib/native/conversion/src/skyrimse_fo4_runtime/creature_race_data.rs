use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use havok_native::hkx::model::{HkxFile, HkxMember};
use havok_native::hkx::types::HkxValue;
use nif_core_native::world_bounds::{
    WorldBounds, aggregate_named_node_world_bounds, aggregate_render_world_bounds,
};
use serde::{Deserialize, Serialize};

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::source_rig::RaceDataMapping;
use crate::source_rig::race_data::{
    AngularMotionEvidence, CapsuleEvidence, ControllerArchitecture, DefaultWeightEvidence,
    Fo4RaceDataDerivation, Fo4RaceDataScalePolicy, LinearMotionEvidence, MeasuredBounds,
    MeasurementAxis, MovementArchitecture, MovementSemantics, RaceDataDerivationError,
    RaceDataEvidenceField, SourceFamilyRaceDataEvidence, derive_fo4_race_data,
};
use crate::sym::StringInterner;

use super::creature_catalog::{CreatureCorpusPlan, CreatureRacePlan};

const FLAG_SWIMS: u32 = 1 << 6;
const FLAG_FLIES: u32 = 1 << 7;
const FLAG_WALKS: u32 = 1 << 8;
const FLAG_IMMOBILE: u32 = 1 << 9;
const FLAG_NOT_PUSHABLE: u32 = 1 << 10;
const FLAG_ALLOW_RAGDOLL_COLLISION: u32 = 1 << 18;
const FLAG_CANT_OPEN_DOORS: u32 = 1 << 20;
const FLAG_2_NON_HOSTILE: u32 = 1 << 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureRaceDataInputField {
    SourceRaceRecord,
    ProjectPath,
    FamilyEvidence,
    RaceData,
    RaceFlags,
    RaceFlags2,
    MaleActorScale,
    FemaleActorScale,
    MaleDefaultWeight,
    FemaleDefaultWeight,
    GenderSkeletons,
    MovementArchitecture,
    AccelerationRate,
    DecelerationRate,
    AngularAccelerationRate,
    AngularTolerance,
    AimAngleTolerance,
    InjuredHealthPercent,
    UnarmedDamage,
    UnarmedReach,
    BodyBipedObject,
    BodyBounds,
    SkeletonBounds,
    ControllerCapsule,
    GeometryScaleToFo4,
    ScalePolicy,
    MaxLinearSpeed,
    MaxYawSpeed,
    PitchLimit,
    RollLimit,
    LargeActorPathing,
    SubsegmentedDamage,
    XpValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureRaceDataDisposition {
    Complete,
    Missing,
    Invalid,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedCreatureFamilyEvidence {
    pub project_path: String,
    pub project_paths: Vec<String>,
    pub body_bounds: Option<MeasuredBounds>,
    pub skeleton_bounds: Option<MeasuredBounds>,
    pub up_axis: MeasurementAxis,
    pub capsule: Option<CapsuleEvidence>,
    pub geometry_scale_to_fo4: Option<f32>,
    pub movement_architecture: Option<MovementArchitecture>,
    pub max_linear_speed: Option<f32>,
    pub max_yaw_speed_degrees_per_second: Option<f32>,
    pub pitch_limit_degrees: Option<f32>,
    pub roll_limit_degrees: Option<f32>,
    pub use_large_actor_pathing: Option<bool>,
    pub use_subsegmented_damage: Option<bool>,
    pub xp_value: Option<i16>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreatureRaceDataMvpDefaults {
    pub policy_id: String,
    pub geometry_scale_to_fo4: f32,
    pub movement_architecture: MovementArchitecture,
    pub max_linear_speed: Option<f32>,
    pub max_yaw_speed_degrees_per_second: f32,
    pub pitch_limit_degrees: f32,
    pub roll_limit_degrees: f32,
    pub use_large_actor_pathing: bool,
    pub use_subsegmented_damage: bool,
    pub xp_value: i16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureRaceDataFidelityReceipt {
    pub field: CreatureRaceDataInputField,
    pub policy_id: String,
    pub reason: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DefaultedCreatureFamilyEvidence {
    pub evidence: DecodedCreatureFamilyEvidence,
    pub receipts: Vec<CreatureRaceDataFidelityReceipt>,
}

pub fn apply_explicit_mvp_defaults(
    mut evidence: DecodedCreatureFamilyEvidence,
    defaults: &CreatureRaceDataMvpDefaults,
) -> DefaultedCreatureFamilyEvidence {
    let mut receipts = Vec::new();
    apply_default(
        &mut evidence.geometry_scale_to_fo4,
        defaults.geometry_scale_to_fo4,
        CreatureRaceDataInputField::GeometryScaleToFo4,
        &defaults.policy_id,
        &mut receipts,
    );
    apply_default(
        &mut evidence.movement_architecture,
        defaults.movement_architecture,
        CreatureRaceDataInputField::MovementArchitecture,
        &defaults.policy_id,
        &mut receipts,
    );
    if evidence.max_linear_speed.is_none()
        && let Some(value) = defaults.max_linear_speed
    {
        apply_default(
            &mut evidence.max_linear_speed,
            value,
            CreatureRaceDataInputField::MaxLinearSpeed,
            &defaults.policy_id,
            &mut receipts,
        );
    }
    apply_default(
        &mut evidence.max_yaw_speed_degrees_per_second,
        defaults.max_yaw_speed_degrees_per_second,
        CreatureRaceDataInputField::MaxYawSpeed,
        &defaults.policy_id,
        &mut receipts,
    );
    apply_default(
        &mut evidence.pitch_limit_degrees,
        defaults.pitch_limit_degrees,
        CreatureRaceDataInputField::PitchLimit,
        &defaults.policy_id,
        &mut receipts,
    );
    apply_default(
        &mut evidence.roll_limit_degrees,
        defaults.roll_limit_degrees,
        CreatureRaceDataInputField::RollLimit,
        &defaults.policy_id,
        &mut receipts,
    );
    apply_default(
        &mut evidence.use_large_actor_pathing,
        defaults.use_large_actor_pathing,
        CreatureRaceDataInputField::LargeActorPathing,
        &defaults.policy_id,
        &mut receipts,
    );
    apply_default(
        &mut evidence.use_subsegmented_damage,
        defaults.use_subsegmented_damage,
        CreatureRaceDataInputField::SubsegmentedDamage,
        &defaults.policy_id,
        &mut receipts,
    );
    apply_default(
        &mut evidence.xp_value,
        defaults.xp_value,
        CreatureRaceDataInputField::XpValue,
        &defaults.policy_id,
        &mut receipts,
    );
    DefaultedCreatureFamilyEvidence { evidence, receipts }
}

fn apply_default<T>(
    target: &mut Option<T>,
    value: T,
    field: CreatureRaceDataInputField,
    policy_id: &str,
    receipts: &mut Vec<CreatureRaceDataFidelityReceipt>,
) {
    if target.is_none() {
        *target = Some(value);
        receipts.push(CreatureRaceDataFidelityReceipt {
            field,
            policy_id: policy_id.to_string(),
            reason: "explicit source-neutral MVP fallback supplied by creature scaffold",
        });
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreatureRaceDataEvidenceResult {
    pub source_race: FormKey,
    pub project_paths: Vec<String>,
    pub project_path: Option<String>,
    pub evidence: Option<SourceFamilyRaceDataEvidence>,
    pub derivation: Option<Fo4RaceDataDerivation>,
    pub unarmed_data: Option<CreatureRaceUnarmedDataEvidence>,
    pub disposition: CreatureRaceDataDisposition,
    pub missing_fields: Vec<CreatureRaceDataInputField>,
    pub invalid_fields: Vec<CreatureRaceDataInputField>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureRaceUnarmedDataEvidence {
    pub damage_bits: u32,
    pub reach_bits: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreatureFamilyRaceDataEvidenceResult {
    pub project_path: String,
    pub project_paths: Vec<String>,
    pub races: Vec<FormKey>,
    pub disposition: CreatureRaceDataDisposition,
    pub missing_fields: Vec<CreatureRaceDataInputField>,
    pub invalid_fields: Vec<CreatureRaceDataInputField>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreatureRaceDataEvidenceSummary {
    pub race_count: usize,
    pub complete_races: usize,
    pub missing_races: usize,
    pub invalid_races: usize,
    pub family_count: usize,
    pub complete_families: usize,
    pub missing_families: usize,
    pub invalid_families: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreatureRaceDataEvidencePlan {
    pub races: Vec<CreatureRaceDataEvidenceResult>,
    pub families: Vec<CreatureFamilyRaceDataEvidenceResult>,
    pub summary: CreatureRaceDataEvidenceSummary,
}

impl CreatureRaceDataEvidencePlan {
    pub fn race(&self, source_race: FormKey) -> Option<&CreatureRaceDataEvidenceResult> {
        self.races
            .iter()
            .find(|race| race.source_race == source_race)
    }

    pub fn family(&self, project_path: &str) -> Option<&CreatureFamilyRaceDataEvidenceResult> {
        let key = canonical_project_path(project_path);
        self.families
            .iter()
            .find(|family| canonical_project_path(&family.project_path) == key)
    }
}

pub fn build_creature_race_data_evidence(
    catalog: &CreatureCorpusPlan,
    records: &[Record],
    decoded_families: &[DecodedCreatureFamilyEvidence],
    policy: &Fo4RaceDataScalePolicy,
    interner: &StringInterner,
) -> CreatureRaceDataEvidencePlan {
    let records = winning_records(records);
    let decoded = index_decoded_evidence(decoded_families);
    let mut races = catalog
        .races
        .iter()
        .map(|race| build_race_evidence(race, &records, &decoded, policy, interner))
        .collect::<Vec<_>>();
    races.sort_by_key(|result| form_key_sort_key(result.source_race, interner));

    let mut grouped = BTreeMap::<String, Vec<&CreatureRaceDataEvidenceResult>>::new();
    for race in &races {
        if race.project_paths.is_empty() {
            grouped
                .entry(format!(
                    "<missing-project>:{:?}",
                    form_key_sort_key(race.source_race, interner)
                ))
                .or_default()
                .push(race);
        } else {
            for path in &race.project_paths {
                grouped
                    .entry(canonical_asset_path(path))
                    .or_default()
                    .push(race);
            }
        }
    }
    let families = grouped
        .into_iter()
        .map(|(project_path, family_races)| summarize_family(project_path, family_races, interner))
        .collect::<Vec<_>>();

    let summary = CreatureRaceDataEvidenceSummary {
        race_count: races.len(),
        complete_races: races
            .iter()
            .filter(|race| race.disposition == CreatureRaceDataDisposition::Complete)
            .count(),
        missing_races: races
            .iter()
            .filter(|race| race.disposition == CreatureRaceDataDisposition::Missing)
            .count(),
        invalid_races: races
            .iter()
            .filter(|race| race.disposition == CreatureRaceDataDisposition::Invalid)
            .count(),
        family_count: families.len(),
        complete_families: families
            .iter()
            .filter(|family| family.disposition == CreatureRaceDataDisposition::Complete)
            .count(),
        missing_families: families
            .iter()
            .filter(|family| family.disposition == CreatureRaceDataDisposition::Missing)
            .count(),
        invalid_families: families
            .iter()
            .filter(|family| family.disposition == CreatureRaceDataDisposition::Invalid)
            .count(),
    };
    CreatureRaceDataEvidencePlan {
        races,
        families,
        summary,
    }
}

fn build_race_evidence(
    race: &CreatureRacePlan,
    records: &HashMap<FormKey, &Record>,
    decoded: &BTreeMap<String, Vec<&DecodedCreatureFamilyEvidence>>,
    policy: &Fo4RaceDataScalePolicy,
    interner: &StringInterner,
) -> CreatureRaceDataEvidenceResult {
    let mut missing = Vec::new();
    let mut invalid = Vec::new();
    let mut project_paths = race.project_paths.clone();
    sort_dedup_case_insensitive(&mut project_paths);
    if project_paths.is_empty() {
        missing.push(CreatureRaceDataInputField::ProjectPath);
    }
    let project_path = project_paths.first().cloned();
    let decoded_family =
        aggregate_project_evidence(&project_paths, decoded, &mut missing, &mut invalid);

    let source = records.get(&race.source_race).copied();
    if source.is_none() {
        missing.push(CreatureRaceDataInputField::SourceRaceRecord);
    }
    let parsed_data = source.and_then(|record| parsed_race_data(record, interner));
    let race_data = parsed_data.as_deref();
    if source.is_some() && race_data.is_none() {
        missing.push(CreatureRaceDataInputField::RaceData);
    }
    let unarmed_data = source_unarmed_data(race_data, interner, &mut missing, &mut invalid);

    let evidence = decoded_family.map(|decoded_family| {
        assemble_source_evidence(
            source,
            race_data,
            &decoded_family,
            interner,
            &mut missing,
            &mut invalid,
        )
    });

    sort_dedup(&mut missing);
    sort_dedup(&mut invalid);
    let mut derivation = None;
    if invalid.is_empty()
        && let Some(evidence) = &evidence
    {
        match derive_fo4_race_data(evidence, policy) {
            Ok(value) => {
                derivation = Some(value);
            }
            Err(error) => invalid.push(derivation_error_field(&error)),
        }
    }
    sort_dedup(&mut missing);
    sort_dedup(&mut invalid);
    let disposition = if !invalid.is_empty() {
        CreatureRaceDataDisposition::Invalid
    } else if !missing.is_empty()
        || derivation
            .as_ref()
            .is_none_or(|value| matches!(value.mapping, RaceDataMapping::Missing { .. }))
    {
        CreatureRaceDataDisposition::Missing
    } else {
        CreatureRaceDataDisposition::Complete
    };

    CreatureRaceDataEvidenceResult {
        source_race: race.source_race,
        project_paths,
        project_path,
        evidence,
        derivation,
        unarmed_data,
        disposition,
        missing_fields: missing,
        invalid_fields: invalid,
    }
}

fn source_unarmed_data(
    race_data: Option<&[(crate::sym::Sym, FieldValue)]>,
    interner: &StringInterner,
    missing: &mut Vec<CreatureRaceDataInputField>,
    invalid: &mut Vec<CreatureRaceDataInputField>,
) -> Option<CreatureRaceUnarmedDataEvidence> {
    let damage = data_f32_bits(
        race_data,
        "unarmed_damage",
        CreatureRaceDataInputField::UnarmedDamage,
        interner,
        missing,
    );
    let reach = data_f32_bits(
        race_data,
        "unarmed_reach",
        CreatureRaceDataInputField::UnarmedReach,
        interner,
        missing,
    );
    if damage.is_some_and(|bits| {
        let value = f32::from_bits(bits);
        !value.is_finite() || value < 0.0 || value > f32::from(u16::MAX)
    }) {
        invalid.push(CreatureRaceDataInputField::UnarmedDamage);
    }
    if reach.is_some_and(|bits| !f32::from_bits(bits).is_finite()) {
        invalid.push(CreatureRaceDataInputField::UnarmedReach);
    }
    Some(CreatureRaceUnarmedDataEvidence {
        damage_bits: damage?,
        reach_bits: reach?,
    })
}

fn assemble_source_evidence(
    source: Option<&Record>,
    race_data: Option<&[(crate::sym::Sym, FieldValue)]>,
    decoded: &DecodedCreatureFamilyEvidence,
    interner: &StringInterner,
    missing: &mut Vec<CreatureRaceDataInputField>,
    invalid: &mut Vec<CreatureRaceDataInputField>,
) -> SourceFamilyRaceDataEvidence {
    let male_actor_scale = data_f32(
        race_data,
        "male_height",
        CreatureRaceDataInputField::MaleActorScale,
        interner,
        missing,
    );
    let female_actor_scale = data_f32(
        race_data,
        "female_height",
        CreatureRaceDataInputField::FemaleActorScale,
        interner,
        missing,
    );
    let male_weight = data_f32(
        race_data,
        "male_weight",
        CreatureRaceDataInputField::MaleDefaultWeight,
        interner,
        missing,
    );
    let female_weight = data_f32(
        race_data,
        "female_weight",
        CreatureRaceDataInputField::FemaleDefaultWeight,
        interner,
        missing,
    );
    let gender_paths = source.map(|record| string_fields(record, "ANAM", interner));
    let ungendered = match gender_paths.as_deref() {
        Some([male, female, ..]) => Some(male.eq_ignore_ascii_case(female)),
        Some([_]) => Some(true),
        _ => {
            missing.push(CreatureRaceDataInputField::GenderSkeletons);
            None
        }
    };
    let default_weights = match (male_weight, female_weight, ungendered) {
        (Some(male), Some(female), Some(ungendered)) => {
            if !male.is_finite() {
                invalid.push(CreatureRaceDataInputField::MaleDefaultWeight);
            }
            if !female.is_finite() {
                invalid.push(CreatureRaceDataInputField::FemaleDefaultWeight);
            }
            Some(DefaultWeightEvidence {
                male: legacy_weight_vector(male.clamp(0.0, 1.0)),
                female: legacy_weight_vector(female.clamp(0.0, 1.0)),
                ungendered,
            })
        }
        _ => None,
    };

    let flags = data_u32(race_data, "flags", interner);
    if flags.is_none() {
        missing.push(CreatureRaceDataInputField::RaceFlags);
    }
    let flags_2 = data_u32(race_data, "flags_2", interner);
    if flags_2.is_none() {
        missing.push(CreatureRaceDataInputField::RaceFlags2);
    }
    let movement = build_movement(
        race_data,
        flags,
        flags_2.unwrap_or_default(),
        decoded,
        interner,
        missing,
        invalid,
    );
    let injured_health_percent = data_f32(
        race_data,
        "injured_health_pct",
        CreatureRaceDataInputField::InjuredHealthPercent,
        interner,
        missing,
    );
    let body_biped_object = data_i32(
        race_data,
        "body_biped_object",
        CreatureRaceDataInputField::BodyBipedObject,
        interner,
        missing,
    );

    if decoded.body_bounds.is_none() {
        missing.push(CreatureRaceDataInputField::BodyBounds);
    }
    if decoded.skeleton_bounds.is_none() {
        missing.push(CreatureRaceDataInputField::SkeletonBounds);
    }
    if decoded.capsule.is_none() {
        missing.push(CreatureRaceDataInputField::ControllerCapsule);
    }
    if decoded.geometry_scale_to_fo4.is_none() {
        missing.push(CreatureRaceDataInputField::GeometryScaleToFo4);
    }
    if decoded.xp_value.is_none() {
        missing.push(CreatureRaceDataInputField::XpValue);
    }

    SourceFamilyRaceDataEvidence {
        body_bounds: decoded.body_bounds.clone(),
        skeleton_bounds: decoded.skeleton_bounds.clone(),
        up_axis: decoded.up_axis,
        capsule: decoded.capsule.clone(),
        geometry_scale_to_fo4: decoded.geometry_scale_to_fo4,
        male_actor_scale,
        female_actor_scale,
        default_weights,
        movement,
        injured_health_percent,
        body_biped_object,
        xp_value: decoded.xp_value,
    }
}

fn build_movement(
    race_data: Option<&[(crate::sym::Sym, FieldValue)]>,
    flags: Option<u32>,
    flags_2: u32,
    decoded: &DecodedCreatureFamilyEvidence,
    interner: &StringInterner,
    missing: &mut Vec<CreatureRaceDataInputField>,
    invalid: &mut Vec<CreatureRaceDataInputField>,
) -> Option<MovementSemantics> {
    let architecture = match decoded.movement_architecture {
        Some(value) => Some(value),
        None => {
            missing.push(CreatureRaceDataInputField::MovementArchitecture);
            None
        }
    };
    let source_architectures = flags.map(source_movement_architectures).unwrap_or_default();
    if let Some(architecture) = architecture
        && !source_architectures.is_empty()
        && !source_architectures.contains(&architecture)
    {
        invalid.push(CreatureRaceDataInputField::MovementArchitecture);
    }

    let (acceleration, deceleration) = if architecture == Some(MovementArchitecture::Stationary) {
        (None, None)
    } else {
        (
            data_f32(
                race_data,
                "acceleration_rate",
                CreatureRaceDataInputField::AccelerationRate,
                interner,
                missing,
            ),
            data_f32(
                race_data,
                "deceleration_rate",
                CreatureRaceDataInputField::DecelerationRate,
                interner,
                missing,
            ),
        )
    };
    let linear = match architecture {
        Some(MovementArchitecture::Stationary) => None,
        Some(_) => match (decoded.max_linear_speed, acceleration, deceleration) {
            (Some(speed), Some(acceleration), Some(deceleration)) => {
                if speed <= 0.0 || acceleration <= 0.0 || deceleration <= 0.0 {
                    invalid.push(CreatureRaceDataInputField::MaxLinearSpeed);
                    None
                } else {
                    Some(LinearMotionEvidence {
                        max_speed: speed,
                        seconds_to_full_speed: speed / acceleration,
                        seconds_to_stop: speed / deceleration,
                    })
                }
            }
            _ => {
                if decoded.max_linear_speed.is_none() {
                    missing.push(CreatureRaceDataInputField::MaxLinearSpeed);
                }
                None
            }
        },
        None => None,
    };

    let angular_acceleration = data_f32(
        race_data,
        "angular_acceleration_rate",
        CreatureRaceDataInputField::AngularAccelerationRate,
        interner,
        missing,
    );
    let tolerance = data_f32(
        race_data,
        "angular_tolerance",
        CreatureRaceDataInputField::AngularTolerance,
        interner,
        missing,
    );
    let aim_tolerance = data_f32(
        race_data,
        "aim_angle_tolerance",
        CreatureRaceDataInputField::AimAngleTolerance,
        interner,
        missing,
    );
    for (value, field) in [
        (
            decoded.max_yaw_speed_degrees_per_second,
            CreatureRaceDataInputField::MaxYawSpeed,
        ),
        (
            decoded.pitch_limit_degrees,
            CreatureRaceDataInputField::PitchLimit,
        ),
        (
            decoded.roll_limit_degrees,
            CreatureRaceDataInputField::RollLimit,
        ),
    ] {
        if value.is_none() {
            missing.push(field);
        }
    }
    let angular = match (
        decoded.max_yaw_speed_degrees_per_second,
        angular_acceleration,
        tolerance,
        aim_tolerance,
        decoded.pitch_limit_degrees,
        decoded.roll_limit_degrees,
    ) {
        (Some(yaw), Some(acceleration), Some(tolerance), Some(aim), Some(pitch), Some(roll)) => {
            if yaw <= 0.0 {
                invalid.push(CreatureRaceDataInputField::MaxYawSpeed);
                None
            } else if acceleration <= 0.0 {
                invalid.push(CreatureRaceDataInputField::AngularAccelerationRate);
                None
            } else {
                Some(AngularMotionEvidence {
                    max_yaw_speed_degrees_per_second: yaw,
                    seconds_to_full_yaw_speed: yaw / acceleration,
                    tolerance_degrees: tolerance,
                    aim_tolerance_degrees: aim,
                    pitch_limit_degrees: pitch,
                    roll_limit_degrees: roll,
                })
            }
        }
        _ => None,
    };

    if decoded.use_large_actor_pathing.is_none() {
        missing.push(CreatureRaceDataInputField::LargeActorPathing);
    }
    if decoded.use_subsegmented_damage.is_none() {
        missing.push(CreatureRaceDataInputField::SubsegmentedDamage);
    }
    match (
        architecture,
        decoded.use_large_actor_pathing,
        decoded.use_subsegmented_damage,
    ) {
        (Some(architecture), Some(large), Some(subsegmented)) => Some(MovementSemantics {
            architecture,
            linear,
            angular,
            pushable: flags.is_some_and(|value| value & FLAG_NOT_PUSHABLE == 0),
            opens_doors: flags.is_some_and(|value| value & FLAG_CANT_OPEN_DOORS == 0),
            allow_ragdoll_collision: flags
                .is_some_and(|value| value & FLAG_ALLOW_RAGDOLL_COLLISION != 0),
            non_hostile: flags_2 & FLAG_2_NON_HOSTILE != 0,
            use_large_actor_pathing: large,
            use_subsegmented_damage: subsegmented,
        }),
        _ => None,
    }
}

fn summarize_family(
    project_path: String,
    results: Vec<&CreatureRaceDataEvidenceResult>,
    interner: &StringInterner,
) -> CreatureFamilyRaceDataEvidenceResult {
    let project_paths = vec![project_path.clone()];
    let mut races = results
        .iter()
        .map(|result| result.source_race)
        .collect::<Vec<_>>();
    races.sort_by_key(|race| form_key_sort_key(*race, interner));
    let mut missing_fields = results
        .iter()
        .flat_map(|result| result.missing_fields.iter().copied())
        .collect::<Vec<_>>();
    let mut invalid_fields = results
        .iter()
        .flat_map(|result| result.invalid_fields.iter().copied())
        .collect::<Vec<_>>();
    sort_dedup(&mut missing_fields);
    sort_dedup(&mut invalid_fields);
    let disposition = if !invalid_fields.is_empty() {
        CreatureRaceDataDisposition::Invalid
    } else if !missing_fields.is_empty() {
        CreatureRaceDataDisposition::Missing
    } else {
        CreatureRaceDataDisposition::Complete
    };
    CreatureFamilyRaceDataEvidenceResult {
        project_path,
        project_paths,
        races,
        disposition,
        missing_fields,
        invalid_fields,
    }
}

fn winning_records(records: &[Record]) -> HashMap<FormKey, &Record> {
    let mut output = HashMap::new();
    for record in records {
        output.insert(record.form_key, record);
    }
    output
}

fn index_decoded_evidence(
    decoded: &[DecodedCreatureFamilyEvidence],
) -> BTreeMap<String, Vec<&DecodedCreatureFamilyEvidence>> {
    let mut output = BTreeMap::<String, Vec<&DecodedCreatureFamilyEvidence>>::new();
    for evidence in decoded {
        let mut paths = evidence.project_paths.clone();
        if paths.is_empty() {
            paths.push(evidence.project_path.clone());
        }
        sort_dedup_case_insensitive(&mut paths);
        for path in paths {
            output
                .entry(canonical_project_path(&path))
                .or_default()
                .push(evidence);
        }
    }
    output
}

fn aggregate_project_evidence(
    project_paths: &[String],
    decoded: &BTreeMap<String, Vec<&DecodedCreatureFamilyEvidence>>,
    missing: &mut Vec<CreatureRaceDataInputField>,
    invalid: &mut Vec<CreatureRaceDataInputField>,
) -> Option<DecodedCreatureFamilyEvidence> {
    if project_paths.is_empty() {
        return None;
    }
    let mut variants = Vec::with_capacity(project_paths.len());
    for path in project_paths {
        match decoded
            .get(&canonical_project_path(path))
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            [evidence] => variants.push(*evidence),
            [] => {}
            _ => invalid.push(CreatureRaceDataInputField::FamilyEvidence),
        }
    }
    if !invalid.is_empty() {
        return None;
    }
    if variants.is_empty() {
        missing.push(CreatureRaceDataInputField::FamilyEvidence);
        return None;
    }

    let first = variants[0];
    variants.retain(|variant| {
        !(variant.up_axis != first.up_axis
            || !same_optional_f32(variant.geometry_scale_to_fo4, first.geometry_scale_to_fo4)
            || variant.movement_architecture != first.movement_architecture
            || variant
                .capsule
                .as_ref()
                .zip(first.capsule.as_ref())
                .is_some_and(|(left, right)| left.architecture != right.architecture))
    });
    if variants.is_empty() {
        invalid.push(CreatureRaceDataInputField::FamilyEvidence);
        return None;
    }

    Some(DecodedCreatureFamilyEvidence {
        project_path: project_paths.join("|"),
        project_paths: project_paths.to_vec(),
        body_bounds: envelope_measured_bounds(variants.iter().map(|variant| &variant.body_bounds)),
        skeleton_bounds: envelope_measured_bounds(
            variants.iter().map(|variant| &variant.skeleton_bounds),
        ),
        up_axis: first.up_axis,
        capsule: aggregate_capsules(&variants),
        geometry_scale_to_fo4: first.geometry_scale_to_fo4,
        movement_architecture: first.movement_architecture,
        max_linear_speed: max_present_f32(variants.iter().map(|variant| variant.max_linear_speed)),
        max_yaw_speed_degrees_per_second: max_present_f32(
            variants
                .iter()
                .map(|variant| variant.max_yaw_speed_degrees_per_second),
        ),
        pitch_limit_degrees: max_present_abs_f32(
            variants.iter().map(|variant| variant.pitch_limit_degrees),
        ),
        roll_limit_degrees: max_present_abs_f32(
            variants.iter().map(|variant| variant.roll_limit_degrees),
        ),
        use_large_actor_pathing: all_present_bool_or(
            variants
                .iter()
                .map(|variant| variant.use_large_actor_pathing),
        ),
        use_subsegmented_damage: all_present_bool_or(
            variants
                .iter()
                .map(|variant| variant.use_subsegmented_damage),
        ),
        xp_value: variants
            .iter()
            .map(|variant| variant.xp_value)
            .collect::<Option<Vec<_>>>()
            .and_then(|values| values.into_iter().max()),
    })
}

fn same_optional_f32(left: Option<f32>, right: Option<f32>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.to_bits() == right.to_bits(),
        (None, None) => true,
        _ => false,
    }
}

fn envelope_measured_bounds<'a>(
    bounds: impl Iterator<Item = &'a Option<MeasuredBounds>>,
) -> Option<MeasuredBounds> {
    let bounds = bounds.map(Option::as_ref).collect::<Option<Vec<_>>>()?;
    let mut aggregate = None;
    for bounds in bounds {
        merge_bounds(
            &mut aggregate,
            WorldBounds {
                min: bounds.min,
                max: bounds.max,
            },
        );
    }
    aggregate
}

fn aggregate_capsules(variants: &[&DecodedCreatureFamilyEvidence]) -> Option<CapsuleEvidence> {
    let capsules = variants
        .iter()
        .map(|variant| variant.capsule.as_ref())
        .collect::<Option<Vec<_>>>()?;
    Some(CapsuleEvidence {
        radius: capsules
            .iter()
            .map(|capsule| capsule.radius)
            .reduce(f32::max)?,
        total_height: capsules
            .iter()
            .map(|capsule| capsule.total_height)
            .reduce(f32::max)?,
        architecture: capsules.first()?.architecture,
    })
}

fn max_present_f32(values: impl Iterator<Item = Option<f32>>) -> Option<f32> {
    values
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .reduce(f32::max)
}

fn max_present_abs_f32(values: impl Iterator<Item = Option<f32>>) -> Option<f32> {
    values
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .map(f32::abs)
        .reduce(f32::max)
}

fn all_present_bool_or(values: impl Iterator<Item = Option<bool>>) -> Option<bool> {
    Some(
        values
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .any(|value| value),
    )
}

fn parsed_race_data(
    record: &Record,
    interner: &StringInterner,
) -> Option<Vec<(crate::sym::Sym, FieldValue)>> {
    let value = &record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "DATA")?
        .value;
    match value {
        FieldValue::Struct(fields) => Some(fields.clone()),
        FieldValue::Bytes(bytes) => decode_raw_skyrim_race_data(bytes, interner),
        _ => None,
    }
}

fn decode_raw_skyrim_race_data(
    bytes: &[u8],
    interner: &StringInterner,
) -> Option<Vec<(crate::sym::Sym, FieldValue)>> {
    if bytes.len() < 128 {
        return None;
    }
    let f32_at = |offset: usize| {
        FieldValue::Float(f32::from_le_bytes(
            bytes[offset..offset + 4].try_into().unwrap(),
        ))
    };
    let u32_at = |offset: usize| {
        FieldValue::Uint(u64::from(u32::from_le_bytes(
            bytes[offset..offset + 4].try_into().unwrap(),
        )))
    };
    let i32_at = |offset: usize| {
        FieldValue::Int(i64::from(i32::from_le_bytes(
            bytes[offset..offset + 4].try_into().unwrap(),
        )))
    };
    Some(
        [
            ("male_height", f32_at(16)),
            ("female_height", f32_at(20)),
            ("male_weight", f32_at(24)),
            ("female_weight", f32_at(28)),
            ("flags", u32_at(32)),
            ("acceleration_rate", f32_at(56)),
            ("deceleration_rate", f32_at(60)),
            ("injured_health_pct", f32_at(76)),
            ("unarmed_damage", f32_at(96)),
            ("unarmed_reach", f32_at(100)),
            ("body_biped_object", i32_at(104)),
            ("aim_angle_tolerance", f32_at(108)),
            ("angular_acceleration_rate", f32_at(116)),
            ("angular_tolerance", f32_at(120)),
            ("flags_2", u32_at(124)),
        ]
        .into_iter()
        .map(|(name, value)| (interner.intern(name), value))
        .collect(),
    )
}

fn data_field<'a>(
    fields: Option<&'a [(crate::sym::Sym, FieldValue)]>,
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields?.iter().find_map(|(field_name, value)| {
        interner
            .resolve(*field_name)
            .is_some_and(|field_name| field_name.eq_ignore_ascii_case(name))
            .then_some(value)
    })
}

fn data_f32(
    fields: Option<&[(crate::sym::Sym, FieldValue)]>,
    name: &str,
    missing_field: CreatureRaceDataInputField,
    interner: &StringInterner,
    missing: &mut Vec<CreatureRaceDataInputField>,
) -> Option<f32> {
    let value = data_field(fields, name, interner).and_then(field_f32);
    if value.is_none() {
        missing.push(missing_field);
    }
    value
}

fn data_f32_bits(
    fields: Option<&[(crate::sym::Sym, FieldValue)]>,
    name: &str,
    missing_field: CreatureRaceDataInputField,
    interner: &StringInterner,
    missing: &mut Vec<CreatureRaceDataInputField>,
) -> Option<u32> {
    let value = match data_field(fields, name, interner) {
        Some(FieldValue::Float(value)) => Some(value.to_bits()),
        _ => None,
    };
    if value.is_none() {
        missing.push(missing_field);
    }
    value
}

fn data_u32(
    fields: Option<&[(crate::sym::Sym, FieldValue)]>,
    name: &str,
    interner: &StringInterner,
) -> Option<u32> {
    match data_field(fields, name, interner)? {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn data_i32(
    fields: Option<&[(crate::sym::Sym, FieldValue)]>,
    name: &str,
    missing_field: CreatureRaceDataInputField,
    interner: &StringInterner,
    missing: &mut Vec<CreatureRaceDataInputField>,
) -> Option<i32> {
    let value = match data_field(fields, name, interner) {
        Some(FieldValue::Int(value)) => i32::try_from(*value).ok(),
        Some(FieldValue::Uint(value)) => i32::try_from(*value).ok(),
        _ => None,
    };
    if value.is_none() {
        missing.push(missing_field);
    }
    value
}

fn field_f32(value: &FieldValue) -> Option<f32> {
    match value {
        FieldValue::Float(value) => Some(*value),
        FieldValue::Int(value) => Some(*value as f32),
        FieldValue::Uint(value) => Some(*value as f32),
        _ => None,
    }
}

fn string_fields(record: &Record, signature: &str, interner: &StringInterner) -> Vec<String> {
    record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .filter_map(|field| match field.value {
            FieldValue::String(value) => interner.resolve(value).map(str::to_string),
            _ => None,
        })
        .collect()
}

fn legacy_weight_vector(weight: f32) -> [f32; 3] {
    [1.0 - weight, 0.0, weight]
}

fn source_movement_architectures(flags: u32) -> Vec<MovementArchitecture> {
    let mut architectures = Vec::new();
    let walks = flags & FLAG_WALKS != 0;
    let flies = flags & FLAG_FLIES != 0;
    let swims = flags & FLAG_SWIMS != 0;
    if walks {
        architectures.push(MovementArchitecture::Grounded);
    }
    if flies {
        architectures.push(MovementArchitecture::Flying);
    }
    if swims {
        architectures.push(MovementArchitecture::Swimming);
    }
    if walks && flies {
        architectures.push(MovementArchitecture::GroundedFlying);
    }
    if walks && swims {
        architectures.push(MovementArchitecture::GroundedSwimming);
    }
    if flags & FLAG_IMMOBILE != 0 {
        architectures.push(MovementArchitecture::Stationary);
    }
    architectures
}

fn derivation_error_field(error: &RaceDataDerivationError) -> CreatureRaceDataInputField {
    match error {
        RaceDataDerivationError::InvalidEvidence { field, .. } => match field {
            RaceDataEvidenceField::BodyBounds => CreatureRaceDataInputField::BodyBounds,
            RaceDataEvidenceField::SkeletonBounds => CreatureRaceDataInputField::SkeletonBounds,
            RaceDataEvidenceField::ControllerCapsule => {
                CreatureRaceDataInputField::ControllerCapsule
            }
            RaceDataEvidenceField::GeometryScaleToFo4 => {
                CreatureRaceDataInputField::GeometryScaleToFo4
            }
            RaceDataEvidenceField::MaleActorScale => CreatureRaceDataInputField::MaleActorScale,
            RaceDataEvidenceField::FemaleActorScale => CreatureRaceDataInputField::FemaleActorScale,
            RaceDataEvidenceField::DefaultWeights => CreatureRaceDataInputField::MaleDefaultWeight,
            RaceDataEvidenceField::MovementArchitecture => {
                CreatureRaceDataInputField::MovementArchitecture
            }
            RaceDataEvidenceField::LinearMotion => CreatureRaceDataInputField::MaxLinearSpeed,
            RaceDataEvidenceField::AngularMotion => CreatureRaceDataInputField::MaxYawSpeed,
            RaceDataEvidenceField::InjuredHealthPercent => {
                CreatureRaceDataInputField::InjuredHealthPercent
            }
            RaceDataEvidenceField::BodyBipedObject => CreatureRaceDataInputField::BodyBipedObject,
            RaceDataEvidenceField::XpValue => CreatureRaceDataInputField::XpValue,
        },
        RaceDataDerivationError::InvalidSizePolicy(_) => CreatureRaceDataInputField::ScalePolicy,
        RaceDataDerivationError::InvalidTarget(_) => CreatureRaceDataInputField::RaceData,
    }
}

fn canonical_asset_path(path: &str) -> String {
    path.trim().replace('/', "\\").to_ascii_lowercase()
}

fn canonical_project_path(path: &str) -> String {
    let path = canonical_asset_path(path);
    path.strip_prefix("actors\\").unwrap_or(&path).to_string()
}

fn form_key_sort_key(form_key: FormKey, interner: &StringInterner) -> (String, u32) {
    (
        interner
            .resolve(form_key.plugin)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        form_key.local,
    )
}

fn sort_dedup<T: Ord>(values: &mut Vec<T>) {
    values.sort();
    values.dedup();
}

fn sort_dedup_case_insensitive(values: &mut Vec<String>) {
    values.sort_by_key(|value| canonical_asset_path(value));
    values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureFamilySourcePaths {
    pub project_paths: Vec<String>,
    pub character_paths: Vec<String>,
    pub skeleton_paths: Vec<String>,
    pub body_paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreatureFamilyRuntimeSemantics {
    pub up_axis: MeasurementAxis,
    pub controller_architecture: Option<ControllerArchitecture>,
    pub geometry_scale_to_fo4: Option<f32>,
    pub movement_architecture: Option<MovementArchitecture>,
    pub max_linear_speed: Option<f32>,
    pub max_yaw_speed_degrees_per_second: Option<f32>,
    pub pitch_limit_degrees: Option<f32>,
    pub roll_limit_degrees: Option<f32>,
    pub use_large_actor_pathing: Option<bool>,
    pub use_subsegmented_damage: Option<bool>,
    pub xp_value: Option<i16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CreatureRaceDataLoaderSurface {
    CharacterController,
    BodyWorldBounds,
    SkeletonWorldBounds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreatureRaceDataLoaderIssue {
    UnsafeSourcePath {
        path: String,
    },
    MissingSourceFile {
        path: String,
    },
    ReadFailed {
        path: String,
        reason: String,
    },
    MissingCharacterData {
        path: String,
    },
    MissingControllerCapsule {
        path: String,
    },
    MissingExplicitControllerArchitecture,
    MissingSourcePaths {
        surface: CreatureRaceDataLoaderSurface,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadedCreatureFamilyEvidence {
    pub evidence: DecodedCreatureFamilyEvidence,
    pub issues: Vec<CreatureRaceDataLoaderIssue>,
}

pub fn load_creature_family_evidence(
    source_root: &Path,
    paths: &CreatureFamilySourcePaths,
    semantics: &CreatureFamilyRuntimeSemantics,
) -> LoadedCreatureFamilyEvidence {
    let mut issues = Vec::new();
    let mut project_paths = paths.project_paths.clone();
    let mut character_paths = paths.character_paths.clone();
    let mut body_paths = paths.body_paths.clone();
    let mut skeleton_paths = paths.skeleton_paths.clone();
    sort_dedup_case_insensitive(&mut project_paths);
    sort_dedup_case_insensitive(&mut character_paths);
    sort_dedup_case_insensitive(&mut body_paths);
    sort_dedup_case_insensitive(&mut skeleton_paths);
    let capsule = load_character_capsules(
        source_root,
        &character_paths,
        semantics.controller_architecture,
        &mut issues,
    );
    let body_bounds = load_nif_bounds(
        source_root,
        &body_paths,
        CreatureRaceDataLoaderSurface::BodyWorldBounds,
        |path| aggregate_render_world_bounds(path),
        &mut issues,
    );
    let skeleton_bounds = load_nif_bounds(
        source_root,
        &skeleton_paths,
        CreatureRaceDataLoaderSurface::SkeletonWorldBounds,
        |path| aggregate_named_node_world_bounds(path),
        &mut issues,
    );
    LoadedCreatureFamilyEvidence {
        evidence: DecodedCreatureFamilyEvidence {
            project_path: project_paths.join("|"),
            project_paths,
            body_bounds,
            skeleton_bounds,
            up_axis: semantics.up_axis,
            capsule,
            geometry_scale_to_fo4: semantics.geometry_scale_to_fo4,
            movement_architecture: semantics.movement_architecture,
            max_linear_speed: semantics.max_linear_speed,
            max_yaw_speed_degrees_per_second: semantics.max_yaw_speed_degrees_per_second,
            pitch_limit_degrees: semantics.pitch_limit_degrees,
            roll_limit_degrees: semantics.roll_limit_degrees,
            use_large_actor_pathing: semantics.use_large_actor_pathing,
            use_subsegmented_damage: semantics.use_subsegmented_damage,
            xp_value: semantics.xp_value,
        },
        issues,
    }
}

fn load_character_capsules(
    source_root: &Path,
    paths: &[String],
    architecture: Option<ControllerArchitecture>,
    issues: &mut Vec<CreatureRaceDataLoaderIssue>,
) -> Option<CapsuleEvidence> {
    if paths.is_empty() {
        issues.push(CreatureRaceDataLoaderIssue::MissingSourcePaths {
            surface: CreatureRaceDataLoaderSurface::CharacterController,
        });
        return None;
    }
    let capsules = paths
        .iter()
        .filter_map(|path| load_character_capsule(source_root, path, architecture, issues))
        .collect::<Vec<_>>();
    if capsules.len() != paths.len() {
        return None;
    }
    Some(CapsuleEvidence {
        radius: capsules
            .iter()
            .map(|capsule| capsule.radius)
            .reduce(f32::max)?,
        total_height: capsules
            .iter()
            .map(|capsule| capsule.total_height)
            .reduce(f32::max)?,
        architecture: capsules.first()?.architecture,
    })
}

fn load_character_capsule(
    source_root: &Path,
    path: &str,
    architecture: Option<ControllerArchitecture>,
    issues: &mut Vec<CreatureRaceDataLoaderIssue>,
) -> Option<CapsuleEvidence> {
    let Some(path_on_disk) = resolve_source_path(source_root, path, issues) else {
        return None;
    };
    let bytes = match std::fs::read(&path_on_disk) {
        Ok(bytes) => bytes,
        Err(error) => {
            issues.push(CreatureRaceDataLoaderIssue::ReadFailed {
                path: path.to_string(),
                reason: error.to_string(),
            });
            return None;
        }
    };
    let hkx = match HkxFile::read(&bytes) {
        Ok(hkx) => hkx,
        Err(error) => {
            issues.push(CreatureRaceDataLoaderIssue::ReadFailed {
                path: path.to_string(),
                reason: error.to_string(),
            });
            return None;
        }
    };
    let Some((height, radius)) = character_capsule(&hkx) else {
        let has_character_data = hkx
            .objects()
            .iter()
            .any(|object| object.class_name == "hkbCharacterData");
        issues.push(if has_character_data {
            CreatureRaceDataLoaderIssue::MissingControllerCapsule {
                path: path.to_string(),
            }
        } else {
            CreatureRaceDataLoaderIssue::MissingCharacterData {
                path: path.to_string(),
            }
        });
        return None;
    };
    let Some(architecture) = architecture else {
        issues.push(CreatureRaceDataLoaderIssue::MissingExplicitControllerArchitecture);
        return None;
    };
    Some(CapsuleEvidence {
        radius,
        total_height: height,
        architecture,
    })
}

fn character_capsule(hkx: &HkxFile) -> Option<(f32, f32)> {
    let character = hkx
        .objects()
        .iter()
        .find(|object| object.class_name == "hkbCharacterData")?;
    let controller = member_object(&character.members, "characterControllerInfo")
        .or_else(|| member_object(&character.members, "characterControllerSetup"))?;
    find_capsule_members(controller)
}

fn find_capsule_members(members: &[HkxMember]) -> Option<(f32, f32)> {
    let height = member_f32(members, "capsuleHeight");
    let radius = member_f32(members, "capsuleRadius");
    if let (Some(height), Some(radius)) = (height, radius) {
        return Some((height, radius));
    }
    for member in members {
        if let Some(children) = member.value.as_object_members()
            && let Some(capsule) = find_capsule_members(children)
        {
            return Some(capsule);
        }
    }
    None
}

fn member_object<'a>(members: &'a [HkxMember], name: &str) -> Option<&'a [HkxMember]> {
    members
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| member.value.as_object_members())
}

fn member_f32(members: &[HkxMember], name: &str) -> Option<f32> {
    members
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| match member.value {
            HkxValue::F32(value) | HkxValue::Half(value) => Some(value),
            _ => None,
        })
}

fn load_nif_bounds<F, E>(
    source_root: &Path,
    paths: &[String],
    surface: CreatureRaceDataLoaderSurface,
    reader: F,
    issues: &mut Vec<CreatureRaceDataLoaderIssue>,
) -> Option<MeasuredBounds>
where
    F: Fn(&Path) -> Result<WorldBounds, E>,
    E: std::fmt::Display,
{
    if paths.is_empty() {
        issues.push(CreatureRaceDataLoaderIssue::MissingSourcePaths { surface });
        return None;
    }
    let mut aggregate = None;
    for path in paths {
        let Some(path_on_disk) = resolve_source_path(source_root, path, issues) else {
            continue;
        };
        match reader(&path_on_disk) {
            Ok(bounds) => merge_bounds(&mut aggregate, bounds),
            Err(error) => issues.push(CreatureRaceDataLoaderIssue::ReadFailed {
                path: path.clone(),
                reason: error.to_string(),
            }),
        }
    }
    aggregate
}

fn merge_bounds(aggregate: &mut Option<MeasuredBounds>, bounds: WorldBounds) {
    match aggregate {
        Some(current) => {
            for axis in 0..3 {
                current.min[axis] = current.min[axis].min(bounds.min[axis]);
                current.max[axis] = current.max[axis].max(bounds.max[axis]);
            }
        }
        None => {
            *aggregate = Some(MeasuredBounds {
                min: bounds.min,
                max: bounds.max,
            });
        }
    }
}

fn resolve_source_path(
    source_root: &Path,
    asset_path: &str,
    issues: &mut Vec<CreatureRaceDataLoaderIssue>,
) -> Option<PathBuf> {
    let mut relative = PathBuf::new();
    for component in asset_path.split(['/', '\\']) {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." || component.contains(':') {
            issues.push(CreatureRaceDataLoaderIssue::UnsafeSourcePath {
                path: asset_path.to_string(),
            });
            return None;
        }
        relative.push(component);
    }
    let path = source_root.join(relative);
    if !path.is_file() {
        issues.push(CreatureRaceDataLoaderIssue::MissingSourceFile {
            path: asset_path.to_string(),
        });
        return None;
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::skyrimse_fo4_runtime::creature_catalog::{CreatureCorpusPlan, CreatureRacePlan};
    fn key(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            plugin: interner.intern("CreatureRaceData.esm"),
            local,
        }
    }

    fn race_plan(interner: &StringInterner, local: u32, project: &str) -> CreatureRacePlan {
        CreatureRacePlan {
            source_race: key(interner, local),
            source_plugin: "Skyrim.esm".to_string(),
            editor_id: Some(format!("Race{local:06X}")),
            skin: None,
            armor_addons: Vec::new(),
            body_models: Vec::new(),
            body_model_parts: Vec::new(),
            body_part_data: None,
            project_paths: vec![project.to_string()],
            skeleton_paths: vec!["Actors\\Test\\Skeleton.nif".to_string()],
            attack_events: Vec::new(),
            attack_contract: Vec::new(),
            attack_data: Vec::new(),
            attack_spells: Vec::new(),
            issues: Vec::new(),
        }
    }

    fn source_race(interner: &StringInterner, local: u32, flags: u32, flags_2: u32) -> Record {
        let mut record = Record::new(SigCode::from_str("RACE").unwrap(), key(interner, local));
        let values = [
            ("male_height", FieldValue::Float(1.0)),
            ("female_height", FieldValue::Float(1.0)),
            ("male_weight", FieldValue::Float(0.5)),
            ("female_weight", FieldValue::Float(0.5)),
            ("flags", FieldValue::Uint(flags as u64)),
            ("acceleration_rate", FieldValue::Float(240.0)),
            ("deceleration_rate", FieldValue::Float(480.0)),
            ("injured_health_pct", FieldValue::Float(0.2)),
            ("unarmed_damage", FieldValue::Float(10.0)),
            ("unarmed_reach", FieldValue::Float(0.68)),
            ("body_biped_object", FieldValue::Int(0)),
            ("aim_angle_tolerance", FieldValue::Float(30.0)),
            ("angular_acceleration_rate", FieldValue::Float(240.0)),
            ("angular_tolerance", FieldValue::Float(15.0)),
            ("flags_2", FieldValue::Uint(flags_2 as u64)),
        ];
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Struct(
                values
                    .into_iter()
                    .map(|(name, value)| (interner.intern(name), value))
                    .collect(),
            ),
        });
        for path in ["Actors\\Test\\Skeleton.nif", "Actors\\Test\\Skeleton.nif"] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("ANAM").unwrap(),
                value: FieldValue::String(interner.intern(path)),
            });
        }
        record
    }

    #[test]
    fn raw_skyrim_race_data_decodes_the_live_128_byte_layout() {
        let interner = StringInterner::new();
        let mut record = source_race(&interner, 1, 0, 0);
        let mut bytes = vec![0_u8; 128];
        bytes[16..20].copy_from_slice(&1.25_f32.to_le_bytes());
        bytes[20..24].copy_from_slice(&0.75_f32.to_le_bytes());
        bytes[24..28].copy_from_slice(&0.4_f32.to_le_bytes());
        bytes[28..32].copy_from_slice(&0.6_f32.to_le_bytes());
        bytes[32..36].copy_from_slice(&FLAG_WALKS.to_le_bytes());
        bytes[56..60].copy_from_slice(&240.0_f32.to_le_bytes());
        bytes[60..64].copy_from_slice(&480.0_f32.to_le_bytes());
        bytes[76..80].copy_from_slice(&0.2_f32.to_le_bytes());
        bytes[96..100].copy_from_slice(&12.0_f32.to_le_bytes());
        bytes[100..104].copy_from_slice(&0.68_f32.to_le_bytes());
        bytes[104..108].copy_from_slice(&3_i32.to_le_bytes());
        bytes[108..112].copy_from_slice(&30.0_f32.to_le_bytes());
        bytes[116..120].copy_from_slice(&180.0_f32.to_le_bytes());
        bytes[120..124].copy_from_slice(&15.0_f32.to_le_bytes());
        bytes[124..128].copy_from_slice(&FLAG_2_NON_HOSTILE.to_le_bytes());
        record
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "DATA")
            .unwrap()
            .value = FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes));

        let data = parsed_race_data(&record, &interner).unwrap();
        assert_eq!(
            data_field(Some(&data), "male_height", &interner).and_then(field_f32),
            Some(1.25)
        );
        assert_eq!(data_u32(Some(&data), "flags", &interner), Some(FLAG_WALKS));
        assert_eq!(
            data_i32(
                Some(&data),
                "body_biped_object",
                CreatureRaceDataInputField::BodyBipedObject,
                &interner,
                &mut Vec::new(),
            ),
            Some(3)
        );
        assert_eq!(
            data_u32(Some(&data), "flags_2", &interner),
            Some(FLAG_2_NON_HOSTILE)
        );
    }

    fn measured(
        project: &str,
        stature: f32,
        movement: MovementArchitecture,
        controller: ControllerArchitecture,
    ) -> DecodedCreatureFamilyEvidence {
        DecodedCreatureFamilyEvidence {
            project_path: project.to_string(),
            project_paths: vec![project.to_string()],
            body_bounds: Some(MeasuredBounds {
                min: [-stature * 0.4, -stature * 0.3, 0.0],
                max: [stature * 0.4, stature * 0.3, stature],
            }),
            skeleton_bounds: Some(MeasuredBounds {
                min: [-stature * 0.35, -stature * 0.25, stature * 0.05],
                max: [stature * 0.35, stature * 0.25, stature * 0.95],
            }),
            up_axis: MeasurementAxis::Z,
            capsule: Some(CapsuleEvidence {
                radius: stature * 0.15,
                total_height: stature * 0.75,
                architecture: controller,
            }),
            geometry_scale_to_fo4: Some(1.0),
            movement_architecture: Some(movement),
            max_linear_speed: (movement != MovementArchitecture::Stationary).then_some(120.0),
            max_yaw_speed_degrees_per_second: Some(120.0),
            pitch_limit_degrees: Some(45.0),
            roll_limit_degrees: Some(20.0),
            use_large_actor_pathing: Some(stature > 192.0),
            use_subsegmented_damage: Some(false),
            xp_value: Some(10),
        }
    }

    fn policy() -> Fo4RaceDataScalePolicy {
        Fo4RaceDataScalePolicy {
            small_max_stature: 32.0,
            medium_max_stature: 96.0,
            large_max_stature: 192.0,
        }
    }

    #[test]
    fn derives_small_quadruped_flyer_dragon_and_stationary_mechanical_families() {
        let interner = StringInterner::new();
        let declarations = [
            (
                1,
                "Actors\\Hare\\HareProject.hkx",
                12.0,
                MovementArchitecture::Grounded,
                ControllerArchitecture::Quadruped,
                FLAG_WALKS,
            ),
            (
                2,
                "Actors\\Wolf\\WolfProject.hkx",
                80.0,
                MovementArchitecture::Grounded,
                ControllerArchitecture::Quadruped,
                FLAG_WALKS,
            ),
            (
                3,
                "Actors\\Witchlight\\WitchlightProject.hkx",
                120.0,
                MovementArchitecture::Flying,
                ControllerArchitecture::Standard,
                FLAG_FLIES,
            ),
            (
                4,
                "Actors\\Dragon\\DragonProject.hkx",
                300.0,
                MovementArchitecture::Flying,
                ControllerArchitecture::Standard,
                FLAG_FLIES | FLAG_WALKS,
            ),
            (
                5,
                "Actors\\Machine\\MachineProject.hkx",
                140.0,
                MovementArchitecture::Stationary,
                ControllerArchitecture::Fixed,
                FLAG_IMMOBILE,
            ),
        ];
        let catalog = CreatureCorpusPlan {
            races: declarations
                .iter()
                .map(|(local, project, ..)| race_plan(&interner, *local, project))
                .collect(),
            ..Default::default()
        };
        let records = declarations
            .iter()
            .map(|(local, _, _, _, _, flags)| source_race(&interner, *local, *flags, 0))
            .collect::<Vec<_>>();
        let evidence = declarations
            .iter()
            .map(|(_, project, stature, movement, controller, _)| {
                measured(project, *stature, *movement, *controller)
            })
            .collect::<Vec<_>>();

        let first =
            build_creature_race_data_evidence(&catalog, &records, &evidence, &policy(), &interner);
        let mut reversed_records = records.clone();
        reversed_records.reverse();
        let mut reversed_evidence = evidence.clone();
        reversed_evidence.reverse();
        let second = build_creature_race_data_evidence(
            &catalog,
            &reversed_records,
            &reversed_evidence,
            &policy(),
            &interner,
        );

        assert_eq!(first, second);
        assert_eq!(first.summary.race_count, 5);
        assert_eq!(first.summary.family_count, 5);
        assert_eq!(first.summary.complete_races, 5);
        assert_eq!(first.summary.complete_families, 5);
        assert!(first.races.iter().all(|race| {
            matches!(
                race.derivation.as_ref().map(|value| &value.mapping),
                Some(RaceDataMapping::Mapped { .. })
            )
        }));
        assert!(first.races.iter().all(|race| {
            race.unarmed_data
                == Some(CreatureRaceUnarmedDataEvidence {
                    damage_bits: 10.0_f32.to_bits(),
                    reach_bits: 0.68_f32.to_bits(),
                })
        }));
    }

    #[test]
    fn unarmed_data_preserves_fractional_source_damage() {
        let interner = StringInterner::new();
        let project = "Actors\\Wolf\\WolfProject.hkx";
        let catalog = CreatureCorpusPlan {
            races: vec![race_plan(&interner, 1, project)],
            ..Default::default()
        };
        let mut record = source_race(&interner, 1, FLAG_WALKS, 0);
        let damage_field = interner.intern("unarmed_damage");
        let fields = record
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "DATA")
            .and_then(|field| match &mut field.value {
                FieldValue::Struct(fields) => Some(fields),
                _ => None,
            })
            .expect("RACE DATA");
        let damage = fields
            .iter_mut()
            .find(|(name, _)| *name == damage_field)
            .expect("unarmed damage");
        damage.1 = FieldValue::Float(10.5);

        let plan = build_creature_race_data_evidence(
            &catalog,
            &[record],
            &[measured(
                project,
                80.0,
                MovementArchitecture::Grounded,
                ControllerArchitecture::Quadruped,
            )],
            &policy(),
            &interner,
        );

        assert_eq!(
            plan.races[0].unarmed_data,
            Some(CreatureRaceUnarmedDataEvidence {
                damage_bits: 10.5_f32.to_bits(),
                reach_bits: 0.68_f32.to_bits(),
            })
        );
        assert_eq!(plan.races[0].invalid_fields, Vec::new());
        assert_eq!(plan.summary.complete_races, 1);
    }

    #[test]
    fn source_movement_flags_include_hybrid_architectures() {
        assert_eq!(
            source_movement_architectures(FLAG_WALKS | FLAG_FLIES),
            vec![
                MovementArchitecture::Grounded,
                MovementArchitecture::Flying,
                MovementArchitecture::GroundedFlying,
            ]
        );
        assert_eq!(
            source_movement_architectures(FLAG_WALKS | FLAG_SWIMS),
            vec![
                MovementArchitecture::Grounded,
                MovementArchitecture::Swimming,
                MovementArchitecture::GroundedSwimming,
            ]
        );
    }

    #[test]
    fn source_default_weights_saturate_to_fo4_morph_endpoints() {
        let interner = StringInterner::new();
        let project = "Actors\\Giant\\GiantProject.hkx";
        let catalog = CreatureCorpusPlan {
            races: vec![race_plan(&interner, 1, project)],
            ..Default::default()
        };
        let mut record = source_race(&interner, 1, FLAG_WALKS, 0);
        let male_weight = interner.intern("male_weight");
        let fields = record
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "DATA")
            .and_then(|field| match &mut field.value {
                FieldValue::Struct(fields) => Some(fields),
                _ => None,
            })
            .expect("RACE DATA");
        fields
            .iter_mut()
            .find(|(name, _)| *name == male_weight)
            .expect("male weight")
            .1 = FieldValue::Float(1.1);

        let plan = build_creature_race_data_evidence(
            &catalog,
            &[record],
            &[measured(
                project,
                180.0,
                MovementArchitecture::Grounded,
                ControllerArchitecture::Standard,
            )],
            &policy(),
            &interner,
        );

        assert_eq!(plan.summary.complete_races, 1);
        assert_eq!(
            plan.races[0]
                .evidence
                .as_ref()
                .and_then(|evidence| evidence.default_weights.as_ref())
                .map(|weights| weights.male),
            Some([0.0, 0.0, 1.0])
        );
    }

    #[test]
    fn secondary_gender_project_without_family_evidence_does_not_block_primary() {
        let interner = StringInterner::new();
        let primary = "Actors\\Dragon\\DragonProject.hkx";
        let fallback = "Actors\\Character\\DefaultMale.hkx";
        let mut race = race_plan(&interner, 1, primary);
        race.project_paths.push(fallback.to_string());
        let catalog = CreatureCorpusPlan {
            races: vec![race],
            ..Default::default()
        };

        let plan = build_creature_race_data_evidence(
            &catalog,
            &[source_race(&interner, 1, FLAG_WALKS | FLAG_FLIES, 0)],
            &[measured(
                primary,
                300.0,
                MovementArchitecture::GroundedFlying,
                ControllerArchitecture::Standard,
            )],
            &policy(),
            &interner,
        );

        assert_eq!(plan.summary.complete_races, 1);
        assert_eq!(plan.races[0].missing_fields, Vec::new());
        assert_eq!(plan.races[0].invalid_fields, Vec::new());
    }

    #[test]
    fn missing_and_invalid_inputs_are_ordered_and_never_defaulted() {
        let interner = StringInterner::new();
        let project = "Actors\\Wolf\\WolfProject.hkx";
        let catalog = CreatureCorpusPlan {
            races: vec![race_plan(&interner, 1, project)],
            ..Default::default()
        };
        let record = source_race(&interner, 1, FLAG_WALKS, 0);
        let mut evidence = measured(
            project,
            80.0,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
        );
        evidence.body_bounds = None;
        evidence.capsule = None;
        evidence.xp_value = None;
        evidence.max_linear_speed = None;
        evidence.pitch_limit_degrees = None;

        let plan = build_creature_race_data_evidence(
            &catalog,
            &[record],
            &[evidence],
            &policy(),
            &interner,
        );

        assert_eq!(plan.summary.missing_races, 1);
        assert_eq!(
            plan.races[0].missing_fields,
            vec![
                CreatureRaceDataInputField::BodyBounds,
                CreatureRaceDataInputField::ControllerCapsule,
                CreatureRaceDataInputField::MaxLinearSpeed,
                CreatureRaceDataInputField::PitchLimit,
                CreatureRaceDataInputField::XpValue,
            ]
        );
        assert_eq!(plan.races[0].invalid_fields, Vec::new());
    }

    #[test]
    fn gender_project_variants_are_enveloped_without_rejecting_the_race() {
        let interner = StringInterner::new();
        let male_project = "Actors\\Dragon\\DragonProject.hkx";
        let female_project = "Actors\\Character\\DefaultMale.hkx";
        let mut race = race_plan(&interner, 1, male_project);
        race.project_paths.push(female_project.to_string());
        let catalog = CreatureCorpusPlan {
            races: vec![race],
            ..Default::default()
        };
        let source = source_race(&interner, 1, FLAG_FLIES | FLAG_WALKS, 0);
        let male = measured(
            male_project,
            300.0,
            MovementArchitecture::Flying,
            ControllerArchitecture::Standard,
        );
        let mut female = measured(
            female_project,
            240.0,
            MovementArchitecture::Flying,
            ControllerArchitecture::Standard,
        );
        female.max_linear_speed = Some(150.0);

        let plan = build_creature_race_data_evidence(
            &catalog,
            &[source],
            &[male, female],
            &policy(),
            &interner,
        );

        assert_eq!(plan.summary.complete_races, 1);
        assert_eq!(plan.summary.invalid_races, 0);
        assert_eq!(plan.races[0].project_paths.len(), 2);
        assert_eq!(
            plan.races[0]
                .evidence
                .as_ref()
                .and_then(|evidence| evidence.movement.as_ref())
                .and_then(|movement| movement.linear.as_ref())
                .map(|linear| linear.max_speed),
            Some(150.0)
        );
    }

    #[test]
    fn explicit_mvp_defaults_fill_only_schema_absent_fields_with_receipts() {
        let project = "Actors\\Wolf\\WolfProject.hkx";
        let mut evidence = measured(
            project,
            80.0,
            MovementArchitecture::Grounded,
            ControllerArchitecture::Quadruped,
        );
        evidence.geometry_scale_to_fo4 = None;
        evidence.max_linear_speed = None;
        evidence.pitch_limit_degrees = None;
        evidence.xp_value = None;
        let defaulted = apply_explicit_mvp_defaults(
            evidence,
            &CreatureRaceDataMvpDefaults {
                policy_id: "skyrimse_fo4_creature_mvp_v1".to_string(),
                geometry_scale_to_fo4: 1.0,
                movement_architecture: MovementArchitecture::Grounded,
                max_linear_speed: Some(96.0),
                max_yaw_speed_degrees_per_second: 90.0,
                pitch_limit_degrees: 30.0,
                roll_limit_degrees: 20.0,
                use_large_actor_pathing: false,
                use_subsegmented_damage: false,
                xp_value: 0,
            },
        );

        assert_eq!(defaulted.evidence.max_linear_speed, Some(96.0));
        assert_eq!(
            defaulted.evidence.max_yaw_speed_degrees_per_second,
            Some(120.0)
        );
        assert_eq!(
            defaulted
                .receipts
                .iter()
                .map(|receipt| receipt.field)
                .collect::<Vec<_>>(),
            vec![
                CreatureRaceDataInputField::GeometryScaleToFo4,
                CreatureRaceDataInputField::MaxLinearSpeed,
                CreatureRaceDataInputField::PitchLimit,
                CreatureRaceDataInputField::XpValue,
            ]
        );
    }

    #[test]
    fn loader_rejects_inferred_paths_and_reports_invalid_nif_inputs() {
        let source_root = tempfile::tempdir().unwrap();
        for relative in ["Actors\\Test\\Skeleton.nif", "Actors\\Test\\Body.nif"] {
            let path = relative
                .split(['/', '\\'])
                .fold(source_root.path().to_path_buf(), |path, part| {
                    path.join(part)
                });
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            nif_core_native::model::NifFile::new("skyrimse")
                .save(Some(path))
                .unwrap();
        }
        let paths = CreatureFamilySourcePaths {
            project_paths: vec!["Actors\\Test\\TestProject.hkx".to_string()],
            character_paths: vec!["..\\Characters\\Test.hkx".to_string()],
            skeleton_paths: vec!["Actors\\Test\\Skeleton.nif".to_string()],
            body_paths: vec!["Actors\\Test\\Body.nif".to_string()],
        };
        let semantics = CreatureFamilyRuntimeSemantics {
            up_axis: MeasurementAxis::Z,
            controller_architecture: Some(ControllerArchitecture::Standard),
            geometry_scale_to_fo4: Some(1.0),
            movement_architecture: Some(MovementArchitecture::Grounded),
            max_linear_speed: Some(100.0),
            max_yaw_speed_degrees_per_second: Some(90.0),
            pitch_limit_degrees: Some(45.0),
            roll_limit_degrees: Some(20.0),
            use_large_actor_pathing: Some(false),
            use_subsegmented_damage: Some(false),
            xp_value: Some(10),
        };

        let result = load_creature_family_evidence(source_root.path(), &paths, &semantics);

        assert!(
            result
                .issues
                .iter()
                .any(|issue| matches!(issue, CreatureRaceDataLoaderIssue::UnsafeSourcePath { .. }))
        );
        for expected in ["Actors\\Test\\Body.nif", "Actors\\Test\\Skeleton.nif"] {
            assert!(result.issues.iter().any(|issue| matches!(
                issue,
                CreatureRaceDataLoaderIssue::ReadFailed { path, .. } if path == expected
            )));
        }
        assert_eq!(result.evidence.body_bounds, None);
        assert_eq!(result.evidence.skeleton_bounds, None);
    }

    #[test]
    fn world_bounds_merge_is_an_order_independent_envelope() {
        let mut forward = None;
        merge_bounds(
            &mut forward,
            WorldBounds {
                min: [-2.0, 1.0, 0.0],
                max: [4.0, 3.0, 8.0],
            },
        );
        merge_bounds(
            &mut forward,
            WorldBounds {
                min: [-1.0, -5.0, 2.0],
                max: [9.0, 2.0, 6.0],
            },
        );

        assert_eq!(
            forward,
            Some(MeasuredBounds {
                min: [-2.0, -5.0, 0.0],
                max: [9.0, 3.0, 8.0],
            })
        );
    }

    #[test]
    fn optional_live_wolf_loader_reads_controller_and_nif_bounds() {
        let Some(source_root) = std::env::var_os("SKYRIMSE_CREATURE_ASSET_ROOT").map(PathBuf::from)
        else {
            return;
        };
        if !source_root.is_dir() {
            return;
        }
        let result = load_creature_family_evidence(
            &source_root,
            &CreatureFamilySourcePaths {
                project_paths: vec!["Actors\\Canine\\WolfProject.hkx".to_string()],
                character_paths: vec!["Actors\\Canine\\Characters Wolf\\Wolf.hkx".to_string()],
                skeleton_paths: vec![
                    "Actors\\Canine\\Character Assets Wolf\\skeleton.nif".to_string(),
                ],
                body_paths: vec!["Actors\\Canine\\Character Assets Wolf\\wolf.nif".to_string()],
            },
            &CreatureFamilyRuntimeSemantics {
                up_axis: MeasurementAxis::Z,
                controller_architecture: Some(ControllerArchitecture::Quadruped),
                geometry_scale_to_fo4: None,
                movement_architecture: Some(MovementArchitecture::Grounded),
                max_linear_speed: None,
                max_yaw_speed_degrees_per_second: None,
                pitch_limit_degrees: None,
                roll_limit_degrees: None,
                use_large_actor_pathing: None,
                use_subsegmented_damage: None,
                xp_value: None,
            },
        );

        assert_eq!(result.issues, Vec::new());
        assert!(result.evidence.capsule.is_some());
        assert!(result.evidence.body_bounds.is_some());
        assert!(result.evidence.skeleton_bounds.is_some());
    }

    #[test]
    fn optional_live_merged_corpus_accounts_for_every_race() {
        let Some(path) = std::env::var_os("SKYRIMSE_CREATURE_CORPUS_PLUGIN").map(PathBuf::from)
        else {
            return;
        };
        if !path.is_file() {
            return;
        }
        let handle = esp_authoring_core::plugin_runtime::plugin_handle_load_no_py(
            path.to_str().unwrap(),
            Some("skyrimse"),
            None,
            None,
            true,
        )
        .unwrap();
        let interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("skyrimse").unwrap();
        let mut records = Vec::new();
        for signature in [
            "KYWD", "RACE", "NPC_", "LVLN", "ARMO", "ARMA", "BPTD", "SPEL", "SHOU",
        ] {
            let sig = SigCode::from_str(signature).unwrap();
            for form_key in
                crate::source_read::iter_form_keys_of_sig(handle, sig, &interner).unwrap()
            {
                records.push(
                    crate::source_read::read_record_relayout_by_form_key(
                        handle, &form_key, &schema, &interner, None,
                    )
                    .unwrap(),
                );
            }
        }
        esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle);
        let catalog =
            super::super::creature_catalog::build_creature_corpus_plan(&records, &interner);
        let evidence =
            build_creature_race_data_evidence(&catalog, &records, &[], &policy(), &interner);

        assert_eq!(catalog.summary.candidate_races, 121);
        assert_eq!(evidence.summary.race_count, 121);
        assert_eq!(evidence.summary.complete_races, 0);
        assert_eq!(evidence.summary.missing_races, 121);
        assert_eq!(evidence.summary.invalid_races, 0);
        assert_eq!(evidence.summary.family_count, 47);
        assert_eq!(evidence.summary.missing_families, 47);
    }
}
