//! Deterministic, source-neutral creature corpus planning.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::manifest::CreatureGraphTemplate;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceCreatureIdentity {
    pub namespace: String,
    pub plugin: String,
    pub local_form_id: u32,
}

impl SourceCreatureIdentity {
    pub fn stable_key(&self) -> String {
        format!(
            "{}|{}|{:08x}",
            self.namespace.to_ascii_lowercase(),
            self.plugin.to_ascii_lowercase(),
            self.local_form_id
        )
    }

    pub(crate) fn is_valid(&self) -> bool {
        !self.namespace.trim().is_empty()
            && !self.plugin.trim().is_empty()
            && self.local_form_id != 0
            && self.local_form_id <= 0x00ff_ffff
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fo4RaceSize {
    Small,
    Medium,
    Large,
    ExtraLarge,
}

impl Fo4RaceSize {
    pub(crate) fn value(self) -> u64 {
        match self {
            Self::Small => 0,
            Self::Medium => 1,
            Self::Large => 2,
            Self::ExtraLarge => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fo4RaceFlag {
    TiltFrontBack,
    TiltLeftRight,
    NoShadow,
    Swims,
    Flies,
    Walks,
    Immobile,
    NotPushable,
    NoCombatInWater,
    NoRotatingToHeadTrack,
    DontShowBloodSpray,
    DontShowBloodDecal,
    UsesHeadTrackAnimations,
    SpellsAlignWithMagicNode,
    UseWorldRaycastsForFootIk,
    AllowRagdollCollision,
    RegenerateHealthInCombat,
    CantOpenDoors,
    NoKnockdowns,
    AlwaysUseProxyController,
    DontShowWeaponBlood,
    CanPickupItems,
    AllowMultipleMembraneShaders,
    AvoidsRoads,
}

impl Fo4RaceFlag {
    pub(crate) fn bit(self) -> u32 {
        match self {
            Self::TiltFrontBack => 1 << 3,
            Self::TiltLeftRight => 1 << 4,
            Self::NoShadow => 1 << 5,
            Self::Swims => 1 << 6,
            Self::Flies => 1 << 7,
            Self::Walks => 1 << 8,
            Self::Immobile => 1 << 9,
            Self::NotPushable => 1 << 10,
            Self::NoCombatInWater => 1 << 11,
            Self::NoRotatingToHeadTrack => 1 << 12,
            Self::DontShowBloodSpray => 1 << 13,
            Self::DontShowBloodDecal => 1 << 14,
            Self::UsesHeadTrackAnimations => 1 << 15,
            Self::SpellsAlignWithMagicNode => 1 << 16,
            Self::UseWorldRaycastsForFootIk => 1 << 17,
            Self::AllowRagdollCollision => 1 << 18,
            Self::RegenerateHealthInCombat => 1 << 19,
            Self::CantOpenDoors => 1 << 20,
            Self::NoKnockdowns => 1 << 22,
            Self::AlwaysUseProxyController => 1 << 24,
            Self::DontShowWeaponBlood => 1 << 25,
            Self::CanPickupItems => 1 << 28,
            Self::AllowMultipleMembraneShaders => 1 << 29,
            Self::AvoidsRoads => 1 << 31,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fo4RaceFlag2 {
    UseAdvancedAvoidance,
    NonHostile,
    Floats,
    CanMeleeWhenKnockedDown,
    Ungendered,
    CanMoveWhenKnockedDown,
    UseLargeActorPathing,
    UseSubsegmentedDamage,
    FlightDeferKill,
    FlightAllowProceduralCrashLand,
    DisableWeaponCulling,
    UseOptimalSpeeds,
    HasFacialRig,
    CanUseCrippledLimbs,
    UseQuadrupedController,
    LowPriorityPushable,
    CannotUsePlayableItems,
}

impl Fo4RaceFlag2 {
    pub(crate) fn bit(self) -> u32 {
        match self {
            Self::UseAdvancedAvoidance => 1,
            Self::NonHostile => 1 << 1,
            Self::Floats => 1 << 2,
            Self::CanMeleeWhenKnockedDown => 1 << 7,
            Self::Ungendered => 1 << 9,
            Self::CanMoveWhenKnockedDown => 1 << 10,
            Self::UseLargeActorPathing => 1 << 11,
            Self::UseSubsegmentedDamage => 1 << 12,
            Self::FlightDeferKill => 1 << 13,
            Self::FlightAllowProceduralCrashLand => 1 << 15,
            Self::DisableWeaponCulling => 1 << 16,
            Self::UseOptimalSpeeds => 1 << 17,
            Self::HasFacialRig => 1 << 18,
            Self::CanUseCrippledLimbs => 1 << 19,
            Self::UseQuadrupedController => 1 << 20,
            Self::LowPriorityPushable => 1 << 21,
            Self::CannotUsePlayableItems => 1 << 22,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fo4RaceDataTarget {
    pub male_height: f32,
    pub female_height: f32,
    pub male_default_weight: [f32; 3],
    pub female_default_weight: [f32; 3],
    pub flags: Vec<Fo4RaceFlag>,
    pub acceleration_rate: f32,
    pub deceleration_rate: f32,
    pub size: Fo4RaceSize,
    pub injured_health_percent: f32,
    pub body_biped_object: i32,
    pub aim_angle_tolerance: f32,
    pub flight_radius: f32,
    pub angular_acceleration_rate: f32,
    pub angular_tolerance: f32,
    pub flags_2: Vec<Fo4RaceFlag2>,
    pub xp_value: i16,
    pub orientation_limit_pitch: f32,
    pub orientation_limit_roll: f32,
}

impl Fo4RaceDataTarget {
    pub fn validate(&self) -> Result<(), RaceDataValidationError> {
        let values = [
            ("male_height", self.male_height),
            ("female_height", self.female_height),
            ("male_default_weight_thin", self.male_default_weight[0]),
            ("male_default_weight_muscular", self.male_default_weight[1]),
            ("male_default_weight_fat", self.male_default_weight[2]),
            ("female_default_weight_thin", self.female_default_weight[0]),
            (
                "female_default_weight_muscular",
                self.female_default_weight[1],
            ),
            ("female_default_weight_fat", self.female_default_weight[2]),
            ("acceleration_rate", self.acceleration_rate),
            ("deceleration_rate", self.deceleration_rate),
            ("injured_health_percent", self.injured_health_percent),
            ("aim_angle_tolerance", self.aim_angle_tolerance),
            ("flight_radius", self.flight_radius),
            ("angular_acceleration_rate", self.angular_acceleration_rate),
            ("angular_tolerance", self.angular_tolerance),
            ("orientation_limit_pitch", self.orientation_limit_pitch),
            ("orientation_limit_roll", self.orientation_limit_roll),
        ];
        for (field, value) in values {
            if !value.is_finite() {
                return Err(RaceDataValidationError::NonFinite { field });
            }
        }
        if self.male_height <= 0.0 || self.female_height <= 0.0 {
            return Err(RaceDataValidationError::NonPositiveHeight);
        }
        if !(0.0..=1.0).contains(&self.injured_health_percent) {
            return Err(RaceDataValidationError::InvalidInjuredHealthPercent);
        }
        if [
            self.acceleration_rate,
            self.deceleration_rate,
            self.aim_angle_tolerance,
            self.flight_radius,
            self.angular_acceleration_rate,
            self.angular_tolerance,
        ]
        .iter()
        .any(|value| *value < 0.0)
        {
            return Err(RaceDataValidationError::NegativeMovementValue);
        }
        if !(-1..=31).contains(&self.body_biped_object) {
            return Err(RaceDataValidationError::InvalidBodyBipedObject);
        }
        if !self
            .male_default_weight
            .iter()
            .chain(&self.female_default_weight)
            .all(|value| (0.0..=1.0).contains(value))
        {
            return Err(RaceDataValidationError::InvalidDefaultWeight);
        }
        Ok(())
    }

    pub(crate) fn flags_bits(&self) -> u32 {
        self.flags.iter().fold(0, |bits, flag| bits | flag.bit())
    }

    pub(crate) fn flags_2_bits(&self) -> u32 {
        self.flags_2.iter().fold(0, |bits, flag| bits | flag.bit())
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum RaceDataValidationError {
    #[error("FO4 RACE DATA field {field} is not finite")]
    NonFinite { field: &'static str },
    #[error("FO4 RACE DATA height must be positive")]
    NonPositiveHeight,
    #[error("FO4 RACE DATA injured health percent must be in [0, 1]")]
    InvalidInjuredHealthPercent,
    #[error("FO4 RACE DATA default weights must be in [0, 1]")]
    InvalidDefaultWeight,
    #[error("FO4 RACE DATA movement values must be non-negative")]
    NegativeMovementValue,
    #[error("FO4 RACE DATA body biped object must be -1 or an index in [0, 31]")]
    InvalidBodyBipedObject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fo4RaceDataField {
    Heights,
    DefaultWeights,
    MovementFlags,
    LinearAcceleration,
    AngularAcceleration,
    AimAngleTolerance,
    FlightRadius,
    OrientationLimits,
    ControllerCapsule,
    Size,
    BodyBipedObject,
    InjuredHealthPercent,
    XpValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RaceDataMapping {
    Mapped { target: Fo4RaceDataTarget },
    Missing { fields: Vec<Fo4RaceDataField> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RigFamily {
    pub id: String,
    pub root_node: String,
    pub race_data: RaceDataMapping,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotionAttack {
    pub id: String,
    pub event: String,
    pub clip: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionAttackKind {
    MeleeUnarmed,
    RangedProjectile,
    SpellAbility,
    ContinuousRobot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotionOverlayEvidence {
    pub id: String,
    pub clip: String,
    pub start_event: String,
    pub stop_event: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotionRigEvidence {
    pub id: String,
    pub root_node: String,
    pub skeleton_nif: String,
    pub project_hkx: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotionSet {
    pub id: String,
    #[serde(default, skip_serializing_if = "is_ground_melee_template")]
    pub graph_template: CreatureGraphTemplate,
    pub idle_clip: String,
    pub locomotion_clips: BTreeMap<String, String>,
    pub attacks: Vec<MotionAttack>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attack_kinds: BTreeMap<String, MotionAttackKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_overlays: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlays: Vec<MotionOverlayEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_rigs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rigs: Vec<MotionRigEvidence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordVariant {
    pub source_identity: SourceCreatureIdentity,
    pub output_slug: String,
    pub display_name: String,
    pub body_nif: String,
    pub level: u16,
    pub health: u16,
    pub action_points: u16,
    pub primary: bool,
    pub attack_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureCorpusCandidate {
    pub source_identity: SourceCreatureIdentity,
    pub primary_record_identity: SourceCreatureIdentity,
    pub output_slug: String,
    pub rig_family: String,
    pub motion_set: String,
    pub record_variants: Vec<RecordVariant>,
    pub preflight_rejections: Vec<CreatureRejectionReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureCapability {
    StableSourceIdentity,
    CustomRig,
    RaceData,
    Idle,
    Locomotion,
    Melee,
    RangedProjectile,
    SpellAbility,
    Swim,
    Fly,
    Stationary,
    ContinuousAttack,
    Overlay,
    MultiRig,
    RecordProjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureReadiness {
    Ready,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityLedger {
    pub required: Vec<CreatureCapability>,
    pub available: Vec<CreatureCapability>,
    pub missing: Vec<CreatureCapability>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlannedCreature {
    pub source_identity: SourceCreatureIdentity,
    pub primary_record_identity: SourceCreatureIdentity,
    pub source_key: String,
    pub output_slug: String,
    pub readiness: CreatureReadiness,
    pub capabilities: CapabilityLedger,
    pub rig_family: RigFamily,
    pub motion_set: MotionSet,
    pub record_variants: Vec<RecordVariant>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum CreatureRejectionReason {
    InvalidSourceIdentity,
    InvalidPrimaryRecordIdentity,
    InvalidOutputSlug {
        slug: String,
    },
    DuplicateSourceIdentity {
        source_key: String,
    },
    DuplicateOutputSlug {
        slug: String,
    },
    MissingRigFamily {
        id: String,
    },
    AmbiguousRigFamily {
        id: String,
    },
    InvalidRigFamily {
        id: String,
    },
    MissingRaceData {
        family: String,
        fields: Vec<Fo4RaceDataField>,
    },
    InvalidRaceData {
        family: String,
        message: String,
    },
    MissingMotionSet {
        id: String,
    },
    AmbiguousMotionSet {
        id: String,
    },
    InvalidMotionSet {
        id: String,
    },
    MissingPrimaryRecordVariant,
    MultiplePrimaryRecordVariants,
    PrimaryIdentityMismatch {
        primary_source_key: String,
    },
    DuplicateRecordVariantSlug {
        slug: String,
    },
    DuplicateRecordVariantSource {
        source_key: String,
    },
    DuplicateRecordOutput {
        output: String,
    },
    InvalidRecordVariantSlug {
        slug: String,
    },
    InvalidRecordVariant {
        slug: String,
        reason: String,
    },
    MissingAttackMapping {
        variant: String,
        attack: String,
    },
    MissingAttackSemantics {
        motion_set: String,
        attack: String,
    },
    UnsupportedAttackKind {
        motion_set: String,
        attack: String,
        kind: MotionAttackKind,
        template: String,
    },
    MissingOverlayEvidence {
        motion_set: String,
        overlays: Vec<String>,
    },
    InvalidOverlayEvidence {
        motion_set: String,
        overlays: Vec<String>,
    },
    MissingRigEvidence {
        motion_set: String,
        rigs: Vec<String>,
    },
    InvalidRigEvidence {
        motion_set: String,
        rigs: Vec<String>,
    },
    MissingCapabilities {
        capabilities: Vec<CreatureCapability>,
    },
    UpstreamCatalogRejected {
        catalog: String,
        reason_code: String,
        detail: String,
    },
    CuratedExclusion {
        policy: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedCreature {
    pub source_identity: SourceCreatureIdentity,
    pub primary_record_identity: SourceCreatureIdentity,
    pub source_key: String,
    pub output_slug: String,
    pub readiness: CreatureReadiness,
    pub capabilities: CapabilityLedger,
    pub reasons: Vec<CreatureRejectionReason>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureCorpusPlan {
    pub version: u32,
    pub candidate_count: usize,
    pub planned: Vec<PlannedCreature>,
    pub rejected: Vec<RejectedCreature>,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CreatureCorpusPlanError {
    #[error("candidate accounting mismatch: {candidate_count} != {planned} + {rejected}")]
    Accounting {
        candidate_count: usize,
        planned: usize,
        rejected: usize,
    },
    #[error("planned corpus has duplicate source identity {0}")]
    DuplicatePlannedSource(String),
    #[error("planned corpus has duplicate output slug {0}")]
    DuplicatePlannedOutput(String),
    #[error("planned corpus has duplicate record-variant source identity {0}")]
    DuplicatePlannedVariantSource(String),
    #[error("planned corpus has duplicate record output {0}")]
    DuplicatePlannedRecordOutput(String),
    #[error("planned corpus entry {0} is not ready")]
    PlannedEntryNotReady(String),
    #[error("planned corpus entry {0} lost its primary source mapping")]
    InvalidPlannedPrimary(String),
    #[error("rejected corpus entry {0} has no typed reason")]
    EmptyRejection(String),
    #[error("rejected corpus entry {0} has ready status")]
    RejectedEntryReady(String),
    #[error("planned corpus is not in deterministic order")]
    NonDeterministicPlannedOrder,
    #[error("rejection ledger is not in deterministic order")]
    NonDeterministicRejectionOrder,
    #[error("failed to serialize creature corpus plan: {0}")]
    Serialization(String),
}

impl CreatureCorpusPlan {
    pub fn build(
        mut rig_families: Vec<RigFamily>,
        mut motion_sets: Vec<MotionSet>,
        candidates: Vec<CreatureCorpusCandidate>,
    ) -> Result<Self, CreatureCorpusPlanError> {
        for family in &mut rig_families {
            match &mut family.race_data {
                RaceDataMapping::Mapped { target } => {
                    target.flags.sort();
                    target.flags.dedup();
                    target.flags_2.sort();
                    target.flags_2.dedup();
                }
                RaceDataMapping::Missing { fields } => {
                    fields.sort();
                    fields.dedup();
                }
            }
        }
        for motion in &mut motion_sets {
            motion.attacks.sort_by_key(|attack| {
                (
                    attack.id.to_ascii_lowercase(),
                    attack.event.to_ascii_lowercase(),
                    attack.clip.to_ascii_lowercase(),
                )
            });
            motion.attack_kinds = std::mem::take(&mut motion.attack_kinds)
                .into_iter()
                .map(|(id, kind)| (id.to_ascii_lowercase(), kind))
                .collect();
            sort_dedup_case_insensitive(&mut motion.required_overlays);
            sort_dedup_case_insensitive(&mut motion.required_rigs);
            motion.overlays.sort_by_key(|overlay| {
                (
                    overlay.id.to_ascii_lowercase(),
                    overlay.clip.to_ascii_lowercase(),
                )
            });
            motion.rigs.sort_by_key(|rig| {
                (
                    rig.id.to_ascii_lowercase(),
                    rig.skeleton_nif.to_ascii_lowercase(),
                )
            });
        }
        let rig_catalog = group_by_id(rig_families, |family| &family.id);
        let motion_catalog = group_by_id(motion_sets, |motion| &motion.id);
        let source_counts = counts(
            candidates
                .iter()
                .map(|candidate| candidate.source_identity.stable_key()),
        );
        let output_counts = counts(
            candidates
                .iter()
                .map(|candidate| candidate.output_slug.to_ascii_lowercase()),
        );
        let variant_source_counts = counts(candidates.iter().flat_map(|candidate| {
            candidate
                .record_variants
                .iter()
                .map(|variant| variant.source_identity.stable_key())
        }));
        let record_output_counts = counts(candidates.iter().flat_map(|candidate| {
            candidate.record_variants.iter().map(|variant| {
                format!(
                    "{}/{}",
                    candidate.output_slug.to_ascii_lowercase(),
                    variant.output_slug.to_ascii_lowercase()
                )
            })
        }));

        let mut candidates = candidates;
        candidates.sort_by_key(candidate_sort_key);
        let mut planned = Vec::new();
        let mut rejected = Vec::new();

        for mut candidate in candidates {
            candidate.record_variants.sort_by_key(variant_sort_key);
            for variant in &mut candidate.record_variants {
                variant.attack_ids.sort();
                variant.attack_ids.dedup();
            }

            let source_key = candidate.source_identity.stable_key();
            let mut reasons = candidate.preflight_rejections.clone();
            if !candidate.source_identity.is_valid() {
                reasons.push(CreatureRejectionReason::InvalidSourceIdentity);
            }
            if !candidate.primary_record_identity.is_valid() {
                reasons.push(CreatureRejectionReason::InvalidPrimaryRecordIdentity);
            }
            if !valid_slug(&candidate.output_slug) {
                reasons.push(CreatureRejectionReason::InvalidOutputSlug {
                    slug: candidate.output_slug.clone(),
                });
            }
            if source_counts.get(&source_key).copied().unwrap_or_default() > 1 {
                reasons.push(CreatureRejectionReason::DuplicateSourceIdentity {
                    source_key: source_key.clone(),
                });
            }
            let output_key = candidate.output_slug.to_ascii_lowercase();
            if output_counts.get(&output_key).copied().unwrap_or_default() > 1 {
                reasons.push(CreatureRejectionReason::DuplicateOutputSlug {
                    slug: candidate.output_slug.clone(),
                });
            }

            let family = unique_mapping(&rig_catalog, &candidate.rig_family, &mut reasons, true);
            let motion =
                unique_mapping(&motion_catalog, &candidate.motion_set, &mut reasons, false);
            let capabilities = capability_ledger(&candidate, family, motion);

            if let Some(family) = family {
                if family.id.trim().is_empty() || family.root_node.trim().is_empty() {
                    reasons.push(CreatureRejectionReason::InvalidRigFamily {
                        id: family.id.clone(),
                    });
                }
                match &family.race_data {
                    RaceDataMapping::Mapped { target } => {
                        if let Err(error) = target.validate() {
                            reasons.push(CreatureRejectionReason::InvalidRaceData {
                                family: family.id.clone(),
                                message: error.to_string(),
                            });
                        }
                    }
                    RaceDataMapping::Missing { fields } => {
                        let mut fields = fields.clone();
                        fields.sort();
                        fields.dedup();
                        reasons.push(CreatureRejectionReason::MissingRaceData {
                            family: family.id.clone(),
                            fields,
                        });
                    }
                }
            }

            if let Some(motion) = motion {
                reasons.extend(motion_rejections(motion));
            }

            let primary = candidate
                .record_variants
                .iter()
                .filter(|variant| variant.primary)
                .collect::<Vec<_>>();
            match primary.as_slice() {
                [] => reasons.push(CreatureRejectionReason::MissingPrimaryRecordVariant),
                [variant]
                    if variant.source_identity.stable_key()
                        != candidate.primary_record_identity.stable_key() =>
                {
                    reasons.push(CreatureRejectionReason::PrimaryIdentityMismatch {
                        primary_source_key: variant.source_identity.stable_key(),
                    });
                }
                [_] => {}
                _ => reasons.push(CreatureRejectionReason::MultiplePrimaryRecordVariants),
            }

            let mut variant_slugs = BTreeSet::new();
            let attack_ids = motion
                .map(|motion| {
                    motion
                        .attacks
                        .iter()
                        .map(|attack| attack.id.to_ascii_lowercase())
                        .collect::<BTreeSet<_>>()
                })
                .unwrap_or_default();
            for variant in &candidate.record_variants {
                let variant_source_key = variant.source_identity.stable_key();
                let record_output = format!(
                    "{}/{}",
                    candidate.output_slug.to_ascii_lowercase(),
                    variant.output_slug.to_ascii_lowercase()
                );
                if !valid_slug(&variant.output_slug) {
                    reasons.push(CreatureRejectionReason::InvalidRecordVariantSlug {
                        slug: variant.output_slug.clone(),
                    });
                }
                if variant.display_name.trim().is_empty()
                    || !variant.body_nif.to_ascii_lowercase().ends_with(".nif")
                {
                    reasons.push(CreatureRejectionReason::InvalidRecordVariant {
                        slug: variant.output_slug.clone(),
                        reason: "display_name and body_nif are required".to_string(),
                    });
                }
                if !variant_slugs.insert(variant.output_slug.to_ascii_lowercase()) {
                    reasons.push(CreatureRejectionReason::DuplicateRecordVariantSlug {
                        slug: variant.output_slug.clone(),
                    });
                }
                if variant_source_counts
                    .get(&variant_source_key)
                    .copied()
                    .unwrap_or_default()
                    > 1
                {
                    reasons.push(CreatureRejectionReason::DuplicateRecordVariantSource {
                        source_key: variant_source_key,
                    });
                }
                if record_output_counts
                    .get(&record_output)
                    .copied()
                    .unwrap_or_default()
                    > 1
                {
                    reasons.push(CreatureRejectionReason::DuplicateRecordOutput {
                        output: record_output,
                    });
                }
                for attack in &variant.attack_ids {
                    if !attack_ids.contains(&attack.to_ascii_lowercase()) {
                        reasons.push(CreatureRejectionReason::MissingAttackMapping {
                            variant: variant.output_slug.clone(),
                            attack: attack.clone(),
                        });
                    }
                }
            }
            if !capabilities.missing.is_empty() {
                reasons.push(CreatureRejectionReason::MissingCapabilities {
                    capabilities: capabilities.missing.clone(),
                });
            }

            reasons.sort();
            reasons.dedup();
            if reasons.is_empty() {
                planned.push(PlannedCreature {
                    source_identity: candidate.source_identity,
                    primary_record_identity: candidate.primary_record_identity,
                    source_key,
                    output_slug: candidate.output_slug,
                    readiness: CreatureReadiness::Ready,
                    capabilities,
                    rig_family: family.expect("missing family is a typed rejection").clone(),
                    motion_set: motion
                        .expect("missing motion set is a typed rejection")
                        .clone(),
                    record_variants: candidate.record_variants,
                });
            } else {
                rejected.push(RejectedCreature {
                    source_identity: candidate.source_identity,
                    primary_record_identity: candidate.primary_record_identity,
                    source_key,
                    output_slug: candidate.output_slug,
                    readiness: CreatureReadiness::Rejected,
                    capabilities,
                    reasons,
                });
            }
        }

        planned.sort_by_key(planned_sort_key);
        rejected.sort_by_key(rejected_sort_key);
        let plan = Self {
            version: 1,
            candidate_count: planned.len() + rejected.len(),
            planned,
            rejected,
        };
        plan.validate()?;
        Ok(plan)
    }

    pub fn validate(&self) -> Result<(), CreatureCorpusPlanError> {
        if self.candidate_count != self.planned.len() + self.rejected.len() {
            return Err(CreatureCorpusPlanError::Accounting {
                candidate_count: self.candidate_count,
                planned: self.planned.len(),
                rejected: self.rejected.len(),
            });
        }
        if !is_sorted_by(&self.planned, planned_sort_key) {
            return Err(CreatureCorpusPlanError::NonDeterministicPlannedOrder);
        }
        if !is_sorted_by(&self.rejected, rejected_sort_key) {
            return Err(CreatureCorpusPlanError::NonDeterministicRejectionOrder);
        }

        let mut sources = BTreeSet::new();
        let mut outputs = BTreeSet::new();
        let mut variant_sources = BTreeSet::new();
        let mut record_outputs = BTreeSet::new();
        for entry in &self.planned {
            if entry.readiness != CreatureReadiness::Ready || !entry.capabilities.missing.is_empty()
            {
                return Err(CreatureCorpusPlanError::PlannedEntryNotReady(
                    entry.source_key.clone(),
                ));
            }
            if !sources.insert(entry.source_key.to_ascii_lowercase()) {
                return Err(CreatureCorpusPlanError::DuplicatePlannedSource(
                    entry.source_key.clone(),
                ));
            }
            if !outputs.insert(entry.output_slug.to_ascii_lowercase()) {
                return Err(CreatureCorpusPlanError::DuplicatePlannedOutput(
                    entry.output_slug.clone(),
                ));
            }
            let primaries = entry
                .record_variants
                .iter()
                .filter(|variant| variant.primary)
                .collect::<Vec<_>>();
            if !entry.source_identity.is_valid()
                || !entry.primary_record_identity.is_valid()
                || entry.source_key != entry.source_identity.stable_key()
                || !matches!(
                    primaries.as_slice(),
                    [primary]
                        if primary.source_identity.stable_key()
                            == entry.primary_record_identity.stable_key()
                )
            {
                return Err(CreatureCorpusPlanError::InvalidPlannedPrimary(
                    entry.source_key.clone(),
                ));
            }
            for variant in &entry.record_variants {
                let source_key = variant.source_identity.stable_key();
                if !variant_sources.insert(source_key.clone()) {
                    return Err(CreatureCorpusPlanError::DuplicatePlannedVariantSource(
                        source_key,
                    ));
                }
                let output = format!(
                    "{}/{}",
                    entry.output_slug.to_ascii_lowercase(),
                    variant.output_slug.to_ascii_lowercase()
                );
                if !record_outputs.insert(output.clone()) {
                    return Err(CreatureCorpusPlanError::DuplicatePlannedRecordOutput(
                        output,
                    ));
                }
            }
        }
        for entry in &self.rejected {
            if entry.readiness != CreatureReadiness::Rejected {
                return Err(CreatureCorpusPlanError::RejectedEntryReady(
                    entry.source_key.clone(),
                ));
            }
            if entry.reasons.is_empty() {
                return Err(CreatureCorpusPlanError::EmptyRejection(
                    entry.source_key.clone(),
                ));
            }
        }
        Ok(())
    }

    pub fn canonical_json(&self) -> Result<String, CreatureCorpusPlanError> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| CreatureCorpusPlanError::Serialization(error.to_string()))
    }
}

fn required_capabilities(motion: Option<&MotionSet>) -> BTreeSet<CreatureCapability> {
    let mut required = [
        CreatureCapability::StableSourceIdentity,
        CreatureCapability::CustomRig,
        CreatureCapability::RaceData,
        CreatureCapability::Idle,
        CreatureCapability::RecordProjection,
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let template = motion
        .map(|motion| motion.graph_template)
        .unwrap_or(CreatureGraphTemplate::GroundMelee);
    match template {
        CreatureGraphTemplate::GroundMelee => {
            required.insert(CreatureCapability::Locomotion);
            required.insert(CreatureCapability::Melee);
        }
        CreatureGraphTemplate::PassiveGround => {}
        CreatureGraphTemplate::GroundRangedProjectile => {
            required.insert(CreatureCapability::Locomotion);
            require_declared_ranged_capabilities(motion, &mut required);
        }
        CreatureGraphTemplate::GroundMeleeRanged => {
            required.insert(CreatureCapability::Locomotion);
            required.insert(CreatureCapability::Melee);
            require_declared_ranged_capabilities(motion, &mut required);
        }
        CreatureGraphTemplate::GroundSwim => {
            required.insert(CreatureCapability::Locomotion);
            required.insert(CreatureCapability::Swim);
        }
        CreatureGraphTemplate::GroundFly => {
            required.insert(CreatureCapability::Locomotion);
            required.insert(CreatureCapability::Fly);
        }
        CreatureGraphTemplate::Swim => {
            required.insert(CreatureCapability::Swim);
        }
        CreatureGraphTemplate::Fly => {
            required.insert(CreatureCapability::Fly);
        }
        CreatureGraphTemplate::StationaryTurret => {
            required.insert(CreatureCapability::Stationary);
            require_declared_ranged_capabilities(motion, &mut required);
        }
        CreatureGraphTemplate::RobotContinuousAttack => {
            required.insert(CreatureCapability::ContinuousAttack);
        }
    }
    if let Some(motion) = motion {
        for kind in resolved_attack_kinds(motion) {
            required.insert(capability_for_attack_kind(kind));
        }
        if !motion.required_overlays.is_empty() {
            required.insert(CreatureCapability::Overlay);
        }
        if !motion.required_rigs.is_empty() {
            required.insert(CreatureCapability::MultiRig);
        }
    }
    required
}

fn capability_ledger(
    candidate: &CreatureCorpusCandidate,
    family: Option<&RigFamily>,
    motion: Option<&MotionSet>,
) -> CapabilityLedger {
    let required = required_capabilities(motion);
    let mut available = BTreeSet::new();
    if candidate.source_identity.is_valid() && candidate.primary_record_identity.is_valid() {
        available.insert(CreatureCapability::StableSourceIdentity);
    }
    if family.is_some_and(|family| !family.root_node.trim().is_empty()) {
        available.insert(CreatureCapability::CustomRig);
    }
    if family.is_some_and(|family| matches!(family.race_data, RaceDataMapping::Mapped { .. })) {
        available.insert(CreatureCapability::RaceData);
    }
    if motion.is_some_and(|motion| !motion.idle_clip.trim().is_empty()) {
        available.insert(CreatureCapability::Idle);
    }
    if motion.is_some_and(has_locomotion) {
        available.insert(CreatureCapability::Locomotion);
    }
    if let Some(motion) = motion {
        for kind in resolved_attack_kinds(motion) {
            available.insert(capability_for_attack_kind(kind));
        }
        if has_swim_motion(motion) {
            available.insert(CreatureCapability::Swim);
        }
        if has_fly_motion(motion) {
            available.insert(CreatureCapability::Fly);
        }
        if motion.graph_template == CreatureGraphTemplate::StationaryTurret
            && !motion.idle_clip.trim().is_empty()
        {
            available.insert(CreatureCapability::Stationary);
        }
        if has_continuous_motion(motion)
            && resolved_attack_kinds(motion).contains(&MotionAttackKind::ContinuousRobot)
        {
            available.insert(CreatureCapability::ContinuousAttack);
        }
        if has_required_overlay_evidence(motion) {
            available.insert(CreatureCapability::Overlay);
        }
        if has_required_rig_evidence(motion) {
            available.insert(CreatureCapability::MultiRig);
        }
    }
    if !candidate.record_variants.is_empty() {
        available.insert(CreatureCapability::RecordProjection);
    }
    CapabilityLedger {
        required: required.iter().copied().collect(),
        available: available.iter().copied().collect(),
        missing: required.difference(&available).copied().collect(),
    }
}

fn motion_rejections(motion: &MotionSet) -> Vec<CreatureRejectionReason> {
    let mut reasons = Vec::new();
    let attack_ids = motion
        .attacks
        .iter()
        .map(|attack| attack.id.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let attack_events = motion
        .attacks
        .iter()
        .map(|attack| attack.event.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let roles_valid = match motion.graph_template {
        CreatureGraphTemplate::GroundMelee
        | CreatureGraphTemplate::GroundRangedProjectile
        | CreatureGraphTemplate::GroundMeleeRanged => has_locomotion(motion),
        CreatureGraphTemplate::PassiveGround => true,
        CreatureGraphTemplate::GroundSwim => has_locomotion(motion) && has_swim_motion(motion),
        CreatureGraphTemplate::GroundFly => has_locomotion(motion) && has_fly_motion(motion),
        CreatureGraphTemplate::Swim => has_swim_motion(motion),
        CreatureGraphTemplate::Fly => has_fly_motion(motion),
        CreatureGraphTemplate::StationaryTurret => true,
        CreatureGraphTemplate::RobotContinuousAttack => has_continuous_motion(motion),
    };
    let attacks_valid = motion.attacks.iter().all(|attack| {
        !attack.id.trim().is_empty()
            && !attack.event.trim().is_empty()
            && !attack.clip.trim().is_empty()
    });
    let template_requires_attack = matches!(
        motion.graph_template,
        CreatureGraphTemplate::GroundMelee
            | CreatureGraphTemplate::GroundRangedProjectile
            | CreatureGraphTemplate::GroundMeleeRanged
            | CreatureGraphTemplate::GroundSwim
            | CreatureGraphTemplate::GroundFly
            | CreatureGraphTemplate::StationaryTurret
            | CreatureGraphTemplate::RobotContinuousAttack
    );
    if motion.id.trim().is_empty()
        || motion.idle_clip.trim().is_empty()
        || !roles_valid
        || (template_requires_attack && motion.attacks.is_empty())
        || attack_ids.len() != motion.attacks.len()
        || attack_events.len() != motion.attacks.len()
        || !attacks_valid
    {
        reasons.push(CreatureRejectionReason::InvalidMotionSet {
            id: motion.id.clone(),
        });
    }

    let mut declared_attacks = BTreeSet::new();
    for attack in &motion.attacks {
        declared_attacks.insert(attack.id.to_ascii_lowercase());
        let Some(kind) = attack_kind(motion, attack) else {
            reasons.push(CreatureRejectionReason::MissingAttackSemantics {
                motion_set: motion.id.clone(),
                attack: attack.id.clone(),
            });
            continue;
        };
        if !attack_kind_allowed(motion.graph_template, kind)
            || (kind == MotionAttackKind::MeleeUnarmed && !attack.event.starts_with("melee"))
        {
            reasons.push(CreatureRejectionReason::UnsupportedAttackKind {
                motion_set: motion.id.clone(),
                attack: attack.id.clone(),
                kind,
                template: graph_template_name(motion.graph_template).to_string(),
            });
        }
    }
    if motion
        .attack_kinds
        .keys()
        .any(|attack| !declared_attacks.contains(&attack.to_ascii_lowercase()))
    {
        reasons.push(CreatureRejectionReason::InvalidMotionSet {
            id: motion.id.clone(),
        });
    }

    let (missing_overlays, invalid_overlays) = missing_overlay_evidence(motion);
    if !missing_overlays.is_empty() {
        reasons.push(CreatureRejectionReason::MissingOverlayEvidence {
            motion_set: motion.id.clone(),
            overlays: missing_overlays,
        });
    }
    if !invalid_overlays.is_empty() {
        reasons.push(CreatureRejectionReason::InvalidOverlayEvidence {
            motion_set: motion.id.clone(),
            overlays: invalid_overlays,
        });
    }
    let (missing_rigs, invalid_rigs) = missing_rig_evidence(motion);
    if !missing_rigs.is_empty() {
        reasons.push(CreatureRejectionReason::MissingRigEvidence {
            motion_set: motion.id.clone(),
            rigs: missing_rigs,
        });
    }
    if !invalid_rigs.is_empty() {
        reasons.push(CreatureRejectionReason::InvalidRigEvidence {
            motion_set: motion.id.clone(),
            rigs: invalid_rigs,
        });
    }
    reasons
}

fn has_locomotion(motion: &MotionSet) -> bool {
    ["walk_forward", "turn_left_90", "turn_right_90"]
        .iter()
        .all(|role| {
            motion
                .locomotion_clips
                .get(*role)
                .is_some_and(|clip| !clip.trim().is_empty())
        })
}

fn has_swim_motion(motion: &MotionSet) -> bool {
    role_clip(motion, "swim_forward")
        && (motion.graph_template != CreatureGraphTemplate::GroundSwim
            || role_clip(motion, "swim_idle"))
}

fn has_fly_motion(motion: &MotionSet) -> bool {
    role_clip(motion, "fly_forward")
        && (motion.graph_template != CreatureGraphTemplate::GroundFly
            || role_clip(motion, "fly_idle"))
}

fn has_continuous_motion(motion: &MotionSet) -> bool {
    [
        "continuous_attack_start",
        "continuous_attack_loop",
        "continuous_attack_stop",
    ]
    .into_iter()
    .all(|role| role_clip(motion, role))
}

fn role_clip(motion: &MotionSet, role: &str) -> bool {
    motion
        .locomotion_clips
        .get(role)
        .is_some_and(|clip| !clip.trim().is_empty())
}

fn attack_kind(motion: &MotionSet, attack: &MotionAttack) -> Option<MotionAttackKind> {
    motion
        .attack_kinds
        .get(&attack.id.to_ascii_lowercase())
        .copied()
        .or_else(|| {
            (motion.graph_template == CreatureGraphTemplate::GroundMelee
                && motion.attack_kinds.is_empty())
            .then_some(MotionAttackKind::MeleeUnarmed)
        })
}

fn resolved_attack_kinds(motion: &MotionSet) -> BTreeSet<MotionAttackKind> {
    motion
        .attacks
        .iter()
        .filter_map(|attack| attack_kind(motion, attack))
        .filter(|kind| attack_kind_allowed(motion.graph_template, *kind))
        .collect()
}

fn capability_for_attack_kind(kind: MotionAttackKind) -> CreatureCapability {
    match kind {
        MotionAttackKind::MeleeUnarmed => CreatureCapability::Melee,
        MotionAttackKind::RangedProjectile => CreatureCapability::RangedProjectile,
        MotionAttackKind::SpellAbility => CreatureCapability::SpellAbility,
        MotionAttackKind::ContinuousRobot => CreatureCapability::ContinuousAttack,
    }
}

fn require_declared_ranged_capabilities(
    motion: Option<&MotionSet>,
    required: &mut BTreeSet<CreatureCapability>,
) {
    let kinds = motion.map(resolved_attack_kinds).unwrap_or_default();
    let mut inserted = false;
    if kinds.contains(&MotionAttackKind::RangedProjectile) {
        required.insert(CreatureCapability::RangedProjectile);
        inserted = true;
    }
    if kinds.contains(&MotionAttackKind::SpellAbility) {
        required.insert(CreatureCapability::SpellAbility);
        inserted = true;
    }
    if !inserted {
        required.insert(CreatureCapability::RangedProjectile);
    }
}

fn attack_kind_allowed(template: CreatureGraphTemplate, kind: MotionAttackKind) -> bool {
    match template {
        CreatureGraphTemplate::GroundMelee => kind == MotionAttackKind::MeleeUnarmed,
        CreatureGraphTemplate::PassiveGround => false,
        CreatureGraphTemplate::GroundMeleeRanged
        | CreatureGraphTemplate::GroundSwim
        | CreatureGraphTemplate::GroundFly => matches!(
            kind,
            MotionAttackKind::MeleeUnarmed
                | MotionAttackKind::RangedProjectile
                | MotionAttackKind::SpellAbility
        ),
        CreatureGraphTemplate::GroundRangedProjectile | CreatureGraphTemplate::StationaryTurret => {
            matches!(
                kind,
                MotionAttackKind::RangedProjectile | MotionAttackKind::SpellAbility
            )
        }
        CreatureGraphTemplate::Swim | CreatureGraphTemplate::Fly => matches!(
            kind,
            MotionAttackKind::MeleeUnarmed
                | MotionAttackKind::RangedProjectile
                | MotionAttackKind::SpellAbility
        ),
        CreatureGraphTemplate::RobotContinuousAttack => kind == MotionAttackKind::ContinuousRobot,
    }
}

fn has_required_overlay_evidence(motion: &MotionSet) -> bool {
    let (missing, invalid) = missing_overlay_evidence(motion);
    !motion.required_overlays.is_empty() && missing.is_empty() && invalid.is_empty()
}

fn missing_overlay_evidence(motion: &MotionSet) -> (Vec<String>, Vec<String>) {
    let mut evidence = BTreeMap::<String, Vec<&MotionOverlayEvidence>>::new();
    let mut invalid = Vec::new();
    for overlay in &motion.overlays {
        evidence
            .entry(overlay.id.to_ascii_lowercase())
            .or_default()
            .push(overlay);
        if overlay.id.trim().is_empty()
            || overlay.clip.trim().is_empty()
            || overlay.start_event.trim().is_empty()
            || overlay.stop_event.trim().is_empty()
        {
            invalid.push(overlay.id.clone());
        }
    }
    let missing = motion
        .required_overlays
        .iter()
        .filter(|required| {
            !matches!(
                evidence
                    .get(&required.to_ascii_lowercase())
                    .map(Vec::as_slice),
                Some([_])
            )
        })
        .cloned()
        .collect();
    for (id, values) in evidence {
        if values.len() != 1 {
            invalid.push(id);
        }
    }
    sort_unique_pair(missing, invalid)
}

fn has_required_rig_evidence(motion: &MotionSet) -> bool {
    let (missing, invalid) = missing_rig_evidence(motion);
    !motion.required_rigs.is_empty() && missing.is_empty() && invalid.is_empty()
}

fn missing_rig_evidence(motion: &MotionSet) -> (Vec<String>, Vec<String>) {
    let mut evidence = BTreeMap::<String, Vec<&MotionRigEvidence>>::new();
    let mut invalid = Vec::new();
    for rig in &motion.rigs {
        evidence
            .entry(rig.id.to_ascii_lowercase())
            .or_default()
            .push(rig);
        if rig.id.trim().is_empty()
            || rig.root_node.trim().is_empty()
            || !rig.skeleton_nif.to_ascii_lowercase().ends_with(".nif")
            || !rig.project_hkx.to_ascii_lowercase().ends_with(".hkx")
        {
            invalid.push(rig.id.clone());
        }
    }
    let missing = motion
        .required_rigs
        .iter()
        .filter(|required| {
            !matches!(
                evidence
                    .get(&required.to_ascii_lowercase())
                    .map(Vec::as_slice),
                Some([_])
            )
        })
        .cloned()
        .collect();
    for (id, values) in evidence {
        if values.len() != 1 {
            invalid.push(id);
        }
    }
    sort_unique_pair(missing, invalid)
}

fn sort_unique_pair(mut left: Vec<String>, mut right: Vec<String>) -> (Vec<String>, Vec<String>) {
    sort_dedup_case_insensitive(&mut left);
    sort_dedup_case_insensitive(&mut right);
    (left, right)
}

fn sort_dedup_case_insensitive(values: &mut Vec<String>) {
    values.sort_by_key(|value| value.to_ascii_lowercase());
    values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
}

fn is_ground_melee_template(template: &CreatureGraphTemplate) -> bool {
    *template == CreatureGraphTemplate::GroundMelee
}

fn graph_template_name(template: CreatureGraphTemplate) -> &'static str {
    match template {
        CreatureGraphTemplate::GroundMelee => "ground_melee",
        CreatureGraphTemplate::PassiveGround => "passive_ground",
        CreatureGraphTemplate::GroundRangedProjectile => "ground_ranged_projectile",
        CreatureGraphTemplate::GroundMeleeRanged => "ground_melee_ranged",
        CreatureGraphTemplate::GroundSwim => "ground_swim",
        CreatureGraphTemplate::GroundFly => "ground_fly",
        CreatureGraphTemplate::Swim => "swim",
        CreatureGraphTemplate::Fly => "fly",
        CreatureGraphTemplate::StationaryTurret => "stationary_turret",
        CreatureGraphTemplate::RobotContinuousAttack => "robot_continuous_attack",
    }
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && !slug.contains("--")
        && slug.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn counts(values: impl IntoIterator<Item = String>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_insert(0) += 1;
    }
    counts
}

fn group_by_id<T>(values: Vec<T>, id: impl Fn(&T) -> &str) -> BTreeMap<String, Vec<T>> {
    let mut grouped = BTreeMap::<String, Vec<T>>::new();
    for value in values {
        grouped
            .entry(id(&value).to_ascii_lowercase())
            .or_default()
            .push(value);
    }
    grouped
}

fn unique_mapping<'a, T>(
    catalog: &'a BTreeMap<String, Vec<T>>,
    id: &str,
    reasons: &mut Vec<CreatureRejectionReason>,
    rig: bool,
) -> Option<&'a T> {
    match catalog.get(&id.to_ascii_lowercase()).map(Vec::as_slice) {
        Some([mapping]) => Some(mapping),
        Some(_) if rig => {
            reasons.push(CreatureRejectionReason::AmbiguousRigFamily { id: id.to_string() });
            None
        }
        Some(_) => {
            reasons.push(CreatureRejectionReason::AmbiguousMotionSet { id: id.to_string() });
            None
        }
        None if rig => {
            reasons.push(CreatureRejectionReason::MissingRigFamily { id: id.to_string() });
            None
        }
        None => {
            reasons.push(CreatureRejectionReason::MissingMotionSet { id: id.to_string() });
            None
        }
    }
}

fn candidate_sort_key(candidate: &CreatureCorpusCandidate) -> (String, String) {
    (
        candidate.source_identity.stable_key(),
        candidate.output_slug.to_ascii_lowercase(),
    )
}

fn variant_sort_key(variant: &RecordVariant) -> (bool, String, String) {
    (
        !variant.primary,
        variant.source_identity.stable_key(),
        variant.output_slug.to_ascii_lowercase(),
    )
}

fn planned_sort_key(entry: &PlannedCreature) -> (String, String) {
    (
        entry.source_key.clone(),
        entry.output_slug.to_ascii_lowercase(),
    )
}

fn rejected_sort_key(
    entry: &RejectedCreature,
) -> (String, String, String, Vec<CreatureRejectionReason>) {
    (
        entry.source_key.clone(),
        entry.output_slug.to_ascii_lowercase(),
        entry.primary_record_identity.stable_key(),
        entry.reasons.clone(),
    )
}

fn is_sorted_by<T, K: Ord>(values: &[T], key: impl Fn(&T) -> K) -> bool {
    values
        .windows(2)
        .all(|window| key(&window[0]) <= key(&window[1]))
}

#[cfg(test)]
mod capability_tests {
    use super::*;

    fn attack(id: &str) -> MotionAttack {
        MotionAttack {
            id: id.to_string(),
            event: format!("attack{id}"),
            clip: format!("{id}.hkx"),
        }
    }

    fn motion(template: CreatureGraphTemplate) -> MotionSet {
        MotionSet {
            id: "family-motion".to_string(),
            graph_template: template,
            idle_clip: "idle.hkx".to_string(),
            locomotion_clips: BTreeMap::new(),
            attacks: Vec::new(),
            attack_kinds: BTreeMap::new(),
            required_overlays: Vec::new(),
            overlays: Vec::new(),
            required_rigs: Vec::new(),
            rigs: Vec::new(),
        }
    }

    fn candidate() -> CreatureCorpusCandidate {
        let source = SourceCreatureIdentity {
            namespace: "test".to_string(),
            plugin: "Source.esm".to_string(),
            local_form_id: 0x800,
        };
        CreatureCorpusCandidate {
            source_identity: source.clone(),
            primary_record_identity: source.clone(),
            output_slug: "creature".to_string(),
            rig_family: "family".to_string(),
            motion_set: "family-motion".to_string(),
            record_variants: vec![RecordVariant {
                source_identity: source,
                output_slug: "base".to_string(),
                display_name: "Creature".to_string(),
                body_nif: "Actors\\Creature\\Body.nif".to_string(),
                level: 1,
                health: 1,
                action_points: 1,
                primary: true,
                attack_ids: Vec::new(),
            }],
            preflight_rejections: Vec::new(),
        }
    }

    fn family() -> RigFamily {
        RigFamily {
            id: "family".to_string(),
            root_node: "Root".to_string(),
            race_data: RaceDataMapping::Mapped {
                target: Fo4RaceDataTarget {
                    male_height: 1.0,
                    female_height: 1.0,
                    male_default_weight: [0.0; 3],
                    female_default_weight: [0.0; 3],
                    flags: Vec::new(),
                    acceleration_rate: 0.0,
                    deceleration_rate: 0.0,
                    size: Fo4RaceSize::Medium,
                    injured_health_percent: 0.0,
                    body_biped_object: -1,
                    aim_angle_tolerance: 0.0,
                    flight_radius: 0.0,
                    angular_acceleration_rate: 0.0,
                    angular_tolerance: 0.0,
                    flags_2: Vec::new(),
                    xp_value: 0,
                    orientation_limit_pitch: 0.0,
                    orientation_limit_roll: 0.0,
                },
            },
        }
    }

    fn ledger(motion: &MotionSet) -> CapabilityLedger {
        capability_ledger(&candidate(), Some(&family()), Some(motion))
    }

    #[test]
    fn graph_templates_require_their_declared_architecture() {
        let mut ranged = motion(CreatureGraphTemplate::GroundRangedProjectile);
        ranged.locomotion_clips = BTreeMap::from([
            ("walk_forward".to_string(), "walk.hkx".to_string()),
            ("turn_left_90".to_string(), "left.hkx".to_string()),
            ("turn_right_90".to_string(), "right.hkx".to_string()),
        ]);
        ranged.attacks.push(attack("shot"));
        ranged
            .attack_kinds
            .insert("shot".to_string(), MotionAttackKind::RangedProjectile);
        let ranged_ledger = ledger(&ranged);
        assert!(ranged_ledger.missing.is_empty());
        assert!(
            ranged_ledger
                .required
                .contains(&CreatureCapability::RangedProjectile)
        );
        assert!(!ranged_ledger.required.contains(&CreatureCapability::Melee));

        let mut swim = motion(CreatureGraphTemplate::Swim);
        swim.locomotion_clips
            .insert("swim_forward".to_string(), "swim.hkx".to_string());
        assert!(ledger(&swim).missing.is_empty());

        let mut fly = motion(CreatureGraphTemplate::Fly);
        fly.locomotion_clips
            .insert("fly_forward".to_string(), "fly.hkx".to_string());
        assert!(ledger(&fly).missing.is_empty());

        let mut turret = motion(CreatureGraphTemplate::StationaryTurret);
        turret.attacks.push(attack("cast"));
        turret
            .attack_kinds
            .insert("cast".to_string(), MotionAttackKind::SpellAbility);
        let turret_ledger = ledger(&turret);
        assert!(turret_ledger.missing.is_empty());
        assert!(
            turret_ledger
                .required
                .contains(&CreatureCapability::Stationary)
        );
        assert!(
            turret_ledger
                .required
                .contains(&CreatureCapability::SpellAbility)
        );

        let mut robot = motion(CreatureGraphTemplate::RobotContinuousAttack);
        robot.locomotion_clips = BTreeMap::from([
            (
                "continuous_attack_start".to_string(),
                "start.hkx".to_string(),
            ),
            ("continuous_attack_loop".to_string(), "loop.hkx".to_string()),
            ("continuous_attack_stop".to_string(), "stop.hkx".to_string()),
        ]);
        robot.attacks.push(attack("beam"));
        robot
            .attack_kinds
            .insert("beam".to_string(), MotionAttackKind::ContinuousRobot);
        let robot_ledger = ledger(&robot);
        assert!(robot_ledger.missing.is_empty());
        assert!(
            robot_ledger
                .required
                .contains(&CreatureCapability::ContinuousAttack)
        );
        assert!(
            !robot_ledger
                .required
                .contains(&CreatureCapability::Locomotion)
        );
    }

    #[test]
    fn an_attack_does_not_substitute_for_missing_template_motion() {
        let mut fly = motion(CreatureGraphTemplate::Fly);
        fly.attacks.push(attack("shot"));
        fly.attack_kinds
            .insert("shot".to_string(), MotionAttackKind::RangedProjectile);

        let ledger = ledger(&fly);

        assert!(
            ledger
                .available
                .contains(&CreatureCapability::RangedProjectile)
        );
        assert!(ledger.missing.contains(&CreatureCapability::Fly));
    }

    #[test]
    fn overlay_and_multi_rig_requirements_fail_with_typed_evidence() {
        let mut ground = motion(CreatureGraphTemplate::GroundMelee);
        ground.locomotion_clips = BTreeMap::from([
            ("walk_forward".to_string(), "walk.hkx".to_string()),
            ("turn_left_90".to_string(), "left.hkx".to_string()),
            ("turn_right_90".to_string(), "right.hkx".to_string()),
        ]);
        let mut melee = attack("bite");
        melee.event = "meleeBite".to_string();
        ground.attacks.push(melee);
        ground.required_overlays.push("head-aim".to_string());
        ground.required_rigs.push("weapon-arm".to_string());

        let reasons = motion_rejections(&ground);
        let capability_ledger = ledger(&ground);

        assert!(reasons.iter().any(|reason| matches!(
            reason,
            CreatureRejectionReason::MissingOverlayEvidence { overlays, .. }
                if overlays == &["head-aim"]
        )));
        assert!(reasons.iter().any(|reason| matches!(
            reason,
            CreatureRejectionReason::MissingRigEvidence { rigs, .. }
                if rigs == &["weapon-arm"]
        )));
        assert!(
            capability_ledger
                .missing
                .contains(&CreatureCapability::Overlay)
        );
        assert!(
            capability_ledger
                .missing
                .contains(&CreatureCapability::MultiRig)
        );
    }

    #[test]
    fn legacy_ground_melee_json_remains_compatible() {
        let json = r#"{
            "id":"legacy",
            "idle_clip":"idle.hkx",
            "locomotion_clips":{
                "walk_forward":"walk.hkx",
                "turn_left_90":"left.hkx",
                "turn_right_90":"right.hkx"
            },
            "attacks":[{"id":"bite","event":"meleeBite","clip":"bite.hkx"}]
        }"#;
        let motion: MotionSet = serde_json::from_str(json).unwrap();

        assert_eq!(motion.graph_template, CreatureGraphTemplate::GroundMelee);
        assert!(motion.attack_kinds.is_empty());
        assert!(
            !serde_json::to_string(&motion)
                .unwrap()
                .contains("graph_template")
        );
        assert!(motion_rejections(&motion).is_empty());
    }
}
