use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

use crate::ids::FormKey;
use crate::record::Record;
use crate::source_rig::race_data::{
    CapsuleEvidence, ControllerArchitecture, Fo4RaceDataDerivation, Fo4RaceDataScalePolicy,
    MeasurementAxis, SourceFamilyRaceDataEvidence,
};
use crate::source_rig::{
    CapabilityClipRole, CapabilityGraphManifest, CreatureClipRole, CreatureGraphTemplate,
    EventDecl, MotionSet, OverlayClipRole,
};
use crate::sym::StringInterner;

use super::creature_catalog::{
    CreatureCatalogIssue, CreatureCorpusPlan, CreatureNpcPlan, CreatureRaceAttackDataPlan,
    CreatureRacePlan, build_creature_corpus_plan,
};
use super::creature_motion::{
    SkyrimCatalogClip, SkyrimCharacterControllerEvidence, SkyrimClipDisposition,
    SkyrimControllerArchitectureEvidence, SkyrimControllerEvidenceDisposition,
    SkyrimControllerLayoutEvidence as DecodedControllerLayout, SkyrimControllerModelEvidence,
    SkyrimCreatureFamilyInventory, SkyrimCreatureMotionCatalog, SkyrimCreatureMotionRole,
    SkyrimLivingFamilyDisposition, SkyrimLivingFamilyEvidence, SkyrimLivingTemplateDisposition,
    SkyrimMotionEvidenceSource, SkyrimRequiredRoleDisposition, SkyrimRootMotion,
    SkyrimRootMotionSource, SkyrimRootMotionSourceLocator, SkyrimUnsupportedMotionReason,
};
use super::creature_race_data::{
    CreatureRaceDataDisposition, CreatureRaceDataEvidencePlan, CreatureRaceDataInputField,
    CreatureRaceUnarmedDataEvidence, DecodedCreatureFamilyEvidence,
    build_creature_race_data_evidence,
};

pub const SKYRIM_CREATURE_RECIPE_VERSION: u32 = 3;
pub const SKYRIM_CREATURE_FAMILY_COUNT: usize = 46;
// 121, not 122: testDraugrRace is a curated exclusion (see CURATED_TEST_RACES).
// Must match `creature_mvp_adapter::SKYRIM_CREATURE_CANDIDATE_COUNT`.
pub const SKYRIM_CREATURE_CANDIDATE_COUNT: usize = 121;
pub(crate) const SKYRIM_LEGACY_CHARACTER_DATA_SIGNATURE: u32 = 0x300D_6808;
pub(crate) const SKYRIM_LEGACY_CONTROLLER_SIGNATURE: u32 = 0xA0F4_15BF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkyrimCreatureRecipeExpectations {
    pub family_count: usize,
    pub candidate_count: usize,
}

impl SkyrimCreatureRecipeExpectations {
    pub const OFFICIAL: Self = Self {
        family_count: SKYRIM_CREATURE_FAMILY_COUNT,
        candidate_count: SKYRIM_CREATURE_CANDIDATE_COUNT,
    };
}

