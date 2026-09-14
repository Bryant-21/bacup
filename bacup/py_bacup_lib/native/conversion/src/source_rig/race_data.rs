//! Source-neutral derivation of FO4 RACE DATA from measured family evidence.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::corpus::{
    Fo4RaceDataField, Fo4RaceDataTarget, Fo4RaceFlag, Fo4RaceFlag2, Fo4RaceSize, RaceDataMapping,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementAxis {
    X,
    Y,
    Z,
}

impl MeasurementAxis {
    fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeasuredBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl MeasuredBounds {
    fn extent(&self, axis: MeasurementAxis) -> f32 {
        let index = axis.index();
        self.max[index] - self.min[index]
    }

    fn validate(&self, field: RaceDataEvidenceField) -> Result<(), RaceDataDerivationError> {
        if !self
            .min
            .iter()
            .chain(&self.max)
            .all(|value| value.is_finite())
        {
            return Err(invalid(field, "bounds contain a non-finite coordinate"));
        }
        if self.min.iter().zip(&self.max).any(|(min, max)| min > max) {
            return Err(invalid(field, "bounds minimum exceeds maximum"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControllerArchitecture {
    Standard,
    Quadruped,
    Proxy,
    Fixed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CapsuleEvidence {
    pub radius: f32,
    pub total_height: f32,
    pub architecture: ControllerArchitecture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementArchitecture {
    Grounded,
    GroundedSwimming,
    GroundedFlying,
    Flying,
    Swimming,
    Stationary,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinearMotionEvidence {
    pub max_speed: f32,
    pub seconds_to_full_speed: f32,
    pub seconds_to_stop: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AngularMotionEvidence {
    pub max_yaw_speed_degrees_per_second: f32,
    pub seconds_to_full_yaw_speed: f32,
    pub tolerance_degrees: f32,
    pub aim_tolerance_degrees: f32,
    pub pitch_limit_degrees: f32,
    pub roll_limit_degrees: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MovementSemantics {
    pub architecture: MovementArchitecture,
    pub linear: Option<LinearMotionEvidence>,
    pub angular: Option<AngularMotionEvidence>,
    pub pushable: bool,
    pub opens_doors: bool,
    pub allow_ragdoll_collision: bool,
    pub non_hostile: bool,
    pub use_large_actor_pathing: bool,
    pub use_subsegmented_damage: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DefaultWeightEvidence {
    pub male: [f32; 3],
    pub female: [f32; 3],
    pub ungendered: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceFamilyRaceDataEvidence {
    pub body_bounds: Option<MeasuredBounds>,
    pub skeleton_bounds: Option<MeasuredBounds>,
    pub up_axis: MeasurementAxis,
    pub capsule: Option<CapsuleEvidence>,
    pub geometry_scale_to_fo4: Option<f32>,
    pub male_actor_scale: Option<f32>,
    pub female_actor_scale: Option<f32>,
    pub default_weights: Option<DefaultWeightEvidence>,
    pub movement: Option<MovementSemantics>,
    pub injured_health_percent: Option<f32>,
    pub body_biped_object: Option<i32>,
    pub xp_value: Option<i16>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fo4RaceDataScalePolicy {
    pub small_max_stature: f32,
    pub medium_max_stature: f32,
    pub large_max_stature: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaceDataEvidenceField {
    BodyBounds,
    SkeletonBounds,
    ControllerCapsule,
    GeometryScaleToFo4,
    MaleActorScale,
    FemaleActorScale,
    DefaultWeights,
    MovementArchitecture,
    LinearMotion,
    AngularMotion,
    InjuredHealthPercent,
    BodyBipedObject,
    XpValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RaceDataScaleAudit {
    pub male_runtime_scale: f32,
    pub female_runtime_scale: f32,
    pub scaled_body_stature: f32,
    pub scaled_skeleton_stature: f32,
    pub scaled_capsule_height: f32,
    pub scaled_capsule_radius: f32,
    pub size: Fo4RaceSize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fo4RaceDataDerivation {
    pub mapping: RaceDataMapping,
    pub missing_evidence: Vec<RaceDataEvidenceField>,
    pub scale_audit: Option<RaceDataScaleAudit>,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RaceDataDerivationError {
    #[error("invalid {field:?} evidence: {reason}")]
    InvalidEvidence {
        field: RaceDataEvidenceField,
        reason: String,
    },
    #[error("invalid FO4 size policy: {0}")]
    InvalidSizePolicy(String),
    #[error("derived FO4 RACE DATA is invalid: {0}")]
    InvalidTarget(String),
}

pub fn derive_fo4_race_data(
    evidence: &SourceFamilyRaceDataEvidence,
    policy: &Fo4RaceDataScalePolicy,
) -> Result<Fo4RaceDataDerivation, RaceDataDerivationError> {
    validate_policy(policy)?;
    validate_present_evidence(evidence)?;

    let missing_evidence = missing_evidence(evidence);
    if !missing_evidence.is_empty() {
        return Ok(Fo4RaceDataDerivation {
            mapping: RaceDataMapping::Missing {
                fields: missing_target_fields(&missing_evidence),
            },
            missing_evidence,
            scale_audit: None,
        });
    }

    let body_bounds = evidence
        .body_bounds
        .as_ref()
        .expect("body bounds were checked above");
    let skeleton_bounds = evidence
        .skeleton_bounds
        .as_ref()
        .expect("skeleton bounds were checked above");
    let capsule = evidence
        .capsule
        .as_ref()
        .expect("capsule evidence was checked above");
    let weights = evidence
        .default_weights
        .as_ref()
        .expect("default weights were checked above");
    let movement = evidence
        .movement
        .as_ref()
        .expect("movement semantics were checked above");
    let angular = movement
        .angular
        .as_ref()
        .expect("angular motion was checked above");
    let geometry_scale = evidence
        .geometry_scale_to_fo4
        .expect("geometry scale was checked above");
    let male_height = geometry_scale
        * evidence
            .male_actor_scale
            .expect("male actor scale was checked above");
    let female_height = geometry_scale
        * evidence
            .female_actor_scale
            .expect("female actor scale was checked above");
    let maximum_runtime_scale = male_height.max(female_height);
    let scaled_body_stature = body_bounds.extent(evidence.up_axis) * maximum_runtime_scale;
    let scaled_skeleton_stature = skeleton_bounds.extent(evidence.up_axis) * maximum_runtime_scale;
    let scaled_capsule_height = capsule.total_height * maximum_runtime_scale;
    let scaled_capsule_radius = capsule.radius * maximum_runtime_scale;
    let scaled_stature = scaled_body_stature
        .max(scaled_skeleton_stature)
        .max(scaled_capsule_height);
    let size = classify_size(scaled_stature, policy);
    let (acceleration_rate, deceleration_rate) = match &movement.linear {
        Some(linear) => (
            linear.max_speed / linear.seconds_to_full_speed,
            linear.max_speed / linear.seconds_to_stop,
        ),
        None => (0.0, 0.0),
    };

    let mut flags = movement_flags(movement, capsule.architecture);
    let mut flags_2 = movement_flags_2(movement, capsule.architecture, weights.ungendered);
    flags.sort();
    flags.dedup();
    flags_2.sort();
    flags_2.dedup();

    let target = Fo4RaceDataTarget {
        male_height,
        female_height,
        male_default_weight: weights.male,
        female_default_weight: weights.female,
        flags,
        acceleration_rate,
        deceleration_rate,
        size,
        injured_health_percent: evidence
            .injured_health_percent
            .expect("injury semantics were checked above"),
        body_biped_object: evidence
            .body_biped_object
            .expect("body biped object was checked above"),
        aim_angle_tolerance: angular.aim_tolerance_degrees,
        flight_radius: if matches!(
            movement.architecture,
            MovementArchitecture::Flying | MovementArchitecture::GroundedFlying
        ) {
            scaled_capsule_radius
        } else {
            0.0
        },
        angular_acceleration_rate: angular.max_yaw_speed_degrees_per_second
            / angular.seconds_to_full_yaw_speed,
        angular_tolerance: angular.tolerance_degrees,
        flags_2,
        xp_value: evidence.xp_value.expect("XP value was checked above"),
        orientation_limit_pitch: angular.pitch_limit_degrees,
        orientation_limit_roll: angular.roll_limit_degrees,
    };
    target
        .validate()
        .map_err(|error| RaceDataDerivationError::InvalidTarget(error.to_string()))?;

    Ok(Fo4RaceDataDerivation {
        mapping: RaceDataMapping::Mapped { target },
        missing_evidence,
        scale_audit: Some(RaceDataScaleAudit {
            male_runtime_scale: male_height,
            female_runtime_scale: female_height,
            scaled_body_stature,
            scaled_skeleton_stature,
            scaled_capsule_height,
            scaled_capsule_radius,
            size,
        }),
    })
}

fn validate_policy(policy: &Fo4RaceDataScalePolicy) -> Result<(), RaceDataDerivationError> {
    let values = [
        policy.small_max_stature,
        policy.medium_max_stature,
        policy.large_max_stature,
    ];
    if !values.iter().all(|value| value.is_finite() && *value > 0.0) {
        return Err(RaceDataDerivationError::InvalidSizePolicy(
            "thresholds must be finite and positive".to_string(),
        ));
    }
    if !(values[0] < values[1] && values[1] < values[2]) {
        return Err(RaceDataDerivationError::InvalidSizePolicy(
            "thresholds must be strictly increasing".to_string(),
        ));
    }
    Ok(())
}

fn validate_present_evidence(
    evidence: &SourceFamilyRaceDataEvidence,
) -> Result<(), RaceDataDerivationError> {
    if let Some(bounds) = &evidence.body_bounds {
        bounds.validate(RaceDataEvidenceField::BodyBounds)?;
        if bounds.extent(evidence.up_axis) <= 0.0 {
            return Err(invalid(
                RaceDataEvidenceField::BodyBounds,
                "up-axis extent must be positive",
            ));
        }
    }
    if let Some(bounds) = &evidence.skeleton_bounds {
        bounds.validate(RaceDataEvidenceField::SkeletonBounds)?;
        if bounds.extent(evidence.up_axis) <= 0.0 {
            return Err(invalid(
                RaceDataEvidenceField::SkeletonBounds,
                "up-axis extent must be positive",
            ));
        }
    }
    if let Some(capsule) = &evidence.capsule {
        if !capsule.radius.is_finite()
            || !capsule.total_height.is_finite()
            || capsule.radius <= 0.0
            || capsule.total_height < capsule.radius * 2.0
        {
            return Err(invalid(
                RaceDataEvidenceField::ControllerCapsule,
                "capsule radius must be positive and total height must cover its diameter",
            ));
        }
    }
    validate_optional_positive(
        evidence.geometry_scale_to_fo4,
        RaceDataEvidenceField::GeometryScaleToFo4,
    )?;
    validate_optional_positive(
        evidence.male_actor_scale,
        RaceDataEvidenceField::MaleActorScale,
    )?;
    validate_optional_positive(
        evidence.female_actor_scale,
        RaceDataEvidenceField::FemaleActorScale,
    )?;
    if let Some(weights) = &evidence.default_weights
        && !weights
            .male
            .iter()
            .chain(&weights.female)
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    {
        return Err(invalid(
            RaceDataEvidenceField::DefaultWeights,
            "all morph weights must be finite and in [0, 1]",
        ));
    }
    if let Some(movement) = &evidence.movement {
        validate_movement(movement)?;
        if evidence
            .capsule
            .as_ref()
            .is_some_and(|capsule| capsule.architecture == ControllerArchitecture::Fixed)
            && movement.architecture != MovementArchitecture::Stationary
        {
            return Err(invalid(
                RaceDataEvidenceField::MovementArchitecture,
                "a fixed controller requires stationary movement semantics",
            ));
        }
    }
    if let Some(value) = evidence.injured_health_percent
        && (!value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return Err(invalid(
            RaceDataEvidenceField::InjuredHealthPercent,
            "injured health percent must be finite and in [0, 1]",
        ));
    }
    if let Some(value) = evidence.body_biped_object
        && !(-1..=31).contains(&value)
    {
        return Err(invalid(
            RaceDataEvidenceField::BodyBipedObject,
            "body biped object must be -1 or an index in [0, 31]",
        ));
    }
    Ok(())
}

fn validate_optional_positive(
    value: Option<f32>,
    field: RaceDataEvidenceField,
) -> Result<(), RaceDataDerivationError> {
    if value.is_some_and(|value| !value.is_finite() || value <= 0.0) {
        return Err(invalid(field, "value must be finite and positive"));
    }
    Ok(())
}

fn validate_movement(movement: &MovementSemantics) -> Result<(), RaceDataDerivationError> {
    match (&movement.architecture, &movement.linear) {
        (MovementArchitecture::Stationary, Some(_)) => {
            return Err(invalid(
                RaceDataEvidenceField::LinearMotion,
                "stationary movement must not declare linear motion",
            ));
        }
        (MovementArchitecture::Stationary, None) => {}
        (_, None) => {}
        (_, Some(linear)) => {
            if ![
                linear.max_speed,
                linear.seconds_to_full_speed,
                linear.seconds_to_stop,
            ]
            .iter()
            .all(|value| value.is_finite() && *value > 0.0)
            {
                return Err(invalid(
                    RaceDataEvidenceField::LinearMotion,
                    "mobile speed and acceleration times must be finite and positive",
                ));
            }
        }
    }
    if let Some(angular) = &movement.angular {
        let bounded_angles = [
            angular.tolerance_degrees,
            angular.aim_tolerance_degrees,
            angular.pitch_limit_degrees,
            angular.roll_limit_degrees,
        ];
        if !angular.max_yaw_speed_degrees_per_second.is_finite()
            || angular.max_yaw_speed_degrees_per_second < 0.0
            || !bounded_angles
                .iter()
                .all(|value| value.is_finite() && (0.0..=180.0).contains(value))
            || !angular.seconds_to_full_yaw_speed.is_finite()
            || angular.seconds_to_full_yaw_speed <= 0.0
        {
            return Err(invalid(
                RaceDataEvidenceField::AngularMotion,
                "angular measurements must be finite, bounded, and use a positive acceleration time",
            ));
        }
    }
    Ok(())
}

fn missing_evidence(evidence: &SourceFamilyRaceDataEvidence) -> Vec<RaceDataEvidenceField> {
    let mut missing = Vec::new();
    if evidence.body_bounds.is_none() {
        missing.push(RaceDataEvidenceField::BodyBounds);
    }
    if evidence.skeleton_bounds.is_none() {
        missing.push(RaceDataEvidenceField::SkeletonBounds);
    }
    if evidence.capsule.is_none() {
        missing.push(RaceDataEvidenceField::ControllerCapsule);
    }
    if evidence.geometry_scale_to_fo4.is_none() {
        missing.push(RaceDataEvidenceField::GeometryScaleToFo4);
    }
    if evidence.male_actor_scale.is_none() {
        missing.push(RaceDataEvidenceField::MaleActorScale);
    }
    if evidence.female_actor_scale.is_none() {
        missing.push(RaceDataEvidenceField::FemaleActorScale);
    }
    if evidence.default_weights.is_none() {
        missing.push(RaceDataEvidenceField::DefaultWeights);
    }
    match &evidence.movement {
        None => {
            missing.push(RaceDataEvidenceField::MovementArchitecture);
            missing.push(RaceDataEvidenceField::LinearMotion);
            missing.push(RaceDataEvidenceField::AngularMotion);
        }
        Some(movement) => {
            if movement.architecture != MovementArchitecture::Stationary
                && movement.linear.is_none()
            {
                missing.push(RaceDataEvidenceField::LinearMotion);
            }
            if movement.angular.is_none() {
                missing.push(RaceDataEvidenceField::AngularMotion);
            }
        }
    }
    if evidence.injured_health_percent.is_none() {
        missing.push(RaceDataEvidenceField::InjuredHealthPercent);
    }
    if evidence.body_biped_object.is_none() {
        missing.push(RaceDataEvidenceField::BodyBipedObject);
    }
    if evidence.xp_value.is_none() {
        missing.push(RaceDataEvidenceField::XpValue);
    }
    missing.sort();
    missing.dedup();
    missing
}

fn missing_target_fields(missing: &[RaceDataEvidenceField]) -> Vec<Fo4RaceDataField> {
    let mut fields = Vec::new();
    for evidence in missing {
        match evidence {
            RaceDataEvidenceField::BodyBounds | RaceDataEvidenceField::SkeletonBounds => {
                fields.push(Fo4RaceDataField::Size);
            }
            RaceDataEvidenceField::ControllerCapsule => {
                fields.push(Fo4RaceDataField::ControllerCapsule);
                fields.push(Fo4RaceDataField::FlightRadius);
                fields.push(Fo4RaceDataField::Size);
            }
            RaceDataEvidenceField::GeometryScaleToFo4
            | RaceDataEvidenceField::MaleActorScale
            | RaceDataEvidenceField::FemaleActorScale => {
                fields.push(Fo4RaceDataField::Heights);
                fields.push(Fo4RaceDataField::FlightRadius);
                fields.push(Fo4RaceDataField::Size);
            }
            RaceDataEvidenceField::DefaultWeights => {
                fields.push(Fo4RaceDataField::DefaultWeights);
                fields.push(Fo4RaceDataField::MovementFlags);
            }
            RaceDataEvidenceField::MovementArchitecture => {
                fields.push(Fo4RaceDataField::MovementFlags);
                fields.push(Fo4RaceDataField::FlightRadius);
            }
            RaceDataEvidenceField::LinearMotion => {
                fields.push(Fo4RaceDataField::LinearAcceleration);
            }
            RaceDataEvidenceField::AngularMotion => {
                fields.push(Fo4RaceDataField::AngularAcceleration);
                fields.push(Fo4RaceDataField::AimAngleTolerance);
                fields.push(Fo4RaceDataField::OrientationLimits);
            }
            RaceDataEvidenceField::InjuredHealthPercent => {
                fields.push(Fo4RaceDataField::InjuredHealthPercent);
            }
            RaceDataEvidenceField::BodyBipedObject => {
                fields.push(Fo4RaceDataField::BodyBipedObject);
            }
            RaceDataEvidenceField::XpValue => fields.push(Fo4RaceDataField::XpValue),
        }
    }
    fields.sort();
    fields.dedup();
    fields
}

fn movement_flags(
    movement: &MovementSemantics,
    controller: ControllerArchitecture,
) -> Vec<Fo4RaceFlag> {
    let mut flags = match movement.architecture {
        MovementArchitecture::Grounded => vec![Fo4RaceFlag::Walks],
        MovementArchitecture::GroundedSwimming => {
            vec![Fo4RaceFlag::Walks, Fo4RaceFlag::Swims]
        }
        MovementArchitecture::GroundedFlying => {
            vec![Fo4RaceFlag::Walks, Fo4RaceFlag::Flies]
        }
        MovementArchitecture::Flying => vec![Fo4RaceFlag::Flies],
        MovementArchitecture::Swimming => vec![Fo4RaceFlag::Swims],
        MovementArchitecture::Stationary => vec![Fo4RaceFlag::Immobile],
    };
    if !movement.pushable {
        flags.push(Fo4RaceFlag::NotPushable);
    }
    if !movement.opens_doors {
        flags.push(Fo4RaceFlag::CantOpenDoors);
    }
    if movement.allow_ragdoll_collision {
        flags.push(Fo4RaceFlag::AllowRagdollCollision);
    }
    if controller == ControllerArchitecture::Proxy {
        flags.push(Fo4RaceFlag::AlwaysUseProxyController);
    }
    flags
}

fn movement_flags_2(
    movement: &MovementSemantics,
    controller: ControllerArchitecture,
    ungendered: bool,
) -> Vec<Fo4RaceFlag2> {
    let mut flags = Vec::new();
    if controller == ControllerArchitecture::Quadruped {
        flags.push(Fo4RaceFlag2::UseQuadrupedController);
    }
    if movement.non_hostile {
        flags.push(Fo4RaceFlag2::NonHostile);
    }
    if movement.use_large_actor_pathing {
        flags.push(Fo4RaceFlag2::UseLargeActorPathing);
    }
    if movement.use_subsegmented_damage {
        flags.push(Fo4RaceFlag2::UseSubsegmentedDamage);
    }
    if ungendered {
        flags.push(Fo4RaceFlag2::Ungendered);
    }
    flags
}

fn classify_size(stature: f32, policy: &Fo4RaceDataScalePolicy) -> Fo4RaceSize {
    if stature <= policy.small_max_stature {
        Fo4RaceSize::Small
    } else if stature <= policy.medium_max_stature {
        Fo4RaceSize::Medium
    } else if stature <= policy.large_max_stature {
        Fo4RaceSize::Large
    } else {
        Fo4RaceSize::ExtraLarge
    }
}

fn invalid(field: RaceDataEvidenceField, reason: &str) -> RaceDataDerivationError {
    RaceDataDerivationError::InvalidEvidence {
        field,
        reason: reason.to_string(),
    }
}
