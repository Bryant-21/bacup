use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::corpus::SourceCreatureIdentity;

/// Source-game-neutral FO4 runtime contract.
///
/// A Skyrim HKX adapter or FNV/FO3 KF adapter constructs this value after it
/// has recovered ordered bones, ordered clip tracks, and graph declarations.
/// Emission and packing do not branch on the source game.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CreatureManifest {
    pub creature_name: String,
    pub visual_skeleton_nif: String,
    pub animation_skeleton: SkeletonDecl,
    pub controller: CreatureControllerDecl,
    pub ragdoll: RagdollDisposition,
    pub clips: Vec<ClipDecl>,
    pub idle_clip: String,
    pub capsule: Capsule,
    pub paths: ScaffoldPaths,
    pub root: GraphDeclarations,
    pub core: GraphDeclarations,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct CreatureControllerDecl {
    pub collision_filter_info: u32,
    pub rigid_body_type: i32,
    pub model_up_ms: [f32; 4],
    pub model_forward_ms: [f32; 4],
    pub model_right_ms: [f32; 4],
    pub model_scale: f32,
}

/// Explicit runtime disposition for creature ragdoll support.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "disposition", rename_all = "snake_case")]
pub enum RagdollDisposition {
    SourceOwned {
        runtime_path: String,
        receipt: SourceOwnedRagdollReceipt,
    },
    NoRagdoll {
        reason: NoRagdollReason,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoRagdollReason {
    SourceHasNoRagdoll,
    UnsupportedSourceRagdoll,
    Deferred,
    NotApplicable,
}

/// Hash-bound proof for bytes emitted by source-rig ragdoll reconstruction.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceOwnedRagdollReceipt {
    pub byte_len: u64,
    pub blake3: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub powered_ragdoll: Option<SourceOwnedPoweredRagdollConfig>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceOwnedPoweredRagdollConfig {
    pub source: SourcePoweredRagdollEvidence,
    pub target_control_policy: PoweredRagdollTargetControlPolicy,
    pub proven_inert_source_fields: Vec<PoweredRagdollInertFieldReceipt>,
}

pub const POWERED_RAGDOLL_INERT_PROOF_SCHEMA: &str = "source-rig-powered-ragdoll-runtime-inert-v1";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PoweredRagdollInertFieldReceipt {
    pub field: PoweredRagdollSourceField,
    pub proof_schema: String,
    pub canonical_json: String,
    pub canonical_json_blake3: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourcePoweredRagdollEvidence {
    pub dynamic_bone_count: u32,
    pub unknown_byte_2: u8,
    pub unknown_byte_3: u8,
    pub unknown_byte_4: u8,
    pub unknown_byte_5: u8,
    pub enabled_feedback: bool,
    pub enabled_foot_ik: bool,
    pub enabled_look_ik: bool,
    pub enabled_grab_ik: bool,
    pub enabled_pose_matching: bool,
    pub unknown_byte_11: u8,
    pub unknown_trailing_u8: Option<u8>,
    pub feedback: Option<SourcePoweredRagdollFeedbackEvidence>,
    pub pose_matching: Option<SourcePoweredRagdollPoseEvidence>,
    pub death_pose_animation: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourcePoweredRagdollFeedbackEvidence {
    pub dynamic_keyframe_blend_amount_bits: u32,
    pub hierarchy_gain_bits: u32,
    pub position_gain_bits: u32,
    pub velocity_gain_bits: u32,
    pub acceleration_gain_bits: u32,
    pub snap_gain_bits: u32,
    pub velocity_damping_bits: u32,
    pub snap_max_linear_velocity_bits: u32,
    pub snap_max_angular_velocity_bits: u32,
    pub snap_max_linear_distance_bits: u32,
    pub snap_max_angular_distance_bits: u32,
    pub position_max_velocity_linear_bits: u32,
    pub position_max_velocity_angular_bits: u32,
    pub position_max_velocity_projectile: i32,
    pub position_max_velocity_melee: i32,
    pub bones: Vec<u16>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourcePoweredRagdollPoseEvidence {
    pub matching_bones: [Option<u16>; 3],
    pub flags: u8,
    pub unknown: u8,
    pub motors_strength_bits: u32,
    pub pose_activation_delay_time_bits: u32,
    pub match_error_allowance_bits: u32,
    pub displacement_to_disable_bits: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PoweredRagdollTargetControlPolicy {
    Fo4RuntimeDefaultsV1,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PoweredRagdollSourceField {
    DataUnknownByte2,
    DataUnknownByte3,
    DataUnknownByte4,
    DataUnknownByte5,
    DataEnabledFootIk,
    DataEnabledLookIk,
    DataEnabledGrabIk,
    DataUnknownByte11,
    DataUnknownTrailingU8,
    FeedbackDynamicKeyframeBlendAmount,
    FeedbackHierarchyGain,
    FeedbackPositionGain,
    FeedbackVelocityGain,
    FeedbackAccelerationGain,
    FeedbackSnapGain,
    FeedbackVelocityDamping,
    FeedbackSnapMaxLinearVelocity,
    FeedbackSnapMaxAngularVelocity,
    FeedbackSnapMaxLinearDistance,
    FeedbackSnapMaxAngularDistance,
    FeedbackPositionMaxVelocityLinear,
    FeedbackPositionMaxVelocityAngular,
    FeedbackPositionMaxVelocityProjectile,
    FeedbackPositionMaxVelocityMelee,
    FeedbackBoneIndicesWhileDisabled,
    PoseMatchingBoneIndicesWhileDisabled,
    PoseFlags,
    PoseUnknown,
    PoseMotorsStrength,
    PoseActivationDelayTime,
    PoseMatchErrorAllowance,
    PoseDisplacementToDisable,
    DeathPoseAnimation,
}

impl RagdollDisposition {
    pub fn deferred() -> Self {
        Self::NoRagdoll {
            reason: NoRagdollReason::Deferred,
        }
    }

    pub fn runtime_path(&self) -> Option<&str> {
        match self {
            Self::SourceOwned { runtime_path, .. } => Some(runtime_path),
            Self::NoRagdoll { .. } => None,
        }
    }

    pub(crate) fn resolves_articulated_collision(&self) -> bool {
        matches!(
            self,
            Self::SourceOwned { .. }
                | Self::NoRagdoll {
                    reason: NoRagdollReason::UnsupportedSourceRagdoll
                }
        )
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SkeletonDecl {
    pub path: String,
    /// Exact `hkaSkeleton.name` persisted in the reconstructed runtime HKX.
    pub runtime_name: String,
    /// Parent-before-child order used by every clip binding.
    pub bones: Vec<BoneDecl>,
    /// Ordered `hkaSkeleton.floatSlots` contract.
    pub float_slots: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct BoneDecl {
    pub name: String,
    pub parent_index: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ClipDecl {
    pub name: String,
    pub path: String,
    pub binding: ClipBinding,
    pub looping: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ClipBinding {
    pub skeleton_path: String,
    /// Exact source-format `hkaAnimationBinding.originalSkeletonName`.
    pub original_skeleton_name: String,
    pub declared_transform_tracks: usize,
    /// Track-order mapping supplied by the source animation adapter.
    pub transform_track_to_bone_indices: Vec<usize>,
    pub declared_float_tracks: usize,
    /// Float-track order mapped into [`SkeletonDecl::float_slots`].
    pub float_track_to_float_slot_indices: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Capsule {
    pub height: f32,
    pub radius: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ScaffoldPaths {
    pub project: String,
    pub character: String,
    pub root_behavior: String,
    pub core_behavior: String,
}

/// Source-neutral role binding for the minimal FO4 creature behavior graph.
///
/// The names point at [`ClipDecl::name`] values. Source adapters retain their
/// own runtime filenames and only assign the five roles after decoding.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct MvpGraphManifest {
    pub idle_clip: String,
    pub walk_forward_clip: String,
    pub turn_left_90_clip: String,
    pub turn_right_90_clip: String,
    pub attack_1_clip: String,
    pub melee_event: String,
}

impl MvpGraphManifest {
    pub(crate) fn named_clips(&self) -> [(&'static str, &str); 5] {
        [
            ("idle_clip", self.idle_clip.as_str()),
            ("walk_forward_clip", self.walk_forward_clip.as_str()),
            ("turn_left_90_clip", self.turn_left_90_clip.as_str()),
            ("turn_right_90_clip", self.turn_right_90_clip.as_str()),
            ("attack_1_clip", self.attack_1_clip.as_str()),
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct ClipMotionPolicy {
    pub animation_driven: bool,
    pub extracted_planar_reference_frames: usize,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct MvpMotionManifest {
    pub idle: ClipMotionPolicy,
    pub walk_forward: ClipMotionPolicy,
    pub turn_left_90: ClipMotionPolicy,
    pub turn_right_90: ClipMotionPolicy,
    pub attack_1: ClipMotionPolicy,
}

impl MvpMotionManifest {
    pub(crate) fn named_roles(&self) -> [(&'static str, ClipMotionPolicy); 5] {
        [
            ("idle", self.idle),
            ("walk_forward", self.walk_forward),
            ("turn_left_90", self.turn_left_90),
            ("turn_right_90", self.turn_right_90),
            ("attack_1", self.attack_1),
        ]
    }

    pub(crate) fn any_animation_driven(&self) -> bool {
        self.named_roles()
            .into_iter()
            .any(|(_, policy)| policy.animation_driven)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureGraphTemplate {
    #[default]
    GroundMelee,
    PassiveGround,
    GroundRangedProjectile,
    GroundMeleeRanged,
    GroundSwim,
    GroundFly,
    Swim,
    Fly,
    StationaryTurret,
    RobotContinuousAttack,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureClipRole {
    Idle,
    GroundForward,
    TurnLeft90,
    TurnRight90,
    MeleeAttack,
    ProjectileAttack,
    SwimIdle,
    SwimForward,
    FlyIdle,
    FlyForward,
    StationaryIdle,
    ContinuousAttackStart,
    ContinuousAttackLoop,
    ContinuousAttackStop,
}

impl CreatureClipRole {
    fn allows_multiple(self) -> bool {
        matches!(self, Self::MeleeAttack | Self::ProjectileAttack)
    }

    fn expects_looping(self) -> bool {
        matches!(
            self,
            Self::Idle
                | Self::GroundForward
                | Self::SwimIdle
                | Self::SwimForward
                | Self::FlyIdle
                | Self::FlyForward
                | Self::StationaryIdle
                | Self::ContinuousAttackLoop
        )
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityClipRole {
    pub role: CreatureClipRole,
    pub state_name: String,
    pub clip_name: String,
    #[serde(default)]
    pub generator: CapabilityRoleGenerator,
    pub trigger_event: Option<String>,
    #[serde(default)]
    pub trigger_aliases: Vec<String>,
    pub motion: ClipMotionPolicy,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapabilityRoleGenerator {
    #[default]
    Single,
    ManualSelector {
        children: Vec<String>,
        selected_generator_index: i16,
        selected_index_can_change_after_activate: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selected_index_variable: Option<String>,
    },
    Blender {
        children: Vec<CapabilityBlenderChild>,
        reference_pose_weight_threshold_bits: u32,
        blend_parameter_bits: u32,
        min_cyclic_blend_parameter_bits: u32,
        max_cyclic_blend_parameter_bits: u32,
        index_of_sync_master_child: i16,
        flags: u16,
        subtract_last_child: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blend_parameter_variable: Option<String>,
    },
    Tree {
        root: CapabilityGeneratorNode,
    },
}

impl CapabilityRoleGenerator {
    pub(crate) fn clip_names<'a>(&'a self, anchor: &'a str) -> Vec<&'a str> {
        match self {
            Self::Single => vec![anchor],
            Self::ManualSelector { children, .. } => children.iter().map(String::as_str).collect(),
            Self::Blender { children, .. } => children
                .iter()
                .map(|child| child.clip_name.as_str())
                .collect(),
            Self::Tree { root } => root.clip_names(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorSourceObject {
    pub behavior_path: String,
    pub object_index: u32,
    pub class_name: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorAnonymousSourceObject {
    pub behavior_path: String,
    pub object_index: u32,
    pub class_name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySourceVariableType {
    Bool,
    Int8,
    Int16,
    Int32,
    Real,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityVariableBindingType {
    Variable,
    CharacterProperty,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorVariableBinding {
    pub member_path: String,
    pub variable_index: i32,
    pub variable_name: String,
    pub bit_index: i32,
    pub binding_type: CapabilityVariableBindingType,
    pub source_variable_type: CapabilitySourceVariableType,
    pub initial_word_value: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorEventRef {
    pub source_id: i32,
    pub event_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorEventProperty {
    pub event: CapabilityGeneratorEventRef,
    pub payload: Option<CapabilityGeneratorSourceObject>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorInterval {
    pub enter_event: CapabilityGeneratorEventRef,
    pub exit_event: CapabilityGeneratorEventRef,
    pub enter_time_bits: u32,
    pub exit_time_bits: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorBlendingTransitionEffect {
    pub source: CapabilityGeneratorSourceObject,
    pub bindings: Vec<CapabilityGeneratorVariableBinding>,
    pub self_transition_mode: u8,
    pub event_mode: u8,
    pub duration_bits: u32,
    pub to_generator_start_fraction_bits: u32,
    pub flags: u16,
    pub end_mode: u8,
    pub blend_curve: u8,
    pub alignment_bone: i16,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorTransition {
    pub trigger_interval: CapabilityGeneratorInterval,
    pub initiate_interval: CapabilityGeneratorInterval,
    pub transition_effect: Option<CapabilityGeneratorBlendingTransitionEffect>,
    pub condition: Option<CapabilityGeneratorSourceObject>,
    pub event: CapabilityGeneratorEventRef,
    pub to_state_id: i32,
    pub from_nested_state_id: i32,
    pub to_nested_state_id: i32,
    pub priority: i32,
    pub flags: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorTransitionArray {
    pub source: CapabilityGeneratorSourceObject,
    pub has_eventless_transitions: Option<bool>,
    pub has_time_bounded_transitions: Option<bool>,
    pub transitions: Vec<CapabilityGeneratorTransition>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorState {
    pub source: CapabilityGeneratorSourceObject,
    pub listeners: Vec<CapabilityGeneratorSourceObject>,
    pub enter_notify_events: Vec<CapabilityGeneratorEventProperty>,
    pub exit_notify_events: Vec<CapabilityGeneratorEventProperty>,
    pub transitions: Option<CapabilityGeneratorTransitionArray>,
    pub generator: Box<CapabilityGeneratorNode>,
    pub name: String,
    pub state_id: i32,
    pub probability_bits: u32,
    pub enable: bool,
    pub has_eventless_transitions: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorBlenderChild {
    pub generator: CapabilityGeneratorNode,
    pub bone_weights: Option<CapabilityGeneratorSourceObject>,
    pub weight_bits: u32,
    pub world_from_model_weight_bits: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorExpressionVariableReference {
    pub variable_index: i32,
    pub variable_name: String,
    pub source_variable_type: CapabilitySourceVariableType,
    pub initial_word_value: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorExpression {
    pub expression: String,
    pub referenced_variables: Vec<CapabilityGeneratorExpressionVariableReference>,
    pub assignment_variable_index: i32,
    pub assignment_event_index: i32,
    pub event_mode: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityGeneratorExpressionArray {
    pub source: CapabilityGeneratorAnonymousSourceObject,
    pub expressions: Vec<CapabilityGeneratorExpression>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapabilityModifierNode {
    List {
        source: CapabilityGeneratorSourceObject,
        bindings: Vec<CapabilityGeneratorVariableBinding>,
        user_data: u64,
        enable: bool,
        modifiers: Vec<CapabilityModifierNode>,
    },
    Damping {
        source: CapabilityGeneratorSourceObject,
        bindings: Vec<CapabilityGeneratorVariableBinding>,
        user_data: u64,
        enable: bool,
        k_p_bits: u32,
        k_i_bits: u32,
        k_d_bits: u32,
        enable_scalar_damping: bool,
        enable_vector_damping: bool,
        raw_value_bits: u32,
        damped_value_bits: u32,
        raw_vector_bits: [u32; 4],
        damped_vector_bits: [u32; 4],
        vector_error_sum_bits: [u32; 4],
        vector_previous_error_bits: [u32; 4],
        error_sum_bits: u32,
        previous_error_bits: u32,
    },
    EvaluateExpression {
        source: CapabilityGeneratorSourceObject,
        bindings: Vec<CapabilityGeneratorVariableBinding>,
        user_data: u64,
        enable: bool,
        expressions: CapabilityGeneratorExpressionArray,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapabilityGeneratorNode {
    Clip {
        source: CapabilityGeneratorSourceObject,
        clip_name: String,
    },
    ManualSelector {
        source: CapabilityGeneratorSourceObject,
        bindings: Vec<CapabilityGeneratorVariableBinding>,
        children: Vec<CapabilityGeneratorNode>,
        selected_generator_index: i32,
        index_selector: Option<CapabilityGeneratorSourceObject>,
        selected_index_can_change_after_activate: bool,
        generator_changed_transition_effect: Option<CapabilityGeneratorSourceObject>,
    },
    Blender {
        source: CapabilityGeneratorSourceObject,
        bindings: Vec<CapabilityGeneratorVariableBinding>,
        children: Vec<CapabilityGeneratorBlenderChild>,
        reference_pose_weight_threshold_bits: u32,
        blend_parameter_bits: u32,
        min_cyclic_blend_parameter_bits: u32,
        max_cyclic_blend_parameter_bits: u32,
        index_of_sync_master_child: i32,
        flags: u16,
        subtract_last_child: bool,
    },
    StateMachine {
        source: CapabilityGeneratorSourceObject,
        bindings: Vec<CapabilityGeneratorVariableBinding>,
        event_to_send_when_state_or_transition_changes: CapabilityGeneratorEventProperty,
        start_state_id_selector: Option<CapabilityGeneratorSourceObject>,
        start_state_id: i32,
        return_to_previous_state_event: CapabilityGeneratorEventRef,
        random_transition_event: CapabilityGeneratorEventRef,
        transition_to_next_higher_state_event: CapabilityGeneratorEventRef,
        transition_to_next_lower_state_event: CapabilityGeneratorEventRef,
        sync_variable_index: i32,
        wrap_around_state_id: bool,
        max_simultaneous_transitions: i32,
        start_state_mode: i32,
        self_transition_mode: i32,
        states: Vec<CapabilityGeneratorState>,
        wildcard_transitions: Option<CapabilityGeneratorTransitionArray>,
    },
    ModifierGenerator {
        source: CapabilityGeneratorSourceObject,
        bindings: Vec<CapabilityGeneratorVariableBinding>,
        user_data: u64,
        modifier: Box<CapabilityModifierNode>,
        generator: Box<CapabilityGeneratorNode>,
    },
}

impl CapabilityGeneratorNode {
    fn clip_names<'a>(&'a self) -> Vec<&'a str> {
        let mut names = Vec::new();
        self.collect_clip_names(&mut names);
        names
    }

    fn collect_clip_names<'a>(&'a self, names: &mut Vec<&'a str>) {
        match self {
            Self::Clip { clip_name, .. } => names.push(clip_name),
            Self::ManualSelector { children, .. } => {
                for child in children {
                    child.collect_clip_names(names);
                }
            }
            Self::Blender { children, .. } => {
                for child in children {
                    child.generator.collect_clip_names(names);
                }
            }
            Self::StateMachine { states, .. } => {
                for state in states {
                    state.generator.collect_clip_names(names);
                }
            }
            Self::ModifierGenerator { generator, .. } => {
                generator.collect_clip_names(names);
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityBlenderChild {
    pub clip_name: String,
    pub weight_bits: u32,
    pub world_from_model_weight_bits: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CapabilityCandidateAttackBinding {
    pub source_identity: SourceCreatureIdentity,
    pub event: String,
    pub role: CreatureClipRole,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct OverlayClipRole {
    pub name: String,
    pub clip_name: String,
    pub start_event: String,
    pub stop_event: String,
    pub motion: ClipMotionPolicy,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CapabilityGraphManifest {
    pub template: CreatureGraphTemplate,
    pub roles: Vec<CapabilityClipRole>,
    #[serde(default)]
    pub candidate_attack_bindings: Vec<CapabilityCandidateAttackBinding>,
    pub idle_event: String,
    pub explicit_events: Vec<EventDecl>,
    pub overlays: Vec<OverlayClipRole>,
}

impl CapabilityGraphManifest {
    pub fn from_mvp(graph: &MvpGraphManifest, motion: &MvpMotionManifest) -> Self {
        let role =
            |role, state_name: &str, clip_name: &str, trigger_event: Option<&str>, motion| {
                CapabilityClipRole {
                    role,
                    state_name: state_name.to_string(),
                    clip_name: clip_name.to_string(),
                    generator: CapabilityRoleGenerator::Single,
                    trigger_event: trigger_event.map(str::to_string),
                    trigger_aliases: Vec::new(),
                    motion,
                }
            };
        Self {
            template: CreatureGraphTemplate::GroundMelee,
            roles: vec![
                role(
                    CreatureClipRole::Idle,
                    "Idle",
                    &graph.idle_clip,
                    None,
                    motion.idle,
                ),
                role(
                    CreatureClipRole::GroundForward,
                    "WalkForward",
                    &graph.walk_forward_clip,
                    Some("startWalk"),
                    motion.walk_forward,
                ),
                role(
                    CreatureClipRole::TurnLeft90,
                    "TurnLeft90",
                    &graph.turn_left_90_clip,
                    Some("TurnLeft90"),
                    motion.turn_left_90,
                ),
                role(
                    CreatureClipRole::TurnRight90,
                    "TurnRight90",
                    &graph.turn_right_90_clip,
                    Some("TurnRight90"),
                    motion.turn_right_90,
                ),
                role(
                    CreatureClipRole::MeleeAttack,
                    "Attack1",
                    &graph.attack_1_clip,
                    Some(&graph.melee_event),
                    motion.attack_1,
                ),
            ],
            candidate_attack_bindings: Vec::new(),
            idle_event: "Idle".to_string(),
            explicit_events: vec![
                EventDecl {
                    name: "Idle".to_string(),
                    usage: EventUsage::Generic,
                    flags: 0,
                },
                EventDecl {
                    name: "startWalk".to_string(),
                    usage: EventUsage::Generic,
                    flags: 0,
                },
                EventDecl {
                    name: "TurnLeft90".to_string(),
                    usage: EventUsage::Generic,
                    flags: 0,
                },
                EventDecl {
                    name: "TurnRight90".to_string(),
                    usage: EventUsage::Generic,
                    flags: 0,
                },
                EventDecl {
                    name: graph.melee_event.clone(),
                    usage: EventUsage::MeleeAttack,
                    flags: 0,
                },
            ],
            overlays: Vec::new(),
        }
    }

    pub(crate) fn any_animation_driven(&self) -> bool {
        self.roles.iter().any(|role| role.motion.animation_driven)
            || self
                .overlays
                .iter()
                .any(|overlay| overlay.motion.animation_driven)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct GraphDeclarations {
    pub events: Vec<EventDecl>,
    pub variables: Vec<VariableDecl>,
    pub character_properties: Vec<PropertyDecl>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct EventDecl {
    pub name: String,
    pub usage: EventUsage,
    pub flags: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventUsage {
    Generic,
    MeleeAttack,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VariableDecl {
    pub name: String,
    pub variable_type: VariableType,
    pub initial_value: VariableValue,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PropertyDecl {
    pub name: String,
    pub variable_type: VariableType,
    pub initial_value: VariableValue,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableType {
    Bool,
    Int32,
    Real,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableValue {
    Bool(bool),
    Int32(i32),
    Real(f32),
}

impl VariableType {
    pub(crate) fn hk_name(self) -> &'static str {
        match self {
            Self::Bool => "VARIABLE_TYPE_BOOL",
            Self::Int32 => "VARIABLE_TYPE_INT32",
            Self::Real => "VARIABLE_TYPE_REAL",
        }
    }
}

impl VariableValue {
    pub(crate) fn word_bits(self) -> i64 {
        match self {
            Self::Bool(value) => i64::from(value),
            Self::Int32(value) => i64::from(value),
            Self::Real(value) => i64::from(value.to_bits() as i32),
        }
    }

    fn matches(self, variable_type: VariableType) -> bool {
        matches!(
            (self, variable_type),
            (Self::Bool(_), VariableType::Bool)
                | (Self::Int32(_), VariableType::Int32)
                | (Self::Real(_), VariableType::Real)
        )
    }

    fn is_finite(self) -> bool {
        !matches!(self, Self::Real(value) if !value.is_finite())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValidationErrors(pub Vec<ValidationError>);

impl ValidationErrors {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ValidationError> {
        self.0.iter()
    }

    pub(crate) fn push(&mut self, code: &'static str, message: impl Into<String>) {
        self.0.push(ValidationError {
            code,
            message: message.into(),
        });
    }

    pub(crate) fn finish(self) -> Result<(), Self> {
        if self.is_empty() { Ok(()) } else { Err(self) }
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; ")?;
            }
            write!(formatter, "{}: {}", error.code, error.message)?;
        }
        Ok(())
    }
}

impl Error for ValidationErrors {}

impl CreatureManifest {
    pub fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::default();

        validate_identifier("creature_name", &self.creature_name, &mut errors);
        validate_path(
            "visual_skeleton_nif",
            &self.visual_skeleton_nif,
            ".nif",
            &mut errors,
        );
        validate_path(
            "animation_skeleton.path",
            &self.animation_skeleton.path,
            ".hkx",
            &mut errors,
        );
        if let RagdollDisposition::SourceOwned {
            runtime_path,
            receipt,
        } = &self.ragdoll
        {
            validate_path("ragdoll.runtime_path", runtime_path, ".hkx", &mut errors);
            if receipt.byte_len == 0 {
                errors.push(
                    "invalid_ragdoll_receipt",
                    "source-owned ragdoll receipt byte_len must be positive",
                );
            }
            if !is_blake3(&receipt.blake3) {
                errors.push(
                    "invalid_ragdoll_receipt",
                    "source-owned ragdoll receipt must contain a 64-digit BLAKE3 hash",
                );
            }
            if let Some(powered_ragdoll) = &receipt.powered_ragdoll {
                validate_powered_ragdoll_config(
                    powered_ragdoll,
                    self.animation_skeleton.bones.len(),
                    &mut errors,
                );
            }
        } else if matches!(
            self.ragdoll,
            RagdollDisposition::NoRagdoll {
                reason: NoRagdollReason::Deferred
            }
        ) {
            errors.push(
                "ragdoll_deferred",
                "deferred ragdoll evidence is not an executable no-ragdoll disposition",
            );
        }
        validate_path("paths.project", &self.paths.project, ".hkx", &mut errors);
        validate_path(
            "paths.character",
            &self.paths.character,
            ".hkx",
            &mut errors,
        );
        validate_path(
            "paths.root_behavior",
            &self.paths.root_behavior,
            ".hkx",
            &mut errors,
        );
        validate_path(
            "paths.core_behavior",
            &self.paths.core_behavior,
            ".hkx",
            &mut errors,
        );

        let mut path_owners = BTreeMap::new();
        for (label, path) in self.named_paths() {
            let key = path.to_ascii_lowercase();
            if let Some(previous) = path_owners.insert(key, label) {
                errors.push(
                    "duplicate_path",
                    format!("{label} and {previous} use the same Windows path {path:?}"),
                );
            }
        }

        validate_capsule(self.capsule, &mut errors);
        validate_controller(self.controller, &mut errors);
        validate_skeleton(&self.animation_skeleton, &mut errors);
        validate_clips(self, &mut errors);
        validate_graph("root", &self.root, &mut errors);
        validate_graph("core", &self.core, &mut errors);
        validate_root_union(self, &mut errors);
        validate_ragdoll_declarations(self, &mut errors);
        for (label, path) in [
            (
                "animation_skeleton.path",
                self.animation_skeleton.path.as_str(),
            ),
            ("paths.character", self.paths.character.as_str()),
            ("paths.root_behavior", self.paths.root_behavior.as_str()),
            ("paths.core_behavior", self.paths.core_behavior.as_str()),
        ] {
            if self.project_relative_havok_path(path).is_none() {
                errors.push(
                    "havok_path_outside_project",
                    format!(
                        "{label} path {path:?} must be below the project directory containing {:?}",
                        self.paths.project
                    ),
                );
            }
        }
        if let Some(path) = self.ragdoll.runtime_path()
            && self.project_relative_havok_path(path).is_none()
        {
            errors.push(
                "havok_path_outside_project",
                format!(
                    "ragdoll path {path:?} must be below the project directory containing {:?}",
                    self.paths.project
                ),
            );
        }
        for clip in &self.clips {
            if self.project_relative_havok_path(&clip.path).is_none() {
                errors.push(
                    "havok_path_outside_project",
                    format!(
                        "clip {:?} path {:?} must be below the project directory containing {:?}",
                        clip.name, clip.path, self.paths.project
                    ),
                );
            }
        }

        errors.finish()
    }

    pub fn validate_mvp(&self, graph: &MvpGraphManifest) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::default();
        if let Err(manifest_errors) = self.validate() {
            errors.0.extend(manifest_errors.0);
        }

        let mut role_names = BTreeSet::new();
        for (role, clip_name) in graph.named_clips() {
            validate_identifier(&format!("mvp.{role}"), clip_name, &mut errors);
            if !role_names.insert(clip_name.to_ascii_lowercase()) {
                errors.push(
                    "duplicate_mvp_clip_role",
                    format!("MVP clip roles must reference distinct clips, repeated {clip_name:?}"),
                );
            }
            if !self.clips.iter().any(|clip| clip.name == clip_name) {
                errors.push(
                    "missing_mvp_clip",
                    format!("MVP role {role} references undeclared clip {clip_name:?}"),
                );
            }
        }

        if graph.idle_clip != self.idle_clip {
            errors.push(
                "mvp_idle_mismatch",
                format!(
                    "MVP idle clip {:?} must match creature idle_clip {:?}",
                    graph.idle_clip, self.idle_clip
                ),
            );
        }

        for (role, clip_name, expected_looping) in [
            ("idle_clip", graph.idle_clip.as_str(), true),
            ("walk_forward_clip", graph.walk_forward_clip.as_str(), true),
            ("turn_left_90_clip", graph.turn_left_90_clip.as_str(), false),
            (
                "turn_right_90_clip",
                graph.turn_right_90_clip.as_str(),
                false,
            ),
            ("attack_1_clip", graph.attack_1_clip.as_str(), false),
        ] {
            if let Some(clip) = self.clips.iter().find(|clip| clip.name == clip_name)
                && clip.looping != expected_looping
            {
                errors.push(
                    "mvp_clip_mode",
                    format!(
                        "MVP role {role} expects clip {clip_name:?} looping={expected_looping}"
                    ),
                );
            }
        }

        for trigger in ["Idle", "startWalk", "TurnLeft90", "TurnRight90"] {
            match self.core.events.iter().find(|event| event.name == trigger) {
                Some(event) if event.usage == EventUsage::Generic => {}
                Some(_) => errors.push(
                    "mvp_event_usage",
                    format!("MVP trigger {trigger:?} must use EventUsage::Generic"),
                ),
                None => errors.push(
                    "missing_mvp_event",
                    format!("core graph must declare MVP trigger {trigger:?}"),
                ),
            }
        }

        validate_identifier("mvp.melee_event", &graph.melee_event, &mut errors);
        match self
            .core
            .events
            .iter()
            .find(|event| event.name == graph.melee_event)
        {
            Some(event) if event.usage == EventUsage::MeleeAttack => {}
            Some(_) => errors.push(
                "mvp_event_usage",
                format!(
                    "MVP melee trigger {:?} must use EventUsage::MeleeAttack",
                    graph.melee_event
                ),
            ),
            None => errors.push(
                "missing_mvp_event",
                format!(
                    "core graph must declare MVP melee trigger {:?}",
                    graph.melee_event
                ),
            ),
        }

        errors.finish()
    }

    pub fn validate_mvp_motion(
        &self,
        graph: &MvpGraphManifest,
        motion: &MvpMotionManifest,
    ) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::default();
        if let Err(manifest_errors) = self.validate_mvp(graph) {
            errors.0.extend(manifest_errors.0);
        }

        for (role, policy) in motion.named_roles() {
            if policy.animation_driven && policy.extracted_planar_reference_frames == 0 {
                errors.push(
                    "animation_driven_without_extracted_motion",
                    format!(
                        "MVP role {role} cannot be animation-driven without extracted planar reference frames"
                    ),
                );
            } else if !policy.animation_driven && policy.extracted_planar_reference_frames > 0 {
                errors.push(
                    "extracted_motion_without_animation_driven",
                    format!(
                        "MVP role {role} declares {} extracted planar reference frames but is not animation-driven",
                        policy.extracted_planar_reference_frames
                    ),
                );
            }
        }

        if motion.any_animation_driven() {
            for (label, declarations) in [("root", &self.root), ("core", &self.core)] {
                if let Some(event) = declarations
                    .events
                    .iter()
                    .find(|event| event.name == "startAnimationDriven")
                    && event.usage != EventUsage::Generic
                {
                    errors.push(
                        "animation_driven_event_conflict",
                        format!(
                            "{label} startAnimationDriven must use EventUsage::Generic when declared"
                        ),
                    );
                }
                if let Some(variable) = declarations
                    .variables
                    .iter()
                    .find(|variable| variable.name == "bAnimationDriven")
                    && (variable.variable_type != VariableType::Bool
                        || variable.initial_value != VariableValue::Bool(false))
                {
                    errors.push(
                        "animation_driven_variable_conflict",
                        format!(
                            "{label} bAnimationDriven must be a bool initially false when declared"
                        ),
                    );
                }
            }
        }

        errors.finish()
    }

    pub fn validate_capability_graph(
        &self,
        graph: &CapabilityGraphManifest,
    ) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::default();
        if let Err(manifest_errors) = self.validate() {
            errors.0.extend(manifest_errors.0);
        }

        if graph.roles.len() > 100 {
            errors.push(
                "graph_role_capacity",
                "the deterministic scaffold currently represents at most 100 core states",
            );
        }

        let mut explicit_events = BTreeMap::new();
        for event in &graph.explicit_events {
            validate_name("capability.explicit_event", &event.name, &mut errors);
            let key = event.name.to_ascii_lowercase();
            if explicit_events.insert(key, event).is_some() {
                errors.push(
                    "duplicate_explicit_event",
                    format!("capability graph repeats explicit event {:?}", event.name),
                );
            }
            if event.usage == EventUsage::MeleeAttack
                && (!event.name.starts_with("melee") || event.name.len() == "melee".len())
            {
                errors.push(
                    "melee_event_name",
                    format!(
                        "melee attack event {:?} must start with lowercase 'melee' and include a suffix",
                        event.name
                    ),
                );
            }
            for (label, declarations) in [("root", &self.root), ("core", &self.core)] {
                if let Some(existing) = declarations
                    .events
                    .iter()
                    .find(|existing| existing.name.eq_ignore_ascii_case(&event.name))
                    && existing != event
                {
                    errors.push(
                        "explicit_event_conflict",
                        format!(
                            "{label} event {:?} conflicts with the capability declaration",
                            event.name
                        ),
                    );
                }
            }
        }

        validate_name("capability.idle_event", &graph.idle_event, &mut errors);
        validate_capability_event(
            "idle_event",
            &graph.idle_event,
            EventUsage::Generic,
            &explicit_events,
            &mut errors,
        );

        let mut candidate_attack_events: BTreeMap<String, BTreeSet<CreatureClipRole>> =
            BTreeMap::new();
        let mut candidate_attack_keys = BTreeSet::new();
        let mut previous_binding_key: Option<(String, String, CreatureClipRole)> = None;
        for binding in &graph.candidate_attack_bindings {
            let event_key = binding.event.to_ascii_lowercase();
            let binding_key = (
                binding.source_identity.stable_key(),
                event_key.clone(),
                binding.role,
            );
            if previous_binding_key
                .as_ref()
                .is_some_and(|previous| previous >= &binding_key)
            {
                errors.push(
                    "candidate_attack_binding_order",
                    "candidate attack bindings must be sorted and unique by source identity, event, and role",
                );
            }
            previous_binding_key = Some(binding_key);
            if !binding.source_identity.is_valid() {
                errors.push(
                    "candidate_attack_binding_identity",
                    "candidate attack binding has an incomplete source identity",
                );
            }
            validate_name(
                "capability.candidate_attack_binding.event",
                &binding.event,
                &mut errors,
            );
            if !matches!(
                binding.role,
                CreatureClipRole::MeleeAttack | CreatureClipRole::ProjectileAttack
            ) {
                errors.push(
                    "candidate_attack_binding_role",
                    format!(
                        "candidate attack binding for {:?} uses non-attack role {:?}",
                        binding.event, binding.role
                    ),
                );
            } else if !template_allows_attack_role(graph.template, binding.role) {
                errors.push(
                    "candidate_attack_binding_template",
                    format!(
                        "template {:?} does not support candidate attack role {:?}",
                        graph.template, binding.role
                    ),
                );
            }
            if !candidate_attack_keys
                .insert((binding.source_identity.stable_key(), event_key.clone()))
            {
                errors.push(
                    "duplicate_candidate_attack_binding",
                    format!(
                        "source {} repeats candidate attack event {:?}",
                        binding.source_identity.stable_key(),
                        binding.event
                    ),
                );
            }
            validate_capability_event(
                "candidate attack binding",
                &binding.event,
                EventUsage::Generic,
                &explicit_events,
                &mut errors,
            );
            candidate_attack_events
                .entry(event_key)
                .or_default()
                .insert(binding.role);
        }

        let mut state_names = BTreeSet::new();
        let mut clip_names = BTreeSet::new();
        let mut trigger_events = BTreeSet::from([graph.idle_event.to_ascii_lowercase()]);
        let mut role_event_roles = BTreeMap::new();
        let mut singleton_roles = BTreeSet::new();
        for role in &graph.roles {
            validate_identifier("capability.state_name", &role.state_name, &mut errors);
            validate_identifier("capability.clip_name", &role.clip_name, &mut errors);
            if !state_names.insert(role.state_name.to_ascii_lowercase()) {
                errors.push(
                    "duplicate_capability_state",
                    format!("capability graph repeats state name {:?}", role.state_name),
                );
            }
            validate_role_generator(self, role, &mut clip_names, &mut errors);
            validate_motion_policy(
                &format!("capability role {:?}", role.role),
                role.motion,
                &mut errors,
            );

            if !role.role.allows_multiple() && !singleton_roles.insert(role.role) {
                errors.push(
                    "duplicate_capability_role",
                    format!("capability graph repeats singleton role {:?}", role.role),
                );
            }
            let primary_idle_role = primary_idle_role(graph.template);
            match &role.trigger_event {
                None if role.role == primary_idle_role => {}
                None => errors.push(
                    "missing_role_event",
                    format!(
                        "capability role {:?} requires an explicit trigger",
                        role.role
                    ),
                ),
                Some(_) if role.role == primary_idle_role => errors.push(
                    "idle_role_event",
                    format!("idle role {:?} cannot have a trigger", role.role),
                ),
                Some(event_name) => {
                    if !trigger_events.insert(event_name.to_ascii_lowercase()) {
                        errors.push(
                            "duplicate_role_event",
                            format!("more than one state uses trigger {event_name:?}"),
                        );
                    }
                    let event_key = event_name.to_ascii_lowercase();
                    role_event_roles.insert(event_key.clone(), role.role);
                    let expected_usage = if candidate_attack_events.contains_key(&event_key) {
                        if !matches!(
                            role.role,
                            CreatureClipRole::MeleeAttack | CreatureClipRole::ProjectileAttack
                        ) {
                            errors.push(
                                "candidate_attack_binding_state",
                                format!(
                                    "candidate attack event {event_name:?} enters non-attack role {:?}",
                                    role.role
                                ),
                            );
                        }
                        EventUsage::Generic
                    } else if role.role == CreatureClipRole::MeleeAttack {
                        EventUsage::MeleeAttack
                    } else {
                        EventUsage::Generic
                    };
                    validate_capability_event(
                        &format!("role {:?}", role.role),
                        event_name,
                        expected_usage,
                        &explicit_events,
                        &mut errors,
                    );
                }
            }
            let normalized_aliases = role
                .trigger_aliases
                .iter()
                .map(|alias| alias.to_ascii_lowercase())
                .collect::<Vec<_>>();
            if !normalized_aliases.windows(2).all(|pair| pair[0] < pair[1]) {
                errors.push(
                    "capability_trigger_alias_order",
                    format!(
                        "trigger aliases for role {:?} must be case-insensitively sorted and unique",
                        role.role
                    ),
                );
            }
            let expected_usage = if role.role == CreatureClipRole::MeleeAttack {
                EventUsage::MeleeAttack
            } else {
                EventUsage::Generic
            };
            for alias in &role.trigger_aliases {
                if !trigger_events.insert(alias.to_ascii_lowercase()) {
                    errors.push(
                        "duplicate_role_event",
                        format!("trigger alias {alias:?} is assigned to more than one transition"),
                    );
                }
                role_event_roles.insert(alias.to_ascii_lowercase(), role.role);
                let expected_usage =
                    if candidate_attack_events.contains_key(&alias.to_ascii_lowercase()) {
                        EventUsage::Generic
                    } else {
                        expected_usage
                    };
                validate_capability_event(
                    &format!("role {:?} trigger alias", role.role),
                    alias,
                    expected_usage,
                    &explicit_events,
                    &mut errors,
                );
            }
        }

        for event in candidate_attack_events.keys() {
            if !role_event_roles.get(event).is_some_and(|role| {
                matches!(
                    role,
                    CreatureClipRole::MeleeAttack | CreatureClipRole::ProjectileAttack
                )
            }) {
                errors.push(
                    "candidate_attack_binding_state",
                    format!(
                        "candidate attack event {event:?} does not enter one declared attack state"
                    ),
                );
            }
        }

        let default_idle_role = match graph.template {
            CreatureGraphTemplate::GroundMelee
            | CreatureGraphTemplate::PassiveGround
            | CreatureGraphTemplate::GroundRangedProjectile
            | CreatureGraphTemplate::GroundMeleeRanged
            | CreatureGraphTemplate::GroundSwim
            | CreatureGraphTemplate::GroundFly
            | CreatureGraphTemplate::RobotContinuousAttack => CreatureClipRole::Idle,
            CreatureGraphTemplate::Swim => CreatureClipRole::SwimIdle,
            CreatureGraphTemplate::Fly => CreatureClipRole::FlyIdle,
            CreatureGraphTemplate::StationaryTurret => CreatureClipRole::StationaryIdle,
        };
        if let Some(idle) = graph
            .roles
            .iter()
            .find(|role| role.role == default_idle_role)
            && idle.clip_name != self.idle_clip
        {
            errors.push(
                "capability_idle_mismatch",
                format!(
                    "template idle role must use creature idle_clip {:?}, got {:?}",
                    self.idle_clip, idle.clip_name
                ),
            );
        }

        validate_template_roles(graph, &mut errors);

        for overlay in &graph.overlays {
            validate_identifier("capability.overlay.name", &overlay.name, &mut errors);
            validate_identifier(
                "capability.overlay.clip_name",
                &overlay.clip_name,
                &mut errors,
            );
            if !clip_names.insert(overlay.clip_name.to_ascii_lowercase()) {
                errors.push(
                    "duplicate_capability_clip",
                    format!(
                        "overlay {:?} aliases another role clip {:?}",
                        overlay.name, overlay.clip_name
                    ),
                );
            }
            match self
                .clips
                .iter()
                .find(|clip| clip.name == overlay.clip_name)
            {
                None => errors.push(
                    "missing_capability_clip",
                    format!(
                        "overlay {:?} references undeclared clip {:?}",
                        overlay.name, overlay.clip_name
                    ),
                ),
                Some(clip) if !clip.looping => errors.push(
                    "overlay_clip_mode",
                    format!(
                        "overlay {:?} requires looping clip {:?}",
                        overlay.name, overlay.clip_name
                    ),
                ),
                Some(_) => {}
            }
            validate_motion_policy(
                &format!("overlay {:?}", overlay.name),
                overlay.motion,
                &mut errors,
            );
            if overlay.motion != ClipMotionPolicy::default() {
                errors.push(
                    "overlay_root_motion",
                    format!(
                        "overlay {:?} must not contribute extracted or animation-driven root motion",
                        overlay.name
                    ),
                );
            }
            if overlay
                .start_event
                .eq_ignore_ascii_case(&overlay.stop_event)
            {
                errors.push(
                    "overlay_event_collision",
                    format!(
                        "overlay {:?} requires distinct start and stop events",
                        overlay.name
                    ),
                );
            }
            for (label, event_name) in [
                ("overlay start", overlay.start_event.as_str()),
                ("overlay stop", overlay.stop_event.as_str()),
            ] {
                validate_capability_event(
                    label,
                    event_name,
                    EventUsage::Generic,
                    &explicit_events,
                    &mut errors,
                );
            }
        }
        if graph.template == CreatureGraphTemplate::StationaryTurret {
            validate_stationary_turret_aim(self, &mut errors);
        }
        if graph.any_animation_driven() {
            for (label, declarations) in [("root", &self.root), ("core", &self.core)] {
                if let Some(event) = declarations
                    .events
                    .iter()
                    .find(|event| event.name == "startAnimationDriven")
                    && event.usage != EventUsage::Generic
                {
                    errors.push(
                        "animation_driven_event_conflict",
                        format!(
                            "{label} startAnimationDriven must use EventUsage::Generic when declared"
                        ),
                    );
                }
                if let Some(variable) = declarations
                    .variables
                    .iter()
                    .find(|variable| variable.name == "bAnimationDriven")
                    && (variable.variable_type != VariableType::Bool
                        || variable.initial_value != VariableValue::Bool(false))
                {
                    errors.push(
                        "animation_driven_variable_conflict",
                        format!(
                            "{label} bAnimationDriven must be a bool initially false when declared"
                        ),
                    );
                }
            }
        }
        errors.finish()
    }

    pub fn required_runtime_paths(&self) -> BTreeSet<String> {
        self.named_paths()
            .into_iter()
            .map(|(_, path)| path.to_string())
            .collect()
    }

    pub fn validate_file_tree<I, S>(&self, paths: I) -> Result<(), ValidationErrors>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut errors = ValidationErrors::default();
        let available: BTreeSet<String> = paths
            .into_iter()
            .map(|path| path.as_ref().to_ascii_lowercase())
            .collect();

        for required in self.required_runtime_paths() {
            if !available.contains(&required.to_ascii_lowercase()) {
                errors.push(
                    "missing_file",
                    format!("runtime tree is missing required path {required:?}"),
                );
            }
        }
        errors.finish()
    }

    fn named_paths(&self) -> Vec<(&'static str, &str)> {
        let mut paths = vec![
            ("visual_skeleton_nif", self.visual_skeleton_nif.as_str()),
            (
                "animation_skeleton.path",
                self.animation_skeleton.path.as_str(),
            ),
            ("paths.project", self.paths.project.as_str()),
            ("paths.character", self.paths.character.as_str()),
            ("paths.root_behavior", self.paths.root_behavior.as_str()),
            ("paths.core_behavior", self.paths.core_behavior.as_str()),
        ];
        if let Some(path) = self.ragdoll.runtime_path() {
            paths.push(("ragdoll.runtime_path", path));
        }
        for clip in &self.clips {
            paths.push(("clip.path", clip.path.as_str()));
        }
        paths
    }

    pub(crate) fn project_relative_havok_path<'a>(&self, runtime_path: &'a str) -> Option<&'a str> {
        let Some((project_root, _)) = self.paths.project.rsplit_once('\\') else {
            return Some(runtime_path);
        };
        let prefix_len = project_root.len();
        let runtime_prefix = runtime_path.get(..prefix_len)?;
        if runtime_path.len() <= prefix_len
            || !runtime_prefix.eq_ignore_ascii_case(project_root)
            || runtime_path.as_bytes().get(prefix_len) != Some(&b'\\')
        {
            return None;
        }
        runtime_path.get(prefix_len + 1..)
    }
}

fn validate_powered_ragdoll_config(
    config: &SourceOwnedPoweredRagdollConfig,
    skeleton_bone_count: usize,
    errors: &mut ValidationErrors,
) {
    let source = &config.source;
    let mut required_inert = BTreeSet::from([
        PoweredRagdollSourceField::DataUnknownByte2,
        PoweredRagdollSourceField::DataUnknownByte3,
        PoweredRagdollSourceField::DataUnknownByte4,
        PoweredRagdollSourceField::DataUnknownByte5,
        PoweredRagdollSourceField::DataEnabledFootIk,
        PoweredRagdollSourceField::DataEnabledLookIk,
        PoweredRagdollSourceField::DataEnabledGrabIk,
        PoweredRagdollSourceField::DataUnknownByte11,
    ]);
    if source.unknown_trailing_u8.is_some() {
        required_inert.insert(PoweredRagdollSourceField::DataUnknownTrailingU8);
    }
    match &source.feedback {
        Some(feedback) => {
            required_inert.extend([
                PoweredRagdollSourceField::FeedbackDynamicKeyframeBlendAmount,
                PoweredRagdollSourceField::FeedbackHierarchyGain,
                PoweredRagdollSourceField::FeedbackPositionGain,
                PoweredRagdollSourceField::FeedbackVelocityGain,
                PoweredRagdollSourceField::FeedbackAccelerationGain,
                PoweredRagdollSourceField::FeedbackSnapGain,
                PoweredRagdollSourceField::FeedbackVelocityDamping,
                PoweredRagdollSourceField::FeedbackSnapMaxLinearVelocity,
                PoweredRagdollSourceField::FeedbackSnapMaxAngularVelocity,
                PoweredRagdollSourceField::FeedbackSnapMaxLinearDistance,
                PoweredRagdollSourceField::FeedbackSnapMaxAngularDistance,
                PoweredRagdollSourceField::FeedbackPositionMaxVelocityLinear,
                PoweredRagdollSourceField::FeedbackPositionMaxVelocityAngular,
                PoweredRagdollSourceField::FeedbackPositionMaxVelocityProjectile,
                PoweredRagdollSourceField::FeedbackPositionMaxVelocityMelee,
            ]);
            for bits in [
                feedback.dynamic_keyframe_blend_amount_bits,
                feedback.hierarchy_gain_bits,
                feedback.position_gain_bits,
                feedback.velocity_gain_bits,
                feedback.acceleration_gain_bits,
                feedback.snap_gain_bits,
                feedback.velocity_damping_bits,
                feedback.snap_max_linear_velocity_bits,
                feedback.snap_max_angular_velocity_bits,
                feedback.snap_max_linear_distance_bits,
                feedback.snap_max_angular_distance_bits,
                feedback.position_max_velocity_linear_bits,
                feedback.position_max_velocity_angular_bits,
            ] {
                if !f32::from_bits(bits).is_finite() {
                    errors.push(
                        "invalid_powered_ragdoll_feedback",
                        "source feedback evidence contains a non-finite scalar",
                    );
                }
            }
            if source.dynamic_bone_count as usize != feedback.bones.len() {
                errors.push(
                    "powered_ragdoll_feedback_count",
                    "source dynamic_bone_count differs from the ordered feedback bone count",
                );
            }
            if feedback
                .bones
                .iter()
                .any(|bone| usize::from(*bone) >= skeleton_bone_count)
            {
                errors.push(
                    "powered_ragdoll_feedback_bone",
                    "source feedback references a bone outside the animation skeleton",
                );
            }
            if !source.enabled_feedback && !feedback.bones.is_empty() {
                required_inert.insert(PoweredRagdollSourceField::FeedbackBoneIndicesWhileDisabled);
            }
        }
        None => {
            if source.enabled_feedback || source.dynamic_bone_count != 0 {
                errors.push(
                    "missing_powered_ragdoll_feedback",
                    "enabled or nonempty source feedback has no RAFD/RAFB evidence",
                );
            }
        }
    }
    match &source.pose_matching {
        Some(pose) => {
            required_inert.extend([
                PoweredRagdollSourceField::PoseFlags,
                PoweredRagdollSourceField::PoseUnknown,
                PoweredRagdollSourceField::PoseMotorsStrength,
                PoweredRagdollSourceField::PoseActivationDelayTime,
                PoweredRagdollSourceField::PoseMatchErrorAllowance,
                PoweredRagdollSourceField::PoseDisplacementToDisable,
            ]);
            for bits in [
                pose.motors_strength_bits,
                pose.pose_activation_delay_time_bits,
                pose.match_error_allowance_bits,
                pose.displacement_to_disable_bits,
            ] {
                if !f32::from_bits(bits).is_finite() {
                    errors.push(
                        "invalid_powered_ragdoll_pose",
                        "source pose-matching evidence contains a non-finite scalar",
                    );
                }
            }
            if pose
                .matching_bones
                .iter()
                .flatten()
                .any(|bone| usize::from(*bone) >= skeleton_bone_count)
            {
                errors.push(
                    "powered_ragdoll_pose_bone",
                    "source pose matching references a bone outside the animation skeleton",
                );
            }
            if !source.enabled_pose_matching && pose.matching_bones.iter().any(Option::is_some) {
                required_inert
                    .insert(PoweredRagdollSourceField::PoseMatchingBoneIndicesWhileDisabled);
            }
        }
        None if source.enabled_pose_matching => errors.push(
            "missing_powered_ragdoll_pose",
            "enabled source pose matching has no RAPS evidence",
        ),
        None => {}
    }
    if let Some(path) = &source.death_pose_animation {
        if path.trim().is_empty() || !path.to_ascii_lowercase().ends_with(".psa") {
            errors.push(
                "invalid_powered_ragdoll_death_pose",
                "source death-pose evidence must be a nonempty .psa path",
            );
        }
        required_inert.insert(PoweredRagdollSourceField::DeathPoseAnimation);
    }
    let actual_inert = config
        .proven_inert_source_fields
        .iter()
        .map(|receipt| receipt.field)
        .collect::<BTreeSet<_>>();
    for receipt in &config.proven_inert_source_fields {
        validate_powered_ragdoll_inert_receipt(receipt, errors);
    }
    if actual_inert.len() != config.proven_inert_source_fields.len()
        || actual_inert != required_inert
    {
        errors.push(
            "powered_ragdoll_source_accounting",
            "proven-inert receipts must account exactly once for every unbound source RGDL field",
        );
    }
}

fn validate_powered_ragdoll_inert_receipt(
    receipt: &PoweredRagdollInertFieldReceipt,
    errors: &mut ValidationErrors,
) {
    let value = serde_json::from_str::<serde_json::Value>(&receipt.canonical_json).ok();
    let field = serde_json::to_value(receipt.field).expect("ragdoll field serializes");
    let valid = receipt.proof_schema == POWERED_RAGDOLL_INERT_PROOF_SCHEMA
        && blake3::hash(receipt.canonical_json.as_bytes())
            .to_hex()
            .as_str()
            == receipt.canonical_json_blake3.as_str()
        && value.as_ref().is_some_and(|value| {
            serde_json::to_string(value).ok().as_deref() == Some(receipt.canonical_json.as_str())
                && value.get("field") == Some(&field)
                && value.get("disposition").and_then(serde_json::Value::as_str)
                    == Some("runtime_inert")
                && value
                    .get("evidence_blake3")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(is_blake3)
        });
    if !valid {
        errors.push(
            "powered_ragdoll_inert_proof",
            format!(
                "source RGDL field {:?} lacks a canonical hash-bound runtime-inert proof",
                receipt.field
            ),
        );
    }
}

pub(crate) const RAGDOLL_TRANSITION_EVENTS: [&str; 4] = [
    "Ragdoll",
    "RagdollInstant",
    "g_InitializeGraph",
    "g_InitializeGraphInstant",
];
pub(crate) const RAGDOLL_ENTER_EVENTS: [&str; 2] =
    ["AddRagdollToWorld", "RemoveCharacterControllerFromWorld"];

fn validate_ragdoll_declarations(manifest: &CreatureManifest, errors: &mut ValidationErrors) {
    let RagdollDisposition::SourceOwned { .. } = &manifest.ragdoll else {
        return;
    };
    for name in RAGDOLL_TRANSITION_EVENTS
        .into_iter()
        .chain(RAGDOLL_ENTER_EVENTS)
    {
        match manifest.root.events.iter().find(|event| event.name == name) {
            Some(EventDecl {
                usage: EventUsage::Generic,
                flags: 0,
                ..
            }) => {}
            Some(_) => errors.push(
                "ragdoll_event_declaration",
                format!("root ragdoll event {name:?} must be generic with flags=0"),
            ),
            None => errors.push(
                "ragdoll_event_declaration",
                format!("root graph must explicitly declare ragdoll event {name:?}"),
            ),
        }
        if manifest.core.events.iter().any(|event| event.name == name) {
            errors.push(
                "ragdoll_event_scope",
                format!("ragdoll event {name:?} is root-owned and must not appear in core"),
            );
        }
    }
    if manifest.root.events.len() <= manifest.core.events.len() {
        errors.push(
            "ragdoll_root_not_strict_superset",
            "ragdoll-enabled root declarations must be a strict superset of core declarations",
        );
    }
}

fn is_blake3(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_controller(controller: CreatureControllerDecl, errors: &mut ValidationErrors) {
    if !(0..=255).contains(&controller.rigid_body_type) {
        errors.push(
            "controller_type",
            "controller rigid_body_type must fit the FO4 hkbRigidBodySetup enum byte",
        );
    }
    if !controller.model_scale.is_finite() || controller.model_scale <= 0.0 {
        errors.push(
            "controller_scale",
            "controller model_scale must be finite and positive",
        );
    }
    for (name, axis) in [
        ("up", controller.model_up_ms),
        ("forward", controller.model_forward_ms),
        ("right", controller.model_right_ms),
    ] {
        if !axis.iter().all(|value| value.is_finite())
            || axis[..3].iter().map(|value| value * value).sum::<f32>() <= f32::EPSILON
        {
            errors.push(
                "controller_basis",
                format!("controller {name} axis must be finite and nondegenerate"),
            );
        }
    }
}

fn validate_motion_policy(label: &str, policy: ClipMotionPolicy, errors: &mut ValidationErrors) {
    if policy.animation_driven && policy.extracted_planar_reference_frames == 0 {
        errors.push(
            "animation_driven_without_extracted_motion",
            format!("{label} cannot be animation-driven without extracted planar reference frames"),
        );
    } else if !policy.animation_driven && policy.extracted_planar_reference_frames > 0 {
        errors.push(
            "extracted_motion_without_animation_driven",
            format!(
                "{label} declares {} extracted planar reference frames but is not animation-driven",
                policy.extracted_planar_reference_frames
            ),
        );
    }
}

fn validate_capability_event(
    label: &str,
    event_name: &str,
    expected_usage: EventUsage,
    explicit_events: &BTreeMap<String, &EventDecl>,
    errors: &mut ValidationErrors,
) {
    validate_name(label, event_name, errors);
    match explicit_events.get(&event_name.to_ascii_lowercase()) {
        Some(event) if event.usage == expected_usage => {}
        Some(_) => errors.push(
            "capability_event_usage",
            format!("{label} event {event_name:?} must use {expected_usage:?}"),
        ),
        None => errors.push(
            "missing_explicit_event",
            format!("{label} references undeclared explicit event {event_name:?}"),
        ),
    }
}

fn validate_role_generator(
    manifest: &CreatureManifest,
    role: &CapabilityClipRole,
    graph_clip_names: &mut BTreeSet<String>,
    errors: &mut ValidationErrors,
) {
    let clip_names = role.generator.clip_names(&role.clip_name);
    if !clip_names
        .iter()
        .any(|clip_name| *clip_name == role.clip_name)
    {
        errors.push(
            "capability_generator_anchor",
            format!(
                "capability role {:?} generator does not contain anchor clip {:?}",
                role.role, role.clip_name
            ),
        );
    }
    match &role.generator {
        CapabilityRoleGenerator::Single => {}
        CapabilityRoleGenerator::ManualSelector {
            children,
            selected_generator_index,
            selected_index_can_change_after_activate,
            selected_index_variable,
        } => {
            if !(2..=16).contains(&children.len()) {
                errors.push(
                    "manual_selector_child_count",
                    "manual selector must retain between 2 and 16 ordered source children",
                );
            }
            if *selected_generator_index < 0
                || usize::try_from(*selected_generator_index)
                    .map_or(true, |index| index >= children.len())
            {
                errors.push(
                    "manual_selector_selected_index",
                    format!(
                        "manual selector selected index {} is outside {} children",
                        selected_generator_index,
                        children.len()
                    ),
                );
            }
            if *selected_index_can_change_after_activate && selected_index_variable.is_none() {
                errors.push(
                    "manual_selector_dynamic_control",
                    "a dynamic manual selector requires an exact selectedGeneratorIndex variable binding",
                );
            }
            if let Some(variable) = selected_index_variable {
                validate_generator_variable(
                    manifest,
                    variable,
                    VariableType::Int32,
                    "selectedGeneratorIndex",
                    errors,
                );
            }
        }
        CapabilityRoleGenerator::Blender {
            children,
            reference_pose_weight_threshold_bits,
            blend_parameter_bits,
            min_cyclic_blend_parameter_bits,
            max_cyclic_blend_parameter_bits,
            index_of_sync_master_child,
            flags,
            blend_parameter_variable,
            ..
        } => {
            if !(2..=16).contains(&children.len()) {
                errors.push(
                    "blender_child_count",
                    "blender must retain between 2 and 16 ordered source children",
                );
            }
            let reference_pose_weight_threshold =
                f32::from_bits(*reference_pose_weight_threshold_bits);
            let blend_parameter = f32::from_bits(*blend_parameter_bits);
            let min_cyclic = f32::from_bits(*min_cyclic_blend_parameter_bits);
            let max_cyclic = f32::from_bits(*max_cyclic_blend_parameter_bits);
            if !reference_pose_weight_threshold.is_finite()
                || reference_pose_weight_threshold < 0.0
                || !blend_parameter.is_finite()
                || !min_cyclic.is_finite()
                || !max_cyclic.is_finite()
                || min_cyclic > max_cyclic
            {
                errors.push(
                    "blender_numeric_contract",
                    "blender scalar fields must be finite, with a non-negative reference threshold and ordered cyclic bounds",
                );
            }
            if *index_of_sync_master_child < -1
                || (*index_of_sync_master_child >= 0
                    && usize::try_from(*index_of_sync_master_child)
                        .map_or(true, |index| index >= children.len()))
            {
                errors.push(
                    "blender_sync_master",
                    format!(
                        "blender sync master {} is outside {} children",
                        index_of_sync_master_child,
                        children.len()
                    ),
                );
            }
            if *flags & !0x01fd != 0 {
                errors.push(
                    "blender_flags",
                    format!("blender uses unknown hk2014 flags 0x{flags:04x}"),
                );
            }
            for child in children {
                if !f32::from_bits(child.weight_bits).is_finite()
                    || !f32::from_bits(child.world_from_model_weight_bits).is_finite()
                {
                    errors.push(
                        "blender_child_numeric_contract",
                        format!(
                            "blender child {:?} has a non-finite weight",
                            child.clip_name
                        ),
                    );
                }
            }
            if let Some(variable) = blend_parameter_variable {
                validate_generator_variable(
                    manifest,
                    variable,
                    VariableType::Real,
                    "blendParameter",
                    errors,
                );
            }
        }
        CapabilityRoleGenerator::Tree { root } => {
            let mut owned_objects = BTreeSet::new();
            let mut transition_effects = BTreeMap::new();
            validate_capability_generator_node(
                manifest,
                root,
                0,
                &mut owned_objects,
                &mut transition_effects,
                errors,
            );
        }
    }

    let allow_repeated_tree_clip = matches!(&role.generator, CapabilityRoleGenerator::Tree { .. });
    let mut role_clip_names = BTreeSet::new();
    for clip_name in clip_names {
        validate_identifier("capability.generator.clip_name", clip_name, errors);
        let canonical_clip_name = clip_name.to_ascii_lowercase();
        let first_role_reference = role_clip_names.insert(canonical_clip_name.clone());
        if !first_role_reference && allow_repeated_tree_clip {
            continue;
        }
        if !graph_clip_names.insert(canonical_clip_name) {
            errors.push(
                "duplicate_capability_clip",
                format!("capability graph repeats generator clip {clip_name:?}"),
            );
        }
        let Some(clip) = manifest.clips.iter().find(|clip| clip.name == clip_name) else {
            errors.push(
                "missing_capability_clip",
                format!(
                    "capability role {:?} references undeclared clip {clip_name:?}",
                    role.role
                ),
            );
            continue;
        };
        if clip.looping != role.role.expects_looping() {
            errors.push(
                "capability_clip_mode",
                format!(
                    "capability role {:?} expects clip {clip_name:?} looping={}",
                    role.role,
                    role.role.expects_looping()
                ),
            );
        }
    }
}

fn validate_capability_generator_node(
    manifest: &CreatureManifest,
    node: &CapabilityGeneratorNode,
    depth: usize,
    owned_objects: &mut BTreeSet<(String, u32)>,
    transition_effects: &mut BTreeMap<(String, u32), CapabilityGeneratorBlendingTransitionEffect>,
    errors: &mut ValidationErrors,
) {
    if depth > 32 {
        errors.push(
            "capability_generator_depth",
            "recursive capability generator exceeds the 32-node depth limit",
        );
        return;
    }
    let source = match node {
        CapabilityGeneratorNode::Clip { source, .. }
        | CapabilityGeneratorNode::ManualSelector { source, .. }
        | CapabilityGeneratorNode::Blender { source, .. }
        | CapabilityGeneratorNode::StateMachine { source, .. }
        | CapabilityGeneratorNode::ModifierGenerator { source, .. } => source,
    };
    validate_generator_source_object(source, owned_objects, errors);

    match node {
        CapabilityGeneratorNode::Clip { source, clip_name } => {
            if source.class_name != "hkbClipGenerator" {
                errors.push(
                    "capability_generator_class",
                    format!(
                        "recursive clip object {} uses source class {:?}",
                        source.object_index, source.class_name
                    ),
                );
            }
            validate_identifier("capability.generator.tree.clip", clip_name, errors);
        }
        CapabilityGeneratorNode::ManualSelector {
            source,
            bindings,
            children,
            selected_generator_index,
            index_selector,
            selected_index_can_change_after_activate,
            generator_changed_transition_effect,
        } => {
            if source.class_name != "hkbManualSelectorGenerator" {
                errors.push(
                    "capability_generator_class",
                    format!(
                        "recursive selector object {} uses source class {:?}",
                        source.object_index, source.class_name
                    ),
                );
            }
            if !(2..=32).contains(&children.len()) {
                errors.push(
                    "manual_selector_child_count",
                    "recursive manual selector must retain between 2 and 32 ordered source children",
                );
            }
            if *selected_generator_index < 0
                || usize::try_from(*selected_generator_index)
                    .map_or(true, |index| index >= children.len())
            {
                errors.push(
                    "manual_selector_selected_index",
                    format!(
                        "recursive selector selected index {selected_generator_index} is outside {} children",
                        children.len()
                    ),
                );
            }
            if index_selector.is_some() || generator_changed_transition_effect.is_some() {
                errors.push(
                    "manual_selector_unlowered_policy_object",
                    "recursive selector indexSelector/transition-effect objects require an exact target lowerer",
                );
            }
            validate_generator_bindings(manifest, bindings, Some("selectedGeneratorIndex"), errors);
            if *selected_index_can_change_after_activate
                && !bindings
                    .iter()
                    .any(|binding| binding.member_path == "selectedGeneratorIndex")
            {
                errors.push(
                    "manual_selector_dynamic_control",
                    "a dynamic recursive selector requires its exact selectedGeneratorIndex binding",
                );
            }
            for child in children {
                validate_capability_generator_node(
                    manifest,
                    child,
                    depth + 1,
                    owned_objects,
                    transition_effects,
                    errors,
                );
            }
        }
        CapabilityGeneratorNode::Blender {
            source,
            bindings,
            children,
            reference_pose_weight_threshold_bits,
            blend_parameter_bits,
            min_cyclic_blend_parameter_bits,
            max_cyclic_blend_parameter_bits,
            index_of_sync_master_child,
            flags,
            ..
        } => {
            if source.class_name != "hkbBlenderGenerator" {
                errors.push(
                    "capability_generator_class",
                    format!(
                        "recursive blender object {} uses source class {:?}",
                        source.object_index, source.class_name
                    ),
                );
            }
            if !(2..=32).contains(&children.len()) {
                errors.push(
                    "blender_child_count",
                    "recursive blender must retain between 2 and 32 ordered source children",
                );
            }
            let values = [
                f32::from_bits(*reference_pose_weight_threshold_bits),
                f32::from_bits(*blend_parameter_bits),
                f32::from_bits(*min_cyclic_blend_parameter_bits),
                f32::from_bits(*max_cyclic_blend_parameter_bits),
            ];
            if values.iter().any(|value| !value.is_finite())
                || values[0] < 0.0
                || values[2] > values[3]
            {
                errors.push(
                    "blender_numeric_contract",
                    "recursive blender scalar fields must be finite with ordered cyclic bounds",
                );
            }
            if *index_of_sync_master_child < -1
                || (*index_of_sync_master_child >= 0
                    && usize::try_from(*index_of_sync_master_child)
                        .map_or(true, |index| index >= children.len()))
            {
                errors.push(
                    "blender_sync_master",
                    "recursive blender sync-master index is outside its child list",
                );
            }
            if *flags & !0x01fd != 0 {
                errors.push(
                    "blender_flags",
                    format!("recursive blender uses unknown hk2014 flags 0x{flags:04x}"),
                );
            }
            validate_generator_bindings(manifest, bindings, Some("blendParameter"), errors);
            for child in children {
                if child.bone_weights.is_some() {
                    errors.push(
                        "blender_bone_weights_unlowered",
                        "recursive blender bone-weight objects require an exact target lowerer",
                    );
                }
                if !f32::from_bits(child.weight_bits).is_finite()
                    || !f32::from_bits(child.world_from_model_weight_bits).is_finite()
                {
                    errors.push(
                        "blender_child_numeric_contract",
                        "recursive blender child weights must be finite",
                    );
                }
                validate_capability_generator_node(
                    manifest,
                    &child.generator,
                    depth + 1,
                    owned_objects,
                    transition_effects,
                    errors,
                );
            }
        }
        CapabilityGeneratorNode::StateMachine {
            source,
            bindings,
            event_to_send_when_state_or_transition_changes,
            start_state_id_selector,
            start_state_id,
            return_to_previous_state_event,
            random_transition_event,
            transition_to_next_higher_state_event,
            transition_to_next_lower_state_event,
            sync_variable_index,
            max_simultaneous_transitions,
            start_state_mode,
            self_transition_mode,
            states,
            wildcard_transitions,
            ..
        } => {
            if source.class_name != "hkbStateMachine" {
                errors.push(
                    "capability_generator_class",
                    format!(
                        "recursive state machine object {} uses source class {:?}",
                        source.object_index, source.class_name
                    ),
                );
            }
            validate_generator_bindings(manifest, bindings, Some("startStateId"), errors);
            if start_state_id_selector.is_some() {
                errors.push(
                    "state_machine_start_selector_unlowered",
                    "recursive state-machine startStateIdSelector requires an exact target lowerer",
                );
            }
            if states.is_empty() || states.len() > 64 {
                errors.push(
                    "state_machine_state_count",
                    "recursive state machine must retain between 1 and 64 ordered source states",
                );
            }
            let state_ids = states
                .iter()
                .map(|state| state.state_id)
                .collect::<BTreeSet<_>>();
            if state_ids.len() != states.len() || !state_ids.contains(start_state_id) {
                errors.push(
                    "state_machine_state_ids",
                    "recursive state machine requires unique state IDs and an exact existing startStateId",
                );
            }
            if *max_simultaneous_transitions <= 0
                || *start_state_mode != 0
                || *self_transition_mode != 0
            {
                errors.push(
                    "state_machine_mode_unlowered",
                    "recursive state machine currently requires positive max transitions and exact default start/self-transition modes",
                );
            }
            for event in [
                return_to_previous_state_event,
                random_transition_event,
                transition_to_next_higher_state_event,
                transition_to_next_lower_state_event,
            ] {
                validate_generator_event_ref(manifest, event, errors);
            }
            if *sync_variable_index >= 0
                && usize::try_from(*sync_variable_index)
                    .map_or(true, |index| index >= manifest.core.variables.len())
            {
                errors.push(
                    "state_machine_sync_variable",
                    "recursive state-machine sync variable is outside target declarations",
                );
            }
            validate_generator_event_property(
                manifest,
                event_to_send_when_state_or_transition_changes,
                errors,
            );
            for state in states {
                validate_generator_source_object(&state.source, owned_objects, errors);
                validate_identifier("capability.generator.tree.state", &state.name, errors);
                if !state.listeners.is_empty() {
                    errors.push(
                        "state_machine_listeners_unlowered",
                        "recursive state listeners require an exact target listener lowerer",
                    );
                }
                if !f32::from_bits(state.probability_bits).is_finite() {
                    errors.push(
                        "state_machine_probability",
                        "recursive state probability must be finite",
                    );
                }
                for event in state
                    .enter_notify_events
                    .iter()
                    .chain(&state.exit_notify_events)
                {
                    validate_generator_event_property(manifest, event, errors);
                }
                if let Some(transitions) = &state.transitions {
                    validate_generator_transition_array(
                        manifest,
                        transitions,
                        &state_ids,
                        owned_objects,
                        transition_effects,
                        errors,
                    );
                }
                validate_capability_generator_node(
                    manifest,
                    &state.generator,
                    depth + 1,
                    owned_objects,
                    transition_effects,
                    errors,
                );
            }
            if let Some(transitions) = wildcard_transitions {
                validate_generator_transition_array(
                    manifest,
                    transitions,
                    &state_ids,
                    owned_objects,
                    transition_effects,
                    errors,
                );
            }
        }
        CapabilityGeneratorNode::ModifierGenerator {
            source,
            bindings,
            modifier,
            generator,
            ..
        } => {
            if source.class_name != "hkbModifierGenerator" {
                errors.push(
                    "capability_generator_class",
                    format!(
                        "recursive modifier-generator object {} uses source class {:?}",
                        source.object_index, source.class_name
                    ),
                );
            }
            if !bindings.is_empty() {
                errors.push(
                    "modifier_generator_bindings_unlowered",
                    "recursive hkbModifierGenerator bindings require an exact member policy",
                );
            }
            validate_capability_modifier_node(manifest, modifier, depth + 1, owned_objects, errors);
            validate_capability_generator_node(
                manifest,
                generator,
                depth + 1,
                owned_objects,
                transition_effects,
                errors,
            );
        }
    }
}

fn validate_capability_modifier_node(
    manifest: &CreatureManifest,
    modifier: &CapabilityModifierNode,
    depth: usize,
    owned_objects: &mut BTreeSet<(String, u32)>,
    errors: &mut ValidationErrors,
) {
    if depth > 32 {
        errors.push(
            "capability_modifier_depth",
            "recursive capability modifier exceeds the 32-node depth limit",
        );
        return;
    }
    let source = match modifier {
        CapabilityModifierNode::List { source, .. }
        | CapabilityModifierNode::Damping { source, .. }
        | CapabilityModifierNode::EvaluateExpression { source, .. } => source,
    };
    validate_generator_source_object(source, owned_objects, errors);

    match modifier {
        CapabilityModifierNode::List {
            source,
            bindings,
            modifiers,
            ..
        } => {
            if source.class_name != "hkbModifierList" {
                errors.push(
                    "capability_modifier_class",
                    format!(
                        "recursive modifier-list object {} uses source class {:?}",
                        source.object_index, source.class_name
                    ),
                );
            }
            if !bindings.is_empty() {
                errors.push(
                    "modifier_list_bindings_unlowered",
                    "recursive hkbModifierList bindings require an exact member policy",
                );
            }
            if modifiers.is_empty() || modifiers.len() > 32 {
                errors.push(
                    "modifier_list_count",
                    "recursive modifier list must retain between 1 and 32 ordered source modifiers",
                );
            }
            for child in modifiers {
                validate_capability_modifier_node(
                    manifest,
                    child,
                    depth + 1,
                    owned_objects,
                    errors,
                );
            }
        }
        CapabilityModifierNode::Damping {
            source,
            bindings,
            k_p_bits,
            k_i_bits,
            k_d_bits,
            raw_value_bits,
            damped_value_bits,
            raw_vector_bits,
            damped_vector_bits,
            vector_error_sum_bits,
            vector_previous_error_bits,
            error_sum_bits,
            previous_error_bits,
            ..
        } => {
            if source.class_name != "hkbDampingModifier" {
                errors.push(
                    "capability_modifier_class",
                    format!(
                        "recursive damping object {} uses source class {:?}",
                        source.object_index, source.class_name
                    ),
                );
            }
            let supported_members = [
                "kP",
                "kI",
                "kD",
                "enableScalarDamping",
                "enableVectorDamping",
                "rawValue",
                "dampedValue",
                "rawVector",
                "dampedVector",
                "vecErrorSum",
                "vecPreviousError",
                "errorSum",
                "previousError",
            ];
            for binding in bindings {
                if !supported_members.contains(&binding.member_path.as_str()) {
                    errors.push(
                        "damping_modifier_binding_member",
                        format!(
                            "damping modifier binding member {:?} has no exact target field",
                            binding.member_path
                        ),
                    );
                }
            }
            validate_generator_bindings(manifest, bindings, None, errors);
            let scalar_bits = [
                *k_p_bits,
                *k_i_bits,
                *k_d_bits,
                *raw_value_bits,
                *damped_value_bits,
                *error_sum_bits,
                *previous_error_bits,
            ];
            if scalar_bits
                .into_iter()
                .chain(raw_vector_bits.iter().copied())
                .chain(damped_vector_bits.iter().copied())
                .chain(vector_error_sum_bits.iter().copied())
                .chain(vector_previous_error_bits.iter().copied())
                .any(|bits| !f32::from_bits(bits).is_finite())
            {
                errors.push(
                    "damping_modifier_numeric_contract",
                    "recursive damping modifier requires finite exact source scalar/vector fields",
                );
            }
        }
        CapabilityModifierNode::EvaluateExpression {
            source,
            bindings,
            expressions,
            ..
        } => {
            if source.class_name != "hkbEvaluateExpressionModifier" {
                errors.push(
                    "capability_modifier_class",
                    format!(
                        "recursive evaluate-expression object {} uses source class {:?}",
                        source.object_index, source.class_name
                    ),
                );
            }
            if !bindings.is_empty() {
                errors.push(
                    "evaluate_expression_bindings_unlowered",
                    "recursive hkbEvaluateExpressionModifier bindings require an exact member policy",
                );
            }
            validate_generator_anonymous_source_object(&expressions.source, owned_objects, errors);
            if expressions.source.class_name != "hkbExpressionDataArray" {
                errors.push(
                    "capability_expression_array_class",
                    format!(
                        "recursive expression-array object {} uses source class {:?}",
                        expressions.source.object_index, expressions.source.class_name
                    ),
                );
            }
            if expressions.expressions.is_empty() || expressions.expressions.len() > 64 {
                errors.push(
                    "capability_expression_count",
                    "recursive expression array must retain between 1 and 64 ordered source expressions",
                );
            }
            for expression in &expressions.expressions {
                validate_name(
                    "capability.generator.expression",
                    &expression.expression,
                    errors,
                );
                if expression.assignment_variable_index != -1
                    || expression.assignment_event_index != -1
                {
                    errors.push(
                        "capability_expression_assignment_unlowered",
                        "recursive expression assignment targets require exact named target receipts",
                    );
                }
                if expression.event_mode > 3 {
                    errors.push(
                        "capability_expression_event_mode",
                        format!(
                            "recursive expression event mode {} is outside the exact target enum",
                            expression.event_mode
                        ),
                    );
                }
                let mut variables = BTreeSet::new();
                for variable in &expression.referenced_variables {
                    if !variables.insert(variable.variable_index) {
                        errors.push(
                            "duplicate_capability_expression_variable",
                            format!(
                                "recursive expression repeats variable index {}",
                                variable.variable_index
                            ),
                        );
                    }
                    validate_generator_source_variable(
                        manifest,
                        variable.variable_index,
                        &variable.variable_name,
                        variable.source_variable_type,
                        variable.initial_word_value,
                        errors,
                    );
                    if !contains_identifier(&expression.expression, &variable.variable_name) {
                        errors.push(
                            "capability_expression_variable_boundary",
                            format!(
                                "recursive expression {:?} does not contain exact variable identifier {:?}",
                                expression.expression, variable.variable_name
                            ),
                        );
                    }
                }
                for (variable_index, variable) in manifest.core.variables.iter().enumerate() {
                    if contains_identifier(&expression.expression, &variable.name)
                        && !variables.contains(&(variable_index as i32))
                    {
                        errors.push(
                            "missing_capability_expression_variable_receipt",
                            format!(
                                "recursive expression {:?} references variable {:?} without exact source receipt",
                                expression.expression, variable.name
                            ),
                        );
                    }
                }
            }
        }
    }
}

fn contains_identifier(expression: &str, identifier: &str) -> bool {
    expression.match_indices(identifier).any(|(start, _)| {
        let end = start + identifier.len();
        let boundary = |character: char| !character.is_ascii_alphanumeric() && character != '_';
        expression[..start].chars().next_back().is_none_or(boundary)
            && expression[end..].chars().next().is_none_or(boundary)
    })
}

fn validate_generator_source_object(
    source: &CapabilityGeneratorSourceObject,
    owned_objects: &mut BTreeSet<(String, u32)>,
    errors: &mut ValidationErrors,
) {
    validate_path(
        "capability.generator.source.behavior_path",
        &source.behavior_path,
        "hkx",
        errors,
    );
    validate_name(
        "capability.generator.source.class_name",
        &source.class_name,
        errors,
    );
    validate_name("capability.generator.source.name", &source.name, errors);
    register_generator_source_object(
        &source.behavior_path,
        source.object_index,
        owned_objects,
        errors,
    );
}

fn validate_generator_anonymous_source_object(
    source: &CapabilityGeneratorAnonymousSourceObject,
    owned_objects: &mut BTreeSet<(String, u32)>,
    errors: &mut ValidationErrors,
) {
    validate_path(
        "capability.generator.source.behavior_path",
        &source.behavior_path,
        "hkx",
        errors,
    );
    validate_name(
        "capability.generator.source.class_name",
        &source.class_name,
        errors,
    );
    register_generator_source_object(
        &source.behavior_path,
        source.object_index,
        owned_objects,
        errors,
    );
}

fn register_generator_source_object(
    behavior_path: &str,
    object_index: u32,
    owned_objects: &mut BTreeSet<(String, u32)>,
    errors: &mut ValidationErrors,
) {
    let key = (behavior_path.to_ascii_lowercase(), object_index);
    if !owned_objects.insert(key) {
        errors.push(
            "duplicate_generator_source_object",
            format!(
                "recursive generator repeats source object {}#{}",
                behavior_path, object_index
            ),
        );
    }
}

fn validate_generator_bindings(
    manifest: &CreatureManifest,
    bindings: &[CapabilityGeneratorVariableBinding],
    only_member_path: Option<&str>,
    errors: &mut ValidationErrors,
) {
    let mut member_paths = BTreeSet::new();
    for binding in bindings {
        validate_name(
            "capability.generator.binding.member_path",
            &binding.member_path,
            errors,
        );
        if only_member_path.is_some_and(|member| binding.member_path != member) {
            errors.push(
                "capability_generator_binding_member",
                format!(
                    "generator binding member {:?} is not supported by this exact node",
                    binding.member_path
                ),
            );
        }
        if !member_paths.insert(binding.member_path.to_ascii_lowercase()) {
            errors.push(
                "duplicate_capability_generator_binding",
                format!("generator repeats binding member {:?}", binding.member_path),
            );
        }
        if binding.bit_index != -1 {
            errors.push(
                "capability_generator_binding_bit",
                "recursive generator bindings currently require exact whole-word bitIndex -1",
            );
        }
        match binding.binding_type {
            CapabilityVariableBindingType::Variable => validate_generator_source_variable(
                manifest,
                binding.variable_index,
                &binding.variable_name,
                binding.source_variable_type,
                binding.initial_word_value,
                errors,
            ),
            CapabilityVariableBindingType::CharacterProperty => {
                errors.push(
                    "capability_generator_property_binding_unlowered",
                    "recursive generator character-property bindings require an exact target policy",
                );
            }
        }
    }
}

fn validate_generator_source_variable(
    manifest: &CreatureManifest,
    source_index: i32,
    source_name: &str,
    source_type: CapabilitySourceVariableType,
    initial_word_value: Option<u32>,
    errors: &mut ValidationErrors,
) {
    let Some(variable_index) = usize::try_from(source_index).ok() else {
        errors.push(
            "capability_generator_variable_index",
            "recursive generator variable index cannot be negative",
        );
        return;
    };
    let Some(variable) = manifest.core.variables.get(variable_index) else {
        errors.push(
            "capability_generator_variable_index",
            format!("recursive generator variable index {source_index} is outside declarations"),
        );
        return;
    };
    if variable.name != source_name {
        errors.push(
            "capability_generator_variable_identity",
            format!(
                "recursive generator source variable {source_name:?} does not match target declaration {:?} at index {source_index}",
                variable.name
            ),
        );
    }
    let exact_type = match source_type {
        CapabilitySourceVariableType::Bool => VariableType::Bool,
        CapabilitySourceVariableType::Int32 => VariableType::Int32,
        CapabilitySourceVariableType::Real => VariableType::Real,
        CapabilitySourceVariableType::Int8 | CapabilitySourceVariableType::Int16 => {
            errors.push(
                "capability_generator_narrow_integer_unlowered",
                "source Int8/Int16 generator variables require an explicit target widening policy",
            );
            return;
        }
    };
    if variable.variable_type != exact_type {
        errors.push(
            "capability_generator_variable_type",
            format!(
                "recursive generator variable {source_name:?} requires exact source type {source_type:?}"
            ),
        );
    }
    match initial_word_value {
        Some(word) if word == variable.initial_value.word_bits() as u32 => {}
        Some(_) => errors.push(
            "capability_generator_initial_value",
            format!(
                "recursive generator variable {source_name:?} initial word differs from target declaration"
            ),
        ),
        None => errors.push(
            "capability_generator_missing_initial_value",
            format!(
                "recursive generator variable {source_name:?} lacks exact source initial-word evidence"
            ),
        ),
    }
}

fn validate_generator_event_ref(
    manifest: &CreatureManifest,
    event: &CapabilityGeneratorEventRef,
    errors: &mut ValidationErrors,
) {
    match (&event.event_name, event.source_id) {
        (None, -1) => {}
        (Some(name), source_id) if source_id >= 0 => {
            validate_name("capability.generator.event", name, errors);
            if !manifest.core.events.iter().any(|event| event.name == *name) {
                errors.push(
                    "missing_capability_generator_event",
                    format!("recursive generator references undeclared event {name:?}"),
                );
            }
        }
        _ => errors.push(
            "capability_generator_event_identity",
            "recursive generator event must pair source -1 with no name or a nonnegative source ID with an exact name",
        ),
    }
}

fn validate_generator_event_property(
    manifest: &CreatureManifest,
    event: &CapabilityGeneratorEventProperty,
    errors: &mut ValidationErrors,
) {
    validate_generator_event_ref(manifest, &event.event, errors);
    if event.payload.is_some() {
        errors.push(
            "capability_generator_event_payload_unlowered",
            "recursive generator event payload objects require an exact target lowerer",
        );
    }
}

fn validate_generator_transition_array(
    manifest: &CreatureManifest,
    transitions: &CapabilityGeneratorTransitionArray,
    state_ids: &BTreeSet<i32>,
    owned_objects: &mut BTreeSet<(String, u32)>,
    transition_effects: &mut BTreeMap<(String, u32), CapabilityGeneratorBlendingTransitionEffect>,
    errors: &mut ValidationErrors,
) {
    validate_generator_source_object(&transitions.source, owned_objects, errors);
    for transition in &transitions.transitions {
        for interval in [&transition.trigger_interval, &transition.initiate_interval] {
            validate_generator_event_ref(manifest, &interval.enter_event, errors);
            validate_generator_event_ref(manifest, &interval.exit_event, errors);
            if !f32::from_bits(interval.enter_time_bits).is_finite()
                || !f32::from_bits(interval.exit_time_bits).is_finite()
            {
                errors.push(
                    "capability_generator_transition_time",
                    "recursive transition interval times must be finite",
                );
            }
        }
        validate_generator_event_ref(manifest, &transition.event, errors);
        if let Some(effect) = &transition.transition_effect {
            let key = (
                effect.source.behavior_path.to_ascii_lowercase(),
                effect.source.object_index,
            );
            let first_receipt = match transition_effects.get(&key) {
                Some(previous) => {
                    if previous != effect {
                        errors.push(
                            "conflicting_capability_generator_transition_effect",
                            format!(
                                "transition-effect source object {}#{} has conflicting receipts",
                                effect.source.behavior_path, effect.source.object_index
                            ),
                        );
                    }
                    false
                }
                None => {
                    validate_generator_source_object(&effect.source, owned_objects, errors);
                    transition_effects.insert(key, effect.clone());
                    true
                }
            };
            if first_receipt {
                if effect.source.class_name != "hkbBlendingTransitionEffect" {
                    errors.push(
                        "capability_generator_transition_effect_class",
                        format!(
                            "recursive transition effect object {} uses source class {:?}",
                            effect.source.object_index, effect.source.class_name
                        ),
                    );
                }
                validate_generator_bindings(manifest, &effect.bindings, Some("duration"), errors);
                let duration = f32::from_bits(effect.duration_bits);
                let start_fraction = f32::from_bits(effect.to_generator_start_fraction_bits);
                if !duration.is_finite()
                    || duration < 0.0
                    || !start_fraction.is_finite()
                    || !(0.0..=1.0).contains(&start_fraction)
                {
                    errors.push(
                        "capability_generator_transition_effect_numeric",
                        "transition-effect duration must be finite/nonnegative and start fraction must be within 0..=1",
                    );
                }
                if effect.self_transition_mode > 3
                    || effect.event_mode > 3
                    || effect.end_mode > 1
                    || effect.blend_curve > 1
                {
                    errors.push(
                        "capability_generator_transition_effect_policy",
                        "transition-effect enum or blend-curve values are outside the exact FO4 target domain",
                    );
                }
            }
        }
        if transition.condition.is_some() {
            errors.push(
                "capability_generator_transition_object_unlowered",
                "recursive transition condition objects require an exact target lowerer",
            );
        }
        if !state_ids.contains(&transition.to_state_id) {
            errors.push(
                "capability_generator_transition_target",
                format!(
                    "recursive transition targets unknown state {}",
                    transition.to_state_id
                ),
            );
        }
    }
}

fn validate_generator_variable(
    manifest: &CreatureManifest,
    variable_name: &str,
    variable_type: VariableType,
    member_path: &str,
    errors: &mut ValidationErrors,
) {
    validate_name("capability.generator.variable", variable_name, errors);
    match manifest
        .core
        .variables
        .iter()
        .find(|variable| variable.name == variable_name)
    {
        Some(variable) if variable.variable_type == variable_type => {}
        Some(_) => errors.push(
            "capability_generator_variable_type",
            format!(
                "generator binding {member_path} requires {variable_name:?} to use {variable_type:?}"
            ),
        ),
        None => errors.push(
            "missing_capability_generator_variable",
            format!(
                "generator binding {member_path} references undeclared variable {variable_name:?}"
            ),
        ),
    }
}

fn validate_template_roles(graph: &CapabilityGraphManifest, errors: &mut ValidationErrors) {
    let allowed: &[CreatureClipRole] = match graph.template {
        CreatureGraphTemplate::GroundMelee => {
            require_role_count(graph, CreatureClipRole::Idle, false, errors);
            require_role_count(graph, CreatureClipRole::GroundForward, false, errors);
            require_role_count(graph, CreatureClipRole::MeleeAttack, true, errors);
            &[
                CreatureClipRole::Idle,
                CreatureClipRole::GroundForward,
                CreatureClipRole::TurnLeft90,
                CreatureClipRole::TurnRight90,
                CreatureClipRole::MeleeAttack,
            ]
        }
        CreatureGraphTemplate::PassiveGround => {
            require_role_count(graph, CreatureClipRole::Idle, false, errors);
            &[
                CreatureClipRole::Idle,
                CreatureClipRole::GroundForward,
                CreatureClipRole::TurnLeft90,
                CreatureClipRole::TurnRight90,
            ]
        }
        CreatureGraphTemplate::GroundRangedProjectile => {
            require_role_count(graph, CreatureClipRole::Idle, false, errors);
            require_role_count(graph, CreatureClipRole::GroundForward, false, errors);
            require_role_count(graph, CreatureClipRole::ProjectileAttack, true, errors);
            &[
                CreatureClipRole::Idle,
                CreatureClipRole::GroundForward,
                CreatureClipRole::TurnLeft90,
                CreatureClipRole::TurnRight90,
                CreatureClipRole::ProjectileAttack,
            ]
        }
        CreatureGraphTemplate::GroundMeleeRanged => {
            require_role_count(graph, CreatureClipRole::Idle, false, errors);
            require_role_count(graph, CreatureClipRole::GroundForward, false, errors);
            require_role_count(graph, CreatureClipRole::MeleeAttack, true, errors);
            require_role_count(graph, CreatureClipRole::ProjectileAttack, true, errors);
            &[
                CreatureClipRole::Idle,
                CreatureClipRole::GroundForward,
                CreatureClipRole::TurnLeft90,
                CreatureClipRole::TurnRight90,
                CreatureClipRole::MeleeAttack,
                CreatureClipRole::ProjectileAttack,
            ]
        }
        CreatureGraphTemplate::GroundSwim => {
            require_role_count(graph, CreatureClipRole::Idle, false, errors);
            require_role_count(graph, CreatureClipRole::GroundForward, false, errors);
            require_role_count(graph, CreatureClipRole::SwimIdle, false, errors);
            require_role_count(graph, CreatureClipRole::SwimForward, false, errors);
            &[
                CreatureClipRole::Idle,
                CreatureClipRole::GroundForward,
                CreatureClipRole::SwimIdle,
                CreatureClipRole::SwimForward,
                CreatureClipRole::TurnLeft90,
                CreatureClipRole::TurnRight90,
                CreatureClipRole::MeleeAttack,
                CreatureClipRole::ProjectileAttack,
            ]
        }
        CreatureGraphTemplate::GroundFly => {
            require_role_count(graph, CreatureClipRole::Idle, false, errors);
            require_role_count(graph, CreatureClipRole::GroundForward, false, errors);
            require_role_count(graph, CreatureClipRole::FlyIdle, false, errors);
            require_role_count(graph, CreatureClipRole::FlyForward, false, errors);
            &[
                CreatureClipRole::Idle,
                CreatureClipRole::GroundForward,
                CreatureClipRole::FlyIdle,
                CreatureClipRole::FlyForward,
                CreatureClipRole::TurnLeft90,
                CreatureClipRole::TurnRight90,
                CreatureClipRole::MeleeAttack,
                CreatureClipRole::ProjectileAttack,
            ]
        }
        CreatureGraphTemplate::Swim => {
            require_role_count(graph, CreatureClipRole::SwimIdle, false, errors);
            require_role_count(graph, CreatureClipRole::SwimForward, false, errors);
            &[
                CreatureClipRole::SwimIdle,
                CreatureClipRole::SwimForward,
                CreatureClipRole::MeleeAttack,
                CreatureClipRole::ProjectileAttack,
            ]
        }
        CreatureGraphTemplate::Fly => {
            require_role_count(graph, CreatureClipRole::FlyIdle, false, errors);
            require_role_count(graph, CreatureClipRole::FlyForward, false, errors);
            &[
                CreatureClipRole::FlyIdle,
                CreatureClipRole::FlyForward,
                CreatureClipRole::MeleeAttack,
                CreatureClipRole::ProjectileAttack,
            ]
        }
        CreatureGraphTemplate::StationaryTurret => {
            require_role_count(graph, CreatureClipRole::StationaryIdle, false, errors);
            require_role_count(graph, CreatureClipRole::ProjectileAttack, true, errors);
            &[
                CreatureClipRole::StationaryIdle,
                CreatureClipRole::ProjectileAttack,
            ]
        }
        CreatureGraphTemplate::RobotContinuousAttack => {
            require_role_count(graph, CreatureClipRole::Idle, false, errors);
            require_role_count(
                graph,
                CreatureClipRole::ContinuousAttackStart,
                false,
                errors,
            );
            require_role_count(graph, CreatureClipRole::ContinuousAttackLoop, false, errors);
            require_role_count(graph, CreatureClipRole::ContinuousAttackStop, false, errors);
            &[
                CreatureClipRole::Idle,
                CreatureClipRole::ContinuousAttackStart,
                CreatureClipRole::ContinuousAttackLoop,
                CreatureClipRole::ContinuousAttackStop,
            ]
        }
    };

    for role in &graph.roles {
        if !allowed.contains(&role.role) {
            errors.push(
                "capability_role_not_allowed",
                format!(
                    "role {:?} is not valid for template {:?}",
                    role.role, graph.template
                ),
            );
        }
    }
}

fn template_allows_attack_role(template: CreatureGraphTemplate, role: CreatureClipRole) -> bool {
    match role {
        CreatureClipRole::MeleeAttack => matches!(
            template,
            CreatureGraphTemplate::GroundMelee
                | CreatureGraphTemplate::GroundMeleeRanged
                | CreatureGraphTemplate::GroundSwim
                | CreatureGraphTemplate::GroundFly
                | CreatureGraphTemplate::Swim
                | CreatureGraphTemplate::Fly
        ),
        CreatureClipRole::ProjectileAttack => matches!(
            template,
            CreatureGraphTemplate::GroundRangedProjectile
                | CreatureGraphTemplate::GroundMeleeRanged
                | CreatureGraphTemplate::GroundSwim
                | CreatureGraphTemplate::GroundFly
                | CreatureGraphTemplate::Swim
                | CreatureGraphTemplate::Fly
                | CreatureGraphTemplate::StationaryTurret
        ),
        _ => false,
    }
}

fn primary_idle_role(template: CreatureGraphTemplate) -> CreatureClipRole {
    match template {
        CreatureGraphTemplate::GroundMelee
        | CreatureGraphTemplate::PassiveGround
        | CreatureGraphTemplate::GroundRangedProjectile
        | CreatureGraphTemplate::GroundMeleeRanged
        | CreatureGraphTemplate::GroundSwim
        | CreatureGraphTemplate::GroundFly
        | CreatureGraphTemplate::RobotContinuousAttack => CreatureClipRole::Idle,
        CreatureGraphTemplate::Swim => CreatureClipRole::SwimIdle,
        CreatureGraphTemplate::Fly => CreatureClipRole::FlyIdle,
        CreatureGraphTemplate::StationaryTurret => CreatureClipRole::StationaryIdle,
    }
}

fn validate_stationary_turret_aim(manifest: &CreatureManifest, errors: &mut ValidationErrors) {
    for (name, variable_type) in [
        ("AimHeadingMaxCCW", VariableType::Real),
        ("AimHeadingMaxCW", VariableType::Real),
        ("fDirectAtHeadingSavedGain", VariableType::Real),
        ("bAimActive", VariableType::Bool),
        ("AimHeadingCurrent", VariableType::Real),
        ("AimPitchCurrent", VariableType::Real),
        ("fAimOnGain", VariableType::Real),
        ("camerafromx", VariableType::Real),
        ("camerafromy", VariableType::Real),
        ("camerafromz", VariableType::Real),
    ] {
        match manifest
            .core
            .variables
            .iter()
            .find(|variable| variable.name == name)
        {
            None => errors.push(
                "missing_turret_aim_declaration",
                format!("stationary turret core graph must declare {name:?}"),
            ),
            Some(variable) if variable.variable_type != variable_type => errors.push(
                "turret_aim_declaration_type",
                format!("stationary turret variable {name:?} must use {variable_type:?}"),
            ),
            Some(variable)
                if name == "bAimActive" && variable.initial_value != VariableValue::Bool(false) =>
            {
                errors.push(
                    "turret_aim_declaration_value",
                    "stationary turret bAimActive must initially be false",
                )
            }
            Some(_) => {}
        }
    }

    for name in ["DirectAtHeadingSourceBoneIndex", "DirectAtHeadingBoneIndex"] {
        match manifest
            .core
            .character_properties
            .iter()
            .find(|property| property.name == name)
        {
            None => errors.push(
                "missing_turret_aim_declaration",
                format!("stationary turret core graph must declare {name:?}"),
            ),
            Some(property) if property.variable_type != VariableType::Int32 => errors.push(
                "turret_aim_declaration_type",
                format!("stationary turret property {name:?} must use Int32"),
            ),
            Some(property) => match property.initial_value {
                VariableValue::Int32(index)
                    if index >= 0
                        && usize::try_from(index)
                            .is_ok_and(|index| index < manifest.animation_skeleton.bones.len()) => {
                }
                _ => errors.push(
                    "turret_aim_bone_index",
                    format!(
                        "stationary turret property {name:?} must name an in-range source-rig bone"
                    ),
                ),
            },
        }
    }
}

fn require_role_count(
    graph: &CapabilityGraphManifest,
    role: CreatureClipRole,
    allow_multiple: bool,
    errors: &mut ValidationErrors,
) {
    let role_count = graph
        .roles
        .iter()
        .filter(|candidate| candidate.role == role)
        .count();
    let candidate_bound = graph
        .candidate_attack_bindings
        .iter()
        .any(|candidate| candidate.role == role);
    let count = role_count.max(usize::from(candidate_bound));
    if count == 0 || (!allow_multiple && count != 1) {
        let cardinality = if allow_multiple {
            "at least one"
        } else {
            "exactly one"
        };
        errors.push(
            "missing_capability_role",
            format!(
                "template {:?} requires {cardinality} {role:?} role",
                graph.template
            ),
        );
    }
}

fn validate_identifier(label: &str, value: &str, errors: &mut ValidationErrors) {
    let valid = !value.is_empty()
        && value == value.trim()
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_');
    if !valid {
        errors.push(
            "invalid_identifier",
            format!("{label} must be a trimmed ASCII identifier, got {value:?}"),
        );
    }
}

fn validate_name(label: &str, value: &str, errors: &mut ValidationErrors) {
    if value.is_empty() || value != value.trim() || value.chars().any(char::is_control) {
        errors.push(
            "invalid_name",
            format!("{label} must be non-empty, trimmed, and contain no controls"),
        );
    }
}

fn validate_bone_name(label: &str, value: &str, errors: &mut ValidationErrors) {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        errors.push(
            "invalid_name",
            format!("{label} must contain a non-whitespace name and no controls"),
        );
    }
}

fn validate_path(label: &str, path: &str, extension: &str, errors: &mut ValidationErrors) {
    let valid_components = path.split('\\').all(|component| {
        !component.is_empty()
            && component != "."
            && component != ".."
            && component.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, ' ' | '_' | '-' | '.')
            })
    });
    if path.is_empty()
        || path != path.trim()
        || path.contains('/')
        || path.starts_with('\\')
        || path.contains(':')
        || !valid_components
    {
        errors.push(
            "noncanonical_path",
            format!(
                "{label} must be a root-relative canonical Windows path with safe components, got {path:?}"
            ),
        );
    }
    if !path.to_ascii_lowercase().ends_with(extension) {
        errors.push(
            "wrong_extension",
            format!("{label} must end in {extension}, got {path:?}"),
        );
    }
}

fn validate_capsule(capsule: Capsule, errors: &mut ValidationErrors) {
    if !capsule.height.is_finite() || capsule.height <= 0.0 {
        errors.push(
            "invalid_capsule",
            "capsule height must be finite and positive",
        );
    }
    if !capsule.radius.is_finite() || capsule.radius <= 0.0 {
        errors.push(
            "invalid_capsule",
            "capsule radius must be finite and positive",
        );
    }
    if capsule.height.is_finite()
        && capsule.radius.is_finite()
        && capsule.height < capsule.radius * 2.0
    {
        errors.push(
            "invalid_capsule",
            "capsule height includes both caps and cannot be less than twice its radius",
        );
    }
}

fn validate_skeleton(skeleton: &SkeletonDecl, errors: &mut ValidationErrors) {
    validate_name(
        "animation_skeleton.runtime_name",
        &skeleton.runtime_name,
        errors,
    );
    if skeleton.bones.is_empty() {
        errors.push(
            "empty_skeleton",
            "animation skeleton must declare at least one bone",
        );
        return;
    }

    let mut names = BTreeSet::new();
    let mut root_count = 0;
    for (index, bone) in skeleton.bones.iter().enumerate() {
        validate_bone_name(&format!("bone[{index}].name"), &bone.name, errors);
        if !names.insert(bone.name.to_ascii_lowercase()) {
            errors.push(
                "duplicate_bone",
                format!(
                    "animation skeleton contains duplicate bone name {:?}",
                    bone.name
                ),
            );
        }
        match bone.parent_index {
            None => root_count += 1,
            Some(parent) if parent >= index => errors.push(
                "invalid_bone_parent",
                format!(
                    "bone {:?} parent index {parent} must precede child index {index}",
                    bone.name
                ),
            ),
            Some(_) => {}
        }
    }
    if skeleton.bones[0].parent_index.is_some() || root_count == 0 {
        errors.push(
            "invalid_skeleton_root",
            "animation skeleton must have a root at bone index 0",
        );
    }

    let mut float_slots = BTreeSet::new();
    for (index, slot) in skeleton.float_slots.iter().enumerate() {
        validate_name(&format!("float_slot[{index}]"), slot, errors);
        if !float_slots.insert(slot.to_ascii_lowercase()) {
            errors.push(
                "duplicate_float_slot",
                format!("animation skeleton contains duplicate float slot {slot:?}"),
            );
        }
    }
}

fn validate_clips(manifest: &CreatureManifest, errors: &mut ValidationErrors) {
    if manifest.clips.is_empty() {
        errors.push("missing_clip", "at least one animation clip is required");
        return;
    }

    let skeleton_path = manifest.animation_skeleton.path.to_ascii_lowercase();
    let mut names = BTreeSet::new();
    let mut idle_matches = 0;
    for clip in &manifest.clips {
        validate_identifier("clip.name", &clip.name, errors);
        validate_path("clip.path", &clip.path, ".hkx", errors);
        validate_name(
            "clip.binding.original_skeleton_name",
            &clip.binding.original_skeleton_name,
            errors,
        );
        if !names.insert(clip.name.to_ascii_lowercase()) {
            errors.push(
                "duplicate_clip",
                format!("duplicate clip name {:?}", clip.name),
            );
        }
        if clip.name == manifest.idle_clip {
            idle_matches += 1;
        }
        if clip.binding.skeleton_path.to_ascii_lowercase() != skeleton_path {
            errors.push(
                "clip_rig_mismatch",
                format!(
                    "clip {:?} binds to {:?}, expected source rig {:?}",
                    clip.name, clip.binding.skeleton_path, manifest.animation_skeleton.path
                ),
            );
        }
        if clip.binding.declared_transform_tracks
            != clip.binding.transform_track_to_bone_indices.len()
        {
            errors.push(
                "clip_track_count",
                format!(
                    "clip {:?} declares {} transform tracks but maps {}",
                    clip.name,
                    clip.binding.declared_transform_tracks,
                    clip.binding.transform_track_to_bone_indices.len()
                ),
            );
        }
        if clip.binding.transform_track_to_bone_indices.is_empty() {
            errors.push(
                "empty_clip_binding",
                format!("clip {:?} has no transform-to-bone mappings", clip.name),
            );
        }
        let mut indices = BTreeSet::new();
        for &bone_index in &clip.binding.transform_track_to_bone_indices {
            if bone_index >= manifest.animation_skeleton.bones.len() {
                errors.push(
                    "clip_bone_index",
                    format!(
                        "clip {:?} maps a transform track to out-of-range bone index {bone_index}",
                        clip.name
                    ),
                );
            }
            if !indices.insert(bone_index) {
                errors.push(
                    "duplicate_clip_bone",
                    format!(
                        "clip {:?} maps more than one transform track to bone index {bone_index}",
                        clip.name
                    ),
                );
            }
        }
        if clip.binding.declared_float_tracks
            != clip.binding.float_track_to_float_slot_indices.len()
        {
            errors.push(
                "clip_float_track_count",
                format!(
                    "clip {:?} declares {} float tracks but maps {}",
                    clip.name,
                    clip.binding.declared_float_tracks,
                    clip.binding.float_track_to_float_slot_indices.len()
                ),
            );
        }
        let mut float_indices = BTreeSet::new();
        for &slot_index in &clip.binding.float_track_to_float_slot_indices {
            if slot_index >= manifest.animation_skeleton.float_slots.len() {
                errors.push(
                    "clip_float_slot_index",
                    format!(
                        "clip {:?} maps a float track to out-of-range slot index {slot_index}",
                        clip.name
                    ),
                );
            }
            if !float_indices.insert(slot_index) {
                errors.push(
                    "duplicate_clip_float_slot",
                    format!(
                        "clip {:?} maps more than one float track to slot index {slot_index}",
                        clip.name
                    ),
                );
            }
        }
    }

    if idle_matches != 1 {
        errors.push(
            "idle_clip",
            format!(
                "idle_clip {:?} must name exactly one declared clip",
                manifest.idle_clip
            ),
        );
    }
}

fn validate_graph(label: &str, graph: &GraphDeclarations, errors: &mut ValidationErrors) {
    let mut event_names = BTreeSet::new();
    for event in &graph.events {
        validate_name(&format!("{label}.event"), &event.name, errors);
        if !event_names.insert(event.name.to_ascii_lowercase()) {
            errors.push(
                "duplicate_event",
                format!("{label} graph contains duplicate event {:?}", event.name),
            );
        }
        if event.usage == EventUsage::MeleeAttack
            && (!event.name.starts_with("melee") || event.name.len() == "melee".len())
        {
            errors.push(
                "melee_event_name",
                format!(
                    "melee attack event {:?} must start with lowercase 'melee' and include a suffix",
                    event.name
                ),
            );
        }
    }

    validate_typed_names(label, "variable", &graph.variables, errors, |item| {
        (&item.name, item.variable_type, item.initial_value)
    });
    validate_typed_names(
        label,
        "character property",
        &graph.character_properties,
        errors,
        |item| (&item.name, item.variable_type, item.initial_value),
    );
}

fn validate_typed_names<T, F>(
    graph_label: &str,
    item_label: &str,
    items: &[T],
    errors: &mut ValidationErrors,
    fields: F,
) where
    F: Fn(&T) -> (&String, VariableType, VariableValue),
{
    let mut names = BTreeSet::new();
    for item in items {
        let (name, variable_type, initial_value) = fields(item);
        validate_name(&format!("{graph_label}.{item_label}"), name, errors);
        if !names.insert(name.to_ascii_lowercase()) {
            errors.push(
                "duplicate_declaration",
                format!("{graph_label} graph contains duplicate {item_label} {name:?}"),
            );
        }
        if !initial_value.matches(variable_type) {
            errors.push(
                "declaration_type",
                format!(
                    "{graph_label} {item_label} {name:?} has a default incompatible with {variable_type:?}"
                ),
            );
        }
        if !initial_value.is_finite() {
            errors.push(
                "nonfinite_default",
                format!("{graph_label} {item_label} {name:?} has a non-finite default"),
            );
        }
    }
}

fn validate_root_union(manifest: &CreatureManifest, errors: &mut ValidationErrors) {
    let root_events: BTreeMap<&str, &EventDecl> = manifest
        .root
        .events
        .iter()
        .map(|event| (event.name.as_str(), event))
        .collect();
    for event in &manifest.core.events {
        match root_events.get(event.name.as_str()) {
            Some(root_event) if *root_event == event => {}
            Some(_) => errors.push(
                "root_union_mismatch",
                format!(
                    "root event {:?} does not match the core declaration",
                    event.name
                ),
            ),
            None => errors.push(
                "root_union_missing",
                format!("root graph does not declare core event {:?}", event.name),
            ),
        }
    }

    validate_typed_union(
        "variable",
        &manifest.root.variables,
        &manifest.core.variables,
        errors,
        |item| (&item.name, item.variable_type),
    );
    validate_typed_union(
        "character property",
        &manifest.root.character_properties,
        &manifest.core.character_properties,
        errors,
        |item| (&item.name, item.variable_type),
    );
}

fn validate_typed_union<T, F>(
    label: &str,
    root: &[T],
    child: &[T],
    errors: &mut ValidationErrors,
    fields: F,
) where
    F: Fn(&T) -> (&String, VariableType),
{
    let root_types: BTreeMap<&str, VariableType> = root
        .iter()
        .map(|item| {
            let (name, variable_type) = fields(item);
            (name.as_str(), variable_type)
        })
        .collect();
    for item in child {
        let (name, variable_type) = fields(item);
        match root_types.get(name.as_str()) {
            Some(root_type) if *root_type == variable_type => {}
            Some(root_type) => errors.push(
                "root_union_mismatch",
                format!(
                    "root {label} {name:?} has type {root_type:?}, core uses {variable_type:?}"
                ),
            ),
            None => errors.push(
                "root_union_missing",
                format!("root graph does not declare core {label} {name:?}"),
            ),
        }
    }
}