pub struct SkyrimCreatureRecipeInput<'a> {
    pub winning_records: &'a [Record],
    pub motion: &'a SkyrimCreatureMotionCatalog,
    pub decoded_race_data: &'a [DecodedCreatureFamilyEvidence],
    pub assets: &'a [SkyrimCreatureFamilyAssetEvidence],
    pub race_data_policy: &'a SkyrimRaceDataPolicyReceipt,
    pub expectations: SkyrimCreatureRecipeExpectations,
    pub interner: &'a StringInterner,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimCreatureRecipeLedger {
    pub version: u32,
    pub source_game: String,
    pub target_game: String,
    pub race_data_policy: SkyrimRaceDataPolicyReceipt,
    pub candidates: Vec<SkyrimCreatureCandidateRecipe>,
    pub family_jobs: Vec<SkyrimCreatureFamilyJob>,
    pub accounting: SkyrimCreatureRecipeAccounting,
    pub content_blake3: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimCreatureRecipeAccounting {
    pub candidates: usize,
    pub ready_candidates: usize,
    pub blocked_candidates: usize,
    pub unsupported_candidates: usize,
    pub ambiguous_candidates: usize,
    pub family_jobs: usize,
    pub ready_family_jobs: usize,
    pub blocked_family_jobs: usize,
    pub unsupported_family_jobs: usize,
    pub ambiguous_family_jobs: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimCreatureCandidateRecipe {
    pub source_race: String,
    pub editor_id: Option<String>,
    pub source_skin: Option<String>,
    pub source_body_part_data: Option<String>,
    pub source_armor_addons: Vec<String>,
    pub source_body_models: Vec<String>,
    pub project_paths: Vec<String>,
    pub skeleton_paths: Vec<String>,
    pub attack_events: Vec<String>,
    pub attack_contract: Vec<String>,
    pub attack_data: Vec<SkyrimRaceAttackDataReceipt>,
    pub attack_spells: Vec<String>,
    pub npc_templates: Vec<SkyrimCreatureNpcEvidence>,
    pub race_data: SkyrimCreatureRaceDataReceipt,
    pub disposition: SkyrimCreatureCandidateDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimRaceAttackDataReceipt {
    pub ordinal: usize,
    pub event: String,
    pub damage_multiplier_bits: u32,
    pub attack_chance_bits: u32,
    pub attack_spell: Option<SkyrimRaceAttackFormReceipt>,
    pub attack_flags: u32,
    pub attack_angle_bits: u32,
    pub strike_angle_bits: u32,
    pub stagger_bits: u32,
    pub attack_type: Option<SkyrimRaceAttackFormReceipt>,
    pub knockdown_bits: u32,
    pub recovery_time_bits: u32,
    pub stamina_multiplier_bits: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimRaceAttackFormReceipt {
    pub source: String,
    pub signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimCreatureNpcEvidence {
    pub source_npc: String,
    pub effective_races: Vec<String>,
    pub template_records: Vec<String>,
    pub family_ids: Vec<String>,
    pub issues: Vec<SkyrimCreatureRecordIssue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SkyrimCreatureCandidateDisposition {
    Ready {
        family_job_ids: Vec<String>,
    },
    Blocked {
        issues: Vec<SkyrimCreatureRecipeIssue>,
    },
    Unsupported {
        issues: Vec<SkyrimCreatureRecipeIssue>,
    },
    Ambiguous {
        issues: Vec<SkyrimCreatureRecipeIssue>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimCreatureFamilyJob {
    pub family_id: String,
    pub project_path: String,
    pub source_races: Vec<String>,
    pub inventory: SkyrimCreatureInventoryReceipt,
    pub graph_variants: Vec<SkyrimCreatureGraphVariantEvidence>,
    pub graph: SkyrimCreatureGraphEvidence,
    pub clips: Vec<SkyrimCreatureClipReceipt>,
    pub controllers: Vec<SkyrimCreatureControllerReceipt>,
    pub ragdoll: SkyrimRagdollCapabilityEvidence,
    pub weapon_bearing: SkyrimExplicitCapabilityEvidence,
    pub body_variants: Vec<SkyrimCreatureBodyVariantEvidence>,
    pub asset_claims: Vec<SkyrimRecursiveAssetClaim>,
    pub disposition: SkyrimCreatureFamilyDisposition,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SkyrimCreatureFamilyDisposition {
    Ready {
        capabilities: Vec<SkyrimCreatureCapability>,
    },
    Blocked {
        issues: Vec<SkyrimCreatureRecipeIssue>,
    },
    Unsupported {
        issues: Vec<SkyrimCreatureRecipeIssue>,
    },
    Ambiguous {
        issues: Vec<SkyrimCreatureRecipeIssue>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimCreatureCapability {
    GroundLocomotion,
    MeleeAttack,
    RangedProjectile,
    Swimming,
    Flying,
    Stationary,
    ContinuousAttack,
    Overlay,
    MultiRig,
    WeaponBearing,
    Ragdoll,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimCreatureInventoryReceipt {
    pub project_path: String,
    pub character_paths: Vec<String>,
    pub animation_skeleton_paths: Vec<String>,
    pub ragdoll_paths: Vec<String>,
    pub behavior_paths: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimCreatureSex {
    Male,
    Female,
    Ungendered,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimCreatureGraphVariantEvidence {
    pub sex: SkyrimCreatureSex,
    pub project_path: String,
    pub character_path: String,
    pub skeleton_path: String,
    pub actual_root_bone: String,
    pub behavior_paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimCreatureGraphEvidence {
    pub template: CreatureGraphTemplate,
    pub ready: bool,
    pub roles: Vec<crate::source_rig::CapabilityClipRole>,
    #[serde(default)]
    pub candidate_attack_bindings: Vec<crate::source_rig::CapabilityCandidateAttackBinding>,
    pub idle_event: Option<String>,
    pub explicit_events: Vec<EventDecl>,
    #[serde(default)]
    pub variables: Vec<crate::source_rig::VariableDecl>,
    pub overlays: Vec<OverlayClipRole>,
    pub required_rigs: Vec<String>,
    pub rigs: Vec<crate::source_rig::MotionRigEvidence>,
    pub blockers: Vec<SkyrimCreatureMotionBlockerReceipt>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimCreatureClipReceipt {
    pub clip_id: String,
    pub clip_path: String,
    pub family_ids: Vec<String>,
    pub role: SkyrimCreatureClipRoleReceipt,
    pub trigger_event: Option<String>,
    #[serde(default)]
    pub trigger_aliases: Vec<String>,
    pub role_evidence: Vec<SkyrimMotionRoleEvidenceReceipt>,
    pub root_motion: SkyrimRootMotionReceipt,
    pub root_motion_locators: Vec<SkyrimRootMotionLocatorReceipt>,
    pub events: Vec<SkyrimCatalogEventReceipt>,
    pub annotations: Vec<SkyrimTimedEventReceipt>,
    pub original_skeleton_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkyrimRootMotionLocatorReceipt {
    HavokReferenceFrame {
        clip_path: String,
    },
    BoundAnims {
        source_path: String,
        project_stem: String,
        animation_index: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SkyrimCreatureClipRoleReceipt {
    Role(SkyrimCreatureMotionRoleReceipt),
    SharedPaired,
    SharedOverlay,
    Unsupported(SkyrimUnsupportedMotionReasonReceipt),
    Ambiguous(Vec<SkyrimCreatureMotionRoleReceipt>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimCreatureMotionRoleReceipt {
    Idle,
    GroundLocomotion,
    TurnLeft,
    TurnRight,
    Turn,
    MeleeAttack,
    RangedAttack,
    ProjectileAttack,
    SpellAttack,
    SwimIdle,
    SwimLocomotion,
    FlyIdle,
    FlyLocomotion,
    StationaryIdle,
    MechanicalStart,
    MechanicalLoop,
    MechanicalStop,
    Hurt,
    Death,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimMotionEvidenceSourceReceipt {
    RaceAttackEvent,
    BehaviorTransition,
    BehaviorGenerator,
    AnimationData,
    ClipAnnotation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimMotionRoleEvidenceReceipt {
    pub source: SkyrimMotionEvidenceSourceReceipt,
    pub detail: String,
    pub time: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkyrimRootMotionReceipt {
    Unknown,
    Stationary {
        source: SkyrimRootMotionSourceReceipt,
        sample_count: usize,
    },
    Sampled {
        source: SkyrimRootMotionSourceReceipt,
        sample_count: usize,
        translation_delta: [f32; 3],
        rotation_start: Option<[f32; 4]>,
        rotation_end: Option<[f32; 4]>,
    },
    Unsupported {
        detail: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimRootMotionSourceReceipt {
    HavokReferenceFrame,
    BoundAnims,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimCatalogEventReceipt {
    pub name: String,
    pub time: Option<f32>,
    pub source: SkyrimMotionEvidenceSourceReceipt,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimTimedEventReceipt {
    pub name: String,
    pub time: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimUnsupportedMotionReasonReceipt {
    NoSemanticRoleEvidence,
    BehaviorReferenceMissing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkyrimCreatureMotionBlockerReceipt {
    MissingRole {
        alternatives: Vec<SkyrimCreatureMotionRoleReceipt>,
    },
    MissingTriggerEvent {
        role: SkyrimCreatureMotionRoleReceipt,
        clip_ids: Vec<String>,
    },
    UnknownRootMotion {
        role: SkyrimCreatureMotionRoleReceipt,
        clip_ids: Vec<String>,
    },
    AmbiguousRole {
        clip_id: String,
        roles: Vec<SkyrimCreatureMotionRoleReceipt>,
    },
    AmbiguousRequiredRole {
        alternatives: Vec<SkyrimCreatureMotionRoleReceipt>,
        clip_ids: Vec<String>,
    },
    MissingRequiredTrigger {
        alternatives: Vec<SkyrimCreatureMotionRoleReceipt>,
        clip_ids: Vec<String>,
    },
    AmbiguousRequiredTrigger {
        alternatives: Vec<SkyrimCreatureMotionRoleReceipt>,
        clip_ids: Vec<String>,
        trigger_events: Vec<String>,
    },
    UnknownRequiredRootMotion {
        alternatives: Vec<SkyrimCreatureMotionRoleReceipt>,
        clip_ids: Vec<String>,
    },
    UnsupportedRequiredRootMotion {
        alternatives: Vec<SkyrimCreatureMotionRoleReceipt>,
        clip_ids: Vec<String>,
    },
    AmbiguousTemplate {
        candidates: Vec<CreatureGraphTemplate>,
    },
    UnsupportedTemplate {
        evidenced_roles: Vec<SkyrimCreatureMotionRoleReceipt>,
    },
    AdapterUnavailable {
        detail: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimCreatureControllerReceipt {
    pub character_path: String,
    pub character_data_class: String,
    pub controller_class: Option<String>,
    pub controller_cinfo_class: Option<String>,
    pub architecture: Option<SkyrimControllerArchitectureReceipt>,
    pub layout: SkyrimControllerLayoutEvidence,
    pub collision_filter_info: Option<u32>,
    pub rigid_body_type: Option<i32>,
    pub shape_type: Option<i32>,
    pub capsule: Option<CapsuleEvidence>,
    pub axes: Option<SkyrimControllerAxesEvidence>,
    pub model: Option<SkyrimControllerModelReceipt>,
    pub disposition: SkyrimControllerDispositionReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkyrimControllerArchitectureReceipt {
    InlineRigidBodySetup,
    CharacterProxyCinfo,
    CharacterRigidBodyCinfo,
    FixedCinfo,
    CustomCinfo { class_name: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkyrimControllerLayoutEvidence {
    NestedCharacterControllerSetup {
        contents_version: String,
        character_data_signature: u32,
        controller_signature: u32,
    },
    LegacyCharacterControllerInfo {
        contents_version: String,
        character_data_signature: u32,
        controller_signature: u32,
    },
    Missing,
    Unsupported {
        detail: String,
    },
    Ambiguous {
        candidates: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimControllerAxesEvidence {
    pub up: MeasurementAxis,
    pub forward: MeasurementAxis,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimControllerModelReceipt {
    pub up_ms: [f32; 4],
    pub forward_ms: [f32; 4],
    pub right_ms: [f32; 4],
    pub scale: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkyrimControllerDispositionReceipt {
    Complete,
    MissingControllerSetup,
    MissingRigidBodySetup,
    MissingShapeSetup,
    InvalidCapsuleDimensions {
        total_height_bits: Option<u32>,
        radius_bits: Option<u32>,
    },
    LegacyLayoutUnsupported {
        contents_version: String,
        total_height_bits: Option<u32>,
        radius_bits: Option<u32>,
    },
    InvalidModelTransform,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimCreatureControllerRecipeEvidence {
    pub character_path: String,
    pub layout: SkyrimControllerLayoutEvidence,
    pub axes: Option<SkyrimControllerAxesEvidence>,
    pub capsule: Option<SkyrimControllerCapsuleReceipt>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimControllerCapsuleReceipt {
    pub total_height: f32,
    pub radius: f32,
    pub architecture: ControllerArchitecture,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SkyrimRagdollCapabilityEvidence {
    Present { paths: Vec<String> },
    NotApplicable { reason: String },
    Missing { expected_paths: Vec<String> },
    Unsupported { paths: Vec<String>, reason: String },
    Ambiguous { paths: Vec<String>, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SkyrimExplicitCapabilityEvidence {
    Supported { evidence: String },
    NotApplicable { evidence: String },
    Missing { reason: String },
    Unsupported { reason: String },
    Ambiguous { candidates: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimCreatureBodyVariantEvidence {
    pub source_race: String,
    pub sex: SkyrimCreatureSex,
    pub armor_addon: String,
    pub body_nif: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimAssetRole {
    Project,
    Character,
    AnimationSkeleton,
    VisualSkeletonNif,
    Ragdoll,
    Behavior,
    AnimationClip,
    BodyNif,
    Material,
    Texture,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimRecursiveAssetClaim {
    pub path: String,
    pub role: SkyrimAssetRole,
    pub dependencies: Vec<String>,
    pub disposition: SkyrimAssetClaimDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SkyrimAssetClaimDisposition {
    Present { blake3: String },
    Missing,
    Unsupported { reason: String },
    Ambiguous { candidates: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimCreatureFamilyAssetEvidence {
    pub family_id: String,
    pub graph_variants: Vec<SkyrimCreatureGraphVariantEvidence>,
    pub controllers: Vec<SkyrimCreatureControllerRecipeEvidence>,
    pub ragdoll: SkyrimRagdollCapabilityEvidence,
    pub weapon_bearing: SkyrimExplicitCapabilityEvidence,
    pub body_variants: Vec<SkyrimCreatureBodyVariantEvidence>,
    pub asset_claims: Vec<SkyrimRecursiveAssetClaim>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimCreatureRaceDataReceipt {
    pub disposition: CreatureRaceDataDisposition,
    pub evidence: Option<SourceFamilyRaceDataEvidence>,
    pub derivation: Option<Fo4RaceDataDerivation>,
    pub unarmed_data: Option<CreatureRaceUnarmedDataEvidence>,
    pub missing_fields: Vec<CreatureRaceDataInputField>,
    pub invalid_fields: Vec<CreatureRaceDataInputField>,
    pub field_receipts: Vec<SkyrimRaceDataFieldReceipt>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkyrimRaceDataPolicyReceipt {
    pub policy_id: String,
    pub schema_id: String,
    pub source_data_schema: String,
    pub target_data_schema: String,
    pub scale_policy: Fo4RaceDataScalePolicy,
    pub fields: Vec<SkyrimRaceDataFieldReceipt>,
}

pub(crate) fn build_skyrim_creature_race_data_policy_receipt(
    scale_policy: Fo4RaceDataScalePolicy,
) -> Result<SkyrimRaceDataPolicyReceipt, SkyrimCreatureRecipeError> {
    let policy_id = "skyrimse-creature-race-data-scale-v1".to_string();
    let receipt = SkyrimRaceDataPolicyReceipt {
        policy_id: policy_id.clone(),
        schema_id: "skyrimse-race-data-to-fo4-v1".to_string(),
        source_data_schema: "skyrimse:RACE.DATA".to_string(),
        target_data_schema: "fo4:RACE.DATA".to_string(),
        scale_policy,
        fields: vec![SkyrimRaceDataFieldReceipt {
            field: CreatureRaceDataInputField::ScalePolicy,
            provenance: SkyrimRaceDataFieldProvenance::ExplicitPolicy,
            policy_id: Some(policy_id),
            reason: "explicit source-stature thresholds select the FO4 creature size profile"
                .to_string(),
        }],
    };
    validate_policy(&receipt)?;
    Ok(canonical_policy(receipt))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimRaceDataFieldReceipt {
    pub field: CreatureRaceDataInputField,
    pub provenance: SkyrimRaceDataFieldProvenance,
    pub policy_id: Option<String>,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkyrimRaceDataFieldProvenance {
    SourceRaceData,
    MeasuredBodyNif,
    MeasuredSkeletonNif,
    CharacterController,
    ExplicitPolicy,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkyrimCreatureRecordIssue {
    CuratedNonCreatureRace,
    MissingProject,
    MissingSkeleton,
    MissingRaceSkin,
    MissingBodyPartData,
    MissingDependency {
        reference: String,
        expected_signature: String,
    },
    MissingArmorAddonLinks {
        skin: String,
    },
    MissingRaceConditionedArmorAddon {
        skin: String,
    },
    MissingBodyModel {
        armor_addon: String,
    },
    MissingDirectRace,
    MissingTemplateTarget,
    EmptyLeveledTemplate {
        leveled_list: String,
    },
    UnresolvedTemplateTarget {
        source: String,
        target: String,
    },
    UnsupportedTemplateTarget {
        target: String,
        signature: String,
    },
    TemplateCycle {
        nodes: Vec<String>,
    },
    AmbiguousEmbeddedFormId {
        owner: String,
        field: String,
        raw: u32,
        candidates: Vec<String>,
    },
    UnresolvedEmbeddedFormId {
        owner: String,
        field: String,
        raw: u32,
    },
    MalformedRaceAttackData {
        ordinal: usize,
        detail: String,
    },
    MixedCreatureAndNonCreatureRaces {
        races: Vec<String>,
    },
    MultipleCreatureFamilies {
        family_ids: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkyrimCreatureRecipeIssue {
    Record {
        issue: SkyrimCreatureRecordIssue,
    },
    MissingMotionFamily {
        project_path: String,
    },
    AmbiguousMotionFamily {
        project_path: String,
        family_ids: Vec<String>,
    },
    MissingAssetEvidence {
        family_id: String,
    },
    MissingGraphVariant {
        family_id: String,
    },
    InvalidRootBone {
        family_id: String,
        value: String,
    },
    InventoryPathUnclaimed {
        family_id: String,
        path: String,
        role: SkyrimAssetRole,
    },
    DanglingAssetDependency {
        family_id: String,
        owner: String,
        dependency: String,
    },
    UnreachableAssetClaim {
        family_id: String,
        path: String,
    },
    AssetUnavailable {
        family_id: String,
        path: String,
        disposition: SkyrimAssetClaimDisposition,
    },
    MissingBodyVariant {
        family_id: String,
        source_race: String,
    },
    BodyVariantNotRaceConditioned {
        family_id: String,
        source_race: String,
        body_nif: String,
    },
    MissingControllerEvidence {
        family_id: String,
        character_path: String,
    },
    ControllerEvidenceConflict {
        family_id: String,
        character_path: String,
        detail: String,
    },
    ControllerUnavailable {
        family_id: String,
        character_path: String,
        disposition: SkyrimControllerDispositionReceipt,
    },
    MissingControllerAxes {
        family_id: String,
        character_path: String,
    },
    InvalidControllerAxes {
        family_id: String,
        character_path: String,
    },
    MissingControllerCapsule {
        family_id: String,
        character_path: String,
    },
    Motion {
        family_id: String,
        blocker: SkyrimCreatureMotionBlockerReceipt,
    },
    RaceDataMissing {
        fields: Vec<CreatureRaceDataInputField>,
    },
    RaceDataInvalid {
        fields: Vec<CreatureRaceDataInputField>,
    },
    RagdollUnavailable {
        family_id: String,
        evidence: SkyrimRagdollCapabilityEvidence,
    },
    WeaponBearingUnavailable {
        family_id: String,
        evidence: SkyrimExplicitCapabilityEvidence,
    },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SkyrimCreatureRecipeError {
    #[error("expected {expected} creature families, found {actual}")]
    FamilyCount { expected: usize, actual: usize },
    #[error("expected {expected} creature candidates, found {actual}")]
    CandidateCount { expected: usize, actual: usize },
    #[error("winning record input contains duplicate {signature} {form_key}")]
    DuplicateWinningRecord { signature: String, form_key: String },
    #[error("duplicate motion family {0}")]
    DuplicateMotionFamily(String),
    #[error("motion family {0} has no unique inventory")]
    MissingOrDuplicateInventory(String),
    #[error("duplicate family asset evidence {0}")]
    DuplicateAssetFamily(String),
    #[error("asset evidence references unknown motion family {0}")]
    UnknownAssetFamily(String),
    #[error("RACE.DATA policy receipt is invalid: {0}")]
    InvalidRaceDataPolicy(String),
    #[error("recipe accounting or canonical order is invalid: {0}")]
    InvalidLedger(String),
    #[error("recipe content hash mismatch: expected {expected}, actual {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("failed to serialize creature recipe: {0}")]
    Serialization(String),
}

pub fn build_skyrim_creature_recipe_ledger(
    input: SkyrimCreatureRecipeInput<'_>,
) -> Result<SkyrimCreatureRecipeLedger, SkyrimCreatureRecipeError> {
    validate_winning_records(input.winning_records, input.interner)?;
    validate_policy(input.race_data_policy)?;
    if input.motion.families.len() != input.expectations.family_count {
        return Err(SkyrimCreatureRecipeError::FamilyCount {
            expected: input.expectations.family_count,
            actual: input.motion.families.len(),
        });
    }

    let catalog = build_creature_corpus_plan(input.winning_records, input.interner);
    if catalog.races.len() != input.expectations.candidate_count {
        return Err(SkyrimCreatureRecipeError::CandidateCount {
            expected: input.expectations.candidate_count,
            actual: catalog.races.len(),
        });
    }
    let race_data = build_creature_race_data_evidence(
        &catalog,
        input.winning_records,
        input.decoded_race_data,
        &input.race_data_policy.scale_policy,
        input.interner,
    );

    let motion_index = index_motion(input.motion)?;
    let asset_index = index_assets(input.assets, &motion_index)?;
    let project_index = index_motion_projects(input.motion);
    let mut family_jobs = Vec::with_capacity(input.motion.families.len());
    for family in &input.motion.families {
        let inventory = motion_index
            .get(&family.family_id)
            .expect("motion index covers every family");
        let assets = asset_index.get(&family.family_id).copied();
        let races = races_for_project(&catalog, &family.project_path, input.interner);
        family_jobs.push(build_family_job(
            family.family_id.as_str(),
            family.project_path.as_str(),
            &family.clip_ids,
            inventory,
            assets,
            input.motion,
            races,
            input.interner,
        ));
    }
    family_jobs.sort_by(|left, right| left.family_id.cmp(&right.family_id));

    let family_dispositions = family_jobs
        .iter()
        .map(|job| (job.family_id.clone(), job.disposition.clone()))
        .collect::<BTreeMap<_, _>>();
    let candidates = catalog
        .races
        .iter()
        .map(|race| {
            build_candidate(
                race,
                &catalog,
                &race_data,
                &project_index,
                &family_dispositions,
                &asset_index,
                input.race_data_policy,
                input.interner,
            )
        })
        .collect::<Vec<_>>();

    let accounting = account(&candidates, &family_jobs);
    let mut ledger = SkyrimCreatureRecipeLedger {
        version: SKYRIM_CREATURE_RECIPE_VERSION,
        source_game: "skyrimse".to_string(),
        target_game: "fo4".to_string(),
        race_data_policy: canonical_policy(input.race_data_policy.clone()),
        candidates,
        family_jobs,
        accounting,
        content_blake3: String::new(),
    };
    ledger.content_blake3 = ledger.compute_hash()?;
    ledger.validate()?;
    Ok(ledger)
}

impl SkyrimCreatureRecipeLedger {
    pub fn canonical_json(&self) -> Result<String, SkyrimCreatureRecipeError> {
        self.validate()?;
        serde_json::to_string_pretty(self)
            .map_err(|error| SkyrimCreatureRecipeError::Serialization(error.to_string()))
    }

    pub fn from_json(json: &str) -> Result<Self, SkyrimCreatureRecipeError> {
        let ledger: Self = serde_json::from_str(json)
            .map_err(|error| SkyrimCreatureRecipeError::Serialization(error.to_string()))?;
        ledger.validate()?;
        Ok(ledger)
    }

    pub fn validate(&self) -> Result<(), SkyrimCreatureRecipeError> {
        if self.version != SKYRIM_CREATURE_RECIPE_VERSION
            || self.source_game != "skyrimse"
            || self.target_game != "fo4"
        {
            return Err(SkyrimCreatureRecipeError::InvalidLedger(
                "schema or game identity differs".to_string(),
            ));
        }
        validate_policy(&self.race_data_policy)?;
        if !strictly_sorted_unique(
            self.candidates
                .iter()
                .map(|candidate| &candidate.source_race),
        ) {
            return Err(SkyrimCreatureRecipeError::InvalidLedger(
                "candidate order is not canonical".to_string(),
            ));
        }
        if !strictly_sorted_unique(self.family_jobs.iter().map(|job| &job.family_id)) {
            return Err(SkyrimCreatureRecipeError::InvalidLedger(
                "family order is not canonical".to_string(),
            ));
        }
        let expected_accounting = account(&self.candidates, &self.family_jobs);
        if self.accounting != expected_accounting {
            return Err(SkyrimCreatureRecipeError::InvalidLedger(
                "accounting differs from dispositions".to_string(),
            ));
        }
        let job_ids = self
            .family_jobs
            .iter()
            .map(|job| job.family_id.as_str())
            .collect::<BTreeSet<_>>();
        for candidate in &self.candidates {
            if let SkyrimCreatureCandidateDisposition::Ready { family_job_ids } =
                &candidate.disposition
                && (family_job_ids.is_empty()
                    || !strictly_sorted_unique(family_job_ids.iter())
                    || family_job_ids
                        .iter()
                        .any(|id| !job_ids.contains(id.as_str())))
            {
                return Err(SkyrimCreatureRecipeError::InvalidLedger(format!(
                    "{} has invalid ready-family references",
                    candidate.source_race
                )));
            }
        }
        let actual = self.compute_hash()?;
        if self.content_blake3 != actual {
            return Err(SkyrimCreatureRecipeError::HashMismatch {
                expected: self.content_blake3.clone(),
                actual,
            });
        }
        Ok(())
    }

    fn compute_hash(&self) -> Result<String, SkyrimCreatureRecipeError> {
        let mut unhashed = self.clone();
        unhashed.content_blake3.clear();
        let bytes = serde_json::to_vec(&unhashed)
            .map_err(|error| SkyrimCreatureRecipeError::Serialization(error.to_string()))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}

fn validate_winning_records(
    records: &[Record],
    interner: &StringInterner,
) -> Result<(), SkyrimCreatureRecipeError> {
    let mut seen = HashSet::<FormKey>::new();
    for record in records
        .iter()
        .filter(|record| matches!(record.sig.as_str(), "RACE" | "NPC_" | "LVLN"))
    {
        if !seen.insert(record.form_key) {
            return Err(SkyrimCreatureRecipeError::DuplicateWinningRecord {
                signature: record.sig.as_str().to_string(),
                form_key: record.form_key.format(interner),
            });
        }
    }
    Ok(())
}

fn validate_policy(policy: &SkyrimRaceDataPolicyReceipt) -> Result<(), SkyrimCreatureRecipeError> {
    if policy.policy_id.trim().is_empty()
        || policy.schema_id.trim().is_empty()
        || policy.source_data_schema.trim().is_empty()
        || policy.target_data_schema.trim().is_empty()
    {
        return Err(SkyrimCreatureRecipeError::InvalidRaceDataPolicy(
            "policy and schema identities must be explicit".to_string(),
        ));
    }
    let thresholds = [
        policy.scale_policy.small_max_stature,
        policy.scale_policy.medium_max_stature,
        policy.scale_policy.large_max_stature,
    ];
    if !thresholds
        .iter()
        .all(|value| value.is_finite() && *value > 0.0)
        || !(thresholds[0] < thresholds[1] && thresholds[1] < thresholds[2])
    {
        return Err(SkyrimCreatureRecipeError::InvalidRaceDataPolicy(
            "scale thresholds must be finite, positive, and strictly increasing".to_string(),
        ));
    }
    if policy.fields.is_empty() {
        return Err(SkyrimCreatureRecipeError::InvalidRaceDataPolicy(
            "field provenance receipts are required".to_string(),
        ));
    }
    let mut fields = BTreeSet::new();
    for receipt in &policy.fields {
        if receipt.reason.trim().is_empty() || !fields.insert(receipt.field) {
            return Err(SkyrimCreatureRecipeError::InvalidRaceDataPolicy(
                "field receipts must have unique fields and reasons".to_string(),
            ));
        }
        let expects_policy = receipt.provenance == SkyrimRaceDataFieldProvenance::ExplicitPolicy;
        if expects_policy
            != receipt
                .policy_id
                .as_deref()
                .is_some_and(|value| value == policy.policy_id)
        {
            return Err(SkyrimCreatureRecipeError::InvalidRaceDataPolicy(format!(
                "{:?} has inconsistent policy provenance",
                receipt.field
            )));
        }
    }
    Ok(())
}

fn canonical_policy(mut policy: SkyrimRaceDataPolicyReceipt) -> SkyrimRaceDataPolicyReceipt {
    policy.fields.sort_by_key(|receipt| receipt.field);
    policy
}

fn index_motion(
    motion: &SkyrimCreatureMotionCatalog,
) -> Result<BTreeMap<String, &SkyrimCreatureFamilyInventory>, SkyrimCreatureRecipeError> {
    let mut families = BTreeSet::new();
    for family in &motion.families {
        if !families.insert(family.family_id.clone()) {
            return Err(SkyrimCreatureRecipeError::DuplicateMotionFamily(
                family.family_id.clone(),
            ));
        }
    }
    let mut inventories = BTreeMap::new();
    for inventory in &motion.inventories {
        if inventories
            .insert(inventory.family_id.clone(), inventory)
            .is_some()
        {
            return Err(SkyrimCreatureRecipeError::MissingOrDuplicateInventory(
                inventory.family_id.clone(),
            ));
        }
    }
    for family_id in families {
        if !inventories.contains_key(&family_id) {
            return Err(SkyrimCreatureRecipeError::MissingOrDuplicateInventory(
                family_id,
            ));
        }
    }
    Ok(inventories)
}

fn index_assets<'a>(
    assets: &'a [SkyrimCreatureFamilyAssetEvidence],
    motion: &BTreeMap<String, &SkyrimCreatureFamilyInventory>,
) -> Result<BTreeMap<String, &'a SkyrimCreatureFamilyAssetEvidence>, SkyrimCreatureRecipeError> {
    let mut output = BTreeMap::new();
    for evidence in assets {
        if !motion.contains_key(&evidence.family_id) {
            return Err(SkyrimCreatureRecipeError::UnknownAssetFamily(
                evidence.family_id.clone(),
            ));
        }
        if output
            .insert(evidence.family_id.clone(), evidence)
            .is_some()
        {
            return Err(SkyrimCreatureRecipeError::DuplicateAssetFamily(
                evidence.family_id.clone(),
            ));
        }
    }
    Ok(output)
}

fn index_motion_projects(motion: &SkyrimCreatureMotionCatalog) -> BTreeMap<String, Vec<String>> {
    let mut output = BTreeMap::<String, Vec<String>>::new();
    for family in &motion.families {
        output
            .entry(project_key(&family.project_path))
            .or_default()
            .push(family.family_id.clone());
    }
    for ids in output.values_mut() {
        ids.sort();
        ids.dedup();
    }
    output
}

fn build_family_job(
    family_id: &str,
    project_path: &str,
    clip_ids: &[String],
    inventory: &SkyrimCreatureFamilyInventory,
    assets: Option<&SkyrimCreatureFamilyAssetEvidence>,
    motion: &SkyrimCreatureMotionCatalog,
    races: Vec<&CreatureRacePlan>,
    interner: &StringInterner,
) -> SkyrimCreatureFamilyJob {
    let living = motion
        .living_family_evidence()
        .into_iter()
        .find(|evidence| evidence.family_id == family_id)
        .expect("living motion evidence covers every family");
    let (template, mut motion_blockers) = living_motion_receipts(&living);
    let manifest = motion.capability_graph_bundle(family_id);
    let motion_set = motion.motion_set(family_id).ok();
    let mut issues = motion_blockers
        .iter()
        .cloned()
        .map(|blocker| SkyrimCreatureRecipeIssue::Motion {
            family_id: family_id.to_string(),
            blocker,
        })
        .collect::<Vec<_>>();
    if let Err(error) = &manifest {
        let blocker = SkyrimCreatureMotionBlockerReceipt::AdapterUnavailable {
            detail: error.to_string(),
        };
        motion_blockers.push(blocker.clone());
        issues.push(SkyrimCreatureRecipeIssue::Motion {
            family_id: family_id.to_string(),
            blocker,
        });
    }
    let graph_roles = manifest
        .as_ref()
        .ok()
        .map(|(manifest, _)| manifest.roles.as_slice())
        .unwrap_or_default();
    let clips = clip_ids
        .iter()
        .filter_map(|clip_id| motion.clip(clip_id))
        .map(|clip| clip_receipt(clip, graph_roles))
        .collect::<Vec<_>>();
    let graph = graph_receipt(
        template,
        living.disposition == SkyrimLivingFamilyDisposition::Ready && manifest.is_ok(),
        manifest.ok(),
        motion_set,
        &motion_blockers,
    );

    let mut graph_variants = Vec::new();
    let mut controllers = Vec::new();
    let mut body_variants = Vec::new();
    let mut asset_claims = Vec::new();
    let (ragdoll, weapon_bearing) = if let Some(assets) = assets {
        graph_variants = assets.graph_variants.clone();
        canonicalize_graph_variants(&mut graph_variants);
        body_variants = assets.body_variants.clone();
        canonicalize_body_variants(&mut body_variants);
        asset_claims = assets.asset_claims.clone();
        canonicalize_claims(&mut asset_claims);
        validate_asset_evidence(
            family_id,
            inventory,
            assets,
            &graph_variants,
            &body_variants,
            &asset_claims,
            &clips,
            &races,
            interner,
            &mut issues,
        );
        controllers = controller_receipts(family_id, inventory, assets, &mut issues);
        (assets.ragdoll.clone(), assets.weapon_bearing.clone())
    } else {
        issues.push(SkyrimCreatureRecipeIssue::MissingAssetEvidence {
            family_id: family_id.to_string(),
        });
        (
            SkyrimRagdollCapabilityEvidence::Missing {
                expected_paths: canonical_paths(&inventory.ragdoll_paths),
            },
            SkyrimExplicitCapabilityEvidence::Missing {
                reason: "family asset evidence is absent".to_string(),
            },
        )
    };

    match &ragdoll {
        SkyrimRagdollCapabilityEvidence::Present { .. }
        | SkyrimRagdollCapabilityEvidence::NotApplicable { .. } => {}
        _ => issues.push(SkyrimCreatureRecipeIssue::RagdollUnavailable {
            family_id: family_id.to_string(),
            evidence: ragdoll.clone(),
        }),
    }
    match &weapon_bearing {
        SkyrimExplicitCapabilityEvidence::Supported { .. }
        | SkyrimExplicitCapabilityEvidence::NotApplicable { .. } => {}
        _ => issues.push(SkyrimCreatureRecipeIssue::WeaponBearingUnavailable {
            family_id: family_id.to_string(),
            evidence: weapon_bearing.clone(),
        }),
    }
    canonicalize_issues(&mut issues);
    let disposition = family_disposition(&graph, &ragdoll, &weapon_bearing, issues);
    let mut source_races = races
        .iter()
        .map(|race| race.source_race.format(interner))
        .collect::<Vec<_>>();
    source_races.sort_by_key(|value| value.to_ascii_lowercase());
    source_races.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    SkyrimCreatureFamilyJob {
        family_id: family_id.to_string(),
        project_path: canonical_path(project_path),
        source_races,
        inventory: inventory_receipt(inventory),
        graph_variants,
        graph,
        clips,
        controllers,
        ragdoll,
        weapon_bearing,
        body_variants,
        asset_claims,
        disposition,
    }
}

fn build_candidate(
    race: &CreatureRacePlan,
    catalog: &CreatureCorpusPlan,
    race_data: &CreatureRaceDataEvidencePlan,
    project_index: &BTreeMap<String, Vec<String>>,
    family_dispositions: &BTreeMap<String, SkyrimCreatureFamilyDisposition>,
    assets: &BTreeMap<String, &SkyrimCreatureFamilyAssetEvidence>,
    policy: &SkyrimRaceDataPolicyReceipt,
    interner: &StringInterner,
) -> SkyrimCreatureCandidateRecipe {
    let source_race = race.source_race.format(interner);
    let mut issues = race
        .issues
        .iter()
        .map(|issue| SkyrimCreatureRecipeIssue::Record {
            issue: record_issue(issue, interner),
        })
        .collect::<Vec<_>>();
    let mut family_ids = Vec::new();
    for project_path in race.project_paths.iter().take(1) {
        match project_index
            .get(&project_key(project_path))
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            [] => issues.push(SkyrimCreatureRecipeIssue::MissingMotionFamily {
                project_path: canonical_path(project_path),
            }),
            [family_id] => family_ids.push(family_id.clone()),
            candidates => issues.push(SkyrimCreatureRecipeIssue::AmbiguousMotionFamily {
                project_path: canonical_path(project_path),
                family_ids: candidates.to_vec(),
            }),
        }
    }
    family_ids.sort();
    family_ids.dedup();
    for family_id in &family_ids {
        if let Some(disposition) = family_dispositions.get(family_id) {
            match disposition {
                SkyrimCreatureFamilyDisposition::Ready { .. } => {}
                SkyrimCreatureFamilyDisposition::Blocked {
                    issues: family_issues,
                }
                | SkyrimCreatureFamilyDisposition::Unsupported {
                    issues: family_issues,
                }
                | SkyrimCreatureFamilyDisposition::Ambiguous {
                    issues: family_issues,
                } => issues.extend(family_issues.iter().cloned()),
            }
        }
        let has_body = assets.get(family_id).is_some_and(|family| {
            family
                .body_variants
                .iter()
                .any(|variant| variant.source_race.eq_ignore_ascii_case(&source_race))
        });
        if !has_body {
            issues.push(SkyrimCreatureRecipeIssue::MissingBodyVariant {
                family_id: family_id.clone(),
                source_race: source_race.clone(),
            });
        }
    }

    let race_result = race_data
        .race(race.source_race)
        .expect("RACE.DATA plan covers every catalog race");
    match race_result.disposition {
        CreatureRaceDataDisposition::Complete => {}
        CreatureRaceDataDisposition::Missing => {
            issues.push(SkyrimCreatureRecipeIssue::RaceDataMissing {
                fields: race_result.missing_fields.clone(),
            })
        }
        CreatureRaceDataDisposition::Invalid => {
            issues.push(SkyrimCreatureRecipeIssue::RaceDataInvalid {
                fields: race_result.invalid_fields.clone(),
            })
        }
    }
    canonicalize_issues(&mut issues);
    let disposition = candidate_disposition(family_ids, issues);
    SkyrimCreatureCandidateRecipe {
        source_race,
        editor_id: race.editor_id.clone(),
        source_skin: race.skin.map(|key| key.format(interner)),
        source_body_part_data: race.body_part_data.map(|key| key.format(interner)),
        source_armor_addons: form_keys(&race.armor_addons, interner),
        source_body_models: canonical_paths(&race.body_models),
        project_paths: canonical_paths(&race.project_paths),
        skeleton_paths: canonical_paths(&race.skeleton_paths),
        attack_events: canonical_strings(&race.attack_events),
        attack_contract: canonical_strings(&race.attack_contract),
        attack_data: race
            .attack_data
            .iter()
            .map(|attack| attack_data_receipt(attack, interner))
            .collect(),
        attack_spells: form_keys(&race.attack_spells, interner),
        npc_templates: catalog
            .npcs_for_race(race.source_race)
            .map(|npc| npc_receipt(npc, interner))
            .collect(),
        race_data: SkyrimCreatureRaceDataReceipt {
            disposition: race_result.disposition,
            evidence: race_result.evidence.clone(),
            derivation: race_result.derivation.clone(),
            unarmed_data: race_result.unarmed_data.clone(),
            missing_fields: race_result.missing_fields.clone(),
            invalid_fields: race_result.invalid_fields.clone(),
            field_receipts: policy.fields.clone(),
        },
        disposition,
    }
}

fn attack_data_receipt(
    attack: &CreatureRaceAttackDataPlan,
    interner: &StringInterner,
) -> SkyrimRaceAttackDataReceipt {
    let form_receipt = |form_key: Option<FormKey>, signature: &Option<String>| {
        form_key.map(|form_key| SkyrimRaceAttackFormReceipt {
            source: form_key.format(interner),
            signature: signature.clone().unwrap_or_default(),
        })
    };
    SkyrimRaceAttackDataReceipt {
        ordinal: attack.ordinal,
        event: attack.event.clone(),
        damage_multiplier_bits: attack.damage_multiplier_bits,
        attack_chance_bits: attack.attack_chance_bits,
        attack_spell: form_receipt(attack.attack_spell, &attack.attack_spell_signature),
        attack_flags: attack.attack_flags,
        attack_angle_bits: attack.attack_angle_bits,
        strike_angle_bits: attack.strike_angle_bits,
        stagger_bits: attack.stagger_bits,
        attack_type: form_receipt(attack.attack_type, &attack.attack_type_signature),
        knockdown_bits: attack.knockdown_bits,
        recovery_time_bits: attack.recovery_time_bits,
        stamina_multiplier_bits: attack.stamina_multiplier_bits,
    }
}

fn candidate_disposition(
    family_job_ids: Vec<String>,
    issues: Vec<SkyrimCreatureRecipeIssue>,
) -> SkyrimCreatureCandidateDisposition {
    if issues.is_empty() && !family_job_ids.is_empty() {
        return SkyrimCreatureCandidateDisposition::Ready { family_job_ids };
    }
    if issues.iter().any(is_ambiguous_issue) {
        SkyrimCreatureCandidateDisposition::Ambiguous { issues }
    } else if issues.iter().any(is_unsupported_issue) {
        SkyrimCreatureCandidateDisposition::Unsupported { issues }
    } else {
        SkyrimCreatureCandidateDisposition::Blocked { issues }
    }
}

fn family_disposition(
    graph: &SkyrimCreatureGraphEvidence,
    ragdoll: &SkyrimRagdollCapabilityEvidence,
    weapon_bearing: &SkyrimExplicitCapabilityEvidence,
    issues: Vec<SkyrimCreatureRecipeIssue>,
) -> SkyrimCreatureFamilyDisposition {
    if !issues.is_empty() {
        if issues.iter().any(is_ambiguous_issue) {
            return SkyrimCreatureFamilyDisposition::Ambiguous { issues };
        }
        if issues.iter().any(is_unsupported_issue) {
            return SkyrimCreatureFamilyDisposition::Unsupported { issues };
        }
        return SkyrimCreatureFamilyDisposition::Blocked { issues };
    }
    let mut capabilities = capabilities(graph);
    if matches!(ragdoll, SkyrimRagdollCapabilityEvidence::Present { .. }) {
        capabilities.push(SkyrimCreatureCapability::Ragdoll);
    }
    if matches!(
        weapon_bearing,
        SkyrimExplicitCapabilityEvidence::Supported { .. }
    ) {
        capabilities.push(SkyrimCreatureCapability::WeaponBearing);
    }
    capabilities.sort();
    capabilities.dedup();
    SkyrimCreatureFamilyDisposition::Ready { capabilities }
}

fn capabilities(graph: &SkyrimCreatureGraphEvidence) -> Vec<SkyrimCreatureCapability> {
    let mut values = match graph.template {
        CreatureGraphTemplate::PassiveGround => graph
            .roles
            .iter()
            .any(|role| role.role == crate::source_rig::CreatureClipRole::GroundForward)
            .then_some(SkyrimCreatureCapability::GroundLocomotion)
            .into_iter()
            .collect(),
        CreatureGraphTemplate::GroundMelee => vec![
            SkyrimCreatureCapability::GroundLocomotion,
            SkyrimCreatureCapability::MeleeAttack,
        ],
        CreatureGraphTemplate::GroundRangedProjectile => vec![
            SkyrimCreatureCapability::GroundLocomotion,
            SkyrimCreatureCapability::RangedProjectile,
        ],
        CreatureGraphTemplate::GroundMeleeRanged => vec![
            SkyrimCreatureCapability::GroundLocomotion,
            SkyrimCreatureCapability::MeleeAttack,
            SkyrimCreatureCapability::RangedProjectile,
        ],
        CreatureGraphTemplate::GroundSwim => {
            let mut values = vec![
                SkyrimCreatureCapability::GroundLocomotion,
                SkyrimCreatureCapability::Swimming,
            ];
            append_graph_attack_capabilities(graph, &mut values);
            values
        }
        CreatureGraphTemplate::GroundFly => {
            let mut values = vec![
                SkyrimCreatureCapability::GroundLocomotion,
                SkyrimCreatureCapability::Flying,
            ];
            append_graph_attack_capabilities(graph, &mut values);
            values
        }
        CreatureGraphTemplate::Swim => vec![SkyrimCreatureCapability::Swimming],
        CreatureGraphTemplate::Fly => vec![SkyrimCreatureCapability::Flying],
        CreatureGraphTemplate::StationaryTurret => vec![
            SkyrimCreatureCapability::Stationary,
            SkyrimCreatureCapability::RangedProjectile,
        ],
        CreatureGraphTemplate::RobotContinuousAttack => {
            vec![SkyrimCreatureCapability::ContinuousAttack]
        }
    };
    if !graph.overlays.is_empty() {
        values.push(SkyrimCreatureCapability::Overlay);
    }
    if !graph.required_rigs.is_empty() || !graph.rigs.is_empty() {
        values.push(SkyrimCreatureCapability::MultiRig);
    }
    values
}

fn append_graph_attack_capabilities(
    graph: &SkyrimCreatureGraphEvidence,
    values: &mut Vec<SkyrimCreatureCapability>,
) {
    if graph
        .roles
        .iter()
        .any(|role| role.role == crate::source_rig::CreatureClipRole::MeleeAttack)
    {
        values.push(SkyrimCreatureCapability::MeleeAttack);
    }
    if graph
        .roles
        .iter()
        .any(|role| role.role == crate::source_rig::CreatureClipRole::ProjectileAttack)
    {
        values.push(SkyrimCreatureCapability::RangedProjectile);
    }
}

fn validate_asset_evidence(
    family_id: &str,
    inventory: &SkyrimCreatureFamilyInventory,
    assets: &SkyrimCreatureFamilyAssetEvidence,
    graph_variants: &[SkyrimCreatureGraphVariantEvidence],
    body_variants: &[SkyrimCreatureBodyVariantEvidence],
    claims: &[SkyrimRecursiveAssetClaim],
    clips: &[SkyrimCreatureClipReceipt],
    races: &[&CreatureRacePlan],
    interner: &StringInterner,
    issues: &mut Vec<SkyrimCreatureRecipeIssue>,
) {
    if graph_variants.is_empty() {
        issues.push(SkyrimCreatureRecipeIssue::MissingGraphVariant {
            family_id: family_id.to_string(),
        });
    }
    for variant in graph_variants {
        if variant.actual_root_bone.trim().is_empty()
            || variant.actual_root_bone.contains(['/', '\\'])
            || variant
                .actual_root_bone
                .to_ascii_lowercase()
                .ends_with(".nif")
            || variant
                .actual_root_bone
                .to_ascii_lowercase()
                .ends_with(".hkx")
        {
            issues.push(SkyrimCreatureRecipeIssue::InvalidRootBone {
                family_id: family_id.to_string(),
                value: variant.actual_root_bone.clone(),
            });
        }
    }

    let claim_index = claims
        .iter()
        .map(|claim| (path_key(&claim.path), claim))
        .collect::<BTreeMap<_, _>>();
    let mut roots = Vec::<(String, SkyrimAssetRole)>::new();
    roots.push((inventory.project_path.clone(), SkyrimAssetRole::Project));
    roots.extend(
        inventory
            .character_paths
            .iter()
            .cloned()
            .map(|path| (path, SkyrimAssetRole::Character)),
    );
    roots.extend(
        inventory
            .animation_skeleton_paths
            .iter()
            .cloned()
            .map(|path| (path, SkyrimAssetRole::AnimationSkeleton)),
    );
    roots.extend(graph_variants.iter().map(|variant| {
        (
            variant.skeleton_path.clone(),
            SkyrimAssetRole::VisualSkeletonNif,
        )
    }));
    roots.extend(
        inventory
            .ragdoll_paths
            .iter()
            .cloned()
            .map(|path| (path, SkyrimAssetRole::Ragdoll)),
    );
    roots.extend(
        inventory
            .behavior_paths
            .iter()
            .cloned()
            .map(|path| (path, SkyrimAssetRole::Behavior)),
    );
    for race in races {
        roots.extend(
            race.body_models
                .iter()
                .cloned()
                .map(|path| (path, SkyrimAssetRole::BodyNif)),
        );
        roots.extend(
            race.skeleton_paths
                .iter()
                .cloned()
                .map(|path| (path, SkyrimAssetRole::VisualSkeletonNif)),
        );
        let source_race = race.source_race.format(interner);
        let variants = body_variants
            .iter()
            .filter(|variant| variant.source_race.eq_ignore_ascii_case(&source_race))
            .collect::<Vec<_>>();
        if variants.is_empty() {
            issues.push(SkyrimCreatureRecipeIssue::MissingBodyVariant {
                family_id: family_id.to_string(),
                source_race,
            });
        }
        for variant in variants {
            if !race
                .body_models
                .iter()
                .any(|body| path_key(body) == path_key(&variant.body_nif))
                || !race.armor_addons.iter().any(|addon| {
                    addon
                        .format(interner)
                        .eq_ignore_ascii_case(&variant.armor_addon)
                })
            {
                issues.push(SkyrimCreatureRecipeIssue::BodyVariantNotRaceConditioned {
                    family_id: family_id.to_string(),
                    source_race: race.source_race.format(interner),
                    body_nif: canonical_path(&variant.body_nif),
                });
            }
        }
    }
    roots.extend(
        clips
            .iter()
            .map(|clip| (clip.clip_path.clone(), SkyrimAssetRole::AnimationClip)),
    );
    for (path, role) in &roots {
        match claim_index.get(&path_key(path)) {
            None => issues.push(SkyrimCreatureRecipeIssue::InventoryPathUnclaimed {
                family_id: family_id.to_string(),
                path: canonical_path(path),
                role: *role,
            }),
            Some(claim)
                if !claim_satisfies_inventory_role(
                    claim,
                    *role,
                    path,
                    &inventory.animation_skeleton_paths,
                    &inventory.ragdoll_paths,
                ) =>
            {
                issues.push(SkyrimCreatureRecipeIssue::InventoryPathUnclaimed {
                    family_id: family_id.to_string(),
                    path: canonical_path(path),
                    role: *role,
                })
            }
            Some(claim) => validate_available_claim(family_id, claim, issues),
        }
    }
    for claim in claims {
        validate_available_claim(family_id, claim, issues);
        for dependency in &claim.dependencies {
            if !claim_index.contains_key(&path_key(dependency)) {
                issues.push(SkyrimCreatureRecipeIssue::DanglingAssetDependency {
                    family_id: family_id.to_string(),
                    owner: claim.path.clone(),
                    dependency: canonical_path(dependency),
                });
            }
        }
    }
    let reachable = reachable_claims(&roots, &claim_index);
    for claim in claims {
        if !reachable.contains(&path_key(&claim.path)) {
            issues.push(SkyrimCreatureRecipeIssue::UnreachableAssetClaim {
                family_id: family_id.to_string(),
                path: claim.path.clone(),
            });
        }
    }

    let ragdolls = match &assets.ragdoll {
        SkyrimRagdollCapabilityEvidence::Present { paths }
        | SkyrimRagdollCapabilityEvidence::Unsupported { paths, .. }
        | SkyrimRagdollCapabilityEvidence::Ambiguous { paths, .. } => paths.as_slice(),
        SkyrimRagdollCapabilityEvidence::Missing { expected_paths } => expected_paths.as_slice(),
        SkyrimRagdollCapabilityEvidence::NotApplicable { .. } => &[],
    };
    if !same_path_set(ragdolls, &inventory.ragdoll_paths) {
        issues.push(SkyrimCreatureRecipeIssue::RagdollUnavailable {
            family_id: family_id.to_string(),
            evidence: assets.ragdoll.clone(),
        });
    }
}

fn claim_satisfies_inventory_role(
    claim: &SkyrimRecursiveAssetClaim,
    requested_role: SkyrimAssetRole,
    path: &str,
    animation_skeleton_paths: &[String],
    ragdoll_paths: &[String],
) -> bool {
    if claim.role == requested_role {
        return true;
    }
    if matches!(
        (claim.role, requested_role),
        (SkyrimAssetRole::BodyNif, SkyrimAssetRole::VisualSkeletonNif)
            | (SkyrimAssetRole::VisualSkeletonNif, SkyrimAssetRole::BodyNif)
    ) {
        return true;
    }
    if !matches!(
        (claim.role, requested_role),
        (SkyrimAssetRole::AnimationSkeleton, SkyrimAssetRole::Ragdoll)
            | (SkyrimAssetRole::Ragdoll, SkyrimAssetRole::AnimationSkeleton)
    ) {
        return false;
    }
    let key = path_key(path);
    animation_skeleton_paths
        .iter()
        .any(|candidate| path_key(candidate) == key)
        && ragdoll_paths
            .iter()
            .any(|candidate| path_key(candidate) == key)
}

fn validate_available_claim(
    family_id: &str,
    claim: &SkyrimRecursiveAssetClaim,
    issues: &mut Vec<SkyrimCreatureRecipeIssue>,
) {
    match &claim.disposition {
        SkyrimAssetClaimDisposition::Present { blake3 }
            if blake3.len() == 64 && blake3.bytes().all(|value| value.is_ascii_hexdigit()) => {}
        disposition => issues.push(SkyrimCreatureRecipeIssue::AssetUnavailable {
            family_id: family_id.to_string(),
            path: claim.path.clone(),
            disposition: disposition.clone(),
        }),
    }
}

fn reachable_claims(
    roots: &[(String, SkyrimAssetRole)],
    claims: &BTreeMap<String, &SkyrimRecursiveAssetClaim>,
) -> BTreeSet<String> {
    let mut pending = roots
        .iter()
        .map(|(path, _)| path_key(path))
        .collect::<Vec<_>>();
    let mut visited = BTreeSet::new();
    while let Some(path) = pending.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
        if let Some(claim) = claims.get(&path) {
            pending.extend(claim.dependencies.iter().map(|path| path_key(path)));
        }
    }
    visited
}

fn controller_receipts(
    family_id: &str,
    inventory: &SkyrimCreatureFamilyInventory,
    assets: &SkyrimCreatureFamilyAssetEvidence,
    issues: &mut Vec<SkyrimCreatureRecipeIssue>,
) -> Vec<SkyrimCreatureControllerReceipt> {
    let explicit = assets
        .controllers
        .iter()
        .map(|controller| (path_key(&controller.character_path), controller))
        .collect::<BTreeMap<_, _>>();
    let mut receipts = Vec::new();
    for controller in &inventory.controllers {
        let character_path = canonical_path(&controller.character_path);
        let Some(recipe) = explicit.get(&path_key(&controller.character_path)).copied() else {
            issues.push(SkyrimCreatureRecipeIssue::MissingControllerEvidence {
                family_id: family_id.to_string(),
                character_path,
            });
            receipts.push(controller_receipt(controller, None));
            continue;
        };
        if recipe.axes.is_none() {
            issues.push(SkyrimCreatureRecipeIssue::MissingControllerAxes {
                family_id: family_id.to_string(),
                character_path: character_path.clone(),
            });
        } else if recipe.axes.is_some_and(|axes| axes.up == axes.forward) {
            issues.push(SkyrimCreatureRecipeIssue::InvalidControllerAxes {
                family_id: family_id.to_string(),
                character_path: character_path.clone(),
            });
        }
        if recipe.capsule.is_none() {
            issues.push(SkyrimCreatureRecipeIssue::MissingControllerCapsule {
                family_id: family_id.to_string(),
                character_path: character_path.clone(),
            });
        }
        if let (Some(decoded), Some(explicit_capsule)) = (&controller.capsule, &recipe.capsule)
            && (decoded.total_height.to_bits() != explicit_capsule.total_height.to_bits()
                || decoded.radius.to_bits() != explicit_capsule.radius.to_bits())
        {
            issues.push(SkyrimCreatureRecipeIssue::ControllerEvidenceConflict {
                family_id: family_id.to_string(),
                character_path: character_path.clone(),
                detail: "explicit capsule differs from decoded controller inventory".to_string(),
            });
        }
        let decoded_layout = controller.layout.as_ref().map(controller_layout);
        if decoded_layout.as_ref() != Some(&recipe.layout) {
            issues.push(SkyrimCreatureRecipeIssue::ControllerEvidenceConflict {
                family_id: family_id.to_string(),
                character_path: character_path.clone(),
                detail: "explicit layout differs from decoded controller provenance".to_string(),
            });
        }
        let exact_legacy_layout = matches!(
            &recipe.layout,
            SkyrimControllerLayoutEvidence::LegacyCharacterControllerInfo {
                contents_version,
                character_data_signature: SKYRIM_LEGACY_CHARACTER_DATA_SIGNATURE,
                controller_signature: SKYRIM_LEGACY_CONTROLLER_SIGNATURE,
            } if contents_version == "hk_2010.2.0-r1"
        );
        if !exact_legacy_layout || controller.legacy_controller_recipe_evidence().is_none() {
            issues.push(SkyrimCreatureRecipeIssue::ControllerEvidenceConflict {
                family_id: family_id.to_string(),
                character_path: character_path.clone(),
                detail: "controller lacks the exact Skyrim 2010 legacy layout recipe evidence"
                    .to_string(),
            });
        }
        if !matches!(
            controller.disposition,
            SkyrimControllerEvidenceDisposition::Complete
        ) {
            issues.push(SkyrimCreatureRecipeIssue::ControllerUnavailable {
                family_id: family_id.to_string(),
                character_path,
                disposition: controller_disposition(&controller.disposition),
            });
        }
        receipts.push(controller_receipt(controller, Some(recipe)));
    }
    for character_path in &inventory.character_paths {
        if !inventory
            .controllers
            .iter()
            .any(|controller| path_key(&controller.character_path) == path_key(character_path))
        {
            issues.push(SkyrimCreatureRecipeIssue::MissingControllerEvidence {
                family_id: family_id.to_string(),
                character_path: canonical_path(character_path),
            });
        }
    }
    receipts.sort_by(|left, right| left.character_path.cmp(&right.character_path));
    receipts
}

fn controller_receipt(
    controller: &SkyrimCharacterControllerEvidence,
    explicit: Option<&SkyrimCreatureControllerRecipeEvidence>,
) -> SkyrimCreatureControllerReceipt {
    let legacy = controller.legacy_controller_recipe_evidence();
    SkyrimCreatureControllerReceipt {
        character_path: canonical_path(&controller.character_path),
        character_data_class: controller.character_data_class.clone(),
        controller_class: controller.controller_class.clone(),
        controller_cinfo_class: controller.controller_cinfo_class.clone(),
        architecture: controller
            .architecture
            .as_ref()
            .map(controller_architecture),
        layout: controller
            .layout
            .as_ref()
            .map(controller_layout)
            .unwrap_or(SkyrimControllerLayoutEvidence::Missing),
        collision_filter_info: controller.collision_filter_info,
        rigid_body_type: controller.rigid_body_type,
        shape_type: controller.shape_type,
        capsule: explicit.and_then(|value| {
            value.capsule.as_ref().map(|capsule| CapsuleEvidence {
                radius: capsule.radius,
                total_height: capsule.total_height,
                architecture: capsule.architecture,
            })
        }),
        axes: explicit.and_then(|value| value.axes),
        model: legacy
            .as_ref()
            .map(|value| controller_model(&value.model))
            .or_else(|| controller.model.as_ref().map(controller_model)),
        disposition: controller_disposition(&controller.disposition),
    }
}

fn controller_layout(value: &DecodedControllerLayout) -> SkyrimControllerLayoutEvidence {
    match value {
        DecodedControllerLayout::NestedCharacterControllerSetup {
            contents_version,
            character_data_signature,
            controller_signature,
        } => SkyrimControllerLayoutEvidence::NestedCharacterControllerSetup {
            contents_version: contents_version.clone(),
            character_data_signature: *character_data_signature,
            controller_signature: *controller_signature,
        },
        DecodedControllerLayout::LegacyCharacterControllerInfo {
            contents_version,
            character_data_signature,
            controller_signature,
        } => SkyrimControllerLayoutEvidence::LegacyCharacterControllerInfo {
            contents_version: contents_version.clone(),
            character_data_signature: *character_data_signature,
            controller_signature: *controller_signature,
        },
    }
}

fn controller_model(value: &SkyrimControllerModelEvidence) -> SkyrimControllerModelReceipt {
    SkyrimControllerModelReceipt {
        up_ms: value.up_ms,
        forward_ms: value.forward_ms,
        right_ms: value.right_ms,
        scale: value.scale,
    }
}

fn inventory_receipt(inventory: &SkyrimCreatureFamilyInventory) -> SkyrimCreatureInventoryReceipt {
    SkyrimCreatureInventoryReceipt {
        project_path: canonical_path(&inventory.project_path),
        character_paths: canonical_paths(&inventory.character_paths),
        animation_skeleton_paths: canonical_paths(&inventory.animation_skeleton_paths),
        ragdoll_paths: canonical_paths(&inventory.ragdoll_paths),
        behavior_paths: canonical_paths(&inventory.behavior_paths),
    }
}

fn graph_receipt(
    template: CreatureGraphTemplate,
    ready: bool,
    manifest: Option<(
        CapabilityGraphManifest,
        Vec<crate::source_rig::VariableDecl>,
    )>,
    motion: Option<MotionSet>,
    blockers: &[SkyrimCreatureMotionBlockerReceipt],
) -> SkyrimCreatureGraphEvidence {
    let (
        template,
        roles,
        candidate_attack_bindings,
        idle_event,
        explicit_events,
        variables,
        overlays,
    ) = match manifest {
        Some((manifest, variables)) => (
            manifest.template,
            manifest.roles,
            manifest.candidate_attack_bindings,
            Some(manifest.idle_event),
            manifest.explicit_events,
            variables,
            manifest.overlays,
        ),
        None => (
            template,
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ),
    };
    let (required_rigs, rigs) = motion
        .map(|motion| (motion.required_rigs, motion.rigs))
        .unwrap_or_default();
    SkyrimCreatureGraphEvidence {
        template,
        ready,
        roles,
        candidate_attack_bindings,
        idle_event,
        explicit_events,
        variables,
        overlays,
        required_rigs,
        rigs,
        blockers: blockers.to_vec(),
    }
}

fn clip_receipt(
    clip: &SkyrimCatalogClip,
    graph_roles: &[CapabilityClipRole],
) -> SkyrimCreatureClipReceipt {
    let root_motion_locators = match &clip.disposition {
        SkyrimClipDisposition::Role { evidence, .. }
        | SkyrimClipDisposition::SharedOverlay { evidence }
        | SkyrimClipDisposition::Ambiguous { evidence, .. } => evidence
            .iter()
            .filter_map(|entry| entry.root_motion_locator.as_ref())
            .map(root_motion_locator_receipt)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        SkyrimClipDisposition::SharedPaired { .. } | SkyrimClipDisposition::Unsupported { .. } => {
            Vec::new()
        }
    };
    let (mut role, mut trigger_event, mut trigger_aliases, role_evidence, original_skeleton_name) =
        match &clip.disposition {
            SkyrimClipDisposition::Role {
                role,
                trigger_event,
                trigger_aliases,
                evidence,
            } => (
                SkyrimCreatureClipRoleReceipt::Role(motion_role(*role)),
                trigger_event.clone(),
                trigger_aliases.clone(),
                evidence
                    .iter()
                    .map(|entry| SkyrimMotionRoleEvidenceReceipt {
                        source: motion_source(entry.source),
                        detail: entry.detail.clone(),
                        time: entry.time,
                    })
                    .collect(),
                None,
            ),
            SkyrimClipDisposition::SharedPaired {
                original_skeleton_name,
            } => (
                SkyrimCreatureClipRoleReceipt::SharedPaired,
                None,
                Vec::new(),
                Vec::new(),
                Some(original_skeleton_name.clone()),
            ),
            SkyrimClipDisposition::SharedOverlay { evidence } => (
                SkyrimCreatureClipRoleReceipt::SharedOverlay,
                None,
                Vec::new(),
                evidence
                    .iter()
                    .map(|entry| SkyrimMotionRoleEvidenceReceipt {
                        source: motion_source(entry.source),
                        detail: entry.detail.clone(),
                        time: entry.time,
                    })
                    .collect(),
                None,
            ),
            SkyrimClipDisposition::Unsupported { reason } => (
                SkyrimCreatureClipRoleReceipt::Unsupported(unsupported_motion(reason)),
                None,
                Vec::new(),
                Vec::new(),
                None,
            ),
            SkyrimClipDisposition::Ambiguous { roles, evidence } => (
                SkyrimCreatureClipRoleReceipt::Ambiguous(
                    roles.iter().copied().map(motion_role).collect(),
                ),
                None,
                Vec::new(),
                evidence
                    .iter()
                    .map(|entry| SkyrimMotionRoleEvidenceReceipt {
                        source: motion_source(entry.source),
                        detail: entry.detail.clone(),
                        time: entry.time,
                    })
                    .collect(),
                None,
            ),
        };
    let selected_roles = graph_roles
        .iter()
        .filter(|graph_role| {
            graph_role
                .generator
                .clip_names(&graph_role.clip_name)
                .iter()
                .any(|clip_name| *clip_name == clip.clip_id)
        })
        .collect::<Vec<_>>();
    if let [selected] = selected_roles.as_slice() {
        role = SkyrimCreatureClipRoleReceipt::Role(capability_motion_role(selected.role));
        trigger_event.clone_from(&selected.trigger_event);
        trigger_aliases.clone_from(&selected.trigger_aliases);
    }
    let original_skeleton_name = clip
        .original_skeleton_name
        .clone()
        .or(original_skeleton_name);
    SkyrimCreatureClipReceipt {
        clip_id: clip.clip_id.clone(),
        clip_path: canonical_path(&clip.clip_path),
        family_ids: canonical_strings(&clip.family_ids),
        role,
        trigger_event,
        trigger_aliases,
        role_evidence,
        root_motion: root_motion(&clip.root_motion),
        root_motion_locators,
        events: clip
            .events
            .iter()
            .map(|event| SkyrimCatalogEventReceipt {
                name: event.name.clone(),
                time: event.time,
                source: motion_source(event.source),
            })
            .collect(),
        annotations: clip
            .annotations
            .iter()
            .map(|event| SkyrimTimedEventReceipt {
                name: event.name.clone(),
                time: event.time,
            })
            .collect(),
        original_skeleton_name,
    }
}

fn root_motion_locator_receipt(
    locator: &SkyrimRootMotionSourceLocator,
) -> SkyrimRootMotionLocatorReceipt {
    match locator {
        SkyrimRootMotionSourceLocator::HavokReferenceFrame { clip_path } => {
            SkyrimRootMotionLocatorReceipt::HavokReferenceFrame {
                clip_path: canonical_path(clip_path),
            }
        }
        SkyrimRootMotionSourceLocator::BoundAnims {
            source_path,
            project_stem,
            animation_index,
        } => SkyrimRootMotionLocatorReceipt::BoundAnims {
            source_path: source_path.clone(),
            project_stem: project_stem.clone(),
            animation_index: *animation_index,
        },
    }
}

fn capability_motion_role(role: CreatureClipRole) -> SkyrimCreatureMotionRoleReceipt {
    match role {
        CreatureClipRole::Idle => SkyrimCreatureMotionRoleReceipt::Idle,
        CreatureClipRole::GroundForward => SkyrimCreatureMotionRoleReceipt::GroundLocomotion,
        CreatureClipRole::TurnLeft90 => SkyrimCreatureMotionRoleReceipt::TurnLeft,
        CreatureClipRole::TurnRight90 => SkyrimCreatureMotionRoleReceipt::TurnRight,
        CreatureClipRole::MeleeAttack => SkyrimCreatureMotionRoleReceipt::MeleeAttack,
        CreatureClipRole::ProjectileAttack => SkyrimCreatureMotionRoleReceipt::ProjectileAttack,
        CreatureClipRole::SwimIdle => SkyrimCreatureMotionRoleReceipt::SwimIdle,
        CreatureClipRole::SwimForward => SkyrimCreatureMotionRoleReceipt::SwimLocomotion,
        CreatureClipRole::FlyIdle => SkyrimCreatureMotionRoleReceipt::FlyIdle,
        CreatureClipRole::FlyForward => SkyrimCreatureMotionRoleReceipt::FlyLocomotion,
        CreatureClipRole::StationaryIdle => SkyrimCreatureMotionRoleReceipt::StationaryIdle,
        CreatureClipRole::ContinuousAttackStart => SkyrimCreatureMotionRoleReceipt::MechanicalStart,
        CreatureClipRole::ContinuousAttackLoop => SkyrimCreatureMotionRoleReceipt::MechanicalLoop,
        CreatureClipRole::ContinuousAttackStop => SkyrimCreatureMotionRoleReceipt::MechanicalStop,
    }
}

fn root_motion(value: &SkyrimRootMotion) -> SkyrimRootMotionReceipt {
    match value {
        SkyrimRootMotion::Unknown => SkyrimRootMotionReceipt::Unknown,
        SkyrimRootMotion::Stationary {
            source,
            sample_count,
        } => SkyrimRootMotionReceipt::Stationary {
            source: root_motion_source(*source),
            sample_count: *sample_count,
        },
        SkyrimRootMotion::Sampled {
            source,
            sample_count,
            translation_delta,
            rotation_start,
            rotation_end,
        } => SkyrimRootMotionReceipt::Sampled {
            source: root_motion_source(*source),
            sample_count: *sample_count,
            translation_delta: *translation_delta,
            rotation_start: *rotation_start,
            rotation_end: *rotation_end,
        },
        SkyrimRootMotion::Unsupported { detail } => SkyrimRootMotionReceipt::Unsupported {
            detail: detail.clone(),
        },
    }
}

fn root_motion_source(value: SkyrimRootMotionSource) -> SkyrimRootMotionSourceReceipt {
    match value {
        SkyrimRootMotionSource::HavokReferenceFrame => {
            SkyrimRootMotionSourceReceipt::HavokReferenceFrame
        }
        SkyrimRootMotionSource::BoundAnims => SkyrimRootMotionSourceReceipt::BoundAnims,
    }
}

fn motion_source(value: SkyrimMotionEvidenceSource) -> SkyrimMotionEvidenceSourceReceipt {
    match value {
        SkyrimMotionEvidenceSource::RaceAttackEvent => {
            SkyrimMotionEvidenceSourceReceipt::RaceAttackEvent
        }
        SkyrimMotionEvidenceSource::BehaviorTransition => {
            SkyrimMotionEvidenceSourceReceipt::BehaviorTransition
        }
        SkyrimMotionEvidenceSource::BehaviorGenerator => {
            SkyrimMotionEvidenceSourceReceipt::BehaviorGenerator
        }
        SkyrimMotionEvidenceSource::AnimationData => {
            SkyrimMotionEvidenceSourceReceipt::AnimationData
        }
        SkyrimMotionEvidenceSource::ClipAnnotation => {
            SkyrimMotionEvidenceSourceReceipt::ClipAnnotation
        }
    }
}

fn unsupported_motion(
    value: &SkyrimUnsupportedMotionReason,
) -> SkyrimUnsupportedMotionReasonReceipt {
    match value {
        SkyrimUnsupportedMotionReason::NoSemanticRoleEvidence => {
            SkyrimUnsupportedMotionReasonReceipt::NoSemanticRoleEvidence
        }
        SkyrimUnsupportedMotionReason::BehaviorReferenceMissing => {
            SkyrimUnsupportedMotionReasonReceipt::BehaviorReferenceMissing
        }
    }
}

fn motion_role(value: SkyrimCreatureMotionRole) -> SkyrimCreatureMotionRoleReceipt {
    use SkyrimCreatureMotionRole as Source;
    use SkyrimCreatureMotionRoleReceipt as Target;
    match value {
        Source::Idle => Target::Idle,
        Source::GroundLocomotion => Target::GroundLocomotion,
        Source::TurnLeft => Target::TurnLeft,
        Source::TurnRight => Target::TurnRight,
        Source::Turn => Target::Turn,
        Source::MeleeAttack => Target::MeleeAttack,
        Source::RangedAttack => Target::RangedAttack,
        Source::ProjectileAttack => Target::ProjectileAttack,
        Source::SpellAttack => Target::SpellAttack,
        Source::SwimIdle => Target::SwimIdle,
        Source::SwimLocomotion => Target::SwimLocomotion,
        Source::FlyIdle => Target::FlyIdle,
        Source::FlyLocomotion => Target::FlyLocomotion,
        Source::StationaryIdle => Target::StationaryIdle,
        Source::MechanicalStart => Target::MechanicalStart,
        Source::MechanicalLoop => Target::MechanicalLoop,
        Source::MechanicalStop => Target::MechanicalStop,
        Source::Hurt => Target::Hurt,
        Source::Death => Target::Death,
    }
}

fn living_motion_receipts(
    evidence: &SkyrimLivingFamilyEvidence,
) -> (
    CreatureGraphTemplate,
    Vec<SkyrimCreatureMotionBlockerReceipt>,
) {
    let mut blockers = match &evidence.template {
        SkyrimLivingTemplateDisposition::Proven { .. } => Vec::new(),
        SkyrimLivingTemplateDisposition::Ambiguous { candidates, .. } => {
            vec![SkyrimCreatureMotionBlockerReceipt::AmbiguousTemplate {
                candidates: candidates.clone(),
            }]
        }
        SkyrimLivingTemplateDisposition::Unsupported { evidenced_roles } => {
            vec![SkyrimCreatureMotionBlockerReceipt::UnsupportedTemplate {
                evidenced_roles: evidenced_roles.iter().copied().map(motion_role).collect(),
            }]
        }
    };
    for disposition in &evidence.required_roles {
        let blocker = match disposition {
            SkyrimRequiredRoleDisposition::Ready { .. } => continue,
            SkyrimRequiredRoleDisposition::MissingRole { alternatives } => {
                SkyrimCreatureMotionBlockerReceipt::MissingRole {
                    alternatives: motion_roles(alternatives),
                }
            }
            SkyrimRequiredRoleDisposition::AmbiguousRole {
                alternatives,
                candidates,
            } => SkyrimCreatureMotionBlockerReceipt::AmbiguousRequiredRole {
                alternatives: motion_roles(alternatives),
                clip_ids: living_candidate_ids(candidates),
            },
            SkyrimRequiredRoleDisposition::MissingTrigger {
                alternatives,
                candidates,
            } => SkyrimCreatureMotionBlockerReceipt::MissingRequiredTrigger {
                alternatives: motion_roles(alternatives),
                clip_ids: living_candidate_ids(candidates),
            },
            SkyrimRequiredRoleDisposition::AmbiguousTrigger {
                alternatives,
                candidates,
            } => SkyrimCreatureMotionBlockerReceipt::AmbiguousRequiredTrigger {
                alternatives: motion_roles(alternatives),
                clip_ids: living_candidate_ids(candidates),
                trigger_events: canonical_strings(
                    &candidates
                        .iter()
                        .flat_map(|candidate| {
                            candidate
                                .triggers
                                .iter()
                                .map(|trigger| trigger.event.clone())
                        })
                        .collect::<Vec<_>>(),
                ),
            },
            SkyrimRequiredRoleDisposition::UnknownRootMotion {
                alternatives,
                candidates,
            } => SkyrimCreatureMotionBlockerReceipt::UnknownRequiredRootMotion {
                alternatives: motion_roles(alternatives),
                clip_ids: living_candidate_ids(candidates),
            },
            SkyrimRequiredRoleDisposition::UnsupportedRootMotion {
                alternatives,
                candidates,
            } => SkyrimCreatureMotionBlockerReceipt::UnsupportedRequiredRootMotion {
                alternatives: motion_roles(alternatives),
                clip_ids: living_candidate_ids(candidates),
            },
        };
        blockers.push(blocker);
    }
    let template = match &evidence.template {
        SkyrimLivingTemplateDisposition::Proven { template, .. } => *template,
        SkyrimLivingTemplateDisposition::Ambiguous { candidates, .. } => candidates
            .first()
            .copied()
            .unwrap_or(CreatureGraphTemplate::GroundMelee),
        SkyrimLivingTemplateDisposition::Unsupported { .. } => CreatureGraphTemplate::GroundMelee,
    };
    (template, blockers)
}

fn motion_roles(roles: &[SkyrimCreatureMotionRole]) -> Vec<SkyrimCreatureMotionRoleReceipt> {
    roles.iter().copied().map(motion_role).collect()
}

fn living_candidate_ids(
    candidates: &[super::creature_motion::SkyrimLivingClipCandidate],
) -> Vec<String> {
    canonical_strings(
        &candidates
            .iter()
            .map(|candidate| candidate.clip_id.clone())
            .collect::<Vec<_>>(),
    )
}

fn controller_architecture(
    value: &SkyrimControllerArchitectureEvidence,
) -> SkyrimControllerArchitectureReceipt {
    match value {
        SkyrimControllerArchitectureEvidence::InlineRigidBodySetup => {
            SkyrimControllerArchitectureReceipt::InlineRigidBodySetup
        }
        SkyrimControllerArchitectureEvidence::CharacterProxyCinfo => {
            SkyrimControllerArchitectureReceipt::CharacterProxyCinfo
        }
        SkyrimControllerArchitectureEvidence::CharacterRigidBodyCinfo => {
            SkyrimControllerArchitectureReceipt::CharacterRigidBodyCinfo
        }
        SkyrimControllerArchitectureEvidence::FixedCinfo => {
            SkyrimControllerArchitectureReceipt::FixedCinfo
        }
        SkyrimControllerArchitectureEvidence::CustomCinfo { class_name } => {
            SkyrimControllerArchitectureReceipt::CustomCinfo {
                class_name: class_name.clone(),
            }
        }
    }
}

fn controller_disposition(
    value: &SkyrimControllerEvidenceDisposition,
) -> SkyrimControllerDispositionReceipt {
    match value {
        SkyrimControllerEvidenceDisposition::Complete => {
            SkyrimControllerDispositionReceipt::Complete
        }
        SkyrimControllerEvidenceDisposition::MissingControllerSetup => {
            SkyrimControllerDispositionReceipt::MissingControllerSetup
        }
        SkyrimControllerEvidenceDisposition::MissingRigidBodySetup => {
            SkyrimControllerDispositionReceipt::MissingRigidBodySetup
        }
        SkyrimControllerEvidenceDisposition::MissingShapeSetup => {
            SkyrimControllerDispositionReceipt::MissingShapeSetup
        }
        SkyrimControllerEvidenceDisposition::InvalidCapsuleDimensions {
            total_height_bits,
            radius_bits,
        } => SkyrimControllerDispositionReceipt::InvalidCapsuleDimensions {
            total_height_bits: *total_height_bits,
            radius_bits: *radius_bits,
        },
        SkyrimControllerEvidenceDisposition::LegacyLayoutUnsupported {
            contents_version,
            total_height_bits,
            radius_bits,
            ..
        } => SkyrimControllerDispositionReceipt::LegacyLayoutUnsupported {
            contents_version: contents_version.clone(),
            total_height_bits: *total_height_bits,
            radius_bits: *radius_bits,
        },
        SkyrimControllerEvidenceDisposition::InvalidModelTransform => {
            SkyrimControllerDispositionReceipt::InvalidModelTransform
        }
    }
}

fn record_issue(
    issue: &CreatureCatalogIssue,
    interner: &StringInterner,
) -> SkyrimCreatureRecordIssue {
    match issue {
        CreatureCatalogIssue::CuratedNonCreatureRace => {
            SkyrimCreatureRecordIssue::CuratedNonCreatureRace
        }
        CreatureCatalogIssue::MissingProject => SkyrimCreatureRecordIssue::MissingProject,
        CreatureCatalogIssue::MissingSkeleton => SkyrimCreatureRecordIssue::MissingSkeleton,
        CreatureCatalogIssue::MissingRaceSkin => SkyrimCreatureRecordIssue::MissingRaceSkin,
        CreatureCatalogIssue::MissingBodyPartData => SkyrimCreatureRecordIssue::MissingBodyPartData,
        CreatureCatalogIssue::MissingDependency {
            reference,
            expected_signature,
        } => SkyrimCreatureRecordIssue::MissingDependency {
            reference: reference.format(interner),
            expected_signature: (*expected_signature).to_string(),
        },
        CreatureCatalogIssue::MissingArmorAddonLinks { skin } => {
            SkyrimCreatureRecordIssue::MissingArmorAddonLinks {
                skin: skin.format(interner),
            }
        }
        CreatureCatalogIssue::MissingRaceConditionedArmorAddon { skin } => {
            SkyrimCreatureRecordIssue::MissingRaceConditionedArmorAddon {
                skin: skin.format(interner),
            }
        }
        CreatureCatalogIssue::MissingBodyModel { armor_addon } => {
            SkyrimCreatureRecordIssue::MissingBodyModel {
                armor_addon: armor_addon.format(interner),
            }
        }
        CreatureCatalogIssue::MissingDirectRace => SkyrimCreatureRecordIssue::MissingDirectRace,
        CreatureCatalogIssue::MissingTemplateTarget => {
            SkyrimCreatureRecordIssue::MissingTemplateTarget
        }
        CreatureCatalogIssue::EmptyLeveledTemplate { leveled_list } => {
            SkyrimCreatureRecordIssue::EmptyLeveledTemplate {
                leveled_list: leveled_list.format(interner),
            }
        }
        CreatureCatalogIssue::UnresolvedTemplateTarget { source, target } => {
            SkyrimCreatureRecordIssue::UnresolvedTemplateTarget {
                source: source.format(interner),
                target: target.format(interner),
            }
        }
        CreatureCatalogIssue::UnsupportedTemplateTarget { target, signature } => {
            SkyrimCreatureRecordIssue::UnsupportedTemplateTarget {
                target: target.format(interner),
                signature: signature.clone(),
            }
        }
        CreatureCatalogIssue::TemplateCycle { nodes } => SkyrimCreatureRecordIssue::TemplateCycle {
            nodes: form_keys(nodes, interner),
        },
        CreatureCatalogIssue::AmbiguousEmbeddedFormId {
            owner,
            field,
            raw,
            candidates,
        } => SkyrimCreatureRecordIssue::AmbiguousEmbeddedFormId {
            owner: owner.format(interner),
            field: (*field).to_string(),
            raw: *raw,
            candidates: form_keys(candidates, interner),
        },
        CreatureCatalogIssue::UnresolvedEmbeddedFormId { owner, field, raw } => {
            SkyrimCreatureRecordIssue::UnresolvedEmbeddedFormId {
                owner: owner.format(interner),
                field: (*field).to_string(),
                raw: *raw,
            }
        }
        CreatureCatalogIssue::MalformedRaceAttackData { ordinal, detail } => {
            SkyrimCreatureRecordIssue::MalformedRaceAttackData {
                ordinal: *ordinal,
                detail: detail.clone(),
            }
        }
        CreatureCatalogIssue::MixedCreatureAndNonCreatureRaces { races } => {
            SkyrimCreatureRecordIssue::MixedCreatureAndNonCreatureRaces {
                races: form_keys(races, interner),
            }
        }
        CreatureCatalogIssue::MultipleCreatureFamilies { family_ids } => {
            SkyrimCreatureRecordIssue::MultipleCreatureFamilies {
                family_ids: canonical_strings(family_ids),
            }
        }
    }
}

fn npc_receipt(npc: &CreatureNpcPlan, interner: &StringInterner) -> SkyrimCreatureNpcEvidence {
    SkyrimCreatureNpcEvidence {
        source_npc: npc.source_npc.format(interner),
        effective_races: form_keys(&npc.effective_races, interner),
        template_records: form_keys(&npc.template_records, interner),
        family_ids: canonical_strings(&npc.family_ids),
        issues: npc
            .issues
            .iter()
            .map(|issue| record_issue(issue, interner))
            .collect(),
    }
}

fn is_ambiguous_issue(issue: &SkyrimCreatureRecipeIssue) -> bool {
    matches!(
        issue,
        SkyrimCreatureRecipeIssue::AmbiguousMotionFamily { .. }
            | SkyrimCreatureRecipeIssue::AssetUnavailable {
                disposition: SkyrimAssetClaimDisposition::Ambiguous { .. },
                ..
            }
            | SkyrimCreatureRecipeIssue::RagdollUnavailable {
                evidence: SkyrimRagdollCapabilityEvidence::Ambiguous { .. },
                ..
            }
            | SkyrimCreatureRecipeIssue::WeaponBearingUnavailable {
                evidence: SkyrimExplicitCapabilityEvidence::Ambiguous { .. },
                ..
            }
            | SkyrimCreatureRecipeIssue::Motion {
                blocker: SkyrimCreatureMotionBlockerReceipt::AmbiguousRole { .. }
                    | SkyrimCreatureMotionBlockerReceipt::AmbiguousRequiredRole { .. }
                    | SkyrimCreatureMotionBlockerReceipt::AmbiguousRequiredTrigger { .. }
                    | SkyrimCreatureMotionBlockerReceipt::AmbiguousTemplate { .. },
                ..
            }
            | SkyrimCreatureRecipeIssue::Record {
                issue: SkyrimCreatureRecordIssue::AmbiguousEmbeddedFormId { .. }
                    | SkyrimCreatureRecordIssue::MultipleCreatureFamilies { .. }
                    | SkyrimCreatureRecordIssue::MixedCreatureAndNonCreatureRaces { .. }
            }
    )
}

fn is_unsupported_issue(issue: &SkyrimCreatureRecipeIssue) -> bool {
    matches!(
        issue,
        SkyrimCreatureRecipeIssue::AssetUnavailable {
            disposition: SkyrimAssetClaimDisposition::Unsupported { .. },
            ..
        } | SkyrimCreatureRecipeIssue::RagdollUnavailable {
            evidence: SkyrimRagdollCapabilityEvidence::Unsupported { .. },
            ..
        } | SkyrimCreatureRecipeIssue::WeaponBearingUnavailable {
            evidence: SkyrimExplicitCapabilityEvidence::Unsupported { .. },
            ..
        } | SkyrimCreatureRecipeIssue::Motion {
            blocker: SkyrimCreatureMotionBlockerReceipt::UnsupportedRequiredRootMotion { .. }
                | SkyrimCreatureMotionBlockerReceipt::UnsupportedTemplate { .. },
            ..
        } | SkyrimCreatureRecipeIssue::Record {
            issue: SkyrimCreatureRecordIssue::UnsupportedTemplateTarget { .. }
        }
    )
}

fn account(
    candidates: &[SkyrimCreatureCandidateRecipe],
    jobs: &[SkyrimCreatureFamilyJob],
) -> SkyrimCreatureRecipeAccounting {
    SkyrimCreatureRecipeAccounting {
        candidates: candidates.len(),
        ready_candidates: candidates
            .iter()
            .filter(|entry| {
                matches!(
                    entry.disposition,
                    SkyrimCreatureCandidateDisposition::Ready { .. }
                )
            })
            .count(),
        blocked_candidates: candidates
            .iter()
            .filter(|entry| {
                matches!(
                    entry.disposition,
                    SkyrimCreatureCandidateDisposition::Blocked { .. }
                )
            })
            .count(),
        unsupported_candidates: candidates
            .iter()
            .filter(|entry| {
                matches!(
                    entry.disposition,
                    SkyrimCreatureCandidateDisposition::Unsupported { .. }
                )
            })
            .count(),
        ambiguous_candidates: candidates
            .iter()
            .filter(|entry| {
                matches!(
                    entry.disposition,
                    SkyrimCreatureCandidateDisposition::Ambiguous { .. }
                )
            })
            .count(),
        family_jobs: jobs.len(),
        ready_family_jobs: jobs
            .iter()
            .filter(|entry| {
                matches!(
                    entry.disposition,
                    SkyrimCreatureFamilyDisposition::Ready { .. }
                )
            })
            .count(),
        blocked_family_jobs: jobs
            .iter()
            .filter(|entry| {
                matches!(
                    entry.disposition,
                    SkyrimCreatureFamilyDisposition::Blocked { .. }
                )
            })
            .count(),
        unsupported_family_jobs: jobs
            .iter()
            .filter(|entry| {
                matches!(
                    entry.disposition,
                    SkyrimCreatureFamilyDisposition::Unsupported { .. }
                )
            })
            .count(),
        ambiguous_family_jobs: jobs
            .iter()
            .filter(|entry| {
                matches!(
                    entry.disposition,
                    SkyrimCreatureFamilyDisposition::Ambiguous { .. }
                )
            })
            .count(),
    }
}

fn races_for_project<'a>(
    catalog: &'a CreatureCorpusPlan,
    project_path: &str,
    interner: &StringInterner,
) -> Vec<&'a CreatureRacePlan> {
    let key = project_key(project_path);
    let mut output = catalog
        .races
        .iter()
        .filter(|race| {
            // Only the male slot speaks for a race: every DLC race keeps the base
            // creature it was copied from in the female one, and matching that would
            // pull foreign races into this family. Must stay in step with the same
            // rule in `creature_mvp_live::races_for_project`, which decides the
            // family's asset claims — a race admitted here but not there reports its
            // body and skeleton as unclaimed.
            race.project_paths
                .first()
                .is_some_and(|path| project_key(path) == key)
        })
        .collect::<Vec<_>>();
    output.sort_by_key(|race| race.source_race.format(interner).to_ascii_lowercase());
    output
}

fn form_keys(values: &[FormKey], interner: &StringInterner) -> Vec<String> {
    let mut output = values
        .iter()
        .map(|value| value.format(interner))
        .collect::<Vec<_>>();
    output.sort_by_key(|value| value.to_ascii_lowercase());
    output.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    output
}

fn canonical_path(value: &str) -> String {
    value.trim().replace('/', "\\").to_ascii_lowercase()
}

fn path_key(value: &str) -> String {
    canonical_path(value)
}

fn project_key(value: &str) -> String {
    let key = path_key(value);
    key.strip_prefix("actors\\").unwrap_or(&key).to_string()
}

fn canonical_paths(values: &[String]) -> Vec<String> {
    let mut output = values
        .iter()
        .map(|value| canonical_path(value))
        .collect::<Vec<_>>();
    output.sort();
    output.dedup();
    output
}

fn canonical_strings(values: &[String]) -> Vec<String> {
    let mut output = values.to_vec();
    output.sort_by_key(|value| value.to_ascii_lowercase());
    output.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    output
}

fn same_path_set(left: &[String], right: &[String]) -> bool {
    canonical_paths(left) == canonical_paths(right)
}

fn canonicalize_graph_variants(values: &mut Vec<SkyrimCreatureGraphVariantEvidence>) {
    for value in values.iter_mut() {
        value.project_path = canonical_path(&value.project_path);
        value.character_path = canonical_path(&value.character_path);
        value.skeleton_path = canonical_path(&value.skeleton_path);
        value.behavior_paths = canonical_paths(&value.behavior_paths);
    }
    values.sort_by(|left, right| {
        (
            left.sex,
            &left.project_path,
            &left.character_path,
            &left.skeleton_path,
        )
            .cmp(&(
                right.sex,
                &right.project_path,
                &right.character_path,
                &right.skeleton_path,
            ))
    });
    values.dedup();
}

fn canonicalize_body_variants(values: &mut Vec<SkyrimCreatureBodyVariantEvidence>) {
    for value in values.iter_mut() {
        value.body_nif = canonical_path(&value.body_nif);
    }
    values.sort_by(|left, right| {
        (
            left.source_race.to_ascii_lowercase(),
            left.sex,
            &left.body_nif,
        )
            .cmp(&(
                right.source_race.to_ascii_lowercase(),
                right.sex,
                &right.body_nif,
            ))
    });
    values.dedup();
}

fn canonicalize_claims(values: &mut Vec<SkyrimRecursiveAssetClaim>) {
    for value in values.iter_mut() {
        value.path = canonical_path(&value.path);
        value.dependencies = canonical_paths(&value.dependencies);
    }
    values.sort_by(|left, right| (&left.path, left.role).cmp(&(&right.path, right.role)));
    values.dedup();
}

fn canonicalize_issues(values: &mut Vec<SkyrimCreatureRecipeIssue>) {
    values.sort_by_key(|value| serde_json::to_string(value).unwrap_or_default());
    values.dedup();
}

fn strictly_sorted_unique<'a>(mut values: impl Iterator<Item = &'a String>) -> bool {
    let Some(mut previous) = values.next() else {
        return true;
    };
    for value in values {
        if previous >= value {
            return false;
        }
        previous = value;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};
    use crate::skyrimse_fo4_runtime::creature_motion::{
        SkyrimCatalogEvent, SkyrimCreatureMotionAccounting, SkyrimFamilyMotionSet,
    };
    use crate::source_rig::race_data::{MeasuredBounds, MovementArchitecture};

    fn key(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("CreatureRecipe.esm"),
        }
    }

    fn record(interner: &StringInterner, sig: &str, local: u32, eid: &str) -> Record {
        let mut record = Record::new(SigCode::from_str(sig).unwrap(), key(interner, local));
        record.eid = Some(interner.intern(eid));
        record
    }

    fn push(record: &mut Record, sig: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        });
    }

    fn complete_records(interner: &StringInterner) -> Vec<Record> {
        let mut race = record(interner, "RACE", 0x100, "WolfRace");
        push(&mut race, "WNAM", FieldValue::FormKey(key(interner, 0x101)));
        push(&mut race, "GNAM", FieldValue::FormKey(key(interner, 0x103)));
        for path in ["Actors\\Canine\\WolfSkeleton.nif"; 2] {
            push(&mut race, "ANAM", FieldValue::String(interner.intern(path)));
        }
        push(
            &mut race,
            "MODL",
            FieldValue::String(interner.intern("Actors\\Canine\\WolfProject.hkx")),
        );
        push(
            &mut race,
            "ATKE",
            FieldValue::String(interner.intern("attackStart_Attack1")),
        );
        let data = [
            ("male_height", FieldValue::Float(1.0)),
            ("female_height", FieldValue::Float(1.0)),
            ("male_weight", FieldValue::Float(0.5)),
            ("female_weight", FieldValue::Float(0.5)),
            ("flags", FieldValue::Uint((1u32 << 8) as u64)),
            ("acceleration_rate", FieldValue::Float(240.0)),
            ("deceleration_rate", FieldValue::Float(480.0)),
            ("injured_health_pct", FieldValue::Float(0.2)),
            ("unarmed_damage", FieldValue::Float(10.0)),
            ("unarmed_reach", FieldValue::Float(0.68)),
            ("body_biped_object", FieldValue::Int(0)),
            ("aim_angle_tolerance", FieldValue::Float(30.0)),
            ("angular_acceleration_rate", FieldValue::Float(240.0)),
            ("angular_tolerance", FieldValue::Float(15.0)),
            ("flags_2", FieldValue::Uint(0)),
        ];
        push(
            &mut race,
            "DATA",
            FieldValue::Struct(
                data.into_iter()
                    .map(|(name, value)| (interner.intern(name), value))
                    .collect(),
            ),
        );
        let mut skin = record(interner, "ARMO", 0x101, "WolfSkin");
        push(&mut skin, "MODL", FieldValue::FormKey(key(interner, 0x102)));
        let mut addon = record(interner, "ARMA", 0x102, "WolfAddon");
        push(
            &mut addon,
            "RNAM",
            FieldValue::FormKey(key(interner, 0x100)),
        );
        push(
            &mut addon,
            "MOD2",
            FieldValue::String(interner.intern("Actors\\Canine\\WolfBody.nif")),
        );
        vec![
            race,
            skin,
            addon,
            record(interner, "BPTD", 0x103, "WolfBodyParts"),
        ]
    }

    fn clip(
        id: &str,
        path: &str,
        role: SkyrimCreatureMotionRole,
        event: &str,
    ) -> SkyrimCatalogClip {
        SkyrimCatalogClip {
            clip_id: id.to_string(),
            clip_path: path.to_string(),
            original_skeleton_name: Some("WolfSkeleton".to_string()),
            family_ids: vec!["wolf".to_string()],
            disposition: SkyrimClipDisposition::Role {
                role,
                trigger_event: Some(event.to_string()),
                trigger_aliases: Vec::new(),
                evidence: vec![],
            },
            root_motion: SkyrimRootMotion::Stationary {
                source: SkyrimRootMotionSource::HavokReferenceFrame,
                sample_count: 1,
            },
            events: vec![SkyrimCatalogEvent {
                name: event.to_string(),
                time: Some(0.0),
                source: SkyrimMotionEvidenceSource::BehaviorTransition,
            }],
            annotations: vec![],
        }
    }

    fn motion() -> SkyrimCreatureMotionCatalog {
        let layout = DecodedControllerLayout::LegacyCharacterControllerInfo {
            contents_version: "hk_2010.2.0-r1".to_string(),
            character_data_signature: SKYRIM_LEGACY_CHARACTER_DATA_SIGNATURE,
            controller_signature: SKYRIM_LEGACY_CONTROLLER_SIGNATURE,
        };
        let controller = SkyrimCharacterControllerEvidence {
            character_path: "Actors\\Canine\\WolfCharacter.hkx".to_string(),
            contents_version: "hk_2010.2.0-r1".to_string(),
            character_data_class: "hkbCharacterData".to_string(),
            character_data_signature: SKYRIM_LEGACY_CHARACTER_DATA_SIGNATURE,
            controller_class: Some("hkbCharacterControllerInfo".to_string()),
            controller_signature: Some(SKYRIM_LEGACY_CONTROLLER_SIGNATURE),
            layout: Some(layout),
            controller_cinfo_class: None,
            architecture: None,
            collision_filter_info: Some(7),
            rigid_body_type: Some(1),
            shape_type: Some(2),
            capsule: Some(
                super::super::creature_motion::SkyrimControllerCapsuleEvidence {
                    total_height: 80.0,
                    radius: 12.0,
                },
            ),
            model: Some(SkyrimControllerModelEvidence {
                up_ms: [0.0, 0.0, 1.0, 0.0],
                forward_ms: [0.0, 1.0, 0.0, 0.0],
                right_ms: [1.0, 0.0, 0.0, 0.0],
                scale: 1.0,
            }),
            disposition: SkyrimControllerEvidenceDisposition::Complete,
        };
        let clips = vec![
            clip(
                "idle",
                "Actors\\Canine\\Idle.hkx",
                SkyrimCreatureMotionRole::Idle,
                "Idle",
            ),
            clip(
                "walk",
                "Actors\\Canine\\Walk.hkx",
                SkyrimCreatureMotionRole::GroundLocomotion,
                "Walk",
            ),
            clip(
                "attack",
                "Actors\\Canine\\Attack.hkx",
                SkyrimCreatureMotionRole::MeleeAttack,
                "Attack",
            ),
        ];
        SkyrimCreatureMotionCatalog {
            families: vec![SkyrimFamilyMotionSet {
                family_id: "wolf".to_string(),
                project_path: "Canine\\WolfProject.hkx".to_string(),
                race_attacks: Vec::new(),
                race_attack_sources: Vec::new(),
                clip_ids: clips.iter().map(|clip| clip.clip_id.clone()).collect(),
            }],
            inventories: vec![SkyrimCreatureFamilyInventory {
                family_id: "wolf".to_string(),
                project_path: "Actors\\Canine\\WolfProject.hkx".to_string(),
                character_paths: vec!["Actors\\Canine\\WolfCharacter.hkx".to_string()],
                animation_skeleton_paths: vec![
                    "Actors\\Canine\\WolfAnimationSkeleton.hkx".to_string(),
                ],
                ragdoll_paths: vec![],
                behavior_paths: vec!["Actors\\Canine\\WolfBehavior.hkx".to_string()],
                behavior_groups: Vec::new(),
                behavior_variables: Vec::new(),
                controllers: vec![controller],
            }],
            clips,
            accounting: SkyrimCreatureMotionAccounting::default(),
        }
    }

    fn measured() -> DecodedCreatureFamilyEvidence {
        DecodedCreatureFamilyEvidence {
            project_path: "Actors\\Canine\\WolfProject.hkx".to_string(),
            project_paths: vec!["Actors\\Canine\\WolfProject.hkx".to_string()],
            body_bounds: Some(MeasuredBounds {
                min: [-20.0, -10.0, 0.0],
                max: [20.0, 10.0, 80.0],
            }),
            skeleton_bounds: Some(MeasuredBounds {
                min: [-18.0, -9.0, 4.0],
                max: [18.0, 9.0, 76.0],
            }),
            up_axis: MeasurementAxis::Z,
            capsule: Some(CapsuleEvidence {
                radius: 12.0,
                total_height: 80.0,
                architecture: ControllerArchitecture::Quadruped,
            }),
            geometry_scale_to_fo4: Some(1.0),
            movement_architecture: Some(MovementArchitecture::Grounded),
            max_linear_speed: Some(120.0),
            max_yaw_speed_degrees_per_second: Some(120.0),
            pitch_limit_degrees: Some(45.0),
            roll_limit_degrees: Some(20.0),
            use_large_actor_pathing: Some(false),
            use_subsegmented_damage: Some(false),
            xp_value: Some(10),
        }
    }

    fn policy() -> SkyrimRaceDataPolicyReceipt {
        SkyrimRaceDataPolicyReceipt {
            policy_id: "wolf-policy-v1".to_string(),
            schema_id: "skyrimse-race-data-to-fo4-v1".to_string(),
            source_data_schema: "skyrimse:RACE.DATA".to_string(),
            target_data_schema: "fo4:RACE.DATA".to_string(),
            scale_policy: Fo4RaceDataScalePolicy {
                small_max_stature: 32.0,
                medium_max_stature: 96.0,
                large_max_stature: 192.0,
            },
            fields: vec![SkyrimRaceDataFieldReceipt {
                field: CreatureRaceDataInputField::ScalePolicy,
                provenance: SkyrimRaceDataFieldProvenance::ExplicitPolicy,
                policy_id: Some("wolf-policy-v1".to_string()),
                reason: "explicit size thresholds".to_string(),
            }],
        }
    }

    fn claim(
        path: &str,
        role: SkyrimAssetRole,
        dependencies: &[&str],
    ) -> SkyrimRecursiveAssetClaim {
        SkyrimRecursiveAssetClaim {
            path: path.to_string(),
            role,
            dependencies: dependencies
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            disposition: SkyrimAssetClaimDisposition::Present {
                blake3: "11".repeat(32),
            },
        }
    }

    fn assets() -> SkyrimCreatureFamilyAssetEvidence {
        let layout = SkyrimControllerLayoutEvidence::LegacyCharacterControllerInfo {
            contents_version: "hk_2010.2.0-r1".to_string(),
            character_data_signature: SKYRIM_LEGACY_CHARACTER_DATA_SIGNATURE,
            controller_signature: SKYRIM_LEGACY_CONTROLLER_SIGNATURE,
        };
        SkyrimCreatureFamilyAssetEvidence {
            family_id: "wolf".to_string(),
            graph_variants: vec![
                SkyrimCreatureGraphVariantEvidence {
                    sex: SkyrimCreatureSex::Female,
                    project_path: "Actors/Canine/WolfProject.hkx".to_string(),
                    character_path: "Actors/Canine/WolfCharacter.hkx".to_string(),
                    skeleton_path: "Actors/Canine/WolfSkeleton.nif".to_string(),
                    actual_root_bone: "NPC Root [Root]".to_string(),
                    behavior_paths: vec!["Actors/Canine/WolfBehavior.hkx".to_string()],
                },
                SkyrimCreatureGraphVariantEvidence {
                    sex: SkyrimCreatureSex::Male,
                    project_path: "Actors/Canine/WolfProject.hkx".to_string(),
                    character_path: "Actors/Canine/WolfCharacter.hkx".to_string(),
                    skeleton_path: "Actors/Canine/WolfSkeleton.nif".to_string(),
                    actual_root_bone: "NPC Root [Root]".to_string(),
                    behavior_paths: vec!["Actors/Canine/WolfBehavior.hkx".to_string()],
                },
            ],
            controllers: vec![SkyrimCreatureControllerRecipeEvidence {
                character_path: "Actors\\Canine\\WolfCharacter.hkx".to_string(),
                layout,
                axes: Some(SkyrimControllerAxesEvidence {
                    up: MeasurementAxis::Z,
                    forward: MeasurementAxis::Y,
                }),
                capsule: Some(SkyrimControllerCapsuleReceipt {
                    total_height: 80.0,
                    radius: 12.0,
                    architecture: ControllerArchitecture::Quadruped,
                }),
            }],
            ragdoll: SkyrimRagdollCapabilityEvidence::NotApplicable {
                reason: "source project declares no ragdoll".to_string(),
            },
            weapon_bearing: SkyrimExplicitCapabilityEvidence::NotApplicable {
                evidence: "unarmed RACE attack contract".to_string(),
            },
            body_variants: vec![SkyrimCreatureBodyVariantEvidence {
                source_race: "000100@CreatureRecipe.esm".to_string(),
                sex: SkyrimCreatureSex::Ungendered,
                armor_addon: "000102@CreatureRecipe.esm".to_string(),
                body_nif: "Actors\\Canine\\WolfBody.nif".to_string(),
            }],
            asset_claims: vec![
                claim(
                    "Actors\\Canine\\WolfProject.hkx",
                    SkyrimAssetRole::Project,
                    &[],
                ),
                claim(
                    "Actors\\Canine\\WolfCharacter.hkx",
                    SkyrimAssetRole::Character,
                    &[],
                ),
                claim(
                    "Actors\\Canine\\WolfAnimationSkeleton.hkx",
                    SkyrimAssetRole::AnimationSkeleton,
                    &[],
                ),
                claim(
                    "Actors\\Canine\\WolfSkeleton.nif",
                    SkyrimAssetRole::VisualSkeletonNif,
                    &[],
                ),
                claim(
                    "Actors\\Canine\\WolfBehavior.hkx",
                    SkyrimAssetRole::Behavior,
                    &[],
                ),
                claim(
                    "Actors\\Canine\\Idle.hkx",
                    SkyrimAssetRole::AnimationClip,
                    &[],
                ),
                claim(
                    "Actors\\Canine\\Walk.hkx",
                    SkyrimAssetRole::AnimationClip,
                    &[],
                ),
                claim(
                    "Actors\\Canine\\Attack.hkx",
                    SkyrimAssetRole::AnimationClip,
                    &[],
                ),
                claim(
                    "Actors\\Canine\\WolfBody.nif",
                    SkyrimAssetRole::BodyNif,
                    &["Actors\\Canine\\Wolf.bgsm"],
                ),
                claim(
                    "Actors\\Canine\\Wolf.bgsm",
                    SkyrimAssetRole::Material,
                    &["Textures\\Canine\\Wolf_d.dds"],
                ),
                claim(
                    "Textures\\Canine\\Wolf_d.dds",
                    SkyrimAssetRole::Texture,
                    &[],
                ),
            ],
        }
    }

    #[test]
    fn one_hkx_claim_can_satisfy_animation_and_ragdoll_inventory_roles() {
        let path = "Actors\\Canine\\Character Assets\\Skeleton.hkx";
        let inventory = SkyrimCreatureInventoryReceipt {
            project_path: "Actors\\Canine\\WolfProject.hkx".to_string(),
            character_paths: Vec::new(),
            animation_skeleton_paths: vec![path.to_string()],
            ragdoll_paths: vec![path.to_string()],
            behavior_paths: Vec::new(),
        };
        let claim = claim(path, SkyrimAssetRole::AnimationSkeleton, &[]);
        assert!(claim_satisfies_inventory_role(
            &claim,
            SkyrimAssetRole::AnimationSkeleton,
            path,
            &inventory.animation_skeleton_paths,
            &inventory.ragdoll_paths,
        ));
        assert!(claim_satisfies_inventory_role(
            &claim,
            SkyrimAssetRole::Ragdoll,
            path,
            &inventory.animation_skeleton_paths,
            &inventory.ragdoll_paths,
        ));
        assert!(!claim_satisfies_inventory_role(
            &claim,
            SkyrimAssetRole::Character,
            path,
            &inventory.animation_skeleton_paths,
            &inventory.ragdoll_paths,
        ));
    }

    #[test]
    fn one_nif_claim_can_satisfy_body_and_visual_skeleton_roles() {
        let path = "Actors\\Witchlight\\Character Assets\\Witchlight.nif";
        let claim = claim(path, SkyrimAssetRole::BodyNif, &[]);
        assert!(claim_satisfies_inventory_role(
            &claim,
            SkyrimAssetRole::VisualSkeletonNif,
            path,
            &[],
            &[],
        ));
    }

    #[test]
    fn production_race_data_policy_constructor_is_validated_and_canonical() {
        let receipt = build_skyrim_creature_race_data_policy_receipt(Fo4RaceDataScalePolicy {
            small_max_stature: 64.0,
            medium_max_stature: 128.0,
            large_max_stature: 256.0,
        })
        .unwrap();
        assert_eq!(receipt.policy_id, "skyrimse-creature-race-data-scale-v1");
        assert_eq!(receipt.fields.len(), 1);
        assert!(matches!(
            build_skyrim_creature_race_data_policy_receipt(Fo4RaceDataScalePolicy {
                small_max_stature: 64.0,
                medium_max_stature: 32.0,
                large_max_stature: 256.0,
            }),
            Err(SkyrimCreatureRecipeError::InvalidRaceDataPolicy(_))
        ));
    }

    #[test]
    fn complete_wolf_recipe_uses_generic_builder_and_is_canonical() {
        let interner = StringInterner::new();
        let mut records = complete_records(&interner);
        let motion = motion();
        let measured = vec![measured()];
        let policy = policy();
        let mut assets = vec![assets()];
        let first = build_skyrim_creature_recipe_ledger(SkyrimCreatureRecipeInput {
            winning_records: &records,
            motion: &motion,
            decoded_race_data: &measured,
            assets: &assets,
            race_data_policy: &policy,
            expectations: SkyrimCreatureRecipeExpectations {
                family_count: 1,
                candidate_count: 1,
            },
            interner: &interner,
        })
        .unwrap();
        assert!(
            matches!(
                first.candidates[0].disposition,
                SkyrimCreatureCandidateDisposition::Ready { .. }
            ),
            "{:#?}",
            first.candidates[0].disposition
        );
        assert_eq!(first.family_jobs[0].graph_variants.len(), 2);
        assert_eq!(
            first.family_jobs[0].graph_variants[0].actual_root_bone,
            "NPC Root [Root]"
        );
        let json = first.canonical_json().unwrap();
        assert_eq!(SkyrimCreatureRecipeLedger::from_json(&json).unwrap(), first);

        records.reverse();
        assets[0].asset_claims.reverse();
        assets[0].graph_variants.reverse();
        let reversed = build_skyrim_creature_recipe_ledger(SkyrimCreatureRecipeInput {
            winning_records: &records,
            motion: &motion,
            decoded_race_data: &measured,
            assets: &assets,
            race_data_policy: &policy,
            expectations: SkyrimCreatureRecipeExpectations {
                family_count: 1,
                candidate_count: 1,
            },
            interner: &interner,
        })
        .unwrap();
        assert_eq!(
            first.canonical_json().unwrap(),
            reversed.canonical_json().unwrap()
        );

        let mut tampered: SkyrimCreatureRecipeLedger = serde_json::from_str(&json).unwrap();
        tampered.family_jobs[0].graph_variants[0].actual_root_bone =
            "Actors\\Canine\\WolfSkeleton.nif".to_string();
        assert!(matches!(
            tampered.validate(),
            Err(SkyrimCreatureRecipeError::HashMismatch { .. })
        ));
    }

    #[test]
    fn readiness_templates_and_root_path_distinction_are_typed() {
        for (template, capability) in [
            (
                CreatureGraphTemplate::Swim,
                SkyrimCreatureCapability::Swimming,
            ),
            (CreatureGraphTemplate::Fly, SkyrimCreatureCapability::Flying),
            (
                CreatureGraphTemplate::StationaryTurret,
                SkyrimCreatureCapability::Stationary,
            ),
        ] {
            let graph = SkyrimCreatureGraphEvidence {
                template,
                ready: true,
                roles: vec![],
                candidate_attack_bindings: Vec::new(),
                idle_event: Some("Idle".to_string()),
                explicit_events: vec![],
                variables: vec![],
                overlays: vec![],
                required_rigs: vec![],
                rigs: vec![],
                blockers: vec![],
            };
            assert!(capabilities(&graph).contains(&capability));
        }
        let weapon_graph = SkyrimCreatureGraphEvidence {
            template: CreatureGraphTemplate::GroundMelee,
            ready: true,
            roles: vec![],
            candidate_attack_bindings: Vec::new(),
            idle_event: Some("Idle".to_string()),
            explicit_events: vec![],
            variables: vec![],
            overlays: vec![],
            required_rigs: vec![],
            rigs: vec![],
            blockers: vec![],
        };
        let SkyrimCreatureFamilyDisposition::Ready { capabilities } = family_disposition(
            &weapon_graph,
            &SkyrimRagdollCapabilityEvidence::NotApplicable {
                reason: "no source ragdoll".to_string(),
            },
            &SkyrimExplicitCapabilityEvidence::Supported {
                evidence: "weapon attachment and attack role".to_string(),
            },
            vec![],
        ) else {
            panic!("explicit weapon-bearing evidence must be ready");
        };
        assert!(capabilities.contains(&SkyrimCreatureCapability::WeaponBearing));
        let mut variants = vec![SkyrimCreatureGraphVariantEvidence {
            sex: SkyrimCreatureSex::Male,
            project_path: "p.hkx".to_string(),
            character_path: "c.hkx".to_string(),
            skeleton_path: "s.nif".to_string(),
            actual_root_bone: "skeleton.nif".to_string(),
            behavior_paths: vec![],
        }];
        canonicalize_graph_variants(&mut variants);
        assert!(variants[0].actual_root_bone.ends_with(".nif"));
        assert_ne!(variants[0].actual_root_bone, variants[0].skeleton_path);
    }

    #[test]
    fn optional_official_recipe_gate_accounts_for_46_families_and_122_candidates() {
        let Some(path) =
            std::env::var_os("SKYRIMSE_CREATURE_RECIPE_LEDGER").map(std::path::PathBuf::from)
        else {
            return;
        };
        if !path.is_file() {
            return;
        }
        let json = std::fs::read_to_string(path).unwrap();
        let ledger = SkyrimCreatureRecipeLedger::from_json(&json).unwrap();
        assert_eq!(ledger.family_jobs.len(), SKYRIM_CREATURE_FAMILY_COUNT);
        assert_eq!(ledger.candidates.len(), SKYRIM_CREATURE_CANDIDATE_COUNT);
    }
}
