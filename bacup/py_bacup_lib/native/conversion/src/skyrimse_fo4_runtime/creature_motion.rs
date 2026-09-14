use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use havok_native::hkx::model::{HkxFile, HkxMember, HkxObject};
use havok_native::hkx::types::HkxValue;

use crate::ids::FormKey;
use crate::source_rig::{
    CapabilityCandidateAttackBinding, CapabilityClipRole, CapabilityGeneratorAnonymousSourceObject,
    CapabilityGeneratorBlenderChild, CapabilityGeneratorBlendingTransitionEffect,
    CapabilityGeneratorEventProperty, CapabilityGeneratorEventRef, CapabilityGeneratorExpression,
    CapabilityGeneratorExpressionArray, CapabilityGeneratorExpressionVariableReference,
    CapabilityGeneratorInterval, CapabilityGeneratorNode, CapabilityGeneratorSourceObject,
    CapabilityGeneratorState, CapabilityGeneratorTransition, CapabilityGeneratorTransitionArray,
    CapabilityGeneratorVariableBinding, CapabilityGraphManifest, CapabilityModifierNode,
    CapabilityRoleGenerator, CapabilitySourceVariableType, CapabilityVariableBindingType,
    ClipMotionPolicy, CreatureClipRole, CreatureGraphTemplate, EventDecl, EventUsage, MotionAttack,
    MotionSet, OverlayClipRole, SourceCreatureIdentity, VariableDecl, VariableType, VariableValue,
    deterministic_standard_event,
};

use super::creature_catalog::CreatureCorpusPlan;

const SKYRIM_CONTENTS_VERSION: &str = "hk_2010.2.0-r1";
const SKYRIM_CHARACTER_DATA_SIGNATURE: u32 = 0x300d_6808;
const SKYRIM_CHARACTER_CONTROLLER_INFO_CLASS: &str = "hkbCharacterDataCharacterControllerInfo";
const SKYRIM_CHARACTER_CONTROLLER_INFO_SIGNATURE: u32 = 0xa0f4_15bf;
const HKB_CHARACTER_CONTROLLER_SETUP_SIGNATURE: u32 = 0xaf5f_7339;

const RACE_PROJECTS: [(&str, &str); 46] = [
    ("chicken", "ambient/chicken/chickenproject.hkx"),
    ("hare", "ambient/hare/hareproject.hkx"),
    ("atronach_flame", "atronachflame/atronachflame.hkx"),
    ("atronach_frost", "atronachfrost/atronachfrostproject.hkx"),
    ("atronach_storm", "atronachstorm/atronachstormproject.hkx"),
    ("bear", "bear/bearproject.hkx"),
    ("dog", "canine/dogproject.hkx"),
    ("wolf", "canine/wolfproject.hkx"),
    ("chaurus", "chaurus/chaurusproject.hkx"),
    ("chaurus_flyer", "dlc01/chaurusflyer/chaurusflyer.hkx"),
    ("cow", "cow/highlandcowproject.hkx"),
    ("deer", "deer/deerproject.hkx"),
    (
        "vampire_brute",
        "dlc01/vampirebrute/vampirebruteproject.hkx",
    ),
    (
        "benthic_lurker",
        "dlc02/benthiclurker/benthiclurkerproject.hkx",
    ),
    ("boar", "dlc02/boarriekling/boarproject.hkx"),
    (
        "dwarven_ballista",
        "dlc02/dwarvenballistacenturion/ballistacenturion.hkx",
    ),
    ("hm_daedra", "dlc02/hmdaedra/hmdaedra.hkx"),
    ("netch", "dlc02/netch/netchproject.hkx"),
    ("riekling", "dlc02/riekling/rieklingproject.hkx"),
    ("scrib", "dlc02/scrib/scribproject.hkx"),
    ("dragon", "dragon/dragonproject.hkx"),
    ("dragon_priest", "dragonpriest/dragon_priest.hkx"),
    ("draugr", "draugr/draugrproject.hkx"),
    ("draugr_skeleton", "draugr/draugrskeletonproject.hkx"),
    (
        "dwarven_sphere",
        "dwarvenspherecenturion/spherecenturion.hkx",
    ),
    (
        "dwarven_spider",
        "dwarvenspider/dwarvenspidercenturionproject.hkx",
    ),
    ("dwarven_steam", "dwarvensteamcenturion/steamproject.hkx"),
    ("falmer", "falmer/falmerproject.hkx"),
    (
        "frostbite_spider",
        "frostbitespider/frostbitespiderproject.hkx",
    ),
    ("giant", "giant/giantproject.hkx"),
    ("goat", "goat/goatproject.hkx"),
    ("hagraven", "hagraven/hagravenproject.hkx"),
    ("horker", "horker/horkerproject.hkx"),
    ("horse", "horse/horseproject.hkx"),
    ("ice_wraith", "icewraith/icewraithproject.hkx"),
    ("mammoth", "mammoth/mammothproject.hkx"),
    ("mudcrab", "mudcrab/mudcrabproject.hkx"),
    ("sabre_cat", "sabrecat/sabrecatproject.hkx"),
    ("skeever", "skeever/skeeverproject.hkx"),
    ("slaughterfish", "slaughterfish/slaughterfishproject.hkx"),
    ("spriggan", "spriggan/spriggan.hkx"),
    ("troll", "troll/trollproject.hkx"),
    ("vampire_lord", "vampirelord/vampirelord.hkx"),
    ("werewolf", "werewolfbeast/werewolfbeastproject.hkx"),
    ("wisp", "wisp/wispproject.hkx"),
    ("witchlight", "witchlight/witchlightproject.hkx"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkyrimCreatureMotionRole {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkyrimMotionEvidenceSource {
    RaceAttackEvent,
    BehaviorTransition,
    BehaviorGenerator,
    AnimationData,
    ClipAnnotation,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkyrimMotionSourceLocator {
    RaceAttackEvent {
        project_path: String,
        event: String,
    },
    BehaviorTransition {
        behavior_path: String,
        animation_name: String,
        event: String,
    },
    BehaviorGenerator {
        behavior_path: String,
        animation_name: String,
        node: String,
    },
    AnimationDataSequence {
        source_path: String,
        sequence_name: String,
        animation_index: u32,
    },
    AnimationDataEvent {
        source_path: String,
        sequence_name: String,
        animation_index: u32,
        event: String,
        time_bits: u32,
    },
    ClipAnnotation {
        clip_path: String,
        event: String,
        time_bits: u32,
    },
    AnimationBinding {
        clip_path: String,
        member: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SkyrimMotionTriggerEvidence {
    pub event: String,
    pub locator: SkyrimMotionSourceLocator,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimMotionRoleEvidence {
    pub source: SkyrimMotionEvidenceSource,
    pub detail: String,
    pub time: Option<f32>,
    pub locator: SkyrimMotionSourceLocator,
    pub trigger: Option<SkyrimMotionTriggerEvidence>,
    pub root_motion_locator: Option<SkyrimRootMotionSourceLocator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyrimRootMotionSource {
    HavokReferenceFrame,
    BoundAnims,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkyrimRootMotion {
    Unknown,
    Stationary {
        source: SkyrimRootMotionSource,
        sample_count: usize,
    },
    Sampled {
        source: SkyrimRootMotionSource,
        sample_count: usize,
        translation_delta: [f32; 3],
        rotation_start: Option<[f32; 4]>,
        rotation_end: Option<[f32; 4]>,
    },
    Unsupported {
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SkyrimBoundMotionSamples {
    pub duration: f32,
    pub translations: Vec<SkyrimTimedMotionSample>,
    pub rotations: Vec<SkyrimTimedMotionSample>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SkyrimTimedMotionSample {
    pub time: f32,
    pub value: [f32; 4],
}

impl SkyrimRootMotion {
    fn policy(&self) -> ClipMotionPolicy {
        match self {
            Self::Sampled { sample_count, .. } => ClipMotionPolicy {
                animation_driven: true,
                extracted_planar_reference_frames: *sample_count,
            },
            _ => ClipMotionPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimRaceAttackEvidence {
    pub event: String,
    pub has_attack_spell: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimRaceAttackSourceEvidence {
    pub source_race: FormKey,
    pub source_plugin: String,
    pub attack: SkyrimRaceAttackEvidence,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimBehaviorClipEvidence {
    pub behavior_path: String,
    pub animation_name: String,
    pub generator_name: Option<String>,
    pub state_names: Vec<String>,
    pub initial_state_names: Vec<String>,
    pub topology_names: Vec<String>,
    pub incoming_events: Vec<String>,
    pub ancestor_entry_events: Vec<SkyrimBehaviorEntryEventEvidence>,
    pub emitted_events: Vec<String>,
    pub looping: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorObjectEvidence {
    pub object_index: usize,
    pub class_name: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorBlendingTransitionEffectEvidence {
    pub source: SkyrimBehaviorObjectEvidence,
    pub self_transition_mode: Option<i32>,
    pub event_mode: Option<i32>,
    pub duration_bits: Option<u32>,
    pub to_generator_start_time_fraction_bits: Option<u32>,
    pub flags: Option<i32>,
    pub end_mode: Option<i32>,
    pub blend_curve: Option<i32>,
    pub alignment_bone: Option<i32>,
    pub variable_bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorVariableBindingEvidence {
    pub member_path: String,
    pub variable_index: usize,
    pub variable_name: Option<String>,
    pub variable_type: Option<i32>,
    pub initial_word_value: Option<u32>,
    pub bit_index: i32,
    pub binding_type: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SkyrimBehaviorVariableEvidence {
    pub behavior_path: String,
    pub variable_index: usize,
    pub variable_name: Option<String>,
    pub variable_type: Option<i32>,
    pub initial_word_value: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorTerminalClipEvidence {
    pub object_index: usize,
    pub animation_name: String,
    pub resolved_clip_path: Option<String>,
    pub generator_name: Option<String>,
    pub topology_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimBehaviorGeneratorNodeEvidence {
    Missing {
        object_index: usize,
    },
    Clip {
        source: SkyrimBehaviorObjectEvidence,
        animation_name: String,
        resolved_clip_path: Option<String>,
    },
    ManualSelector {
        source: SkyrimBehaviorObjectEvidence,
        selected_generator_index: Option<i32>,
        index_selector: Option<SkyrimBehaviorObjectEvidence>,
        selected_index_can_change_after_activate: Option<bool>,
        transition_effect: Option<SkyrimBehaviorObjectEvidence>,
        blending_transition_effect: Option<SkyrimBehaviorBlendingTransitionEffectEvidence>,
        variable_bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
        children: Vec<SkyrimBehaviorGeneratorNodeEvidence>,
    },
    Blender {
        source: SkyrimBehaviorObjectEvidence,
        reference_pose_weight_threshold_bits: Option<u32>,
        blend_parameter_bits: Option<u32>,
        min_cyclic_blend_parameter_bits: Option<u32>,
        max_cyclic_blend_parameter_bits: Option<u32>,
        index_of_sync_master_child: Option<i32>,
        flags: Option<i32>,
        subtract_last_child: Option<bool>,
        variable_bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
        children: Vec<SkyrimBehaviorBlenderChildNodeEvidence>,
    },
    StateMachine(SkyrimBehaviorStateMachineEvidence),
    ModifierGenerator {
        source: SkyrimBehaviorObjectEvidence,
        user_data: Option<u64>,
        variable_bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
        modifier: Option<Box<SkyrimBehaviorModifierNodeEvidence>>,
        generator: Option<Box<SkyrimBehaviorGeneratorNodeEvidence>>,
    },
    Unsupported {
        source: SkyrimBehaviorObjectEvidence,
        child_members: Vec<SkyrimBehaviorNamedGeneratorChildrenEvidence>,
    },
    Cycle {
        source: SkyrimBehaviorObjectEvidence,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimBehaviorModifierNodeEvidence {
    Missing {
        object_index: usize,
    },
    ModifierList {
        source: SkyrimBehaviorObjectEvidence,
        user_data: Option<u64>,
        enable: Option<bool>,
        variable_bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
        modifiers: Vec<SkyrimBehaviorModifierNodeEvidence>,
    },
    Damping {
        source: SkyrimBehaviorObjectEvidence,
        user_data: Option<u64>,
        enable: Option<bool>,
        variable_bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
        k_p_bits: Option<u32>,
        k_i_bits: Option<u32>,
        k_d_bits: Option<u32>,
        enable_scalar_damping: Option<bool>,
        enable_vector_damping: Option<bool>,
        raw_value_bits: Option<u32>,
        damped_value_bits: Option<u32>,
        raw_vector_bits: Option<[u32; 4]>,
        damped_vector_bits: Option<[u32; 4]>,
        vector_error_sum_bits: Option<[u32; 4]>,
        vector_previous_error_bits: Option<[u32; 4]>,
        error_sum_bits: Option<u32>,
        previous_error_bits: Option<u32>,
    },
    EvaluateExpression {
        source: SkyrimBehaviorObjectEvidence,
        user_data: Option<u64>,
        enable: Option<bool>,
        variable_bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
        expressions: Option<SkyrimBehaviorExpressionDataArrayEvidence>,
    },
    Unsupported {
        source: SkyrimBehaviorObjectEvidence,
        user_data: Option<u64>,
        enable: Option<bool>,
        variable_bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
    },
    Cycle {
        source: SkyrimBehaviorObjectEvidence,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorExpressionDataArrayEvidence {
    pub source: SkyrimBehaviorObjectEvidence,
    pub expressions: Vec<SkyrimBehaviorExpressionEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorExpressionEvidence {
    pub ordinal: usize,
    pub expression: Option<String>,
    pub referenced_variables: Vec<SkyrimBehaviorExpressionVariableReferenceEvidence>,
    pub assignment_variable_index: Option<i32>,
    pub assignment_event_index: Option<i32>,
    pub event_mode: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorExpressionVariableReferenceEvidence {
    pub variable_index: usize,
    pub variable_name: String,
    pub variable_type: Option<i32>,
    pub initial_word_value: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorNamedGeneratorChildrenEvidence {
    pub member_name: String,
    pub children: Vec<SkyrimBehaviorGeneratorNodeEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorBlenderChildNodeEvidence {
    pub ordinal: usize,
    pub source: SkyrimBehaviorObjectEvidence,
    pub generator: SkyrimBehaviorGeneratorNodeEvidence,
    pub weight_bits: Option<u32>,
    pub world_from_model_weight_bits: Option<u32>,
    pub bone_weights: Option<SkyrimBehaviorObjectEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorEventPropertyEvidence {
    pub event_id: Option<i32>,
    pub event_name: Option<String>,
    pub payload: Option<SkyrimBehaviorObjectEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorTransitionIntervalEvidence {
    pub enter_event_id: Option<i32>,
    pub enter_event_name: Option<String>,
    pub exit_event_id: Option<i32>,
    pub exit_event_name: Option<String>,
    pub enter_time_bits: Option<u32>,
    pub exit_time_bits: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorTransitionEvidence {
    pub ordinal: usize,
    pub trigger_interval: Option<SkyrimBehaviorTransitionIntervalEvidence>,
    pub initiate_interval: Option<SkyrimBehaviorTransitionIntervalEvidence>,
    pub transition_effect: Option<SkyrimBehaviorObjectEvidence>,
    pub blending_transition_effect: Option<SkyrimBehaviorBlendingTransitionEffectEvidence>,
    pub condition: Option<SkyrimBehaviorObjectEvidence>,
    pub event_id: Option<i32>,
    pub event_name: Option<String>,
    pub to_state_id: Option<i32>,
    pub from_nested_state_id: Option<i32>,
    pub to_nested_state_id: Option<i32>,
    pub priority: Option<i32>,
    pub flags: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorTransitionArrayEvidence {
    pub source: SkyrimBehaviorObjectEvidence,
    pub transitions: Vec<SkyrimBehaviorTransitionEvidence>,
    pub has_eventless_transitions: Option<bool>,
    pub has_time_bounded_transitions: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorStateEvidence {
    pub ordinal: usize,
    pub source: SkyrimBehaviorObjectEvidence,
    pub listeners: Vec<SkyrimBehaviorObjectEvidence>,
    pub enter_notify_events: Vec<SkyrimBehaviorEventPropertyEvidence>,
    pub exit_notify_events: Vec<SkyrimBehaviorEventPropertyEvidence>,
    pub transitions: Option<SkyrimBehaviorTransitionArrayEvidence>,
    pub generator: Option<SkyrimBehaviorGeneratorNodeEvidence>,
    pub name: Option<String>,
    pub state_id: Option<i32>,
    pub probability_bits: Option<u32>,
    pub enable: Option<bool>,
    pub has_eventless_transitions: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorStateMachineEvidence {
    pub source: SkyrimBehaviorObjectEvidence,
    pub variable_bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
    pub event_to_send_when_state_or_transition_changes: Option<SkyrimBehaviorEventPropertyEvidence>,
    pub start_state_id_selector: Option<SkyrimBehaviorObjectEvidence>,
    pub start_state_id: Option<i32>,
    pub return_to_previous_state_event_id: Option<i32>,
    pub return_to_previous_state_event_name: Option<String>,
    pub random_transition_event_id: Option<i32>,
    pub random_transition_event_name: Option<String>,
    pub transition_to_next_higher_state_event_id: Option<i32>,
    pub transition_to_next_higher_state_event_name: Option<String>,
    pub transition_to_next_lower_state_event_id: Option<i32>,
    pub transition_to_next_lower_state_event_name: Option<String>,
    pub sync_variable_index: Option<i32>,
    pub wrap_around_state_id: Option<bool>,
    pub max_simultaneous_transitions: Option<i32>,
    pub start_state_mode: Option<i32>,
    pub self_transition_mode: Option<i32>,
    pub states: Vec<SkyrimBehaviorStateEvidence>,
    pub wildcard_transitions: Option<SkyrimBehaviorTransitionArrayEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBehaviorGroupChildEvidence {
    pub ordinal: usize,
    pub generator: SkyrimBehaviorObjectEvidence,
    pub generator_tree: SkyrimBehaviorGeneratorNodeEvidence,
    pub terminal_clips: Vec<SkyrimBehaviorTerminalClipEvidence>,
    pub weight_bits: Option<u32>,
    pub world_from_model_weight_bits: Option<u32>,
    pub bone_weights: Option<SkyrimBehaviorObjectEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimManualSelectorEvidence {
    pub behavior_path: String,
    pub selector: SkyrimBehaviorObjectEvidence,
    pub selected_generator_index: i32,
    pub index_selector: Option<SkyrimBehaviorObjectEvidence>,
    pub selected_index_can_change_after_activate: bool,
    pub transition_effect: Option<SkyrimBehaviorObjectEvidence>,
    pub blending_transition_effect: Option<SkyrimBehaviorBlendingTransitionEffectEvidence>,
    pub variable_bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
    pub children: Vec<SkyrimBehaviorGroupChildEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimBlenderEvidence {
    pub behavior_path: String,
    pub blender: SkyrimBehaviorObjectEvidence,
    pub reference_pose_weight_threshold_bits: Option<u32>,
    pub blend_parameter_bits: Option<u32>,
    pub min_cyclic_blend_parameter_bits: Option<u32>,
    pub max_cyclic_blend_parameter_bits: Option<u32>,
    pub index_of_sync_master_child: i32,
    pub flags: i32,
    pub subtract_last_child: bool,
    pub variable_bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
    pub children: Vec<SkyrimBehaviorGroupChildEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimStateMachineGeneratorGroupEvidence {
    pub behavior_path: String,
    pub machine: SkyrimBehaviorStateMachineEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimBehaviorGeneratorGroupEvidence {
    ManualSelector(SkyrimManualSelectorEvidence),
    Blender(SkyrimBlenderEvidence),
    StateMachine(SkyrimStateMachineGeneratorGroupEvidence),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SkyrimBehaviorEntryEventEvidence {
    pub behavior_path: String,
    pub event: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimAnimationDataEvidence {
    pub source_path: String,
    pub root_motion_source_path: Option<String>,
    pub project_stem: String,
    pub sequence_name: String,
    pub animation_index: u32,
    pub playback_speed: f32,
    pub crop_start_local_time: f32,
    pub crop_end_local_time: f32,
    pub events: Vec<SkyrimTimedEvent>,
    pub root_motion: SkyrimRootMotion,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimTimedEvent {
    pub name: String,
    pub time: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimCreatureClipEvidence {
    pub clip_path: String,
    pub original_skeleton_name: Option<String>,
    pub blend_hint: i32,
    pub behavior: Vec<SkyrimBehaviorClipEvidence>,
    pub animation_data: Vec<SkyrimAnimationDataEvidence>,
    pub annotations: Vec<SkyrimTimedEvent>,
    pub root_motion: SkyrimRootMotion,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimControllerCapsuleEvidence {
    pub total_height: f32,
    pub radius: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimControllerModelEvidence {
    pub up_ms: [f32; 4],
    pub forward_ms: [f32; 4],
    pub right_ms: [f32; 4],
    pub scale: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimControllerLayoutEvidence {
    LegacyCharacterControllerInfo {
        contents_version: String,
        character_data_signature: u32,
        controller_signature: u32,
    },
    NestedCharacterControllerSetup {
        contents_version: String,
        character_data_signature: u32,
        controller_signature: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimControllerArchitectureEvidence {
    InlineRigidBodySetup,
    CharacterProxyCinfo,
    CharacterRigidBodyCinfo,
    FixedCinfo,
    CustomCinfo { class_name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimControllerEvidenceDisposition {
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
        character_data_signature: u32,
        controller_class: Option<String>,
        total_height_bits: Option<u32>,
        radius_bits: Option<u32>,
    },
    InvalidModelTransform,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimCharacterControllerEvidence {
    pub character_path: String,
    pub contents_version: String,
    pub character_data_class: String,
    pub character_data_signature: u32,
    pub controller_class: Option<String>,
    pub controller_signature: Option<u32>,
    pub layout: Option<SkyrimControllerLayoutEvidence>,
    pub controller_cinfo_class: Option<String>,
    pub architecture: Option<SkyrimControllerArchitectureEvidence>,
    pub collision_filter_info: Option<u32>,
    pub rigid_body_type: Option<i32>,
    pub shape_type: Option<i32>,
    pub capsule: Option<SkyrimControllerCapsuleEvidence>,
    pub model: Option<SkyrimControllerModelEvidence>,
    pub disposition: SkyrimControllerEvidenceDisposition,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimLegacyControllerRecipeEvidence {
    pub layout: SkyrimControllerLayoutEvidence,
    pub controller_class: String,
    pub controller_signature: u32,
    pub collision_filter_info: u32,
    pub controller_cinfo_class: Option<String>,
    pub capsule: SkyrimControllerCapsuleEvidence,
    pub model: SkyrimControllerModelEvidence,
}

impl SkyrimCharacterControllerEvidence {
    pub fn legacy_controller_recipe_evidence(
        &self,
    ) -> Option<SkyrimLegacyControllerRecipeEvidence> {
        if !matches!(
            &self.layout,
            Some(SkyrimControllerLayoutEvidence::LegacyCharacterControllerInfo { .. })
        ) {
            return None;
        }
        Some(SkyrimLegacyControllerRecipeEvidence {
            layout: self.layout.clone()?,
            controller_class: self.controller_class.clone()?,
            controller_signature: self.controller_signature?,
            collision_filter_info: self.collision_filter_info?,
            controller_cinfo_class: self.controller_cinfo_class.clone(),
            capsule: self.capsule.clone()?,
            model: self.model.clone()?,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkyrimCreatureFamilyInventory {
    pub family_id: String,
    pub project_path: String,
    pub character_paths: Vec<String>,
    pub animation_skeleton_paths: Vec<String>,
    pub ragdoll_paths: Vec<String>,
    pub behavior_paths: Vec<String>,
    pub behavior_groups: Vec<SkyrimBehaviorGeneratorGroupEvidence>,
    pub behavior_variables: Vec<SkyrimBehaviorVariableEvidence>,
    pub controllers: Vec<SkyrimCharacterControllerEvidence>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimCreatureFamilyEvidence {
    pub family_id: String,
    pub project_path: String,
    pub inventory: SkyrimCreatureFamilyInventory,
    pub race_attacks: Vec<SkyrimRaceAttackEvidence>,
    pub race_attack_sources: Vec<SkyrimRaceAttackSourceEvidence>,
    pub clips: Vec<SkyrimCreatureClipEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimUnsupportedMotionReason {
    NoSemanticRoleEvidence,
    BehaviorReferenceMissing,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkyrimClipDisposition {
    Role {
        role: SkyrimCreatureMotionRole,
        trigger_event: Option<String>,
        trigger_aliases: Vec<String>,
        evidence: Vec<SkyrimMotionRoleEvidence>,
    },
    SharedPaired {
        original_skeleton_name: String,
    },
    SharedOverlay {
        evidence: Vec<SkyrimMotionRoleEvidence>,
    },
    Unsupported {
        reason: SkyrimUnsupportedMotionReason,
    },
    Ambiguous {
        roles: Vec<SkyrimCreatureMotionRole>,
        evidence: Vec<SkyrimMotionRoleEvidence>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimCatalogClip {
    pub clip_id: String,
    pub clip_path: String,
    pub original_skeleton_name: Option<String>,
    pub family_ids: Vec<String>,
    pub disposition: SkyrimClipDisposition,
    pub root_motion: SkyrimRootMotion,
    pub events: Vec<SkyrimCatalogEvent>,
    pub annotations: Vec<SkyrimTimedEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimCatalogEvent {
    pub name: String,
    pub time: Option<f32>,
    pub source: SkyrimMotionEvidenceSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimFamilyMotionSet {
    pub family_id: String,
    pub project_path: String,
    pub race_attacks: Vec<SkyrimRaceAttackEvidence>,
    pub race_attack_sources: Vec<SkyrimRaceAttackSourceEvidence>,
    pub clip_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimPassiveGroundAuditReceipt {
    pub family_id: String,
    pub project_path: String,
    pub decoded_behavior_paths: Vec<String>,
    pub decoded_clip_ids: Vec<String>,
    pub race_attack_events: Vec<String>,
    pub semantic_attack_locators: Vec<SkyrimMotionSourceLocator>,
    pub zero_attack_semantics: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkyrimCreatureMotionAccounting {
    pub families: usize,
    pub unique_clips: usize,
    pub role_clips: usize,
    pub paired_clips: usize,
    pub overlay_clips: usize,
    pub unsupported_clips: usize,
    pub ambiguous_clips: usize,
    pub evidence_events: usize,
    pub havok_root_motion_clips: usize,
    pub bound_root_motion_clips: usize,
    pub unknown_root_motion_clips: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimCreatureMotionCatalog {
    pub families: Vec<SkyrimFamilyMotionSet>,
    pub inventories: Vec<SkyrimCreatureFamilyInventory>,
    pub clips: Vec<SkyrimCatalogClip>,
    pub accounting: SkyrimCreatureMotionAccounting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimFamilyMotionReadiness {
    pub family_id: String,
    pub template: CreatureGraphTemplate,
    pub ready: bool,
    pub selected_clip_ids: Vec<String>,
    pub blockers: Vec<SkyrimFamilyMotionBlocker>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimFamilyMotionBlocker {
    MissingRole {
        alternatives: Vec<SkyrimCreatureMotionRole>,
    },
    MissingTriggerEvent {
        role: SkyrimCreatureMotionRole,
        clip_ids: Vec<String>,
    },
    UnknownRootMotion {
        role: SkyrimCreatureMotionRole,
        clip_ids: Vec<String>,
    },
    AmbiguousRole {
        clip_id: String,
        roles: Vec<SkyrimCreatureMotionRole>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimLivingTemplateDisposition {
    Proven {
        template: CreatureGraphTemplate,
        locators: Vec<SkyrimMotionSourceLocator>,
    },
    Ambiguous {
        candidates: Vec<CreatureGraphTemplate>,
        locators: Vec<SkyrimMotionSourceLocator>,
    },
    Unsupported {
        evidenced_roles: Vec<SkyrimCreatureMotionRole>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimLivingClipCandidate {
    pub role: SkyrimCreatureMotionRole,
    pub clip_id: String,
    pub clip_path: String,
    pub triggers: Vec<SkyrimMotionTriggerEvidence>,
    pub root_motion: SkyrimRootMotion,
    pub root_motion_locator: Option<SkyrimRootMotionSourceLocator>,
    pub role_locators: Vec<SkyrimMotionSourceLocator>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimRootMotionSourceLocator {
    HavokReferenceFrame {
        clip_path: String,
    },
    BoundAnims {
        source_path: String,
        project_stem: String,
        animation_index: u32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkyrimRequiredRoleDisposition {
    Ready {
        alternatives: Vec<SkyrimCreatureMotionRole>,
        selected: SkyrimLivingClipCandidate,
        additional: Vec<SkyrimLivingClipCandidate>,
    },
    MissingRole {
        alternatives: Vec<SkyrimCreatureMotionRole>,
    },
    AmbiguousRole {
        alternatives: Vec<SkyrimCreatureMotionRole>,
        candidates: Vec<SkyrimLivingClipCandidate>,
    },
    MissingTrigger {
        alternatives: Vec<SkyrimCreatureMotionRole>,
        candidates: Vec<SkyrimLivingClipCandidate>,
    },
    AmbiguousTrigger {
        alternatives: Vec<SkyrimCreatureMotionRole>,
        candidates: Vec<SkyrimLivingClipCandidate>,
    },
    UnknownRootMotion {
        alternatives: Vec<SkyrimCreatureMotionRole>,
        candidates: Vec<SkyrimLivingClipCandidate>,
    },
    UnsupportedRootMotion {
        alternatives: Vec<SkyrimCreatureMotionRole>,
        candidates: Vec<SkyrimLivingClipCandidate>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyrimLivingFamilyDisposition {
    Ready,
    Blocked,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimLivingFamilyEvidence {
    pub family_id: String,
    pub project_path: String,
    pub template: SkyrimLivingTemplateDisposition,
    pub required_roles: Vec<SkyrimRequiredRoleDisposition>,
    pub disposition: SkyrimLivingFamilyDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkyrimCreatureMotionError {
    #[error("duplicate creature motion family {family_id}")]
    DuplicateFamily { family_id: String },
    #[error("family {family_id} contains duplicate clip {clip_path}")]
    DuplicateFamilyClip {
        family_id: String,
        clip_path: String,
    },
    #[error("shared clip evidence disagrees for {clip_path}: {detail}")]
    SharedClipConflict { clip_path: String, detail: String },
    #[error("missing Skyrim creature project {path}")]
    MissingProject { path: String },
    #[error("failed to read Skyrim creature asset {path}: {detail}")]
    ReadAsset { path: String, detail: String },
    #[error("failed to decode Skyrim creature Havok {path}: {detail}")]
    DecodeHavok { path: String, detail: String },
    #[error("invalid Skyrim AnimationData {path}: {detail}")]
    InvalidAnimationData { path: String, detail: String },
    #[error("motion catalog accounting drifted: {detail}")]
    AccountingDrift { detail: String },
    #[error("family {family_id} cannot produce a source-neutral {target}: {detail}")]
    AdapterUnavailable {
        family_id: String,
        target: &'static str,
        detail: String,
    },
}

impl SkyrimCreatureMotionCatalog {
    pub fn family(&self, family_id: &str) -> Option<&SkyrimFamilyMotionSet> {
        self.families
            .iter()
            .find(|family| family.family_id == family_id)
    }

    pub fn inventory(&self, family_id: &str) -> Option<&SkyrimCreatureFamilyInventory> {
        self.inventories
            .iter()
            .find(|inventory| inventory.family_id == family_id)
    }

    pub fn clip(&self, clip_id: &str) -> Option<&SkyrimCatalogClip> {
        self.clips.iter().find(|clip| clip.clip_id == clip_id)
    }

    pub fn capability_graph_manifest(
        &self,
        family_id: &str,
    ) -> Result<CapabilityGraphManifest, SkyrimCreatureMotionError> {
        self.capability_graph_bundle(family_id)
            .map(|(manifest, _)| manifest)
    }

    pub fn capability_graph_bundle(
        &self,
        family_id: &str,
    ) -> Result<(CapabilityGraphManifest, Vec<VariableDecl>), SkyrimCreatureMotionError> {
        let family = self.family(family_id).ok_or_else(|| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family_id.to_string(),
                target: "CapabilityGraphManifest",
                detail: "family is absent from the catalog".to_string(),
            }
        })?;
        let clips = family
            .clip_ids
            .iter()
            .filter_map(|clip_id| self.clip(clip_id))
            .collect::<Vec<_>>();
        build_capability_manifest(family, self.inventory(family_id), &clips)
    }

    pub fn motion_set(&self, family_id: &str) -> Result<MotionSet, SkyrimCreatureMotionError> {
        let family = self.family(family_id).ok_or_else(|| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family_id.to_string(),
                target: "MotionSet",
                detail: "family is absent from the catalog".to_string(),
            }
        })?;
        let clips = family
            .clip_ids
            .iter()
            .filter_map(|clip_id| self.clip(clip_id))
            .collect::<Vec<_>>();
        build_motion_set(family, &clips)
    }

    pub fn family_readiness(&self) -> Vec<SkyrimFamilyMotionReadiness> {
        self.families
            .iter()
            .map(|family| {
                let clips = family
                    .clip_ids
                    .iter()
                    .filter_map(|clip_id| self.clip(clip_id))
                    .collect::<Vec<_>>();
                build_family_readiness(family, &clips)
            })
            .collect()
    }

    pub fn living_family_evidence(&self) -> Vec<SkyrimLivingFamilyEvidence> {
        self.families
            .iter()
            .map(|family| {
                let clips = family
                    .clip_ids
                    .iter()
                    .filter_map(|clip_id| self.clip(clip_id))
                    .collect::<Vec<_>>();
                build_living_family_evidence(family, self.inventory(&family.family_id), &clips)
            })
            .collect()
    }

    pub fn passive_ground_audits(&self) -> Vec<SkyrimPassiveGroundAuditReceipt> {
        self.families
            .iter()
            .map(|family| {
                let clips = family
                    .clip_ids
                    .iter()
                    .filter_map(|clip_id| self.clip(clip_id))
                    .collect::<Vec<_>>();
                passive_ground_audit(family, self.inventory(&family.family_id), &clips)
            })
            .collect()
    }
}

pub fn build_skyrim_creature_motion_catalog(
    families: &[SkyrimCreatureFamilyEvidence],
) -> Result<SkyrimCreatureMotionCatalog, SkyrimCreatureMotionError> {
    let mut ordered = families.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.family_id.cmp(&right.family_id));
    for pair in ordered.windows(2) {
        if pair[0].family_id == pair[1].family_id {
            return Err(SkyrimCreatureMotionError::DuplicateFamily {
                family_id: pair[0].family_id.clone(),
            });
        }
    }

    let mut merged = BTreeMap::<String, MergedClipEvidence>::new();
    let mut catalog_families = Vec::with_capacity(ordered.len());
    let mut inventories = Vec::with_capacity(ordered.len());
    for family in ordered {
        let mut family_paths = BTreeSet::new();
        for clip in &family.clips {
            let key = path_key(&clip.clip_path);
            if !family_paths.insert(key.clone()) {
                return Err(SkyrimCreatureMotionError::DuplicateFamilyClip {
                    family_id: family.family_id.clone(),
                    clip_path: clip.clip_path.clone(),
                });
            }
            if let Some(existing) = merged.get_mut(&key) {
                existing.merge(family, clip)?;
            } else {
                merged.insert(key, MergedClipEvidence::new(family, clip));
            }
        }
        catalog_families.push(SkyrimFamilyMotionSet {
            family_id: family.family_id.clone(),
            project_path: canonical_runtime_path(&family.project_path),
            race_attacks: {
                let mut attacks = family.race_attacks.clone();
                attacks.sort_by(|left, right| {
                    left.event
                        .to_ascii_lowercase()
                        .cmp(&right.event.to_ascii_lowercase())
                        .then_with(|| left.has_attack_spell.cmp(&right.has_attack_spell))
                });
                attacks.dedup_by(|left, right| {
                    left.event.eq_ignore_ascii_case(&right.event)
                        && left.has_attack_spell == right.has_attack_spell
                });
                attacks
            },
            race_attack_sources: family.race_attack_sources.clone(),
            clip_ids: Vec::new(),
        });
        inventories.push(family.inventory.clone());
    }

    let mut clips = Vec::with_capacity(merged.len());
    for merged_clip in merged.into_values() {
        let clip_id = clip_id(&merged_clip.clip.clip_path);
        let disposition = classify_clip(&merged_clip);
        let events = catalog_events(&merged_clip.clip);
        for family_id in &merged_clip.family_ids {
            catalog_families
                .iter_mut()
                .find(|family| &family.family_id == family_id)
                .expect("merged clips only reference known families")
                .clip_ids
                .push(clip_id.clone());
        }
        clips.push(SkyrimCatalogClip {
            clip_id,
            clip_path: canonical_runtime_path(&merged_clip.clip.clip_path),
            original_skeleton_name: merged_clip.clip.original_skeleton_name.clone(),
            family_ids: merged_clip.family_ids.into_iter().collect(),
            disposition,
            root_motion: merged_clip.clip.root_motion,
            events,
            annotations: merged_clip.clip.annotations,
        });
    }
    clips.sort_by(|left, right| left.clip_path.cmp(&right.clip_path));
    for family in &mut catalog_families {
        family.clip_ids.sort();
    }

    let accounting = account(&catalog_families, &clips)?;
    Ok(SkyrimCreatureMotionCatalog {
        families: catalog_families,
        inventories,
        clips,
        accounting,
    })
}

pub fn load_extracted_skyrim_creature_motion_catalog(
    actors_root: &Path,
    animation_data_root: &Path,
    race_evidence: &[SkyrimCreatureFamilyRaceEvidence],
) -> Result<SkyrimCreatureMotionCatalog, SkyrimCreatureMotionError> {
    let actors =
        actors_root
            .canonicalize()
            .map_err(|error| SkyrimCreatureMotionError::ReadAsset {
                path: actors_root.display().to_string(),
                detail: error.to_string(),
            })?;
    let race_by_project = index_race_evidence(race_evidence)?;
    let animation_data = load_animation_data_catalog(animation_data_root)?;
    let mut family_evidence = Vec::with_capacity(RACE_PROJECTS.len());
    for (family_id, project_relative) in RACE_PROJECTS {
        let project = actors.join(project_relative);
        if !project.exists() {
            return Err(SkyrimCreatureMotionError::MissingProject {
                path: project.display().to_string(),
            });
        }
        let project_stem = project
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let empty_animation_data = ProjectAnimationData::default();
        let project_animation_data = animation_data
            .get(&project_stem)
            .unwrap_or(&empty_animation_data);
        family_evidence.push(load_family_evidence(
            &actors,
            project_animation_data,
            family_id,
            project_relative,
            race_by_project
                .get(&project_path_key(project_relative))
                .cloned()
                .unwrap_or_default(),
        )?);
    }
    build_skyrim_creature_motion_catalog(&family_evidence)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimCreatureFamilyRaceEvidence {
    pub project_path: String,
    pub attacks: Vec<SkyrimRaceAttackSourceEvidence>,
}

pub fn race_motion_evidence_from_creature_plan(
    plan: &CreatureCorpusPlan,
) -> Vec<SkyrimCreatureFamilyRaceEvidence> {
    let mut evidence = BTreeMap::<String, SkyrimCreatureFamilyRaceEvidence>::new();
    for race in &plan.races {
        let mut attacks = attack_evidence_from_contract(&race.attack_contract);
        for attack in &mut attacks {
            if let Some(source_event) = race
                .attack_events
                .iter()
                .find(|event| event.eq_ignore_ascii_case(&attack.event))
            {
                attack.event.clone_from(source_event);
            }
        }
        for project_path in &race.project_paths {
            evidence
                .entry(project_path_key(project_path))
                .or_insert_with(|| SkyrimCreatureFamilyRaceEvidence {
                    project_path: project_path.clone(),
                    attacks: Vec::new(),
                })
                .attacks
                .extend(
                    attacks
                        .iter()
                        .cloned()
                        .map(|attack| SkyrimRaceAttackSourceEvidence {
                            source_race: race.source_race,
                            source_plugin: race.source_plugin.clone(),
                            attack,
                        }),
                );
        }
    }
    evidence.into_values().collect()
}

fn attack_evidence_from_contract(contract: &[String]) -> Vec<SkyrimRaceAttackEvidence> {
    let mut attacks = Vec::new();
    let mut pending_spell = None;
    for entry in contract {
        if let Some(hex) = entry.strip_prefix("atkd:bytes:") {
            pending_spell = hex::decode(hex)
                .ok()
                .and_then(|bytes| bytes.get(8..12).map(|raw| raw != [0, 0, 0, 0]));
        } else if let Some(fields) = entry.strip_prefix("atkd:struct:") {
            pending_spell =
                Some(fields.contains("attack_spell=form:") || fields.contains("attackspell=form:"));
        } else if let Some(event) = entry.strip_prefix("atke:string:") {
            attacks.push(SkyrimRaceAttackEvidence {
                event: event.to_string(),
                has_attack_spell: pending_spell.take().unwrap_or(false),
            });
        }
    }
    attacks
}

fn index_race_evidence(
    evidence: &[SkyrimCreatureFamilyRaceEvidence],
) -> Result<BTreeMap<String, Vec<SkyrimRaceAttackSourceEvidence>>, SkyrimCreatureMotionError> {
    let mut index = BTreeMap::new();
    for family in evidence {
        let key = project_path_key(&family.project_path);
        index
            .entry(key)
            .or_insert_with(Vec::new)
            .extend(family.attacks.iter().cloned());
    }
    Ok(index)
}

#[derive(Clone)]
struct MergedClipEvidence {
    family_ids: BTreeSet<String>,
    race_attacks: Vec<LocatedRaceAttack>,
    clip: SkyrimCreatureClipEvidence,
}

#[derive(Clone)]
struct LocatedRaceAttack {
    project_path: String,
    source_race: Option<FormKey>,
    attack: SkyrimRaceAttackEvidence,
}

impl MergedClipEvidence {
    fn new(family: &SkyrimCreatureFamilyEvidence, clip: &SkyrimCreatureClipEvidence) -> Self {
        Self {
            family_ids: BTreeSet::from([family.family_id.clone()]),
            race_attacks: located_race_attacks(family),
            clip: clip.clone(),
        }
    }

    fn merge(
        &mut self,
        family: &SkyrimCreatureFamilyEvidence,
        clip: &SkyrimCreatureClipEvidence,
    ) -> Result<(), SkyrimCreatureMotionError> {
        if self.clip.original_skeleton_name != clip.original_skeleton_name
            || self.clip.blend_hint != clip.blend_hint
        {
            return Err(SkyrimCreatureMotionError::SharedClipConflict {
                clip_path: self.clip.clip_path.clone(),
                detail: "binding evidence differs between owning families".to_string(),
            });
        }
        self.family_ids.insert(family.family_id.clone());
        extend_unique_by(
            &mut self.race_attacks,
            &located_race_attacks(family),
            |located| {
                format!(
                    "{}:{:?}:{}:{}",
                    project_path_key(&located.project_path),
                    located.source_race,
                    located.attack.event.to_ascii_lowercase(),
                    located.attack.has_attack_spell
                )
            },
        );
        extend_unique_by(&mut self.clip.behavior, &clip.behavior, |reference| {
            format!(
                "{}:{}:{}",
                path_key(&reference.behavior_path),
                path_key(&reference.animation_name),
                reference
                    .generator_name
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
            )
        });
        extend_unique_by(
            &mut self.clip.animation_data,
            &clip.animation_data,
            |entry| {
                let events = entry
                    .events
                    .iter()
                    .map(|event| format!("{}:{:08x}", event.name, event.time.to_bits()))
                    .collect::<Vec<_>>()
                    .join("|");
                format!(
                    "{}:{}:{:08x}:{:08x}:{:08x}:{events}",
                    normalized_words(&entry.sequence_name),
                    entry.animation_index,
                    entry.playback_speed.to_bits(),
                    entry.crop_start_local_time.to_bits(),
                    entry.crop_end_local_time.to_bits(),
                )
            },
        );
        extend_unique_by(&mut self.clip.annotations, &clip.annotations, |event| {
            format!(
                "{}:{:08x}",
                event.name.to_ascii_lowercase(),
                event.time.to_bits()
            )
        });
        if matches!(self.clip.root_motion, SkyrimRootMotion::Unknown)
            && !matches!(clip.root_motion, SkyrimRootMotion::Unknown)
        {
            self.clip.root_motion = clip.root_motion.clone();
        }
        Ok(())
    }
}

fn located_race_attacks(family: &SkyrimCreatureFamilyEvidence) -> Vec<LocatedRaceAttack> {
    if family.race_attack_sources.is_empty() {
        return family
            .race_attacks
            .iter()
            .cloned()
            .map(|attack| LocatedRaceAttack {
                project_path: canonical_runtime_path(&family.project_path),
                source_race: None,
                attack,
            })
            .collect();
    }
    family
        .race_attack_sources
        .iter()
        .cloned()
        .map(|evidence| LocatedRaceAttack {
            project_path: canonical_runtime_path(&family.project_path),
            source_race: Some(evidence.source_race),
            attack: evidence.attack,
        })
        .collect()
}

fn extend_unique_by<T: Clone>(target: &mut Vec<T>, source: &[T], key: impl Fn(&T) -> String) {
    let mut seen = target.iter().map(&key).collect::<BTreeSet<_>>();
    for value in source {
        if seen.insert(key(value)) {
            target.push(value.clone());
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EvidencePriority {
    Race = 0,
    BehaviorGenerator = 1,
    BehaviorTopology = 2,
    BehaviorState = 3,
    AnimationData = 4,
    Annotation = 5,
}

fn classify_clip(merged: &MergedClipEvidence) -> SkyrimClipDisposition {
    let clip = &merged.clip;
    if clip.original_skeleton_name.as_deref() == Some("PairedRoot") {
        return SkyrimClipDisposition::SharedPaired {
            original_skeleton_name: "PairedRoot".to_string(),
        };
    }
    if clip.blend_hint != 0 {
        return SkyrimClipDisposition::SharedOverlay {
            evidence: vec![SkyrimMotionRoleEvidence {
                source: SkyrimMotionEvidenceSource::BehaviorGenerator,
                detail: format!("hkaAnimationBinding.blendHint={}", clip.blend_hint),
                time: None,
                locator: SkyrimMotionSourceLocator::AnimationBinding {
                    clip_path: canonical_runtime_path(&clip.clip_path),
                    member: "hkaAnimationBinding.blendHint".to_string(),
                },
                trigger: None,
                root_motion_locator: None,
            }],
        };
    }

    let mut signals = Vec::<(
        EvidencePriority,
        SkyrimCreatureMotionRole,
        SkyrimMotionRoleEvidence,
        Option<String>,
    )>::new();
    for reference in &clip.behavior {
        let mut nodes = Vec::new();
        nodes.extend(reference.generator_name.iter().map(|text| {
            (
                text,
                EvidencePriority::BehaviorGenerator,
                !reference.initial_state_names.is_empty(),
            )
        }));
        nodes.extend(reference.topology_names.iter().map(|text| {
            (
                text,
                EvidencePriority::BehaviorTopology,
                !reference.initial_state_names.is_empty(),
            )
        }));
        nodes.extend(reference.state_names.iter().map(|text| {
            (
                text,
                EvidencePriority::BehaviorState,
                reference
                    .initial_state_names
                    .iter()
                    .any(|state| state == text),
            )
        }));
        for (text, priority, initial_state) in nodes {
            for (race, role) in matching_race_attacks(text, &merged.race_attacks) {
                let locator = race_locator(race);
                signals.push((
                    EvidencePriority::Race,
                    role,
                    role_evidence(
                        SkyrimMotionEvidenceSource::RaceAttackEvent,
                        format!(
                            "RACE {:?} ATKE {} in {} matched behavior node {text}",
                            race.source_race, race.attack.event, race.project_path
                        ),
                        locator.clone(),
                        Some(trigger_evidence(&race.attack.event, locator)),
                    ),
                    Some(race.attack.event.clone()),
                ));
            }
            if let Some(role) = role_from_text(text) {
                let node = if initial_state {
                    format!(
                        "hkbStateMachine.startStateId={}; generator={text}",
                        reference.initial_state_names.join("|")
                    )
                } else {
                    text.clone()
                };
                let incoming = reference
                    .incoming_events
                    .iter()
                    .map(|event| (event.as_str(), reference.behavior_path.as_str()))
                    .chain(
                        reference
                            .ancestor_entry_events
                            .iter()
                            .map(|event| (event.event.as_str(), event.behavior_path.as_str())),
                    )
                    .filter(|(event, _)| transition_role(event, &merged.race_attacks) == Some(role))
                    .filter(|(event, _)| transition_event_matches_node(event, text))
                    .map(Some)
                    .chain(std::iter::once(None))
                    .collect::<Vec<_>>();
                for event in incoming {
                    let trigger = event.map(|(event, behavior_path)| {
                        trigger_evidence(
                            event,
                            behavior_entry_event_locator(
                                behavior_path,
                                &reference.animation_name,
                                event,
                            ),
                        )
                    });
                    signals.push((
                        priority,
                        role,
                        role_evidence(
                            SkyrimMotionEvidenceSource::BehaviorGenerator,
                            format!("{} node {node}", reference.behavior_path),
                            SkyrimMotionSourceLocator::BehaviorGenerator {
                                behavior_path: canonical_runtime_path(&reference.behavior_path),
                                animation_name: canonical_runtime_path(&reference.animation_name),
                                node: node.clone(),
                            },
                            trigger,
                        ),
                        event.map(|(event, _)| event.to_string()),
                    ));
                }
            }
        }
    }
    for entry in &clip.animation_data {
        for (race, role) in matching_race_attacks(&entry.sequence_name, &merged.race_attacks) {
            let locator = race_locator(race);
            signals.push((
                EvidencePriority::Race,
                role,
                role_evidence(
                    SkyrimMotionEvidenceSource::RaceAttackEvent,
                    format!(
                        "RACE {:?} ATKE {} in {} matched AnimationData sequence {} index {}",
                        race.source_race,
                        race.attack.event,
                        race.project_path,
                        entry.sequence_name,
                        entry.animation_index
                    ),
                    locator.clone(),
                    Some(trigger_evidence(&race.attack.event, locator)),
                ),
                Some(race.attack.event.clone()),
            ));
        }
        if let Some(role) = role_from_text(&entry.sequence_name) {
            let race = linked_race_event(&entry.sequence_name, &merged.race_attacks);
            let trigger = race.map(|race| {
                let locator = race_locator(race);
                trigger_evidence(&race.attack.event, locator)
            });
            signals.push((
                EvidencePriority::AnimationData,
                role,
                role_evidence(
                    SkyrimMotionEvidenceSource::AnimationData,
                    format!(
                        "AnimationData sequence {} index {}",
                        entry.sequence_name, entry.animation_index
                    ),
                    animation_data_sequence_locator(entry),
                    trigger,
                ),
                race.map(|race| race.attack.event.clone()),
            ));
        }
        for event in &entry.events {
            if let Some(role) = role_from_text(&event.name) {
                let race = linked_race_event(&entry.sequence_name, &merged.race_attacks);
                let trigger = race.map(|race| {
                    let locator = race_locator(race);
                    trigger_evidence(&race.attack.event, locator)
                });
                signals.push((
                    EvidencePriority::AnimationData,
                    role,
                    SkyrimMotionRoleEvidence {
                        source: SkyrimMotionEvidenceSource::AnimationData,
                        detail: format!("AnimationData event {}", event.name),
                        time: Some(event.time),
                        locator: SkyrimMotionSourceLocator::AnimationDataEvent {
                            source_path: entry.source_path.clone(),
                            sequence_name: entry.sequence_name.clone(),
                            animation_index: entry.animation_index,
                            event: event.name.clone(),
                            time_bits: event.time.to_bits(),
                        },
                        trigger,
                        root_motion_locator: None,
                    },
                    race.map(|race| race.attack.event.clone()),
                ));
            }
        }
    }
    for event in &clip.annotations {
        if let Some(role) = role_from_text(&event.name) {
            signals.push((
                EvidencePriority::Annotation,
                role,
                SkyrimMotionRoleEvidence {
                    source: SkyrimMotionEvidenceSource::ClipAnnotation,
                    detail: format!("clip annotation {}", event.name),
                    time: Some(event.time),
                    locator: SkyrimMotionSourceLocator::ClipAnnotation {
                        clip_path: canonical_runtime_path(&clip.clip_path),
                        event: event.name.clone(),
                        time_bits: event.time.to_bits(),
                    },
                    trigger: None,
                    root_motion_locator: None,
                },
                None,
            ));
        }
    }
    let Some(best_priority) = signals.iter().map(|signal| signal.0).min() else {
        return SkyrimClipDisposition::Unsupported {
            reason: if clip.behavior.is_empty() {
                SkyrimUnsupportedMotionReason::BehaviorReferenceMissing
            } else {
                SkyrimUnsupportedMotionReason::NoSemanticRoleEvidence
            },
        };
    };
    let best = signals
        .iter()
        .filter(|signal| signal.0 == best_priority)
        .collect::<Vec<_>>();
    let roles = best.iter().map(|signal| signal.1).collect::<BTreeSet<_>>();
    let root_motion_locator = root_motion_source_locator(clip);
    if roles.len() != 1 {
        let evidence = best
            .iter()
            .map(|signal| {
                let mut evidence = signal.2.clone();
                evidence.root_motion_locator = root_motion_locator.clone();
                evidence
            })
            .collect::<Vec<_>>();
        return SkyrimClipDisposition::Ambiguous {
            roles: roles.into_iter().collect(),
            evidence,
        };
    }
    let role = *roles.first().expect("single role");
    let relevant = signals
        .into_iter()
        .filter(|signal| signal.1 == role)
        .collect::<Vec<_>>();
    let evidence = relevant
        .iter()
        .map(|signal| {
            let mut evidence = signal.2.clone();
            evidence.root_motion_locator = root_motion_locator.clone();
            evidence
        })
        .collect::<Vec<_>>();
    let mut triggers = relevant
        .iter()
        .filter_map(|signal| signal.3.clone())
        .collect::<Vec<_>>();
    triggers.sort_by_key(|event| event.to_ascii_lowercase());
    triggers.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    let trigger_event = triggers.first().cloned();
    let trigger_aliases = triggers.into_iter().skip(1).collect();
    SkyrimClipDisposition::Role {
        role,
        trigger_event,
        trigger_aliases,
        evidence,
    }
}

fn root_motion_source_locator(
    clip: &SkyrimCreatureClipEvidence,
) -> Option<SkyrimRootMotionSourceLocator> {
    match &clip.root_motion {
        SkyrimRootMotion::Stationary {
            source: SkyrimRootMotionSource::HavokReferenceFrame,
            ..
        }
        | SkyrimRootMotion::Sampled {
            source: SkyrimRootMotionSource::HavokReferenceFrame,
            ..
        } => Some(SkyrimRootMotionSourceLocator::HavokReferenceFrame {
            clip_path: canonical_runtime_path(&clip.clip_path),
        }),
        SkyrimRootMotion::Stationary {
            source: SkyrimRootMotionSource::BoundAnims,
            ..
        }
        | SkyrimRootMotion::Sampled {
            source: SkyrimRootMotionSource::BoundAnims,
            ..
        } => clip.animation_data.iter().find_map(|entry| {
            if entry.root_motion != clip.root_motion {
                return None;
            }
            Some(SkyrimRootMotionSourceLocator::BoundAnims {
                source_path: entry.root_motion_source_path.clone()?,
                project_stem: entry.project_stem.clone(),
                animation_index: entry.animation_index,
            })
        }),
        SkyrimRootMotion::Unknown | SkyrimRootMotion::Unsupported { .. } => None,
    }
}

fn role_evidence(
    source: SkyrimMotionEvidenceSource,
    detail: String,
    locator: SkyrimMotionSourceLocator,
    trigger: Option<SkyrimMotionTriggerEvidence>,
) -> SkyrimMotionRoleEvidence {
    SkyrimMotionRoleEvidence {
        source,
        detail,
        time: None,
        locator,
        trigger,
        root_motion_locator: None,
    }
}

fn matching_race_attacks<'a>(
    behavior_name: &str,
    attacks: &'a [LocatedRaceAttack],
) -> Vec<(&'a LocatedRaceAttack, SkyrimCreatureMotionRole)> {
    linked_race_events(behavior_name, attacks)
        .into_iter()
        .map(|attack| {
            let role = if attack.attack.has_attack_spell {
                SkyrimCreatureMotionRole::SpellAttack
            } else {
                role_from_text(behavior_name)
                    .filter(|role| is_attack_role(*role))
                    .unwrap_or(SkyrimCreatureMotionRole::MeleeAttack)
            };
            (attack, role)
        })
        .collect()
}

fn matching_race_attack<'a>(
    behavior_name: &str,
    attacks: &'a [LocatedRaceAttack],
) -> Option<(&'a LocatedRaceAttack, SkyrimCreatureMotionRole)> {
    let matches = matching_race_attacks(behavior_name, attacks);
    let role = matches.first()?.1;
    matches
        .iter()
        .all(|(_, candidate)| *candidate == role)
        .then_some((matches[0].0, role))
}

fn transition_role(event: &str, attacks: &[LocatedRaceAttack]) -> Option<SkyrimCreatureMotionRole> {
    if let Some((_, role)) = matching_race_attack(event, attacks) {
        return Some(role);
    }
    let words = normalized_words(event);
    if ["stop", "end", "exit", "outro"]
        .iter()
        .any(|word| contains_word(&words, word))
    {
        return None;
    }
    role_from_text(event)
}

fn transition_event_matches_node(event: &str, node: &str) -> bool {
    let generic = [
        "attack",
        "begin",
        "enter",
        "event",
        "move",
        "power",
        "spell",
        "start",
        "state",
        "transition",
    ];
    let discriminators = normalized_words(event)
        .split_whitespace()
        .filter(|word| !generic.contains(word))
        .filter(|word| word.len() >= 2)
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if discriminators.is_empty() {
        return true;
    }
    let node_words = normalized_words(node)
        .split_whitespace()
        .filter(|word| word.len() >= 2)
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    !discriminators.is_disjoint(&node_words)
}

fn linked_race_event<'a>(
    text: &str,
    attacks: &'a [LocatedRaceAttack],
) -> Option<&'a LocatedRaceAttack> {
    let matches = linked_race_events(text, attacks);
    let first = *matches.first()?;
    matches
        .iter()
        .all(|candidate| {
            candidate
                .attack
                .event
                .eq_ignore_ascii_case(&first.attack.event)
                && candidate.attack.has_attack_spell == first.attack.has_attack_spell
        })
        .then_some(first)
}

fn linked_race_events<'a>(
    text: &str,
    attacks: &'a [LocatedRaceAttack],
) -> Vec<&'a LocatedRaceAttack> {
    let words = normalized_words(text);
    let semantic_words = significant_words(&words);
    attacks
        .iter()
        .filter(|located| {
            let event_words = normalized_words(&located.attack.event);
            words == event_words
                || (!ordinal_attack_tokens(&words).is_empty()
                    && ordinal_attack_tokens(&words) == ordinal_attack_tokens(&event_words))
                || (!semantic_words.is_empty() && semantic_words == significant_words(&event_words))
        })
        .collect()
}

fn race_locator(race: &LocatedRaceAttack) -> SkyrimMotionSourceLocator {
    SkyrimMotionSourceLocator::RaceAttackEvent {
        project_path: race.project_path.clone(),
        event: race.attack.event.clone(),
    }
}

fn behavior_entry_event_locator(
    behavior_path: &str,
    animation_name: &str,
    event: &str,
) -> SkyrimMotionSourceLocator {
    SkyrimMotionSourceLocator::BehaviorTransition {
        behavior_path: canonical_runtime_path(behavior_path),
        animation_name: canonical_runtime_path(animation_name),
        event: event.to_string(),
    }
}

fn animation_data_sequence_locator(
    entry: &SkyrimAnimationDataEvidence,
) -> SkyrimMotionSourceLocator {
    SkyrimMotionSourceLocator::AnimationDataSequence {
        source_path: entry.source_path.clone(),
        sequence_name: entry.sequence_name.clone(),
        animation_index: entry.animation_index,
    }
}

fn trigger_evidence(
    event: &str,
    locator: SkyrimMotionSourceLocator,
) -> SkyrimMotionTriggerEvidence {
    SkyrimMotionTriggerEvidence {
        event: event.to_string(),
        locator,
    }
}

fn ordinal_attack_tokens(words: &str) -> Vec<String> {
    words
        .split_whitespace()
        .filter(|word| {
            let ordinal = word.trim_start_matches("attack");
            word.starts_with("attack")
                && !ordinal.is_empty()
                && ordinal.chars().all(|character| character.is_ascii_digit())
        })
        .map(str::to_string)
        .collect()
}

fn significant_words(words: &str) -> BTreeSet<String> {
    words
        .split_whitespace()
        .filter(|word| word.len() >= 4 && !["start", "attack", "event"].contains(word))
        .map(str::to_string)
        .collect()
}

fn role_from_text(text: &str) -> Option<SkyrimCreatureMotionRole> {
    let words = normalized_words(text);
    let has = |values: &[&str]| values.iter().any(|word| contains_word(&words, word));
    if has(&["death", "dead", "die", "dying", "ragdoll"]) {
        return Some(SkyrimCreatureMotionRole::Death);
    }
    if has(&["hurt", "stagger", "recoil", "knockdown", "knockback"]) || words.contains("hit react")
    {
        return Some(SkyrimCreatureMotionRole::Hurt);
    }
    let attack = has(&[
        "attack",
        "bite",
        "claw",
        "bash",
        "strike",
        "gore",
        "lunge",
        "slam",
        "stamp",
        "spit",
        "shoot",
        "cast",
        "spell",
        "projectile",
        "breath",
    ]);
    if attack && has(&["cast", "spell", "magic", "fireball", "frost", "lightning"]) {
        return Some(SkyrimCreatureMotionRole::SpellAttack);
    }
    if attack && has(&["projectile", "spit", "breath", "bolt", "launch"]) {
        return Some(SkyrimCreatureMotionRole::ProjectileAttack);
    }
    if attack && has(&["shoot", "bow", "ranged", "throw"]) {
        return Some(SkyrimCreatureMotionRole::RangedAttack);
    }
    if attack {
        return Some(SkyrimCreatureMotionRole::MeleeAttack);
    }
    if has(&["turn", "rotate", "pivot"]) {
        if has_directional_token(&words, &["left", "turnl", "rotateleft", "l"]) {
            return Some(SkyrimCreatureMotionRole::TurnLeft);
        }
        if has_directional_token(&words, &["right", "turnr", "rotateright", "r"]) {
            return Some(SkyrimCreatureMotionRole::TurnRight);
        }
        return Some(SkyrimCreatureMotionRole::Turn);
    }
    let idle = has(&["idle"]);
    let locomotion = has(&[
        "walk",
        "run",
        "sprint",
        "locomotion",
        "forward",
        "backward",
        "strafe",
        "move",
        "swim",
        "fly",
        "flight",
    ]);
    let mechanical = has(&["mechanical", "turret", "motor", "gear", "deploy", "retract"])
        || (has(&["spin"]) && has(&["loop", "start", "stop"]));
    if mechanical && has(&["start", "begin", "open", "deploy"]) {
        return Some(SkyrimCreatureMotionRole::MechanicalStart);
    }
    if mechanical && has(&["loop", "continuous", "spin"]) {
        return Some(SkyrimCreatureMotionRole::MechanicalLoop);
    }
    if mechanical && has(&["stop", "end", "close", "retract"]) {
        return Some(SkyrimCreatureMotionRole::MechanicalStop);
    }
    if has(&["swim"]) && (idle || has(&["tread"])) {
        return Some(SkyrimCreatureMotionRole::SwimIdle);
    }
    if has(&["swim"]) && locomotion {
        return Some(SkyrimCreatureMotionRole::SwimLocomotion);
    }
    let flight = has(&["fly", "flight", "hover", "perch"]);
    if flight && (idle || has(&["hover", "perch"])) {
        return Some(SkyrimCreatureMotionRole::FlyIdle);
    }
    if flight && locomotion {
        return Some(SkyrimCreatureMotionRole::FlyLocomotion);
    }
    if has(&["stationary"]) && idle {
        return Some(SkyrimCreatureMotionRole::StationaryIdle);
    }
    if idle {
        return Some(SkyrimCreatureMotionRole::Idle);
    }
    locomotion.then_some(SkyrimCreatureMotionRole::GroundLocomotion)
}

fn has_directional_token(words: &str, prefixes: &[&str]) -> bool {
    words.split_whitespace().any(|word| {
        prefixes.iter().any(|prefix| {
            word == *prefix
                || word.strip_prefix(prefix).is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.chars().all(char::is_numeric)
                })
        })
    })
}

fn contains_word(words: &str, word: &str) -> bool {
    words.split_whitespace().any(|candidate| candidate == word)
}

fn is_attack_role(role: SkyrimCreatureMotionRole) -> bool {
    matches!(
        role,
        SkyrimCreatureMotionRole::MeleeAttack
            | SkyrimCreatureMotionRole::RangedAttack
            | SkyrimCreatureMotionRole::ProjectileAttack
            | SkyrimCreatureMotionRole::SpellAttack
    )
}

fn catalog_events(clip: &SkyrimCreatureClipEvidence) -> Vec<SkyrimCatalogEvent> {
    let mut events = Vec::new();
    for behavior in &clip.behavior {
        for name in behavior
            .incoming_events
            .iter()
            .chain(&behavior.emitted_events)
        {
            events.push(SkyrimCatalogEvent {
                name: name.clone(),
                time: None,
                source: SkyrimMotionEvidenceSource::BehaviorTransition,
            });
        }
        for entry in &behavior.ancestor_entry_events {
            events.push(SkyrimCatalogEvent {
                name: entry.event.clone(),
                time: None,
                source: SkyrimMotionEvidenceSource::BehaviorTransition,
            });
        }
    }
    for animation_data in &clip.animation_data {
        for event in &animation_data.events {
            events.push(SkyrimCatalogEvent {
                name: event.name.clone(),
                time: Some(event.time),
                source: SkyrimMotionEvidenceSource::AnimationData,
            });
        }
    }
    for annotation in &clip.annotations {
        events.push(SkyrimCatalogEvent {
            name: annotation.name.clone(),
            time: Some(annotation.time),
            source: SkyrimMotionEvidenceSource::ClipAnnotation,
        });
    }
    events.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| option_time_bits(left.time).cmp(&option_time_bits(right.time)))
            .then_with(|| left.source.cmp(&right.source))
    });
    events.dedup_by(|left, right| {
        left.name == right.name
            && left.source == right.source
            && option_time_bits(left.time) == option_time_bits(right.time)
    });
    events
}

fn option_time_bits(value: Option<f32>) -> Option<u32> {
    value.map(f32::to_bits)
}

fn account(
    families: &[SkyrimFamilyMotionSet],
    clips: &[SkyrimCatalogClip],
) -> Result<SkyrimCreatureMotionAccounting, SkyrimCreatureMotionError> {
    let mut accounting = SkyrimCreatureMotionAccounting {
        families: families.len(),
        unique_clips: clips.len(),
        ..SkyrimCreatureMotionAccounting::default()
    };
    for clip in clips {
        accounting.evidence_events += clip.events.len();
        match &clip.root_motion {
            SkyrimRootMotion::Stationary {
                source: SkyrimRootMotionSource::HavokReferenceFrame,
                ..
            }
            | SkyrimRootMotion::Sampled {
                source: SkyrimRootMotionSource::HavokReferenceFrame,
                ..
            } => accounting.havok_root_motion_clips += 1,
            SkyrimRootMotion::Stationary {
                source: SkyrimRootMotionSource::BoundAnims,
                ..
            }
            | SkyrimRootMotion::Sampled {
                source: SkyrimRootMotionSource::BoundAnims,
                ..
            } => accounting.bound_root_motion_clips += 1,
            SkyrimRootMotion::Unknown | SkyrimRootMotion::Unsupported { .. } => {
                accounting.unknown_root_motion_clips += 1;
            }
        }
        match clip.disposition {
            SkyrimClipDisposition::Role { .. } => accounting.role_clips += 1,
            SkyrimClipDisposition::SharedPaired { .. } => accounting.paired_clips += 1,
            SkyrimClipDisposition::SharedOverlay { .. } => accounting.overlay_clips += 1,
            SkyrimClipDisposition::Unsupported { .. } => accounting.unsupported_clips += 1,
            SkyrimClipDisposition::Ambiguous { .. } => accounting.ambiguous_clips += 1,
        }
    }
    let terminal = accounting.role_clips
        + accounting.paired_clips
        + accounting.overlay_clips
        + accounting.unsupported_clips
        + accounting.ambiguous_clips;
    if terminal != clips.len() {
        return Err(SkyrimCreatureMotionError::AccountingDrift {
            detail: format!("{terminal} terminal dispositions for {} clips", clips.len()),
        });
    }
    Ok(accounting)
}

fn build_family_readiness(
    family: &SkyrimFamilyMotionSet,
    clips: &[&SkyrimCatalogClip],
) -> SkyrimFamilyMotionReadiness {
    let roles = indexed_role_clips(clips);
    let template = expected_family_template(family, &roles, clips);
    let evidenced_roles = roles.keys().copied().collect::<BTreeSet<_>>();
    let requirements = template_role_requirements(template, &evidenced_roles);
    let ambiguous = clips
        .iter()
        .filter_map(|clip| match &clip.disposition {
            SkyrimClipDisposition::Ambiguous { roles, .. } => Some((*clip, roles.as_slice())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut selected_clip_ids = Vec::new();
    let mut blockers = Vec::new();
    for alternatives in requirements {
        let candidates = alternatives
            .iter()
            .flat_map(|role| {
                roles
                    .get(role)
                    .into_iter()
                    .flatten()
                    .map(move |clip| (*role, *clip))
            })
            .collect::<Vec<_>>();
        if let Some((_, clip)) = candidates.iter().find(|(role, clip)| {
            legacy_trigger_is_valid_for_role(*role, clip)
                && !matches!(
                    &clip.root_motion,
                    SkyrimRootMotion::Unknown | SkyrimRootMotion::Unsupported { .. }
                )
        }) {
            selected_clip_ids.push(clip.clip_id.clone());
            continue;
        }
        if candidates.is_empty() {
            let mut matched_ambiguous = false;
            for (clip, candidate_roles) in &ambiguous {
                let matching = candidate_roles
                    .iter()
                    .copied()
                    .filter(|role| alternatives.contains(role))
                    .collect::<Vec<_>>();
                if !matching.is_empty() {
                    matched_ambiguous = true;
                    blockers.push(SkyrimFamilyMotionBlocker::AmbiguousRole {
                        clip_id: clip.clip_id.clone(),
                        roles: matching,
                    });
                }
            }
            if !matched_ambiguous {
                blockers.push(SkyrimFamilyMotionBlocker::MissingRole {
                    alternatives: alternatives.to_vec(),
                });
            }
            continue;
        }
        let missing_event = candidates
            .iter()
            .filter(|(role, clip)| {
                role_requires_trigger(*role) && disposition_trigger(clip).is_none()
            })
            .map(|(_, clip)| clip.clip_id.clone())
            .collect::<Vec<_>>();
        if !missing_event.is_empty() {
            blockers.push(SkyrimFamilyMotionBlocker::MissingTriggerEvent {
                role: candidates[0].0,
                clip_ids: missing_event,
            });
        }
        let unknown_motion = candidates
            .iter()
            .filter(|(_, clip)| {
                matches!(
                    &clip.root_motion,
                    SkyrimRootMotion::Unknown | SkyrimRootMotion::Unsupported { .. }
                )
            })
            .map(|(_, clip)| clip.clip_id.clone())
            .collect::<Vec<_>>();
        if !unknown_motion.is_empty() {
            blockers.push(SkyrimFamilyMotionBlocker::UnknownRootMotion {
                role: candidates[0].0,
                clip_ids: unknown_motion,
            });
        }
    }
    selected_clip_ids.sort();
    selected_clip_ids.dedup();
    SkyrimFamilyMotionReadiness {
        family_id: family.family_id.clone(),
        template,
        ready: blockers.is_empty(),
        selected_clip_ids,
        blockers,
    }
}

fn expected_family_template(
    family: &SkyrimFamilyMotionSet,
    roles: &BTreeMap<SkyrimCreatureMotionRole, Vec<&SkyrimCatalogClip>>,
    clips: &[&SkyrimCatalogClip],
) -> CreatureGraphTemplate {
    let has = |role| roles.contains_key(&role);
    let has_ground =
        has(SkyrimCreatureMotionRole::Idle) && has(SkyrimCreatureMotionRole::GroundLocomotion);
    let has_swim =
        has(SkyrimCreatureMotionRole::SwimIdle) && has(SkyrimCreatureMotionRole::SwimLocomotion);
    let has_flight =
        has(SkyrimCreatureMotionRole::FlyIdle) && has(SkyrimCreatureMotionRole::FlyLocomotion);
    let has_melee = has(SkyrimCreatureMotionRole::MeleeAttack);
    let has_ranged = [
        SkyrimCreatureMotionRole::RangedAttack,
        SkyrimCreatureMotionRole::ProjectileAttack,
        SkyrimCreatureMotionRole::SpellAttack,
    ]
    .iter()
    .any(|role| has(*role));
    let family_semantics =
        format!("{} {}", family.family_id, family.project_path).to_ascii_lowercase();
    let aquatic_only = !has_ground
        && has(SkyrimCreatureMotionRole::SwimLocomotion)
        && ["fish", "swim", "aquatic"]
            .iter()
            .any(|token| family_semantics.contains(token));
    let aerial_only = !has_ground
        && has(SkyrimCreatureMotionRole::FlyLocomotion)
        && ["bird", "dragon", "fly", "flight"]
            .iter()
            .any(|token| family_semantics.contains(token));

    if has(SkyrimCreatureMotionRole::StationaryIdle) {
        CreatureGraphTemplate::StationaryTurret
    } else if [
        SkyrimCreatureMotionRole::MechanicalStart,
        SkyrimCreatureMotionRole::MechanicalLoop,
        SkyrimCreatureMotionRole::MechanicalStop,
    ]
    .iter()
    .any(|role| roles.contains_key(role))
    {
        CreatureGraphTemplate::RobotContinuousAttack
    } else if has_ground && has_swim {
        CreatureGraphTemplate::GroundSwim
    } else if has_ground && has_flight {
        CreatureGraphTemplate::GroundFly
    } else if has_swim || aquatic_only {
        CreatureGraphTemplate::Swim
    } else if has_flight || aerial_only {
        CreatureGraphTemplate::Fly
    } else if passive_ground_semantics(family, roles, clips) {
        CreatureGraphTemplate::PassiveGround
    } else if has_melee && has_ranged {
        CreatureGraphTemplate::GroundMeleeRanged
    } else if has_ranged {
        CreatureGraphTemplate::GroundRangedProjectile
    } else {
        CreatureGraphTemplate::GroundMelee
    }
}

fn passive_ground_semantics(
    family: &SkyrimFamilyMotionSet,
    roles: &BTreeMap<SkyrimCreatureMotionRole, Vec<&SkyrimCatalogClip>>,
    clips: &[&SkyrimCatalogClip],
) -> bool {
    if !family.race_attacks.is_empty() || !roles.contains_key(&SkyrimCreatureMotionRole::Idle) {
        return false;
    }
    passive_ground_audit(family, None, clips).zero_attack_semantics
}

fn passive_ground_audit(
    family: &SkyrimFamilyMotionSet,
    inventory: Option<&SkyrimCreatureFamilyInventory>,
    clips: &[&SkyrimCatalogClip],
) -> SkyrimPassiveGroundAuditReceipt {
    let mut semantic_attack_locators = clips
        .iter()
        .flat_map(|clip| match &clip.disposition {
            SkyrimClipDisposition::Role { role, evidence, .. } if is_attack_role(*role) => evidence
                .iter()
                .map(|entry| entry.locator.clone())
                .collect::<Vec<_>>(),
            SkyrimClipDisposition::Ambiguous { roles, evidence }
                if roles.iter().any(|role| is_attack_role(*role)) =>
            {
                evidence
                    .iter()
                    .map(|entry| entry.locator.clone())
                    .collect::<Vec<_>>()
            }
            _ => Vec::new(),
        })
        .collect::<Vec<_>>();
    semantic_attack_locators.sort();
    semantic_attack_locators.dedup();
    let mut race_attack_events = family
        .race_attacks
        .iter()
        .map(|attack| attack.event.clone())
        .collect::<Vec<_>>();
    race_attack_events.sort_by_key(|event| event.to_ascii_lowercase());
    race_attack_events.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    let mut decoded_behavior_paths = inventory
        .map(|inventory| inventory.behavior_paths.clone())
        .unwrap_or_default();
    sort_dedup_paths(&mut decoded_behavior_paths);
    let mut decoded_clip_ids = clips
        .iter()
        .map(|clip| clip.clip_id.clone())
        .collect::<Vec<_>>();
    decoded_clip_ids.sort();
    decoded_clip_ids.dedup();
    SkyrimPassiveGroundAuditReceipt {
        family_id: family.family_id.clone(),
        project_path: family.project_path.clone(),
        decoded_behavior_paths,
        decoded_clip_ids,
        zero_attack_semantics: race_attack_events.is_empty() && semantic_attack_locators.is_empty(),
        race_attack_events,
        semantic_attack_locators,
    }
}

fn template_role_requirements(
    template: CreatureGraphTemplate,
    evidenced_roles: &BTreeSet<SkyrimCreatureMotionRole>,
) -> Vec<Vec<SkyrimCreatureMotionRole>> {
    use SkyrimCreatureMotionRole as Role;
    match template {
        CreatureGraphTemplate::PassiveGround => vec![vec![Role::Idle]],
        CreatureGraphTemplate::GroundMelee => vec![
            vec![Role::Idle],
            vec![Role::GroundLocomotion],
            vec![Role::MeleeAttack],
        ],
        CreatureGraphTemplate::GroundRangedProjectile => vec![
            vec![Role::Idle],
            vec![Role::GroundLocomotion],
            vec![
                Role::ProjectileAttack,
                Role::RangedAttack,
                Role::SpellAttack,
            ],
        ],
        CreatureGraphTemplate::GroundMeleeRanged => vec![
            vec![Role::Idle],
            vec![Role::GroundLocomotion],
            vec![Role::MeleeAttack],
            vec![
                Role::ProjectileAttack,
                Role::RangedAttack,
                Role::SpellAttack,
            ],
        ],
        CreatureGraphTemplate::GroundSwim => {
            let mut requirements = vec![
                vec![Role::Idle],
                vec![Role::GroundLocomotion],
                vec![Role::SwimIdle],
                vec![Role::SwimLocomotion],
            ];
            append_evidenced_attack_requirements(&mut requirements, evidenced_roles);
            requirements
        }
        CreatureGraphTemplate::GroundFly => {
            let mut requirements = vec![
                vec![Role::Idle],
                vec![Role::GroundLocomotion],
                vec![Role::FlyIdle],
                vec![Role::FlyLocomotion],
            ];
            append_evidenced_attack_requirements(&mut requirements, evidenced_roles);
            requirements
        }
        CreatureGraphTemplate::Swim => {
            let mut requirements = vec![
                vec![Role::SwimIdle, Role::Idle],
                vec![Role::SwimLocomotion, Role::GroundLocomotion],
            ];
            append_evidenced_attack_requirements(&mut requirements, evidenced_roles);
            requirements
        }
        CreatureGraphTemplate::Fly => {
            let mut requirements = vec![
                vec![Role::FlyIdle, Role::Idle],
                vec![Role::FlyLocomotion, Role::GroundLocomotion],
            ];
            append_evidenced_attack_requirements(&mut requirements, evidenced_roles);
            requirements
        }
        CreatureGraphTemplate::StationaryTurret => vec![
            vec![Role::StationaryIdle],
            vec![
                Role::ProjectileAttack,
                Role::RangedAttack,
                Role::SpellAttack,
            ],
        ],
        CreatureGraphTemplate::RobotContinuousAttack => vec![
            vec![Role::Idle],
            vec![Role::MechanicalStart],
            vec![Role::MechanicalLoop],
            vec![Role::MechanicalStop],
        ],
    }
}

fn append_evidenced_attack_requirements(
    requirements: &mut Vec<Vec<SkyrimCreatureMotionRole>>,
    evidenced_roles: &BTreeSet<SkyrimCreatureMotionRole>,
) {
    use SkyrimCreatureMotionRole as Role;
    if evidenced_roles.contains(&Role::MeleeAttack) {
        requirements.push(vec![Role::MeleeAttack]);
    }
    if [
        Role::ProjectileAttack,
        Role::RangedAttack,
        Role::SpellAttack,
    ]
    .iter()
    .any(|role| evidenced_roles.contains(role))
    {
        requirements.push(vec![
            Role::ProjectileAttack,
            Role::RangedAttack,
            Role::SpellAttack,
        ]);
    }
}

fn build_living_family_evidence(
    family: &SkyrimFamilyMotionSet,
    inventory: Option<&SkyrimCreatureFamilyInventory>,
    clips: &[&SkyrimCatalogClip],
) -> SkyrimLivingFamilyEvidence {
    let mut template_locators =
        Vec::<(CreatureGraphTemplate, BTreeSet<SkyrimMotionSourceLocator>)>::new();
    let mut evidenced_roles = BTreeSet::new();
    for clip in clips {
        let role_evidence = match &clip.disposition {
            SkyrimClipDisposition::Role { role, evidence, .. } => {
                vec![(*role, evidence.as_slice())]
            }
            SkyrimClipDisposition::Ambiguous { roles, evidence } => roles
                .iter()
                .map(|role| (*role, evidence.as_slice()))
                .collect(),
            _ => Vec::new(),
        };
        for (role, evidence) in role_evidence {
            evidenced_roles.insert(role);
            let Some(template) = template_from_evidenced_role(role) else {
                continue;
            };
            let index = template_locators
                .iter()
                .position(|(candidate, _)| *candidate == template)
                .unwrap_or_else(|| {
                    template_locators.push((template, BTreeSet::new()));
                    template_locators.len() - 1
                });
            template_locators[index]
                .1
                .extend(evidence.iter().map(|entry| entry.locator.clone()));
        }
    }
    template_locators.sort_by_key(|(template, _)| template_sort_key(*template));
    let roles = indexed_role_clips(clips);
    let template = SkyrimLivingTemplateDisposition::Proven {
        template: expected_family_template(family, &roles, clips),
        locators: template_locators
            .into_iter()
            .flat_map(|(_, locators)| locators)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    };
    let required_roles = match &template {
        SkyrimLivingTemplateDisposition::Proven { template, .. } => {
            template_role_requirements(*template, &evidenced_roles)
                .into_iter()
                .map(|alternatives| required_role_disposition(&alternatives, inventory, clips))
                .collect()
        }
        SkyrimLivingTemplateDisposition::Ambiguous { .. }
        | SkyrimLivingTemplateDisposition::Unsupported { .. } => Vec::new(),
    };
    let ready = matches!(template, SkyrimLivingTemplateDisposition::Proven { .. })
        && required_roles
            .iter()
            .all(|role| matches!(role, SkyrimRequiredRoleDisposition::Ready { .. }));
    SkyrimLivingFamilyEvidence {
        family_id: family.family_id.clone(),
        project_path: family.project_path.clone(),
        template,
        required_roles,
        disposition: if ready {
            SkyrimLivingFamilyDisposition::Ready
        } else {
            SkyrimLivingFamilyDisposition::Blocked
        },
    }
}

fn multimodal_template(
    candidates: &[CreatureGraphTemplate],
    evidenced_roles: &BTreeSet<SkyrimCreatureMotionRole>,
) -> Option<CreatureGraphTemplate> {
    let has = |template| candidates.contains(&template);
    let has_ground = has(CreatureGraphTemplate::PassiveGround)
        || has(CreatureGraphTemplate::GroundMelee)
        || has(CreatureGraphTemplate::GroundRangedProjectile);
    let has_ground_locomotion =
        evidenced_roles.contains(&SkyrimCreatureMotionRole::GroundLocomotion);
    if !has_ground_locomotion
        && has(CreatureGraphTemplate::Swim)
        && !has(CreatureGraphTemplate::Fly)
    {
        return Some(CreatureGraphTemplate::Swim);
    }
    if !has_ground_locomotion
        && has(CreatureGraphTemplate::Fly)
        && !has(CreatureGraphTemplate::Swim)
    {
        return Some(CreatureGraphTemplate::Fly);
    }
    if has_ground && has(CreatureGraphTemplate::Swim) && !has(CreatureGraphTemplate::Fly) {
        return Some(CreatureGraphTemplate::GroundSwim);
    }
    if has_ground && has(CreatureGraphTemplate::Fly) && !has(CreatureGraphTemplate::Swim) {
        return Some(CreatureGraphTemplate::GroundFly);
    }
    if has(CreatureGraphTemplate::GroundMelee)
        && has(CreatureGraphTemplate::GroundRangedProjectile)
        && !has(CreatureGraphTemplate::Swim)
        && !has(CreatureGraphTemplate::Fly)
    {
        return Some(CreatureGraphTemplate::GroundMeleeRanged);
    }
    if has(CreatureGraphTemplate::GroundMelee)
        && !has(CreatureGraphTemplate::GroundRangedProjectile)
        && !has(CreatureGraphTemplate::Swim)
        && !has(CreatureGraphTemplate::Fly)
    {
        return Some(CreatureGraphTemplate::GroundMelee);
    }
    if has(CreatureGraphTemplate::GroundRangedProjectile)
        && !has(CreatureGraphTemplate::GroundMelee)
        && !has(CreatureGraphTemplate::Swim)
        && !has(CreatureGraphTemplate::Fly)
    {
        return Some(CreatureGraphTemplate::GroundRangedProjectile);
    }
    None
}

fn template_sort_key(template: CreatureGraphTemplate) -> u8 {
    match template {
        CreatureGraphTemplate::PassiveGround => 0,
        CreatureGraphTemplate::GroundMelee => 1,
        CreatureGraphTemplate::GroundRangedProjectile => 2,
        CreatureGraphTemplate::GroundMeleeRanged => 3,
        CreatureGraphTemplate::GroundSwim => 4,
        CreatureGraphTemplate::GroundFly => 5,
        CreatureGraphTemplate::Swim => 6,
        CreatureGraphTemplate::Fly => 7,
        CreatureGraphTemplate::StationaryTurret => 8,
        CreatureGraphTemplate::RobotContinuousAttack => 9,
    }
}

fn template_from_evidenced_role(role: SkyrimCreatureMotionRole) -> Option<CreatureGraphTemplate> {
    use SkyrimCreatureMotionRole as Role;
    match role {
        Role::MeleeAttack => Some(CreatureGraphTemplate::GroundMelee),
        Role::RangedAttack | Role::ProjectileAttack | Role::SpellAttack => {
            Some(CreatureGraphTemplate::GroundRangedProjectile)
        }
        Role::SwimIdle | Role::SwimLocomotion => Some(CreatureGraphTemplate::Swim),
        Role::FlyIdle | Role::FlyLocomotion => Some(CreatureGraphTemplate::Fly),
        Role::StationaryIdle => Some(CreatureGraphTemplate::StationaryTurret),
        Role::MechanicalStart | Role::MechanicalLoop | Role::MechanicalStop => {
            Some(CreatureGraphTemplate::RobotContinuousAttack)
        }
        Role::GroundLocomotion => Some(CreatureGraphTemplate::PassiveGround),
        Role::Idle | Role::TurnLeft | Role::TurnRight | Role::Turn | Role::Hurt | Role::Death => {
            None
        }
    }
}

fn required_role_disposition(
    alternatives: &[SkyrimCreatureMotionRole],
    _inventory: Option<&SkyrimCreatureFamilyInventory>,
    clips: &[&SkyrimCatalogClip],
) -> SkyrimRequiredRoleDisposition {
    let mut proven = Vec::new();
    let mut ambiguous = Vec::new();
    for clip in clips {
        match &clip.disposition {
            SkyrimClipDisposition::Role { role, evidence, .. } if alternatives.contains(role) => {
                proven.push(living_candidate(*role, clip, evidence));
            }
            SkyrimClipDisposition::Ambiguous { roles, evidence }
                if roles.iter().any(|role| alternatives.contains(role)) =>
            {
                for role in roles.iter().filter(|role| alternatives.contains(role)) {
                    ambiguous.push(living_candidate(*role, clip, evidence));
                }
            }
            _ => {}
        }
    }
    sort_living_candidates(&mut proven);
    sort_living_candidates(&mut ambiguous);
    proven.extend(ambiguous);
    sort_living_candidates(&mut proven);
    proven.dedup_by(|left, right| left.clip_id == right.clip_id);
    if proven.is_empty() {
        return SkyrimRequiredRoleDisposition::MissingRole {
            alternatives: alternatives.to_vec(),
        };
    }
    let eligible = proven
        .iter()
        .filter(|candidate| candidate_is_usable_for_template(candidate))
        .cloned()
        .collect::<Vec<_>>();
    let allows_multiple = role_set_allows_multiple_clips(alternatives);
    if allows_multiple && !eligible.is_empty() {
        let mut accepted = eligible.into_iter();
        return SkyrimRequiredRoleDisposition::Ready {
            alternatives: alternatives.to_vec(),
            selected: accepted
                .next()
                .expect("a required multi-clip role has at least one proven candidate"),
            additional: accepted.collect(),
        };
    }
    if !allows_multiple && !eligible.is_empty() {
        let selected =
            select_semantic_primary_candidate(&eligible).unwrap_or_else(|| eligible[0].clone());
        return SkyrimRequiredRoleDisposition::Ready {
            alternatives: alternatives.to_vec(),
            selected,
            additional: Vec::new(),
        };
    }
    unready_required_role_disposition(alternatives, proven)
}

fn candidate_is_usable_for_template(candidate: &SkyrimLivingClipCandidate) -> bool {
    !matches!(candidate.root_motion, SkyrimRootMotion::Unsupported { .. })
        && (candidate.role != SkyrimCreatureMotionRole::StationaryIdle
            || matches!(candidate.root_motion, SkyrimRootMotion::Stationary { .. }))
}

fn unready_required_role_disposition(
    alternatives: &[SkyrimCreatureMotionRole],
    proven: Vec<SkyrimLivingClipCandidate>,
) -> SkyrimRequiredRoleDisposition {
    if proven.iter().any(|candidate| {
        matches!(candidate.root_motion, SkyrimRootMotion::Unsupported { .. })
            || (alternatives.contains(&SkyrimCreatureMotionRole::StationaryIdle)
                && !matches!(candidate.root_motion, SkyrimRootMotion::Stationary { .. }))
    }) {
        return SkyrimRequiredRoleDisposition::UnsupportedRootMotion {
            alternatives: alternatives.to_vec(),
            candidates: proven,
        };
    }
    if proven.iter().any(|candidate| {
        matches!(candidate.root_motion, SkyrimRootMotion::Unknown)
            || candidate.root_motion_locator.is_none()
    }) {
        return SkyrimRequiredRoleDisposition::UnknownRootMotion {
            alternatives: alternatives.to_vec(),
            candidates: proven,
        };
    }
    SkyrimRequiredRoleDisposition::MissingTrigger {
        alternatives: alternatives.to_vec(),
        candidates: proven,
    }
}

fn role_set_allows_multiple_clips(alternatives: &[SkyrimCreatureMotionRole]) -> bool {
    !alternatives.is_empty() && alternatives.iter().all(|role| is_attack_role(*role))
}

fn select_semantic_primary_candidate(
    candidates: &[SkyrimLivingClipCandidate],
) -> Option<SkyrimLivingClipCandidate> {
    let ranked = candidates
        .iter()
        .filter_map(|candidate| semantic_candidate_rank(candidate).map(|rank| (rank, candidate)))
        .collect::<Vec<_>>();
    let best = ranked.iter().map(|(rank, _)| *rank).min()?;
    let selected = ranked
        .into_iter()
        .filter(|(rank, _)| *rank == best)
        .map(|(_, candidate)| candidate)
        .collect::<Vec<_>>();
    let [selected] = selected.as_slice() else {
        return None;
    };
    Some((*selected).clone())
}

fn semantic_candidate_rank(candidate: &SkyrimLivingClipCandidate) -> Option<(u8, u8, u32)> {
    let texts = candidate
        .role_locators
        .iter()
        .flat_map(locator_semantic_texts)
        .map(normalized_words)
        .collect::<Vec<_>>();
    let initial = texts
        .iter()
        .any(|words| words.contains("hkb state machine start state id"));
    let semantic_kind = texts
        .iter()
        .filter_map(|words| {
            if contains_word(&words, "default") || contains_word(&words, "primary") {
                return Some(0);
            }
            if candidate.role == SkyrimCreatureMotionRole::Idle
                && contains_word(words, "idle")
                && !["aggro", "combat", "special"]
                    .iter()
                    .any(|modifier| contains_word(words, modifier))
            {
                return Some(1);
            }
            if candidate.role == SkyrimCreatureMotionRole::GroundLocomotion
                && contains_word(&words, "forward")
            {
                let accelerated = ["fast", "sprint"]
                    .iter()
                    .any(|modifier| contains_word(words, modifier));
                return Some(if accelerated { 2 } else { 1 });
            }
            None
        })
        .min();
    let semantic_ordinal = texts
        .iter()
        .filter_map(|words| semantic_ordinal(words))
        .min();
    let semantic = match (semantic_kind, semantic_ordinal) {
        (Some(kind), ordinal) => Some((kind, ordinal.unwrap_or(u32::MAX))),
        (None, Some(ordinal)) => Some((3, ordinal)),
        (None, None) => None,
    };
    match (initial, semantic) {
        (true, Some((kind, ordinal))) => Some((0, kind, ordinal)),
        (true, None) => Some((0, 3, 0)),
        (false, Some((kind, ordinal))) => Some((1, kind, ordinal)),
        (false, None) => None,
    }
}

fn locator_semantic_texts(locator: &SkyrimMotionSourceLocator) -> Vec<&str> {
    match locator {
        SkyrimMotionSourceLocator::RaceAttackEvent { event, .. }
        | SkyrimMotionSourceLocator::BehaviorTransition { event, .. }
        | SkyrimMotionSourceLocator::ClipAnnotation { event, .. } => vec![event],
        SkyrimMotionSourceLocator::BehaviorGenerator { node, .. } => vec![node],
        SkyrimMotionSourceLocator::AnimationDataSequence { sequence_name, .. } => {
            vec![sequence_name]
        }
        SkyrimMotionSourceLocator::AnimationDataEvent {
            sequence_name,
            event,
            ..
        } => vec![sequence_name, event],
        SkyrimMotionSourceLocator::AnimationBinding { member, .. } => vec![member],
    }
}

fn semantic_ordinal(words: &str) -> Option<u32> {
    words.split_whitespace().filter_map(trailing_ordinal).min()
}

fn trailing_ordinal(word: &str) -> Option<u32> {
    let digits = word
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}

fn select_semantic_catalog_clip<'a>(
    candidates: &[&'a SkyrimCatalogClip],
    role: SkyrimCreatureMotionRole,
) -> Option<&'a SkyrimCatalogClip> {
    if let [candidate] = candidates {
        return Some(*candidate);
    }
    let living = candidates
        .iter()
        .filter_map(|clip| match &clip.disposition {
            SkyrimClipDisposition::Role { evidence, .. } => {
                Some(living_candidate(role, clip, evidence))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let selected = select_semantic_primary_candidate(&living)?;
    candidates
        .iter()
        .find(|clip| clip.clip_id == selected.clip_id)
        .copied()
}

fn behavior_group_clip_paths(
    group: &SkyrimBehaviorGeneratorGroupEvidence,
) -> Option<BTreeSet<String>> {
    let mut paths = BTreeSet::new();
    let complete = match group {
        SkyrimBehaviorGeneratorGroupEvidence::ManualSelector(selector) => {
            selector.children.iter().all(|child| {
                collect_behavior_generator_tree_clip_paths(&child.generator_tree, &mut paths)
            })
        }
        SkyrimBehaviorGeneratorGroupEvidence::Blender(blender) => {
            blender.children.iter().all(|child| {
                collect_behavior_generator_tree_clip_paths(&child.generator_tree, &mut paths)
            })
        }
        SkyrimBehaviorGeneratorGroupEvidence::StateMachine(state_machine) => {
            state_machine.machine.states.iter().all(|state| {
                state.generator.as_ref().is_some_and(|generator| {
                    collect_behavior_generator_tree_clip_paths(generator, &mut paths)
                })
            })
        }
    };
    if !complete {
        return None;
    }
    (!paths.is_empty()).then_some(paths)
}

fn collect_behavior_generator_tree_clip_paths(
    node: &SkyrimBehaviorGeneratorNodeEvidence,
    paths: &mut BTreeSet<String>,
) -> bool {
    match node {
        SkyrimBehaviorGeneratorNodeEvidence::Clip {
            resolved_clip_path, ..
        } => {
            let Some(path) = resolved_clip_path else {
                return false;
            };
            paths.insert(path_key(path));
            true
        }
        SkyrimBehaviorGeneratorNodeEvidence::ManualSelector { children, .. } => children
            .iter()
            .all(|child| collect_behavior_generator_tree_clip_paths(child, paths)),
        SkyrimBehaviorGeneratorNodeEvidence::Blender { children, .. } => children
            .iter()
            .all(|child| collect_behavior_generator_tree_clip_paths(&child.generator, paths)),
        SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine) => {
            machine.states.iter().all(|state| {
                state.generator.as_ref().is_some_and(|generator| {
                    collect_behavior_generator_tree_clip_paths(generator, paths)
                })
            })
        }
        SkyrimBehaviorGeneratorNodeEvidence::ModifierGenerator { generator, .. } => generator
            .as_ref()
            .is_some_and(|generator| collect_behavior_generator_tree_clip_paths(generator, paths)),
        SkyrimBehaviorGeneratorNodeEvidence::Missing { .. }
        | SkyrimBehaviorGeneratorNodeEvidence::Unsupported { .. }
        | SkyrimBehaviorGeneratorNodeEvidence::Cycle { .. } => false,
    }
}

fn living_candidate(
    role: SkyrimCreatureMotionRole,
    clip: &SkyrimCatalogClip,
    evidence: &[SkyrimMotionRoleEvidence],
) -> SkyrimLivingClipCandidate {
    let triggers = evidence
        .iter()
        .filter_map(|entry| entry.trigger.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    SkyrimLivingClipCandidate {
        role,
        clip_id: clip.clip_id.clone(),
        clip_path: clip.clip_path.clone(),
        triggers,
        root_motion: clip.root_motion.clone(),
        root_motion_locator: evidence
            .iter()
            .find_map(|entry| entry.root_motion_locator.clone()),
        role_locators: evidence
            .iter()
            .map(|entry| entry.locator.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }
}

fn candidate_is_ready(candidate: &SkyrimLivingClipCandidate) -> bool {
    ((!role_requires_trigger(candidate.role) && candidate.triggers.is_empty())
        || (role_requires_trigger(candidate.role) && !candidate.triggers.is_empty()))
        && candidate.root_motion_locator.is_some()
        && matches!(
            candidate.root_motion,
            SkyrimRootMotion::Stationary { .. } | SkyrimRootMotion::Sampled { .. }
        )
        && (candidate.role != SkyrimCreatureMotionRole::StationaryIdle
            || matches!(candidate.root_motion, SkyrimRootMotion::Stationary { .. }))
}

fn role_requires_trigger(role: SkyrimCreatureMotionRole) -> bool {
    !matches!(
        role,
        SkyrimCreatureMotionRole::Idle
            | SkyrimCreatureMotionRole::SwimIdle
            | SkyrimCreatureMotionRole::FlyIdle
            | SkyrimCreatureMotionRole::StationaryIdle
    )
}

fn legacy_trigger_is_valid_for_role(
    role: SkyrimCreatureMotionRole,
    clip: &SkyrimCatalogClip,
) -> bool {
    disposition_trigger(clip).is_some() == role_requires_trigger(role)
}

fn sort_living_candidates(candidates: &mut Vec<SkyrimLivingClipCandidate>) {
    candidates.sort_by(|left, right| {
        left.clip_id
            .cmp(&right.clip_id)
            .then_with(|| left.role.cmp(&right.role))
    });
}

fn append_ground_role_mappings(
    mappings: &mut Vec<(SkyrimCreatureMotionRole, CreatureClipRole)>,
    role_clips: &BTreeMap<SkyrimCreatureMotionRole, Vec<&SkyrimCatalogClip>>,
) {
    mappings.push((SkyrimCreatureMotionRole::Idle, CreatureClipRole::Idle));
    mappings.push((
        SkyrimCreatureMotionRole::GroundLocomotion,
        CreatureClipRole::GroundForward,
    ));
    for (source, target) in [
        (
            SkyrimCreatureMotionRole::TurnLeft,
            CreatureClipRole::TurnLeft90,
        ),
        (
            SkyrimCreatureMotionRole::TurnRight,
            CreatureClipRole::TurnRight90,
        ),
    ] {
        if role_clips.contains_key(&source) {
            mappings.push((source, target));
        }
    }
}

fn append_melee_role_mapping(mappings: &mut Vec<(SkyrimCreatureMotionRole, CreatureClipRole)>) {
    mappings.push((
        SkyrimCreatureMotionRole::MeleeAttack,
        CreatureClipRole::MeleeAttack,
    ));
}

fn append_ranged_role_mappings(
    mappings: &mut Vec<(SkyrimCreatureMotionRole, CreatureClipRole)>,
    role_clips: &BTreeMap<SkyrimCreatureMotionRole, Vec<&SkyrimCatalogClip>>,
    required: bool,
) {
    let initial_len = mappings.len();
    for source in [
        SkyrimCreatureMotionRole::ProjectileAttack,
        SkyrimCreatureMotionRole::RangedAttack,
        SkyrimCreatureMotionRole::SpellAttack,
    ] {
        if role_clips.contains_key(&source) {
            mappings.push((source, CreatureClipRole::ProjectileAttack));
        }
    }
    if required && mappings.len() == initial_len {
        mappings.push((
            SkyrimCreatureMotionRole::ProjectileAttack,
            CreatureClipRole::ProjectileAttack,
        ));
    }
}

fn adapter_primary_idle_role(template: CreatureGraphTemplate) -> CreatureClipRole {
    match template {
        CreatureGraphTemplate::Swim => CreatureClipRole::SwimIdle,
        CreatureGraphTemplate::Fly => CreatureClipRole::FlyIdle,
        CreatureGraphTemplate::StationaryTurret => CreatureClipRole::StationaryIdle,
        _ => CreatureClipRole::Idle,
    }
}

fn reconciled_attack_template(
    template: CreatureGraphTemplate,
    roles: &[CapabilityClipRole],
    candidate_attack_bindings: &[CapabilityCandidateAttackBinding],
) -> CreatureGraphTemplate {
    if !matches!(
        template,
        CreatureGraphTemplate::GroundMelee
            | CreatureGraphTemplate::GroundRangedProjectile
            | CreatureGraphTemplate::GroundMeleeRanged
    ) {
        return template;
    }
    let has_role = |role| {
        roles.iter().any(|candidate| candidate.role == role)
            || candidate_attack_bindings
                .iter()
                .any(|candidate| candidate.role == role)
    };
    match (
        has_role(CreatureClipRole::MeleeAttack),
        has_role(CreatureClipRole::ProjectileAttack),
    ) {
        (true, true) => CreatureGraphTemplate::GroundMeleeRanged,
        (false, true) => CreatureGraphTemplate::GroundRangedProjectile,
        _ => CreatureGraphTemplate::GroundMelee,
    }
}

fn capability_generator_source(
    family: &SkyrimFamilyMotionSet,
    behavior_path: &str,
    source: &SkyrimBehaviorObjectEvidence,
) -> Result<CapabilityGeneratorSourceObject, SkyrimCreatureMotionError> {
    let name =
        source
            .name
            .clone()
            .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "CapabilityGraphManifest",
                detail: format!(
                    "{} object {} {} has no exact source name",
                    behavior_path, source.object_index, source.class_name
                ),
            })?;
    let object_index = u32::try_from(source.object_index).map_err(|_| {
        SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "CapabilityGraphManifest",
            detail: format!(
                "{} object index {} does not fit the target receipt",
                behavior_path, source.object_index
            ),
        }
    })?;
    Ok(CapabilityGeneratorSourceObject {
        behavior_path: canonical_runtime_path(behavior_path),
        object_index,
        class_name: source.class_name.clone(),
        name,
    })
}

fn capability_generator_anonymous_source(
    family: &SkyrimFamilyMotionSet,
    behavior_path: &str,
    source: &SkyrimBehaviorObjectEvidence,
) -> Result<CapabilityGeneratorAnonymousSourceObject, SkyrimCreatureMotionError> {
    let object_index = u32::try_from(source.object_index).map_err(|_| {
        SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "CapabilityGraphManifest",
            detail: format!(
                "{} object index {} does not fit the target receipt",
                behavior_path, source.object_index
            ),
        }
    })?;
    Ok(CapabilityGeneratorAnonymousSourceObject {
        behavior_path: canonical_runtime_path(behavior_path),
        object_index,
        class_name: source.class_name.clone(),
    })
}

fn capability_generator_binding(
    family: &SkyrimFamilyMotionSet,
    binding: &SkyrimBehaviorVariableBindingEvidence,
) -> Result<CapabilityGeneratorVariableBinding, SkyrimCreatureMotionError> {
    let variable_index = i32::try_from(binding.variable_index).map_err(|_| {
        SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "CapabilityGraphManifest",
            detail: format!(
                "variable index {} does not fit the recursive generator receipt",
                binding.variable_index
            ),
        }
    })?;
    let variable_name = binding.variable_name.clone().ok_or_else(|| {
        SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "CapabilityGraphManifest",
            detail: format!(
                "variable {variable_index} bound to {:?} has no decoded name",
                binding.member_path
            ),
        }
    })?;
    let source_variable_type = match binding.variable_type {
        Some(0) => CapabilitySourceVariableType::Bool,
        Some(1) => CapabilitySourceVariableType::Int8,
        Some(2) => CapabilitySourceVariableType::Int16,
        Some(3) => CapabilitySourceVariableType::Int32,
        Some(4) => CapabilitySourceVariableType::Real,
        value => {
            return adapter_unavailable(
                family,
                "CapabilityGraphManifest",
                format!(
                    "variable {variable_index} bound to {:?} has unsupported decoded type {value:?}",
                    binding.member_path
                ),
            );
        }
    };
    let binding_type = match binding.binding_type {
        0 => CapabilityVariableBindingType::Variable,
        1 => CapabilityVariableBindingType::CharacterProperty,
        value => {
            return adapter_unavailable(
                family,
                "CapabilityGraphManifest",
                format!(
                    "variable {variable_index} bound to {:?} has unsupported binding type {value}",
                    binding.member_path
                ),
            );
        }
    };
    Ok(CapabilityGeneratorVariableBinding {
        member_path: binding.member_path.clone(),
        variable_index,
        variable_name,
        bit_index: binding.bit_index,
        binding_type,
        source_variable_type,
        initial_word_value: binding.initial_word_value,
    })
}

fn capability_generator_bindings(
    family: &SkyrimFamilyMotionSet,
    bindings: &[SkyrimBehaviorVariableBindingEvidence],
) -> Result<Vec<CapabilityGeneratorVariableBinding>, SkyrimCreatureMotionError> {
    bindings
        .iter()
        .map(|binding| capability_generator_binding(family, binding))
        .collect()
}

fn capability_event_ref(
    family: &SkyrimFamilyMotionSet,
    source_id: Option<i32>,
    event_name: Option<&str>,
    label: &str,
) -> Result<CapabilityGeneratorEventRef, SkyrimCreatureMotionError> {
    let source_id = source_id.ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
        family_id: family.family_id.clone(),
        target: "CapabilityGraphManifest",
        detail: format!("{label} has no decoded event ID"),
    })?;
    if (source_id == -1 && event_name.is_some()) || (source_id >= 0 && event_name.is_none()) {
        return adapter_unavailable(
            family,
            "CapabilityGraphManifest",
            format!("{label} event ID {source_id} has inconsistent name {event_name:?}"),
        );
    }
    if source_id < -1 {
        return adapter_unavailable(
            family,
            "CapabilityGraphManifest",
            format!("{label} has unsupported negative event ID {source_id}"),
        );
    }
    Ok(CapabilityGeneratorEventRef {
        source_id,
        event_name: event_name.map(str::to_string),
    })
}

fn capability_event_property(
    family: &SkyrimFamilyMotionSet,
    behavior_path: &str,
    property: &SkyrimBehaviorEventPropertyEvidence,
    label: &str,
) -> Result<CapabilityGeneratorEventProperty, SkyrimCreatureMotionError> {
    Ok(CapabilityGeneratorEventProperty {
        event: capability_event_ref(
            family,
            property.event_id,
            property.event_name.as_deref(),
            label,
        )?,
        payload: property
            .payload
            .as_ref()
            .map(|source| capability_generator_source(family, behavior_path, source))
            .transpose()?,
    })
}

fn capability_interval(
    family: &SkyrimFamilyMotionSet,
    interval: &SkyrimBehaviorTransitionIntervalEvidence,
    label: &str,
) -> Result<CapabilityGeneratorInterval, SkyrimCreatureMotionError> {
    Ok(CapabilityGeneratorInterval {
        enter_event: capability_event_ref(
            family,
            interval.enter_event_id,
            interval.enter_event_name.as_deref(),
            &format!("{label}.enter"),
        )?,
        exit_event: capability_event_ref(
            family,
            interval.exit_event_id,
            interval.exit_event_name.as_deref(),
            &format!("{label}.exit"),
        )?,
        enter_time_bits: interval.enter_time_bits.ok_or_else(|| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "CapabilityGraphManifest",
                detail: format!("{label} has no decoded enterTime"),
            }
        })?,
        exit_time_bits: interval.exit_time_bits.ok_or_else(|| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "CapabilityGraphManifest",
                detail: format!("{label} has no decoded exitTime"),
            }
        })?,
    })
}

fn capability_blending_transition_effect(
    family: &SkyrimFamilyMotionSet,
    behavior_path: &str,
    effect: &SkyrimBehaviorBlendingTransitionEffectEvidence,
    label: &str,
) -> Result<CapabilityGeneratorBlendingTransitionEffect, SkyrimCreatureMotionError> {
    let required_u8 = |value: Option<i32>, field: &str| {
        u8::try_from(required_i32(family, value, &format!("{label}.{field}"))?).map_err(|_| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "CapabilityGraphManifest",
                detail: format!("{label}.{field} does not fit u8"),
            }
        })
    };
    let required_u16 = |value: Option<i32>, field: &str| {
        u16::try_from(required_i32(family, value, &format!("{label}.{field}"))?).map_err(|_| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "CapabilityGraphManifest",
                detail: format!("{label}.{field} does not fit u16"),
            }
        })
    };
    let required_i16 = |value: Option<i32>, field: &str| {
        i16::try_from(required_i32(family, value, &format!("{label}.{field}"))?).map_err(|_| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "CapabilityGraphManifest",
                detail: format!("{label}.{field} does not fit i16"),
            }
        })
    };
    Ok(CapabilityGeneratorBlendingTransitionEffect {
        source: capability_generator_source(family, behavior_path, &effect.source)?,
        bindings: capability_generator_bindings(family, &effect.variable_bindings)?,
        self_transition_mode: required_u8(effect.self_transition_mode, "selfTransitionMode")?,
        event_mode: required_u8(effect.event_mode, "eventMode")?,
        duration_bits: effect.duration_bits.ok_or_else(|| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "CapabilityGraphManifest",
                detail: format!("{label}.duration has no decoded value"),
            }
        })?,
        to_generator_start_fraction_bits: effect.to_generator_start_time_fraction_bits.ok_or_else(
            || SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "CapabilityGraphManifest",
                detail: format!("{label}.toGeneratorStartTimeFraction has no decoded value"),
            },
        )?,
        flags: required_u16(effect.flags, "flags")?,
        end_mode: required_u8(effect.end_mode, "endMode")?,
        blend_curve: required_u8(effect.blend_curve, "blendCurve")?,
        alignment_bone: required_i16(effect.alignment_bone, "alignmentBone")?,
    })
}

fn capability_transition_array(
    family: &SkyrimFamilyMotionSet,
    behavior_path: &str,
    array: &SkyrimBehaviorTransitionArrayEvidence,
) -> Result<CapabilityGeneratorTransitionArray, SkyrimCreatureMotionError> {
    let mut transitions = Vec::with_capacity(array.transitions.len());
    for transition in &array.transitions {
        let label = format!(
            "{} transition {}#{}",
            behavior_path, array.source.object_index, transition.ordinal
        );
        let trigger_interval = transition.trigger_interval.as_ref().ok_or_else(|| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "CapabilityGraphManifest",
                detail: format!("{label} has no decoded trigger interval"),
            }
        })?;
        let initiate_interval = transition.initiate_interval.as_ref().ok_or_else(|| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "CapabilityGraphManifest",
                detail: format!("{label} has no decoded initiate interval"),
            }
        })?;
        transitions.push(CapabilityGeneratorTransition {
            trigger_interval: capability_interval(
                family,
                trigger_interval,
                &format!("{label}.trigger"),
            )?,
            initiate_interval: capability_interval(
                family,
                initiate_interval,
                &format!("{label}.initiate"),
            )?,
            transition_effect: match (
                &transition.transition_effect,
                &transition.blending_transition_effect,
            ) {
                (None, None) => None,
                (Some(source), Some(effect)) if source == &effect.source => {
                    Some(capability_blending_transition_effect(
                        family,
                        behavior_path,
                        effect,
                        &format!("{label}.transitionEffect"),
                    )?)
                }
                (Some(source), Some(effect)) => {
                    return adapter_unavailable(
                        family,
                        "CapabilityGraphManifest",
                        format!(
                            "{label} transition effect object {} does not match decoded blending effect object {}",
                            source.object_index, effect.source.object_index
                        ),
                    );
                }
                (Some(source), None) => {
                    return adapter_unavailable(
                        family,
                        "CapabilityGraphManifest",
                        format!(
                            "{label} transition effect object {} uses unsupported class {}",
                            source.object_index, source.class_name
                        ),
                    );
                }
                (None, Some(effect)) => {
                    return adapter_unavailable(
                        family,
                        "CapabilityGraphManifest",
                        format!(
                            "{label} decoded blending effect object {} is not attached to the transition",
                            effect.source.object_index
                        ),
                    );
                }
            },
            condition: transition
                .condition
                .as_ref()
                .map(|source| capability_generator_source(family, behavior_path, source))
                .transpose()?,
            event: capability_event_ref(
                family,
                transition.event_id,
                transition.event_name.as_deref(),
                &format!("{label}.event"),
            )?,
            to_state_id: required_i32(family, transition.to_state_id, &format!("{label}.toState"))?,
            from_nested_state_id: required_i32(
                family,
                transition.from_nested_state_id,
                &format!("{label}.fromNestedState"),
            )?,
            to_nested_state_id: required_i32(
                family,
                transition.to_nested_state_id,
                &format!("{label}.toNestedState"),
            )?,
            priority: required_i32(family, transition.priority, &format!("{label}.priority"))?,
            flags: u32::try_from(required_i32(
                family,
                transition.flags,
                &format!("{label}.flags"),
            )?)
            .map_err(|_| SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "CapabilityGraphManifest",
                detail: format!("{label}.flags is negative"),
            })?,
        });
    }
    Ok(CapabilityGeneratorTransitionArray {
        source: capability_generator_source(family, behavior_path, &array.source)?,
        has_eventless_transitions: array.has_eventless_transitions,
        has_time_bounded_transitions: array.has_time_bounded_transitions,
        transitions,
    })
}

fn required_i32(
    family: &SkyrimFamilyMotionSet,
    value: Option<i32>,
    label: &str,
) -> Result<i32, SkyrimCreatureMotionError> {
    value.ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
        family_id: family.family_id.clone(),
        target: "CapabilityGraphManifest",
        detail: format!("{label} has no decoded value"),
    })
}

fn required_capability_value<T>(
    family: &SkyrimFamilyMotionSet,
    value: Option<T>,
    label: &str,
) -> Result<T, SkyrimCreatureMotionError> {
    value.ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
        family_id: family.family_id.clone(),
        target: "CapabilityGraphManifest",
        detail: format!("{label} has no decoded value"),
    })
}

fn capability_expression_variable_reference(
    family: &SkyrimFamilyMotionSet,
    reference: &SkyrimBehaviorExpressionVariableReferenceEvidence,
) -> Result<CapabilityGeneratorExpressionVariableReference, SkyrimCreatureMotionError> {
    let variable_index = i32::try_from(reference.variable_index).map_err(|_| {
        SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "CapabilityGraphManifest",
            detail: format!(
                "expression variable index {} does not fit the target receipt",
                reference.variable_index
            ),
        }
    })?;
    let source_variable_type = match reference.variable_type {
        Some(0) => CapabilitySourceVariableType::Bool,
        Some(1) => CapabilitySourceVariableType::Int8,
        Some(2) => CapabilitySourceVariableType::Int16,
        Some(3) => CapabilitySourceVariableType::Int32,
        Some(4) => CapabilitySourceVariableType::Real,
        value => {
            return adapter_unavailable(
                family,
                "CapabilityGraphManifest",
                format!(
                    "expression variable {variable_index} {:?} has unsupported decoded type {value:?}",
                    reference.variable_name
                ),
            );
        }
    };
    Ok(CapabilityGeneratorExpressionVariableReference {
        variable_index,
        variable_name: reference.variable_name.clone(),
        source_variable_type,
        initial_word_value: reference.initial_word_value,
    })
}

fn capability_modifier_node(
    family: &SkyrimFamilyMotionSet,
    behavior_path: &str,
    evidence: &SkyrimBehaviorModifierNodeEvidence,
) -> Result<CapabilityModifierNode, SkyrimCreatureMotionError> {
    match evidence {
        SkyrimBehaviorModifierNodeEvidence::ModifierList {
            source,
            user_data,
            enable,
            variable_bindings,
            modifiers,
        } => Ok(CapabilityModifierNode::List {
            source: capability_generator_source(family, behavior_path, source)?,
            bindings: capability_generator_bindings(family, variable_bindings)?,
            user_data: required_capability_value(family, *user_data, "modifierList.userData")?,
            enable: required_capability_value(family, *enable, "modifierList.enable")?,
            modifiers: modifiers
                .iter()
                .map(|modifier| capability_modifier_node(family, behavior_path, modifier))
                .collect::<Result<_, _>>()?,
        }),
        SkyrimBehaviorModifierNodeEvidence::Damping {
            source,
            user_data,
            enable,
            variable_bindings,
            k_p_bits,
            k_i_bits,
            k_d_bits,
            enable_scalar_damping,
            enable_vector_damping,
            raw_value_bits,
            damped_value_bits,
            raw_vector_bits,
            damped_vector_bits,
            vector_error_sum_bits,
            vector_previous_error_bits,
            error_sum_bits,
            previous_error_bits,
        } => Ok(CapabilityModifierNode::Damping {
            source: capability_generator_source(family, behavior_path, source)?,
            bindings: capability_generator_bindings(family, variable_bindings)?,
            user_data: required_capability_value(family, *user_data, "damping.userData")?,
            enable: required_capability_value(family, *enable, "damping.enable")?,
            k_p_bits: required_capability_value(family, *k_p_bits, "damping.kP")?,
            k_i_bits: required_capability_value(family, *k_i_bits, "damping.kI")?,
            k_d_bits: required_capability_value(family, *k_d_bits, "damping.kD")?,
            enable_scalar_damping: required_capability_value(
                family,
                *enable_scalar_damping,
                "damping.enableScalarDamping",
            )?,
            enable_vector_damping: required_capability_value(
                family,
                *enable_vector_damping,
                "damping.enableVectorDamping",
            )?,
            raw_value_bits: required_capability_value(family, *raw_value_bits, "damping.rawValue")?,
            damped_value_bits: required_capability_value(
                family,
                *damped_value_bits,
                "damping.dampedValue",
            )?,
            raw_vector_bits: required_capability_value(
                family,
                *raw_vector_bits,
                "damping.rawVector",
            )?,
            damped_vector_bits: required_capability_value(
                family,
                *damped_vector_bits,
                "damping.dampedVector",
            )?,
            vector_error_sum_bits: required_capability_value(
                family,
                *vector_error_sum_bits,
                "damping.vecErrorSum",
            )?,
            vector_previous_error_bits: required_capability_value(
                family,
                *vector_previous_error_bits,
                "damping.vecPreviousError",
            )?,
            error_sum_bits: required_capability_value(family, *error_sum_bits, "damping.errorSum")?,
            previous_error_bits: required_capability_value(
                family,
                *previous_error_bits,
                "damping.previousError",
            )?,
        }),
        SkyrimBehaviorModifierNodeEvidence::EvaluateExpression {
            source,
            user_data,
            enable,
            variable_bindings,
            expressions,
        } => {
            let expressions = expressions.as_ref().ok_or_else(|| {
                SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!(
                        "{} evaluate-expression modifier {} has no decoded expression array",
                        behavior_path, source.object_index
                    ),
                }
            })?;
            Ok(CapabilityModifierNode::EvaluateExpression {
                source: capability_generator_source(family, behavior_path, source)?,
                bindings: capability_generator_bindings(family, variable_bindings)?,
                user_data: required_capability_value(
                    family,
                    *user_data,
                    "evaluateExpression.userData",
                )?,
                enable: required_capability_value(family, *enable, "evaluateExpression.enable")?,
                expressions: CapabilityGeneratorExpressionArray {
                    source: capability_generator_anonymous_source(
                        family,
                        behavior_path,
                        &expressions.source,
                    )?,
                    expressions: expressions
                        .expressions
                        .iter()
                        .map(|expression| {
                            let label =
                                format!("evaluateExpression.expression[{}]", expression.ordinal);
                            Ok(CapabilityGeneratorExpression {
                                expression: required_capability_value(
                                    family,
                                    expression.expression.clone(),
                                    &format!("{label}.expression"),
                                )?,
                                referenced_variables: expression
                                    .referenced_variables
                                    .iter()
                                    .map(|reference| {
                                        capability_expression_variable_reference(family, reference)
                                    })
                                    .collect::<Result<_, _>>()?,
                                assignment_variable_index: required_i32(
                                    family,
                                    expression.assignment_variable_index,
                                    &format!("{label}.assignmentVariableIndex"),
                                )?,
                                assignment_event_index: required_i32(
                                    family,
                                    expression.assignment_event_index,
                                    &format!("{label}.assignmentEventIndex"),
                                )?,
                                event_mode: u8::try_from(required_i32(
                                    family,
                                    expression.event_mode,
                                    &format!("{label}.eventMode"),
                                )?)
                                .map_err(|_| {
                                    SkyrimCreatureMotionError::AdapterUnavailable {
                                        family_id: family.family_id.clone(),
                                        target: "CapabilityGraphManifest",
                                        detail: format!("{label}.eventMode does not fit u8"),
                                    }
                                })?,
                            })
                        })
                        .collect::<Result<_, SkyrimCreatureMotionError>>()?,
                },
            })
        }
        SkyrimBehaviorModifierNodeEvidence::Unsupported { source, .. } => adapter_unavailable(
            family,
            "CapabilityGraphManifest",
            format!(
                "{} modifier {} uses unsupported class {}",
                behavior_path, source.object_index, source.class_name
            ),
        ),
        SkyrimBehaviorModifierNodeEvidence::Missing { object_index } => adapter_unavailable(
            family,
            "CapabilityGraphManifest",
            format!("{} modifier {object_index} is missing", behavior_path),
        ),
        SkyrimBehaviorModifierNodeEvidence::Cycle { source } => adapter_unavailable(
            family,
            "CapabilityGraphManifest",
            format!(
                "{} modifier {} contains a recursive cycle",
                behavior_path, source.object_index
            ),
        ),
    }
}

fn capability_generator_node(
    family: &SkyrimFamilyMotionSet,
    behavior_path: &str,
    evidence: &SkyrimBehaviorGeneratorNodeEvidence,
    clips: &[&SkyrimCatalogClip],
) -> Result<CapabilityGeneratorNode, SkyrimCreatureMotionError> {
    match evidence {
        SkyrimBehaviorGeneratorNodeEvidence::Clip {
            source,
            resolved_clip_path,
            ..
        } => {
            let resolved_clip_path = resolved_clip_path.as_deref().ok_or_else(|| {
                SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!(
                        "{} clip generator {} has no resolved source asset",
                        behavior_path, source.object_index
                    ),
                }
            })?;
            let matching = clips
                .iter()
                .filter(|clip| path_key(&clip.clip_path) == path_key(resolved_clip_path))
                .collect::<Vec<_>>();
            let [clip] = matching.as_slice() else {
                return adapter_unavailable(
                    family,
                    "CapabilityGraphManifest",
                    format!(
                        "{} clip generator {} resolved {:?} to {} catalog clips",
                        behavior_path,
                        source.object_index,
                        resolved_clip_path,
                        matching.len()
                    ),
                );
            };
            Ok(CapabilityGeneratorNode::Clip {
                source: capability_generator_source(family, behavior_path, source)?,
                clip_name: clip.clip_id.clone(),
            })
        }
        SkyrimBehaviorGeneratorNodeEvidence::ManualSelector {
            source,
            selected_generator_index,
            index_selector,
            selected_index_can_change_after_activate,
            transition_effect,
            blending_transition_effect: _,
            variable_bindings,
            children,
        } => Ok(CapabilityGeneratorNode::ManualSelector {
            source: capability_generator_source(family, behavior_path, source)?,
            bindings: capability_generator_bindings(family, variable_bindings)?,
            children: children
                .iter()
                .map(|child| capability_generator_node(family, behavior_path, child, clips))
                .collect::<Result<_, _>>()?,
            selected_generator_index: required_i32(
                family,
                *selected_generator_index,
                "selectedGeneratorIndex",
            )?,
            index_selector: index_selector
                .as_ref()
                .map(|source| capability_generator_source(family, behavior_path, source))
                .transpose()?,
            selected_index_can_change_after_activate: selected_index_can_change_after_activate
                .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!(
                        "{} selector {} has no decoded dynamic-selection flag",
                        behavior_path, source.object_index
                    ),
                })?,
            generator_changed_transition_effect: transition_effect
                .as_ref()
                .map(|source| capability_generator_source(family, behavior_path, source))
                .transpose()?,
        }),
        SkyrimBehaviorGeneratorNodeEvidence::Blender {
            source,
            reference_pose_weight_threshold_bits,
            blend_parameter_bits,
            min_cyclic_blend_parameter_bits,
            max_cyclic_blend_parameter_bits,
            index_of_sync_master_child,
            flags,
            subtract_last_child,
            variable_bindings,
            children,
        } => Ok(CapabilityGeneratorNode::Blender {
            source: capability_generator_source(family, behavior_path, source)?,
            bindings: capability_generator_bindings(family, variable_bindings)?,
            children: children
                .iter()
                .map(|child| {
                    Ok(CapabilityGeneratorBlenderChild {
                        generator: capability_generator_node(
                            family,
                            behavior_path,
                            &child.generator,
                            clips,
                        )?,
                        bone_weights: child
                            .bone_weights
                            .as_ref()
                            .map(|source| {
                                capability_generator_source(family, behavior_path, source)
                            })
                            .transpose()?,
                        weight_bits: child.weight_bits.ok_or_else(|| {
                            SkyrimCreatureMotionError::AdapterUnavailable {
                                family_id: family.family_id.clone(),
                                target: "CapabilityGraphManifest",
                                detail: format!(
                                    "{} blender child {} has no decoded weight",
                                    behavior_path, child.ordinal
                                ),
                            }
                        })?,
                        world_from_model_weight_bits: child
                            .world_from_model_weight_bits
                            .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
                                family_id: family.family_id.clone(),
                                target: "CapabilityGraphManifest",
                                detail: format!(
                                    "{} blender child {} has no decoded worldFromModelWeight",
                                    behavior_path, child.ordinal
                                ),
                            })?,
                    })
                })
                .collect::<Result<_, SkyrimCreatureMotionError>>()?,
            reference_pose_weight_threshold_bits: reference_pose_weight_threshold_bits.ok_or_else(
                || SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!(
                        "{} blender {} has no decoded referencePoseWeightThreshold",
                        behavior_path, source.object_index
                    ),
                },
            )?,
            blend_parameter_bits: blend_parameter_bits.ok_or_else(|| {
                SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!(
                        "{} blender {} has no decoded blendParameter",
                        behavior_path, source.object_index
                    ),
                }
            })?,
            min_cyclic_blend_parameter_bits: min_cyclic_blend_parameter_bits.ok_or_else(|| {
                SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!(
                        "{} blender {} has no decoded minimum cyclic parameter",
                        behavior_path, source.object_index
                    ),
                }
            })?,
            max_cyclic_blend_parameter_bits: max_cyclic_blend_parameter_bits.ok_or_else(|| {
                SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!(
                        "{} blender {} has no decoded maximum cyclic parameter",
                        behavior_path, source.object_index
                    ),
                }
            })?,
            index_of_sync_master_child: required_i32(
                family,
                *index_of_sync_master_child,
                "indexOfSyncMasterChild",
            )?,
            flags: u16::try_from(required_i32(family, *flags, "blender.flags")?).map_err(|_| {
                SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!(
                        "{} blender {} flags do not fit u16",
                        behavior_path, source.object_index
                    ),
                }
            })?,
            subtract_last_child: subtract_last_child.ok_or_else(|| {
                SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!(
                        "{} blender {} has no decoded subtractLastChild",
                        behavior_path, source.object_index
                    ),
                }
            })?,
        }),
        SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine) => {
            capability_state_machine_node(family, behavior_path, machine, clips)
        }
        SkyrimBehaviorGeneratorNodeEvidence::ModifierGenerator {
            source,
            user_data,
            variable_bindings,
            modifier,
            generator,
        } => {
            let modifier =
                modifier
                    .as_ref()
                    .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
                        family_id: family.family_id.clone(),
                        target: "CapabilityGraphManifest",
                        detail: format!(
                            "{} modifier generator {} has no decoded modifier",
                            behavior_path, source.object_index
                        ),
                    })?;
            let generator = generator.as_ref().ok_or_else(|| {
                SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!(
                        "{} modifier generator {} has no decoded child generator",
                        behavior_path, source.object_index
                    ),
                }
            })?;
            Ok(CapabilityGeneratorNode::ModifierGenerator {
                source: capability_generator_source(family, behavior_path, source)?,
                bindings: capability_generator_bindings(family, variable_bindings)?,
                user_data: required_capability_value(
                    family,
                    *user_data,
                    "modifierGenerator.userData",
                )?,
                modifier: Box::new(capability_modifier_node(family, behavior_path, modifier)?),
                generator: Box::new(capability_generator_node(
                    family,
                    behavior_path,
                    generator,
                    clips,
                )?),
            })
        }
        SkyrimBehaviorGeneratorNodeEvidence::Unsupported { source, .. } => adapter_unavailable(
            family,
            "CapabilityGraphManifest",
            format!(
                "{} generator {} uses unsupported class {}",
                behavior_path, source.object_index, source.class_name
            ),
        ),
        SkyrimBehaviorGeneratorNodeEvidence::Missing { object_index } => adapter_unavailable(
            family,
            "CapabilityGraphManifest",
            format!("{} generator {object_index} is missing", behavior_path),
        ),
        SkyrimBehaviorGeneratorNodeEvidence::Cycle { source } => adapter_unavailable(
            family,
            "CapabilityGraphManifest",
            format!(
                "{} generator {} contains a recursive cycle",
                behavior_path, source.object_index
            ),
        ),
    }
}

fn capability_state_machine_node(
    family: &SkyrimFamilyMotionSet,
    behavior_path: &str,
    machine: &SkyrimBehaviorStateMachineEvidence,
    clips: &[&SkyrimCatalogClip],
) -> Result<CapabilityGeneratorNode, SkyrimCreatureMotionError> {
    let event_to_send_when_state_or_transition_changes = machine
        .event_to_send_when_state_or_transition_changes
        .as_ref()
        .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "CapabilityGraphManifest",
            detail: format!(
                "{} state machine {} has no decoded state-change event property",
                behavior_path, machine.source.object_index
            ),
        })?;
    let mut states = Vec::with_capacity(machine.states.len());
    for state in &machine.states {
        let label = format!(
            "{} state machine {} state {}",
            behavior_path, machine.source.object_index, state.ordinal
        );
        let generator = state.generator.as_ref().ok_or_else(|| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "CapabilityGraphManifest",
                detail: format!("{label} has no decoded generator"),
            }
        })?;
        states.push(CapabilityGeneratorState {
            source: capability_generator_source(family, behavior_path, &state.source)?,
            listeners: state
                .listeners
                .iter()
                .map(|source| capability_generator_source(family, behavior_path, source))
                .collect::<Result<_, _>>()?,
            enter_notify_events: state
                .enter_notify_events
                .iter()
                .enumerate()
                .map(|(ordinal, property)| {
                    capability_event_property(
                        family,
                        behavior_path,
                        property,
                        &format!("{label}.enterNotify[{ordinal}]"),
                    )
                })
                .collect::<Result<_, _>>()?,
            exit_notify_events: state
                .exit_notify_events
                .iter()
                .enumerate()
                .map(|(ordinal, property)| {
                    capability_event_property(
                        family,
                        behavior_path,
                        property,
                        &format!("{label}.exitNotify[{ordinal}]"),
                    )
                })
                .collect::<Result<_, _>>()?,
            transitions: state
                .transitions
                .as_ref()
                .map(|array| capability_transition_array(family, behavior_path, array))
                .transpose()?,
            generator: Box::new(capability_generator_node(
                family,
                behavior_path,
                generator,
                clips,
            )?),
            name: state.name.clone().ok_or_else(|| {
                SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!("{label} has no decoded name"),
                }
            })?,
            state_id: required_i32(family, state.state_id, &format!("{label}.stateId"))?,
            probability_bits: state.probability_bits.ok_or_else(|| {
                SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!("{label} has no decoded probability"),
                }
            })?,
            enable: state
                .enable
                .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!("{label} has no decoded enable flag"),
                })?,
            has_eventless_transitions: state.has_eventless_transitions.ok_or_else(|| {
                SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "CapabilityGraphManifest",
                    detail: format!("{label} has no decoded eventless-transition flag"),
                }
            })?,
        });
    }
    Ok(CapabilityGeneratorNode::StateMachine {
        source: capability_generator_source(family, behavior_path, &machine.source)?,
        bindings: capability_generator_bindings(family, &machine.variable_bindings)?,
        event_to_send_when_state_or_transition_changes: capability_event_property(
            family,
            behavior_path,
            event_to_send_when_state_or_transition_changes,
            "stateMachine.stateChangeEvent",
        )?,
        start_state_id_selector: machine
            .start_state_id_selector
            .as_ref()
            .map(|source| capability_generator_source(family, behavior_path, source))
            .transpose()?,
        start_state_id: required_i32(family, machine.start_state_id, "startStateId")?,
        return_to_previous_state_event: capability_event_ref(
            family,
            machine.return_to_previous_state_event_id,
            machine.return_to_previous_state_event_name.as_deref(),
            "returnToPreviousStateEvent",
        )?,
        random_transition_event: capability_event_ref(
            family,
            machine.random_transition_event_id,
            machine.random_transition_event_name.as_deref(),
            "randomTransitionEvent",
        )?,
        transition_to_next_higher_state_event: capability_event_ref(
            family,
            machine.transition_to_next_higher_state_event_id,
            machine
                .transition_to_next_higher_state_event_name
                .as_deref(),
            "transitionToNextHigherStateEvent",
        )?,
        transition_to_next_lower_state_event: capability_event_ref(
            family,
            machine.transition_to_next_lower_state_event_id,
            machine.transition_to_next_lower_state_event_name.as_deref(),
            "transitionToNextLowerStateEvent",
        )?,
        sync_variable_index: required_i32(
            family,
            machine.sync_variable_index,
            "syncVariableIndex",
        )?,
        wrap_around_state_id: machine.wrap_around_state_id.ok_or_else(|| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "CapabilityGraphManifest",
                detail: "state machine has no decoded wrapAroundStateId".to_string(),
            }
        })?,
        max_simultaneous_transitions: required_i32(
            family,
            machine.max_simultaneous_transitions,
            "maxSimultaneousTransitions",
        )?,
        start_state_mode: required_i32(family, machine.start_state_mode, "startStateMode")?,
        self_transition_mode: required_i32(
            family,
            machine.self_transition_mode,
            "selfTransitionMode",
        )?,
        states,
        wildcard_transitions: machine
            .wildcard_transitions
            .as_ref()
            .map(|array| capability_transition_array(family, behavior_path, array))
            .transpose()?,
    })
}

fn capability_generator_variables(
    family: &SkyrimFamilyMotionSet,
    inventory: &SkyrimCreatureFamilyInventory,
    generators: &[&CapabilityRoleGenerator],
) -> Result<Vec<VariableDecl>, SkyrimCreatureMotionError> {
    let mut path_max_indices = BTreeMap::<String, usize>::new();
    for generator in generators {
        if let CapabilityRoleGenerator::Tree { root } = generator {
            collect_generator_binding_indices(root, &mut path_max_indices);
        }
    }
    let Some(global_max_index) = path_max_indices.values().copied().max() else {
        return Ok(Vec::new());
    };
    let mut declarations = Vec::with_capacity(global_max_index + 1);
    for variable_index in 0..=global_max_index {
        let mut evidence = Vec::new();
        for (behavior_path, max_index) in &path_max_indices {
            if variable_index > *max_index {
                continue;
            }
            let matching = inventory
                .behavior_variables
                .iter()
                .filter(|variable| {
                    variable.variable_index == variable_index
                        && path_key(&variable.behavior_path) == path_key(behavior_path)
                })
                .collect::<Vec<_>>();
            let [variable] = matching.as_slice() else {
                return adapter_unavailable(
                    family,
                    "CapabilityGraphManifest",
                    format!(
                        "{} variable index {} has {} exact source receipts",
                        behavior_path,
                        variable_index,
                        matching.len()
                    ),
                );
            };
            evidence.push(*variable);
        }
        let [first, rest @ ..] = evidence.as_slice() else {
            return adapter_unavailable(
                family,
                "CapabilityGraphManifest",
                format!("variable index {variable_index} has no owning behavior table"),
            );
        };
        if rest.iter().any(|candidate| {
            candidate.variable_name != first.variable_name
                || candidate.variable_type != first.variable_type
                || candidate.initial_word_value != first.initial_word_value
        }) {
            return adapter_unavailable(
                family,
                "CapabilityGraphManifest",
                format!("variable index {variable_index} differs across source behavior tables"),
            );
        }
        declarations.push(capability_variable_declaration(family, first)?);
    }
    Ok(declarations)
}

fn collect_generator_binding_indices(
    node: &CapabilityGeneratorNode,
    path_max_indices: &mut BTreeMap<String, usize>,
) {
    let (source, bindings) = match node {
        CapabilityGeneratorNode::Clip { source, .. } => (source, &[][..]),
        CapabilityGeneratorNode::ManualSelector {
            source, bindings, ..
        }
        | CapabilityGeneratorNode::Blender {
            source, bindings, ..
        }
        | CapabilityGeneratorNode::StateMachine {
            source, bindings, ..
        }
        | CapabilityGeneratorNode::ModifierGenerator {
            source, bindings, ..
        } => (source, bindings.as_slice()),
    };
    for binding in bindings {
        if let Ok(variable_index) = usize::try_from(binding.variable_index) {
            collect_variable_index(&source.behavior_path, variable_index, path_max_indices);
        }
    }
    match node {
        CapabilityGeneratorNode::Clip { .. } => {}
        CapabilityGeneratorNode::ManualSelector { children, .. } => {
            for child in children {
                collect_generator_binding_indices(child, path_max_indices);
            }
        }
        CapabilityGeneratorNode::Blender { children, .. } => {
            for child in children {
                collect_generator_binding_indices(&child.generator, path_max_indices);
            }
        }
        CapabilityGeneratorNode::StateMachine { states, .. } => {
            for state in states {
                collect_generator_binding_indices(&state.generator, path_max_indices);
            }
        }
        CapabilityGeneratorNode::ModifierGenerator {
            modifier,
            generator,
            ..
        } => {
            collect_modifier_binding_indices(modifier, path_max_indices);
            collect_generator_binding_indices(generator, path_max_indices);
        }
    }
}

fn collect_modifier_binding_indices(
    modifier: &CapabilityModifierNode,
    path_max_indices: &mut BTreeMap<String, usize>,
) {
    let (source, bindings) = match modifier {
        CapabilityModifierNode::List {
            source, bindings, ..
        }
        | CapabilityModifierNode::Damping {
            source, bindings, ..
        }
        | CapabilityModifierNode::EvaluateExpression {
            source, bindings, ..
        } => (source, bindings),
    };
    for binding in bindings {
        if let Ok(variable_index) = usize::try_from(binding.variable_index) {
            collect_variable_index(&source.behavior_path, variable_index, path_max_indices);
        }
    }
    match modifier {
        CapabilityModifierNode::List { modifiers, .. } => {
            for child in modifiers {
                collect_modifier_binding_indices(child, path_max_indices);
            }
        }
        CapabilityModifierNode::Damping { .. } => {}
        CapabilityModifierNode::EvaluateExpression { expressions, .. } => {
            for expression in &expressions.expressions {
                for reference in &expression.referenced_variables {
                    if let Ok(variable_index) = usize::try_from(reference.variable_index) {
                        collect_variable_index(
                            &expressions.source.behavior_path,
                            variable_index,
                            path_max_indices,
                        );
                    }
                }
            }
        }
    }
}

fn collect_variable_index(
    behavior_path: &str,
    variable_index: usize,
    path_max_indices: &mut BTreeMap<String, usize>,
) {
    path_max_indices
        .entry(path_key(behavior_path))
        .and_modify(|current| *current = (*current).max(variable_index))
        .or_insert(variable_index);
}

fn capability_variable_declaration(
    family: &SkyrimFamilyMotionSet,
    evidence: &SkyrimBehaviorVariableEvidence,
) -> Result<VariableDecl, SkyrimCreatureMotionError> {
    let name = evidence.variable_name.clone().ok_or_else(|| {
        SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "CapabilityGraphManifest",
            detail: format!(
                "{} variable {} has no decoded name",
                evidence.behavior_path, evidence.variable_index
            ),
        }
    })?;
    let word = evidence.initial_word_value.ok_or_else(|| {
        SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "CapabilityGraphManifest",
            detail: format!(
                "{} variable {} {:?} has no decoded initial word",
                evidence.behavior_path, evidence.variable_index, name
            ),
        }
    })?;
    let (variable_type, initial_value) = match evidence.variable_type {
        Some(0) if word <= 1 => (VariableType::Bool, VariableValue::Bool(word != 0)),
        Some(0) => {
            return adapter_unavailable(
                family,
                "CapabilityGraphManifest",
                format!(
                    "{} BOOL variable {} {:?} has non-boolean initial word 0x{word:08x}",
                    evidence.behavior_path, evidence.variable_index, name
                ),
            );
        }
        Some(3) => (
            VariableType::Int32,
            VariableValue::Int32(i32::from_ne_bytes(word.to_ne_bytes())),
        ),
        Some(4) => (
            VariableType::Real,
            VariableValue::Real(f32::from_bits(word)),
        ),
        Some(1 | 2) => {
            return adapter_unavailable(
                family,
                "CapabilityGraphManifest",
                format!(
                    "{} variable {} {:?} requires an exact Int8/Int16 widening policy",
                    evidence.behavior_path, evidence.variable_index, name
                ),
            );
        }
        value => {
            return adapter_unavailable(
                family,
                "CapabilityGraphManifest",
                format!(
                    "{} variable {} {:?} has unsupported decoded type {value:?}",
                    evidence.behavior_path, evidence.variable_index, name
                ),
            );
        }
    };
    Ok(VariableDecl {
        name,
        variable_type,
        initial_value,
    })
}

fn build_capability_manifest(
    family: &SkyrimFamilyMotionSet,
    inventory: Option<&SkyrimCreatureFamilyInventory>,
    clips: &[&SkyrimCatalogClip],
) -> Result<(CapabilityGraphManifest, Vec<VariableDecl>), SkyrimCreatureMotionError> {
    let mut candidate_attack_bindings = candidate_attack_bindings(family, clips);
    let role_clips = indexed_role_clips(clips);
    let mut template = expected_family_template(family, &role_clips, clips);
    let mut role_mapping = Vec::<(SkyrimCreatureMotionRole, CreatureClipRole)>::new();
    match template {
        CreatureGraphTemplate::PassiveGround => {
            role_mapping.push((SkyrimCreatureMotionRole::Idle, CreatureClipRole::Idle));
            for (source, target) in [
                (
                    SkyrimCreatureMotionRole::GroundLocomotion,
                    CreatureClipRole::GroundForward,
                ),
                (
                    SkyrimCreatureMotionRole::TurnLeft,
                    CreatureClipRole::TurnLeft90,
                ),
                (
                    SkyrimCreatureMotionRole::TurnRight,
                    CreatureClipRole::TurnRight90,
                ),
            ] {
                if role_clips.contains_key(&source) {
                    role_mapping.push((source, target));
                }
            }
        }
        CreatureGraphTemplate::GroundMelee => {
            append_ground_role_mappings(&mut role_mapping, &role_clips);
            append_melee_role_mapping(&mut role_mapping);
        }
        CreatureGraphTemplate::GroundRangedProjectile => {
            append_ground_role_mappings(&mut role_mapping, &role_clips);
            append_ranged_role_mappings(&mut role_mapping, &role_clips, true);
        }
        CreatureGraphTemplate::GroundMeleeRanged => {
            append_ground_role_mappings(&mut role_mapping, &role_clips);
            append_melee_role_mapping(&mut role_mapping);
            append_ranged_role_mappings(&mut role_mapping, &role_clips, true);
        }
        CreatureGraphTemplate::GroundSwim | CreatureGraphTemplate::GroundFly => {
            append_ground_role_mappings(&mut role_mapping, &role_clips);
            if template == CreatureGraphTemplate::GroundSwim {
                role_mapping.push((
                    SkyrimCreatureMotionRole::SwimIdle,
                    CreatureClipRole::SwimIdle,
                ));
                role_mapping.push((
                    SkyrimCreatureMotionRole::SwimLocomotion,
                    CreatureClipRole::SwimForward,
                ));
            } else {
                role_mapping.push((SkyrimCreatureMotionRole::FlyIdle, CreatureClipRole::FlyIdle));
                role_mapping.push((
                    SkyrimCreatureMotionRole::FlyLocomotion,
                    CreatureClipRole::FlyForward,
                ));
            }
            if role_clips.contains_key(&SkyrimCreatureMotionRole::MeleeAttack) {
                append_melee_role_mapping(&mut role_mapping);
            }
            append_ranged_role_mappings(&mut role_mapping, &role_clips, false);
        }
        CreatureGraphTemplate::Swim | CreatureGraphTemplate::Fly => {
            if template == CreatureGraphTemplate::Swim {
                role_mapping.push((
                    if role_clips.contains_key(&SkyrimCreatureMotionRole::SwimIdle) {
                        SkyrimCreatureMotionRole::SwimIdle
                    } else {
                        SkyrimCreatureMotionRole::Idle
                    },
                    CreatureClipRole::SwimIdle,
                ));
                role_mapping.push((
                    if role_clips.contains_key(&SkyrimCreatureMotionRole::SwimLocomotion) {
                        SkyrimCreatureMotionRole::SwimLocomotion
                    } else {
                        SkyrimCreatureMotionRole::GroundLocomotion
                    },
                    CreatureClipRole::SwimForward,
                ));
            } else {
                role_mapping.push((
                    if role_clips.contains_key(&SkyrimCreatureMotionRole::FlyIdle) {
                        SkyrimCreatureMotionRole::FlyIdle
                    } else {
                        SkyrimCreatureMotionRole::Idle
                    },
                    CreatureClipRole::FlyIdle,
                ));
                role_mapping.push((
                    if role_clips.contains_key(&SkyrimCreatureMotionRole::FlyLocomotion) {
                        SkyrimCreatureMotionRole::FlyLocomotion
                    } else {
                        SkyrimCreatureMotionRole::GroundLocomotion
                    },
                    CreatureClipRole::FlyForward,
                ));
            }
            if role_clips.contains_key(&SkyrimCreatureMotionRole::MeleeAttack) {
                append_melee_role_mapping(&mut role_mapping);
            }
            append_ranged_role_mappings(&mut role_mapping, &role_clips, false);
        }
        CreatureGraphTemplate::StationaryTurret => {
            role_mapping.push((
                SkyrimCreatureMotionRole::StationaryIdle,
                CreatureClipRole::StationaryIdle,
            ));
            append_ranged_role_mappings(&mut role_mapping, &role_clips, true);
        }
        CreatureGraphTemplate::RobotContinuousAttack => {
            for (source, target) in [
                (SkyrimCreatureMotionRole::Idle, CreatureClipRole::Idle),
                (
                    SkyrimCreatureMotionRole::MechanicalStart,
                    CreatureClipRole::ContinuousAttackStart,
                ),
                (
                    SkyrimCreatureMotionRole::MechanicalLoop,
                    CreatureClipRole::ContinuousAttackLoop,
                ),
                (
                    SkyrimCreatureMotionRole::MechanicalStop,
                    CreatureClipRole::ContinuousAttackStop,
                ),
            ] {
                role_mapping.push((source, target));
            }
        }
    }

    let mut roles = Vec::new();
    let mut explicit_events = BTreeMap::<String, EventDecl>::new();
    let primary_idle_role = adapter_primary_idle_role(template);
    let primary_idle_event = role_mapping
        .iter()
        .find_map(|(source_role, target_role)| {
            (*target_role == primary_idle_role)
                .then(|| role_clips.get(source_role))
                .flatten()
                .and_then(|candidates| {
                    select_semantic_catalog_clip(candidates, *source_role)
                        .or_else(|| candidates.first().copied())
                })
        })
        .and_then(disposition_trigger)
        .unwrap_or("Idle")
        .to_string();
    let mut claimed_trigger_events = BTreeSet::from([primary_idle_event.to_ascii_lowercase()]);
    let mut claimed_clips = BTreeSet::new();
    let mut role_ordinals = BTreeMap::<CreatureClipRole, usize>::new();
    for (source_role, target_role) in role_mapping {
        let Some(candidates) = role_clips.get(&source_role) else {
            if matches!(
                source_role,
                SkyrimCreatureMotionRole::TurnLeft | SkyrimCreatureMotionRole::TurnRight
            ) {
                continue;
            }
            return adapter_unavailable(
                family,
                "CapabilityGraphManifest",
                format!("missing proven {source_role:?} clip"),
            );
        };
        let available = candidates
            .iter()
            .copied()
            .filter(|clip| !claimed_clips.contains(&clip.clip_id.to_ascii_lowercase()))
            .collect::<Vec<_>>();
        if available.is_empty() {
            let candidate_bound = candidate_attack_bindings
                .iter()
                .any(|binding| binding.role == target_role);
            if candidate_bound
                || matches!(
                    target_role,
                    CreatureClipRole::MeleeAttack | CreatureClipRole::ProjectileAttack
                )
                || matches!(
                    target_role,
                    CreatureClipRole::TurnLeft90 | CreatureClipRole::TurnRight90
                )
            {
                continue;
            }
            return adapter_unavailable(
                family,
                "CapabilityGraphManifest",
                format!("no unclaimed proven {source_role:?} clip remains"),
            );
        }
        let is_attack_role = matches!(
            target_role,
            CreatureClipRole::MeleeAttack | CreatureClipRole::ProjectileAttack
        );
        let grouped_generator: Option<CapabilityRoleGenerator> = None;
        let selected = if is_attack_role {
            available.clone()
        } else {
            vec![
                select_semantic_catalog_clip(&available, source_role)
                    .or_else(|| available.first().copied())
                    .expect("a mapped role has candidates"),
            ]
        };
        let selected_count = selected.len();
        for (selected_index, clip) in selected.into_iter().enumerate() {
            claimed_clips.insert(clip.clip_id.to_ascii_lowercase());
            let ordinal = role_ordinals.entry(target_role).or_default();
            *ordinal += 1;
            let requires_trigger = target_role != adapter_primary_idle_role(template);
            let mut triggers = if requires_trigger && grouped_generator.is_some() {
                let mut triggers = candidates
                    .iter()
                    .flat_map(|candidate| disposition_triggers(candidate))
                    .collect::<Vec<_>>();
                triggers.sort_by_key(|event| event.to_ascii_lowercase());
                triggers.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
                triggers
            } else if requires_trigger {
                disposition_triggers(clip)
            } else {
                Vec::new()
            };
            if matches!(
                target_role,
                CreatureClipRole::MeleeAttack | CreatureClipRole::ProjectileAttack
            ) {
                triggers.extend(
                    candidate_attack_bindings
                        .iter()
                        .filter(|binding| binding.role == target_role)
                        .enumerate()
                        .filter(|(index, _)| index % selected_count == selected_index)
                        .map(|(_, binding)| binding.event.as_str()),
                );
            }
            triggers.sort_by_key(|event| event.to_ascii_lowercase());
            triggers.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
            let mut accepted_triggers = triggers
                .into_iter()
                .filter(|event| {
                    let candidate_bound = candidate_attack_bindings
                        .iter()
                        .any(|binding| binding.event.eq_ignore_ascii_case(event));
                    let valid_melee = target_role != CreatureClipRole::MeleeAttack
                        || candidate_bound
                        || (event.starts_with("melee") && event.len() > "melee".len());
                    valid_melee && claimed_trigger_events.insert(event.to_ascii_lowercase())
                })
                .map(str::to_string)
                .collect::<Vec<_>>();
            if requires_trigger && accepted_triggers.is_empty() {
                let family_fragment = identifier_fragment(&family.family_id);
                let event = if matches!(
                    target_role,
                    CreatureClipRole::Idle
                        | CreatureClipRole::SwimIdle
                        | CreatureClipRole::FlyIdle
                        | CreatureClipRole::StationaryIdle
                ) {
                    format!(
                        "{}_{}_{}",
                        identifier_fragment(&format!("{target_role:?}")),
                        family_fragment,
                        *ordinal
                    )
                } else {
                    deterministic_standard_event(target_role, &family_fragment, *ordinal)
                };
                if !claimed_trigger_events.insert(event.to_ascii_lowercase()) {
                    return adapter_unavailable(
                        family,
                        "CapabilityGraphManifest",
                        format!("deterministic event {event:?} collides in the generated graph"),
                    );
                }
                accepted_triggers.push(event);
            }
            let trigger_event = accepted_triggers.first().cloned();
            let mut trigger_aliases = accepted_triggers.into_iter().skip(1).collect::<Vec<_>>();
            trigger_aliases.sort_by_key(|event| event.to_ascii_lowercase());
            for event in trigger_event.iter().chain(&trigger_aliases) {
                let candidate_bound = candidate_attack_bindings
                    .iter()
                    .any(|binding| binding.event.eq_ignore_ascii_case(event));
                let usage = if target_role == CreatureClipRole::MeleeAttack && !candidate_bound {
                    EventUsage::MeleeAttack
                } else {
                    EventUsage::Generic
                };
                explicit_events
                    .entry(event.to_ascii_lowercase())
                    .or_insert_with(|| EventDecl {
                        name: event.clone(),
                        usage,
                        flags: 0,
                    });
            }
            let generator = grouped_generator
                .clone()
                .unwrap_or(CapabilityRoleGenerator::Single);
            let motion = clip.root_motion.policy();
            for tree_clip_name in generator.clip_names(&clip.clip_id) {
                let matching = candidates
                    .iter()
                    .filter(|candidate| candidate.clip_id == tree_clip_name)
                    .collect::<Vec<_>>();
                let [tree_clip] = matching.as_slice() else {
                    return adapter_unavailable(
                        family,
                        "CapabilityGraphManifest",
                        format!(
                            "{source_role:?} tree clip {tree_clip_name:?} has {} exact role receipts",
                            matching.len()
                        ),
                    );
                };
                if tree_clip.root_motion.policy() != motion {
                    return adapter_unavailable(
                        family,
                        "CapabilityGraphManifest",
                        format!(
                            "{source_role:?} tree clips require per-node motion policies; {:?} differs from anchor {:?}",
                            tree_clip.clip_id, clip.clip_id
                        ),
                    );
                }
                claimed_clips.insert(tree_clip.clip_id.to_ascii_lowercase());
            }
            roles.push(CapabilityClipRole {
                role: target_role,
                state_name: format!("{target_role:?}{ordinal}"),
                clip_name: clip.clip_id.clone(),
                generator,
                trigger_event,
                trigger_aliases,
                motion,
            });
        }
    }
    candidate_attack_bindings.retain(|binding| {
        roles.iter().any(|role| {
            role.role == binding.role
                && role
                    .trigger_event
                    .iter()
                    .chain(&role.trigger_aliases)
                    .any(|event| event.eq_ignore_ascii_case(&binding.event))
        })
    });
    template = reconciled_attack_template(template, &roles, &candidate_attack_bindings);
    let _idle = roles
        .iter()
        .find(|role| {
            matches!(
                role.role,
                CreatureClipRole::Idle
                    | CreatureClipRole::SwimIdle
                    | CreatureClipRole::FlyIdle
                    | CreatureClipRole::StationaryIdle
            )
        })
        .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "CapabilityGraphManifest",
            detail: "missing template idle role".to_string(),
        })?;
    let idle_event = primary_idle_event;
    explicit_events
        .entry(idle_event.to_ascii_lowercase())
        .or_insert_with(|| EventDecl {
            name: idle_event.clone(),
            usage: EventUsage::Generic,
            flags: 0,
        });

    let mut overlays = Vec::new();
    for clip in clips {
        if !matches!(
            clip.disposition,
            SkyrimClipDisposition::SharedOverlay { .. }
        ) {
            continue;
        }
        let events = clip
            .annotations
            .iter()
            .map(|event| event.name.clone())
            .collect::<BTreeSet<_>>();
        if events.len() < 2 {
            continue;
        }
        let mut events = events.into_iter();
        let start_event = events.next().expect("two events");
        let stop_event = events.next().expect("two events");
        for event in [&start_event, &stop_event] {
            explicit_events
                .entry(event.to_ascii_lowercase())
                .or_insert_with(|| EventDecl {
                    name: event.clone(),
                    usage: EventUsage::Generic,
                    flags: 0,
                });
        }
        overlays.push(OverlayClipRole {
            name: format!("overlay_{}", overlays.len() + 1),
            clip_name: clip.clip_id.clone(),
            start_event,
            stop_event,
            motion: clip.root_motion.policy(),
        });
    }

    let variables = match inventory {
        Some(inventory) => capability_generator_variables(
            family,
            inventory,
            &roles.iter().map(|role| &role.generator).collect::<Vec<_>>(),
        )?,
        None if roles
            .iter()
            .any(|role| matches!(role.generator, CapabilityRoleGenerator::Tree { .. })) =>
        {
            return adapter_unavailable(
                family,
                "CapabilityGraphManifest",
                "generator Tree has no decoded family inventory".to_string(),
            );
        }
        None => Vec::new(),
    };
    Ok((
        CapabilityGraphManifest {
            template,
            roles,
            candidate_attack_bindings,
            idle_event,
            explicit_events: explicit_events.into_values().collect(),
            overlays,
        },
        variables,
    ))
}

fn build_motion_set(
    family: &SkyrimFamilyMotionSet,
    clips: &[&SkyrimCatalogClip],
) -> Result<MotionSet, SkyrimCreatureMotionError> {
    let roles = indexed_role_clips(clips);
    let idle_candidates = roles
        .get(&SkyrimCreatureMotionRole::Idle)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let idle = select_semantic_catalog_clip(idle_candidates, SkyrimCreatureMotionRole::Idle)
        .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "MotionSet",
            detail: "missing a unique semantic primary idle clip".to_string(),
        })?;
    if passive_ground_semantics(family, &roles, clips) {
        let mut locomotion_clips = BTreeMap::new();
        for (role, key) in [
            (SkyrimCreatureMotionRole::GroundLocomotion, "walk_forward"),
            (SkyrimCreatureMotionRole::TurnLeft, "turn_left_90"),
            (SkyrimCreatureMotionRole::TurnRight, "turn_right_90"),
        ] {
            if let Some(candidates) = roles.get(&role)
                && let Some(clip) = select_semantic_catalog_clip(candidates, role)
            {
                locomotion_clips.insert(key.to_string(), clip.clip_id.clone());
            }
        }
        return Ok(MotionSet {
            id: format!("skyrim_{}_motions", family.family_id),
            graph_template: CreatureGraphTemplate::PassiveGround,
            idle_clip: idle.clip_id.clone(),
            locomotion_clips,
            attacks: Vec::new(),
            attack_kinds: BTreeMap::new(),
            required_overlays: Vec::new(),
            overlays: Vec::new(),
            required_rigs: Vec::new(),
            rigs: Vec::new(),
        });
    }
    let locomotion = first_role_clip(&roles, &[SkyrimCreatureMotionRole::GroundLocomotion])
        .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "MotionSet",
            detail: "missing proven locomotion clip".to_string(),
        })?;
    let turn_left =
        first_role_clip(&roles, &[SkyrimCreatureMotionRole::TurnLeft]).ok_or_else(|| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "MotionSet",
                detail: "missing proven left-turn clip".to_string(),
            }
        })?;
    let turn_right =
        first_role_clip(&roles, &[SkyrimCreatureMotionRole::TurnRight]).ok_or_else(|| {
            SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "MotionSet",
                detail: "missing proven right-turn clip".to_string(),
            }
        })?;
    let mut attacks = Vec::new();
    for clip in roles
        .get(&SkyrimCreatureMotionRole::MeleeAttack)
        .into_iter()
        .flatten()
    {
        disposition_trigger(clip).ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "MotionSet",
            detail: format!("{} has no proven attack event", clip.clip_path),
        })?;
        attacks.push(MotionAttack {
            id: format!("attack_{:02}", attacks.len() + 1),
            event: format!(
                "melee_{}_{:02}",
                identifier_fragment(&family.family_id),
                attacks.len() + 1
            ),
            clip: clip.clip_id.clone(),
        });
    }
    if attacks.is_empty() {
        return adapter_unavailable(
            family,
            "MotionSet",
            "missing proven combat clip".to_string(),
        );
    }
    let mut locomotion_clips = BTreeMap::new();
    locomotion_clips.insert("walk_forward".to_string(), locomotion.clip_id.clone());
    locomotion_clips.insert("turn_left_90".to_string(), turn_left.clip_id.clone());
    locomotion_clips.insert("turn_right_90".to_string(), turn_right.clip_id.clone());
    Ok(MotionSet {
        id: format!("skyrim_{}_motions", family.family_id),
        graph_template: CreatureGraphTemplate::GroundMelee,
        idle_clip: idle.clip_id.clone(),
        locomotion_clips,
        attacks,
        attack_kinds: BTreeMap::new(),
        required_overlays: Vec::new(),
        overlays: Vec::new(),
        required_rigs: Vec::new(),
        rigs: Vec::new(),
    })
}

fn indexed_role_clips<'a>(
    clips: &'a [&'a SkyrimCatalogClip],
) -> BTreeMap<SkyrimCreatureMotionRole, Vec<&'a SkyrimCatalogClip>> {
    let mut roles = BTreeMap::<_, Vec<_>>::new();
    for clip in clips {
        match &clip.disposition {
            SkyrimClipDisposition::Role { role, .. } => {
                roles.entry(*role).or_default().push(*clip);
            }
            SkyrimClipDisposition::Ambiguous {
                roles: candidate_roles,
                ..
            } => {
                for role in candidate_roles {
                    roles.entry(*role).or_default().push(*clip);
                }
            }
            _ => {}
        }
    }
    for candidates in roles.values_mut() {
        candidates.sort_by(|left, right| left.clip_path.cmp(&right.clip_path));
        candidates.dedup_by(|left, right| left.clip_id == right.clip_id);
    }
    roles
}

fn first_role_clip<'a>(
    roles: &'a BTreeMap<SkyrimCreatureMotionRole, Vec<&'a SkyrimCatalogClip>>,
    preferred: &[SkyrimCreatureMotionRole],
) -> Option<&'a SkyrimCatalogClip> {
    preferred
        .iter()
        .find_map(|role| roles.get(role).and_then(|clips| clips.first()).copied())
}

fn disposition_trigger(clip: &SkyrimCatalogClip) -> Option<&str> {
    match &clip.disposition {
        SkyrimClipDisposition::Role { trigger_event, .. } => trigger_event.as_deref(),
        _ => None,
    }
}

fn disposition_triggers(clip: &SkyrimCatalogClip) -> Vec<&str> {
    match &clip.disposition {
        SkyrimClipDisposition::Role {
            trigger_event,
            trigger_aliases,
            ..
        } => trigger_event
            .iter()
            .map(String::as_str)
            .chain(trigger_aliases.iter().map(String::as_str))
            .collect(),
        _ => Vec::new(),
    }
}

fn candidate_attack_bindings(
    family: &SkyrimFamilyMotionSet,
    clips: &[&SkyrimCatalogClip],
) -> Vec<CapabilityCandidateAttackBinding> {
    let mut bindings = family
        .race_attack_sources
        .iter()
        .filter(|evidence| {
            !evidence.source_plugin.trim().is_empty() && evidence.source_race.local != 0
        })
        .map(|evidence| CapabilityCandidateAttackBinding {
            source_identity: SourceCreatureIdentity {
                namespace: "skyrimse".to_string(),
                plugin: evidence.source_plugin.clone(),
                local_form_id: evidence.source_race.local,
            },
            event: evidence.attack.event.clone(),
            role: candidate_attack_role(&evidence.attack, clips),
        })
        .collect::<Vec<_>>();
    bindings.sort_by(|left, right| {
        left.source_identity
            .stable_key()
            .cmp(&right.source_identity.stable_key())
            .then_with(|| {
                left.event
                    .to_ascii_lowercase()
                    .cmp(&right.event.to_ascii_lowercase())
            })
            .then_with(|| left.role.cmp(&right.role))
    });
    bindings.dedup_by(|left, right| {
        left.source_identity == right.source_identity
            && left.event.eq_ignore_ascii_case(&right.event)
            && left.role == right.role
    });
    bindings
}

fn candidate_attack_role(
    attack: &SkyrimRaceAttackEvidence,
    clips: &[&SkyrimCatalogClip],
) -> CreatureClipRole {
    let executable_roles = clips
        .iter()
        .filter_map(|clip| match &clip.disposition {
            SkyrimClipDisposition::Role { role, .. } => motion_attack_role(*role),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let owned_roles = clips
        .iter()
        .filter_map(|clip| match &clip.disposition {
            SkyrimClipDisposition::Role {
                role,
                trigger_event,
                trigger_aliases,
                ..
            } if trigger_event
                .iter()
                .chain(trigger_aliases)
                .any(|event| event.eq_ignore_ascii_case(&attack.event)) =>
            {
                motion_attack_role(*role)
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if let Some(role) = owned_roles.iter().copied().next()
        && owned_roles.len() == 1
    {
        return role;
    }
    if attack.has_attack_spell {
        if executable_roles.contains(&CreatureClipRole::ProjectileAttack)
            || !executable_roles.contains(&CreatureClipRole::MeleeAttack)
        {
            CreatureClipRole::ProjectileAttack
        } else {
            CreatureClipRole::MeleeAttack
        }
    } else {
        CreatureClipRole::MeleeAttack
    }
}

fn motion_attack_role(role: SkyrimCreatureMotionRole) -> Option<CreatureClipRole> {
    match role {
        SkyrimCreatureMotionRole::MeleeAttack => Some(CreatureClipRole::MeleeAttack),
        SkyrimCreatureMotionRole::ProjectileAttack
        | SkyrimCreatureMotionRole::RangedAttack
        | SkyrimCreatureMotionRole::SpellAttack => Some(CreatureClipRole::ProjectileAttack),
        _ => None,
    }
}

#[cfg(test)]
mod candidate_attack_binding_tests {
    use super::*;

    #[test]
    fn a_nonconflicting_source_attack_still_gets_a_candidate_binding() {
        let interner = crate::sym::StringInterner::new();
        let source_race = FormKey {
            local: 0x131E7,
            plugin: interner.intern("Skyrim.esm"),
        };
        let family = SkyrimFamilyMotionSet {
            family_id: "bear".to_string(),
            project_path: "Actors\\Bear\\BearProject.hkx".to_string(),
            race_attacks: vec![SkyrimRaceAttackEvidence {
                event: "attackStart_ForwardPower".to_string(),
                has_attack_spell: false,
            }],
            race_attack_sources: vec![SkyrimRaceAttackSourceEvidence {
                source_race,
                source_plugin: "Skyrim.esm".to_string(),
                attack: SkyrimRaceAttackEvidence {
                    event: "attackStart_ForwardPower".to_string(),
                    has_attack_spell: false,
                },
            }],
            clip_ids: vec!["AttackForwardPower".to_string()],
        };

        let bindings = candidate_attack_bindings(&family, &[]);

        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].source_identity.local_form_id, source_race.local);
        assert_eq!(bindings[0].role, CreatureClipRole::MeleeAttack);
    }

    #[test]
    fn exact_melee_clip_ownership_overrides_an_attack_spell_payload() {
        let interner = crate::sym::StringInterner::new();
        let family = SkyrimFamilyMotionSet {
            family_id: "giant".to_string(),
            project_path: "Actors\\Giant\\GiantProject.hkx".to_string(),
            race_attacks: Vec::new(),
            race_attack_sources: vec![SkyrimRaceAttackSourceEvidence {
                source_race: FormKey {
                    local: 0x131F9,
                    plugin: interner.intern("Skyrim.esm"),
                },
                source_plugin: "Skyrim.esm".to_string(),
                attack: SkyrimRaceAttackEvidence {
                    event: "attackPowerStart_Stomp".to_string(),
                    has_attack_spell: true,
                },
            }],
            clip_ids: vec!["Stomp".to_string()],
        };
        let stomp = SkyrimCatalogClip {
            clip_id: "Stomp".to_string(),
            clip_path: "Animations\\Stomp.hkx".to_string(),
            original_skeleton_name: None,
            family_ids: vec!["giant".to_string()],
            disposition: SkyrimClipDisposition::Role {
                role: SkyrimCreatureMotionRole::MeleeAttack,
                trigger_event: Some("attackPowerStart_Stomp".to_string()),
                trigger_aliases: Vec::new(),
                evidence: Vec::new(),
            },
            root_motion: SkyrimRootMotion::Stationary {
                source: SkyrimRootMotionSource::BoundAnims,
                sample_count: 1,
            },
            events: Vec::new(),
            annotations: Vec::new(),
        };

        let bindings = candidate_attack_bindings(&family, &[&stomp]);

        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].role, CreatureClipRole::MeleeAttack);
    }

    #[test]
    fn spell_payload_uses_the_only_executable_attack_role() {
        let attack = SkyrimRaceAttackEvidence {
            event: "attackPowerStart_Stomp".to_string(),
            has_attack_spell: true,
        };
        let melee_clip = SkyrimCatalogClip {
            clip_id: "Stomp".to_string(),
            clip_path: "Animations\\Stomp.hkx".to_string(),
            original_skeleton_name: None,
            family_ids: vec!["giant".to_string()],
            disposition: SkyrimClipDisposition::Role {
                role: SkyrimCreatureMotionRole::MeleeAttack,
                trigger_event: Some("attackStart_ClubAttack2".to_string()),
                trigger_aliases: Vec::new(),
                evidence: Vec::new(),
            },
            root_motion: SkyrimRootMotion::Stationary {
                source: SkyrimRootMotionSource::BoundAnims,
                sample_count: 1,
            },
            events: Vec::new(),
            annotations: Vec::new(),
        };

        assert_eq!(
            candidate_attack_role(&attack, &[&melee_clip]),
            CreatureClipRole::MeleeAttack
        );
    }
}

fn adapter_unavailable<T>(
    family: &SkyrimFamilyMotionSet,
    target: &'static str,
    detail: String,
) -> Result<T, SkyrimCreatureMotionError> {
    Err(SkyrimCreatureMotionError::AdapterUnavailable {
        family_id: family.family_id.clone(),
        target,
        detail,
    })
}

fn load_family_evidence(
    actors: &Path,
    animation_data: &ProjectAnimationData,
    family_id: &str,
    project_relative: &str,
    race_attack_sources: Vec<SkyrimRaceAttackSourceEvidence>,
) -> Result<SkyrimCreatureFamilyEvidence, SkyrimCreatureMotionError> {
    let project = actors.join(project_relative);
    let family_root = project.parent().expect("project has parent").to_path_buf();
    let mut inventory = SkyrimCreatureFamilyInventory {
        family_id: family_id.to_string(),
        project_path: runtime_asset_path(actors, &project),
        ..SkyrimCreatureFamilyInventory::default()
    };
    let mut queue = VecDeque::from([project]);
    let mut seen = HashSet::new();
    let mut clips = BTreeMap::<String, SkyrimCreatureClipEvidence>::new();
    let mut behavior_references = Vec::<(PathBuf, SkyrimBehaviorClipEvidence)>::new();
    let mut nested_behavior_references = Vec::<(PathBuf, SkyrimNestedBehaviorEvidence)>::new();

    while let Some(path) = queue.pop_front() {
        let canonical =
            path.canonicalize()
                .map_err(|error| SkyrimCreatureMotionError::ReadAsset {
                    path: path.display().to_string(),
                    detail: error.to_string(),
                })?;
        let key = path_key(&canonical.display().to_string());
        if !seen.insert(key) {
            continue;
        }
        let bytes =
            std::fs::read(&canonical).map_err(|error| SkyrimCreatureMotionError::ReadAsset {
                path: canonical.display().to_string(),
                detail: error.to_string(),
            })?;
        for reference in ascii_hkx_references(&bytes) {
            if let Some(resolved) = resolve_reference(actors, &family_root, &canonical, &reference)
            {
                queue.push_back(resolved);
            }
        }
        let hkx =
            HkxFile::read(&bytes).map_err(|error| SkyrimCreatureMotionError::DecodeHavok {
                path: canonical.display().to_string(),
                detail: error.to_string(),
            })?;
        if hkx.contents_version() != SKYRIM_CONTENTS_VERSION {
            continue;
        }
        let runtime_path = runtime_asset_path(actors, &canonical);
        if has_class(&hkx, "hkbCharacterData") {
            inventory.character_paths.push(runtime_path.clone());
            extract_character_inventory(
                &hkx,
                actors,
                &family_root,
                &canonical,
                &runtime_path,
                &mut inventory,
            );
        }
        if has_class(&hkx, "hkbBehaviorGraph") {
            inventory.behavior_paths.push(runtime_path.clone());
            let mut extracted = extract_behavior_clip_evidence(&hkx, actors, &canonical);
            resolve_behavior_group_clip_paths(
                &mut extracted.groups,
                actors,
                &family_root,
                &canonical,
            );
            inventory.behavior_groups.extend(extracted.groups);
            inventory.behavior_variables.extend(extracted.variables);
            for reference in extracted.clips {
                behavior_references.push((canonical.clone(), reference));
            }
            nested_behavior_references.extend(
                extracted
                    .nested_behaviors
                    .into_iter()
                    .map(|reference| (canonical.clone(), reference)),
            );
        }
        if has_class(&hkx, "hkaSkeleton") {
            inventory
                .animation_skeleton_paths
                .push(runtime_path.clone());
        }
        if has_class(&hkx, "hkaSplineCompressedAnimation") {
            let mut clip = extract_clip_evidence(&hkx, &runtime_path);
            clip.animation_data = animation_data_for_clip(&clip, animation_data);
            if matches!(clip.root_motion, SkyrimRootMotion::Unknown) {
                if let Some(root) = clip.animation_data.iter().find_map(|entry| {
                    (!matches!(entry.root_motion, SkyrimRootMotion::Unknown))
                        .then_some(entry.root_motion.clone())
                }) {
                    clip.root_motion = root;
                }
            }
            clips.insert(path_key(&canonical.display().to_string()), clip);
        }
    }

    let nested_behavior_edges = nested_behavior_references
        .into_iter()
        .filter_map(|(parent_path, reference)| {
            let child_path =
                resolve_reference(actors, &family_root, &parent_path, &reference.behavior_name)?;
            Some(ResolvedNestedBehaviorEdge {
                parent_key: path_key(&parent_path.display().to_string()),
                child_key: path_key(&child_path.display().to_string()),
                behavior_path: reference.behavior_path,
                initial: !reference.initial_state_names.is_empty(),
                incoming_events: reference.incoming_events,
            })
        })
        .collect::<Vec<_>>();
    if std::env::var("SKYRIMSE_CREATURE_DIAGNOSTIC_FAMILY")
        .ok()
        .is_some_and(|requested| requested == family_id)
    {
        eprintln!(
            "Skyrim diagnostic nested behavior edges={:?}",
            nested_behavior_edges
                .iter()
                .map(|edge| (
                    &edge.parent_key,
                    &edge.child_key,
                    &edge.behavior_path,
                    edge.initial,
                    &edge.incoming_events,
                ))
                .collect::<Vec<_>>()
        );
    }

    for (behavior_path, mut reference) in behavior_references {
        if !reference.initial_state_names.is_empty() {
            collect_ancestor_behavior_entry_events(
                &path_key(&behavior_path.display().to_string()),
                &nested_behavior_edges,
                &mut BTreeSet::new(),
                &mut reference.ancestor_entry_events,
            );
            reference.ancestor_entry_events.sort();
            reference.ancestor_entry_events.dedup();
        }
        let resolved = resolve_reference(
            actors,
            &family_root,
            &behavior_path,
            &reference.animation_name,
        );
        if resolved.is_none()
            && std::env::var("SKYRIMSE_CREATURE_DIAGNOSTIC_FAMILY")
                .ok()
                .is_some_and(|requested| requested == family_id)
        {
            eprintln!(
                "Skyrim diagnostic unresolved behavior clip behavior={} animation={}",
                runtime_asset_path(actors, &behavior_path),
                reference.animation_name
            );
        }
        let Some(resolved) = resolved else {
            continue;
        };
        let key = path_key(&resolved.display().to_string());
        if let Some(clip) = clips.get_mut(&key) {
            clip.behavior.push(reference);
            clip.animation_data = animation_data_for_clip(clip, animation_data);
            if matches!(clip.root_motion, SkyrimRootMotion::Unknown) {
                if let Some(root) = clip.animation_data.iter().find_map(|entry| {
                    (!matches!(entry.root_motion, SkyrimRootMotion::Unknown))
                        .then_some(entry.root_motion.clone())
                }) {
                    clip.root_motion = root;
                }
            }
        }
    }

    sort_dedup_paths(&mut inventory.character_paths);
    sort_dedup_paths(&mut inventory.animation_skeleton_paths);
    sort_dedup_paths(&mut inventory.ragdoll_paths);
    sort_dedup_paths(&mut inventory.behavior_paths);
    inventory
        .behavior_groups
        .sort_by_key(behavior_group_sort_key);
    inventory.behavior_groups.dedup();
    inventory.behavior_variables.sort();
    inventory.behavior_variables.dedup();
    inventory
        .controllers
        .sort_by(|left, right| left.character_path.cmp(&right.character_path));
    inventory.controllers.dedup_by(|left, right| {
        left.character_path
            .eq_ignore_ascii_case(&right.character_path)
    });

    Ok(SkyrimCreatureFamilyEvidence {
        family_id: family_id.to_string(),
        project_path: project_relative.to_string(),
        inventory,
        race_attacks: race_attack_sources
            .iter()
            .map(|evidence| evidence.attack.clone())
            .collect(),
        race_attack_sources,
        clips: clips.into_values().collect(),
    })
}

fn behavior_group_sort_key(group: &SkyrimBehaviorGeneratorGroupEvidence) -> (String, usize, u8) {
    match group {
        SkyrimBehaviorGeneratorGroupEvidence::ManualSelector(selector) => (
            path_key(&selector.behavior_path),
            selector.selector.object_index,
            0,
        ),
        SkyrimBehaviorGeneratorGroupEvidence::Blender(blender) => (
            path_key(&blender.behavior_path),
            blender.blender.object_index,
            1,
        ),
        SkyrimBehaviorGeneratorGroupEvidence::StateMachine(state_machine) => (
            path_key(&state_machine.behavior_path),
            state_machine.machine.source.object_index,
            2,
        ),
    }
}

struct ResolvedNestedBehaviorEdge {
    parent_key: String,
    child_key: String,
    behavior_path: String,
    initial: bool,
    incoming_events: Vec<String>,
}

fn collect_ancestor_behavior_entry_events(
    child_key: &str,
    edges: &[ResolvedNestedBehaviorEdge],
    visited: &mut BTreeSet<String>,
    events: &mut Vec<SkyrimBehaviorEntryEventEvidence>,
) {
    if !visited.insert(child_key.to_string()) {
        return;
    }
    for edge in edges.iter().filter(|edge| edge.child_key == child_key) {
        events.extend(
            edge.incoming_events
                .iter()
                .map(|event| SkyrimBehaviorEntryEventEvidence {
                    behavior_path: edge.behavior_path.clone(),
                    event: event.clone(),
                }),
        );
        if edge.initial {
            collect_ancestor_behavior_entry_events(&edge.parent_key, edges, visited, events);
        }
    }
}

fn extract_character_inventory(
    file: &HkxFile,
    actors: &Path,
    family_root: &Path,
    character_file: &Path,
    character_runtime_path: &str,
    inventory: &mut SkyrimCreatureFamilyInventory,
) {
    if let Some(strings) = file
        .objects()
        .iter()
        .find(|object| object.class_name == "hkbCharacterStringData")
    {
        if let Some(rig_name) = string_member(strings, "rigName") {
            push_resolved_reference(
                &mut inventory.animation_skeleton_paths,
                actors,
                family_root,
                character_file,
                &rig_name,
            );
        }
        if let Some(ragdoll_name) = string_member(strings, "ragdollName") {
            push_resolved_reference(
                &mut inventory.ragdoll_paths,
                actors,
                family_root,
                character_file,
                &ragdoll_name,
            );
        }
        if let Some(behavior_name) = string_member(strings, "behaviorFilename") {
            push_resolved_reference(
                &mut inventory.behavior_paths,
                actors,
                family_root,
                character_file,
                &behavior_name,
            );
        }
    }

    inventory.controllers.extend(
        file.objects()
            .iter()
            .filter(|object| object.class_name == "hkbCharacterData")
            .map(|object| extract_character_controller(file, object, character_runtime_path)),
    );
}

fn push_resolved_reference(
    paths: &mut Vec<String>,
    actors: &Path,
    family_root: &Path,
    current: &Path,
    reference: &str,
) {
    if let Some(resolved) = resolve_reference(actors, family_root, current, reference) {
        paths.push(runtime_asset_path(actors, &resolved));
    }
}

fn extract_character_controller(
    file: &HkxFile,
    character: &HkxObject,
    character_path: &str,
) -> SkyrimCharacterControllerEvidence {
    let contents_version = file.contents_version().to_string();
    let model = character_controller_model(character);
    if let Some((inline_class, controller)) =
        member_object_with_class(&character.members, "characterControllerInfo")
    {
        let controller_class = inline_class
            .unwrap_or("hkbCharacterControllerInfo")
            .to_string();
        let total_height = float_members(controller, "capsuleHeight");
        let radius = float_members(controller, "capsuleRadius");
        if file.contents_version() != SKYRIM_CONTENTS_VERSION
            || character.signature != SKYRIM_CHARACTER_DATA_SIGNATURE
            || controller_class != SKYRIM_CHARACTER_CONTROLLER_INFO_CLASS
        {
            return SkyrimCharacterControllerEvidence {
                character_path: character_path.to_string(),
                contents_version: contents_version.clone(),
                character_data_class: character.class_name.clone(),
                character_data_signature: character.signature,
                controller_class: Some(controller_class.clone()),
                controller_signature: None,
                layout: None,
                controller_cinfo_class: None,
                architecture: None,
                collision_filter_info: uint_members(controller, "collisionFilterInfo"),
                rigid_body_type: None,
                shape_type: None,
                capsule: None,
                model,
                disposition: SkyrimControllerEvidenceDisposition::LegacyLayoutUnsupported {
                    contents_version,
                    character_data_signature: character.signature,
                    controller_class: Some(controller_class),
                    total_height_bits: total_height.map(f32::to_bits),
                    radius_bits: radius.map(f32::to_bits),
                },
            };
        }
        let controller_cinfo_class = pointer_members(controller, "characterControllerCinfo")
            .and_then(|index| file.objects().get(index))
            .map(|object| object.class_name.clone());
        let architecture = controller_cinfo_class
            .as_deref()
            .and_then(controller_architecture_from_class);
        let capsule = valid_capsule(total_height, radius);
        let disposition = if capsule.is_none() {
            SkyrimControllerEvidenceDisposition::InvalidCapsuleDimensions {
                total_height_bits: total_height.map(f32::to_bits),
                radius_bits: radius.map(f32::to_bits),
            }
        } else if model.is_none() {
            SkyrimControllerEvidenceDisposition::InvalidModelTransform
        } else {
            SkyrimControllerEvidenceDisposition::Complete
        };
        return SkyrimCharacterControllerEvidence {
            character_path: character_path.to_string(),
            contents_version: contents_version.clone(),
            character_data_class: character.class_name.clone(),
            character_data_signature: character.signature,
            controller_class: Some(controller_class),
            controller_signature: Some(SKYRIM_CHARACTER_CONTROLLER_INFO_SIGNATURE),
            layout: Some(
                SkyrimControllerLayoutEvidence::LegacyCharacterControllerInfo {
                    contents_version,
                    character_data_signature: character.signature,
                    controller_signature: SKYRIM_CHARACTER_CONTROLLER_INFO_SIGNATURE,
                },
            ),
            controller_cinfo_class,
            architecture,
            collision_filter_info: uint_members(controller, "collisionFilterInfo"),
            rigid_body_type: None,
            shape_type: None,
            capsule,
            model,
            disposition,
        };
    }

    let Some(controller) = member_object(&character.members, "characterControllerSetup") else {
        return SkyrimCharacterControllerEvidence {
            character_path: character_path.to_string(),
            contents_version,
            character_data_class: character.class_name.clone(),
            character_data_signature: character.signature,
            controller_class: None,
            controller_signature: None,
            layout: None,
            controller_cinfo_class: None,
            architecture: None,
            collision_filter_info: None,
            rigid_body_type: None,
            shape_type: None,
            capsule: None,
            model,
            disposition: SkyrimControllerEvidenceDisposition::MissingControllerSetup,
        };
    };
    if file.contents_version() == SKYRIM_CONTENTS_VERSION {
        return SkyrimCharacterControllerEvidence {
            character_path: character_path.to_string(),
            contents_version: contents_version.clone(),
            character_data_class: character.class_name.clone(),
            character_data_signature: character.signature,
            controller_class: Some("hkbCharacterControllerSetup".to_string()),
            controller_signature: Some(HKB_CHARACTER_CONTROLLER_SETUP_SIGNATURE),
            layout: None,
            controller_cinfo_class: None,
            architecture: None,
            collision_filter_info: None,
            rigid_body_type: None,
            shape_type: None,
            capsule: None,
            model,
            disposition: SkyrimControllerEvidenceDisposition::LegacyLayoutUnsupported {
                contents_version,
                character_data_signature: character.signature,
                controller_class: Some("hkbCharacterControllerSetup".to_string()),
                total_height_bits: None,
                radius_bits: None,
            },
        };
    }
    let controller_cinfo_class = pointer_members(controller, "controllerCinfo")
        .and_then(|index| file.objects().get(index))
        .map(|object| object.class_name.clone());
    let architecture = controller_architecture(controller_cinfo_class.as_deref(), controller);
    let Some(rigid_body) = member_object(controller, "rigidBodySetup") else {
        return SkyrimCharacterControllerEvidence {
            character_path: character_path.to_string(),
            contents_version,
            character_data_class: character.class_name.clone(),
            character_data_signature: character.signature,
            controller_class: Some("hkbCharacterControllerSetup".to_string()),
            controller_signature: Some(HKB_CHARACTER_CONTROLLER_SETUP_SIGNATURE),
            layout: None,
            controller_cinfo_class,
            architecture,
            collision_filter_info: None,
            rigid_body_type: None,
            shape_type: None,
            capsule: None,
            model,
            disposition: SkyrimControllerEvidenceDisposition::MissingRigidBodySetup,
        };
    };
    let collision_filter_info = uint_members(rigid_body, "collisionFilterInfo");
    let rigid_body_type = int_members(rigid_body, "type");
    let Some(shape) = member_object(rigid_body, "shapeSetup") else {
        return SkyrimCharacterControllerEvidence {
            character_path: character_path.to_string(),
            contents_version,
            character_data_class: character.class_name.clone(),
            character_data_signature: character.signature,
            controller_class: Some("hkbCharacterControllerSetup".to_string()),
            controller_signature: Some(HKB_CHARACTER_CONTROLLER_SETUP_SIGNATURE),
            layout: None,
            controller_cinfo_class,
            architecture,
            collision_filter_info,
            rigid_body_type,
            shape_type: None,
            capsule: None,
            model,
            disposition: SkyrimControllerEvidenceDisposition::MissingShapeSetup,
        };
    };
    let shape_type = int_members(shape, "type");
    let total_height = float_members(shape, "capsuleHeight");
    let radius = float_members(shape, "capsuleRadius");
    let capsule = valid_capsule(total_height, radius);
    let disposition = if capsule.is_none() {
        SkyrimControllerEvidenceDisposition::InvalidCapsuleDimensions {
            total_height_bits: total_height.map(f32::to_bits),
            radius_bits: radius.map(f32::to_bits),
        }
    } else if model.is_none() {
        SkyrimControllerEvidenceDisposition::InvalidModelTransform
    } else {
        SkyrimControllerEvidenceDisposition::Complete
    };
    SkyrimCharacterControllerEvidence {
        character_path: character_path.to_string(),
        contents_version: contents_version.clone(),
        character_data_class: character.class_name.clone(),
        character_data_signature: character.signature,
        controller_class: Some("hkbCharacterControllerSetup".to_string()),
        controller_signature: Some(HKB_CHARACTER_CONTROLLER_SETUP_SIGNATURE),
        layout: Some(
            SkyrimControllerLayoutEvidence::NestedCharacterControllerSetup {
                contents_version,
                character_data_signature: character.signature,
                controller_signature: HKB_CHARACTER_CONTROLLER_SETUP_SIGNATURE,
            },
        ),
        controller_cinfo_class,
        architecture,
        collision_filter_info,
        rigid_body_type,
        shape_type,
        capsule,
        model,
        disposition,
    }
}

fn valid_capsule(
    total_height: Option<f32>,
    radius: Option<f32>,
) -> Option<SkyrimControllerCapsuleEvidence> {
    total_height.zip(radius).and_then(|(total_height, radius)| {
        (total_height.is_finite() && radius.is_finite() && total_height > 0.0 && radius > 0.0)
            .then_some(SkyrimControllerCapsuleEvidence {
                total_height,
                radius,
            })
    })
}

fn character_controller_model(character: &HkxObject) -> Option<SkyrimControllerModelEvidence> {
    let up_ms = vector4_member(character, "modelUpMS")?;
    let forward_ms = vector4_member(character, "modelForwardMS")?;
    let right_ms = vector4_member(character, "modelRightMS")?;
    let scale = float_members(&character.members, "scale")?;
    let valid_axis = |axis: [f32; 4]| {
        axis.iter().all(|value| value.is_finite())
            && axis[..3].iter().map(|value| value * value).sum::<f32>() > 0.0
    };
    (valid_axis(up_ms)
        && valid_axis(forward_ms)
        && valid_axis(right_ms)
        && scale.is_finite()
        && scale > 0.0)
        .then_some(SkyrimControllerModelEvidence {
            up_ms,
            forward_ms,
            right_ms,
            scale,
        })
}

fn controller_architecture(
    controller_cinfo_class: Option<&str>,
    controller: &[HkxMember],
) -> Option<SkyrimControllerArchitectureEvidence> {
    if let Some(class_name) = controller_cinfo_class {
        return controller_architecture_from_class(class_name);
    }
    member_object(controller, "rigidBodySetup")
        .is_some()
        .then_some(SkyrimControllerArchitectureEvidence::InlineRigidBodySetup)
}

fn controller_architecture_from_class(
    class_name: &str,
) -> Option<SkyrimControllerArchitectureEvidence> {
    let lower = class_name.to_ascii_lowercase();
    Some(if lower.contains("characterproxy") {
        SkyrimControllerArchitectureEvidence::CharacterProxyCinfo
    } else if lower.contains("characterigidbody") {
        SkyrimControllerArchitectureEvidence::CharacterRigidBodyCinfo
    } else if lower.contains("fixed") {
        SkyrimControllerArchitectureEvidence::FixedCinfo
    } else {
        SkyrimControllerArchitectureEvidence::CustomCinfo {
            class_name: class_name.to_string(),
        }
    })
}

fn has_class(file: &HkxFile, class_name: &str) -> bool {
    file.objects()
        .iter()
        .any(|object| object.class_name == class_name)
}

struct ExtractedSkyrimBehaviorEvidence {
    clips: Vec<SkyrimBehaviorClipEvidence>,
    nested_behaviors: Vec<SkyrimNestedBehaviorEvidence>,
    groups: Vec<SkyrimBehaviorGeneratorGroupEvidence>,
    variables: Vec<SkyrimBehaviorVariableEvidence>,
}

struct SkyrimNestedBehaviorEvidence {
    behavior_path: String,
    behavior_name: String,
    initial_state_names: Vec<String>,
    incoming_events: Vec<String>,
}

fn extract_behavior_clip_evidence(
    file: &HkxFile,
    actors: &Path,
    behavior_path: &Path,
) -> ExtractedSkyrimBehaviorEvidence {
    let event_names = file
        .objects()
        .iter()
        .find(|object| object.class_name == "hkbBehaviorGraphStringData")
        .map(|object| string_array(object, "eventNames"))
        .unwrap_or_default();
    let variable_names = file
        .objects()
        .iter()
        .find(|object| object.class_name == "hkbBehaviorGraphStringData")
        .map(|object| string_array(object, "variableNames"))
        .unwrap_or_default();
    let (variable_types, variable_initial_words) = behavior_variable_data(file);
    let mut incoming = BTreeMap::<usize, BTreeSet<String>>::new();
    let mut initial_states = BTreeSet::new();
    let mut states = Vec::new();
    for (state_index, object) in file
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, object)| object.class_name == "hkbStateMachineStateInfo")
    {
        let name = string_member(object, "name").unwrap_or_default();
        let generator = pointer_member(object, "generator");
        states.push((state_index, name, generator));
    }
    for object in file
        .objects()
        .iter()
        .filter(|object| object.class_name == "hkbStateMachine")
    {
        let mut owned_states = Vec::new();
        if let Some(values) = array_member(object, "states") {
            for value in values {
                collect_pointers(value, &mut owned_states);
            }
        }
        owned_states.retain(|index| {
            file.objects()
                .get(*index)
                .is_some_and(|state| state.class_name == "hkbStateMachineStateInfo")
        });
        let mut machine_incoming = BTreeMap::<i32, BTreeSet<String>>::new();
        for state_index in &owned_states {
            let state = &file.objects()[*state_index];
            if let Some(transitions) = pointer_member(state, "transitions") {
                collect_incoming_transitions(
                    file,
                    transitions,
                    &event_names,
                    &mut machine_incoming,
                );
            }
        }
        if let Some(transitions) = pointer_member(object, "wildcardTransitions") {
            collect_incoming_transitions(file, transitions, &event_names, &mut machine_incoming);
        }
        for state_index in owned_states {
            let Some(state_id) = int_member(&file.objects()[state_index], "stateId") else {
                continue;
            };
            if let Some(events) = machine_incoming.get(&state_id) {
                incoming
                    .entry(state_index)
                    .or_default()
                    .extend(events.iter().cloned());
            }
        }
    }
    for root in file
        .objects()
        .iter()
        .filter(|object| object.class_name == "hkbBehaviorGraph")
        .filter_map(|object| pointer_member(object, "rootGenerator"))
    {
        collect_initial_state_chain(file, root, &mut BTreeSet::new(), &mut initial_states);
    }

    let behavior_runtime = behavior_path
        .strip_prefix(actors)
        .unwrap_or(behavior_path)
        .display()
        .to_string();
    let mut references = BTreeMap::<usize, SkyrimBehaviorClipEvidence>::new();
    for (index, object) in file.objects().iter().enumerate() {
        if object.class_name != "hkbClipGenerator" {
            continue;
        }
        let Some(animation_name) = string_member(object, "animationName") else {
            continue;
        };
        if animation_name.is_empty() {
            continue;
        }
        references.insert(
            index,
            SkyrimBehaviorClipEvidence {
                behavior_path: canonical_runtime_path(&behavior_runtime),
                animation_name: canonical_runtime_path(&animation_name),
                generator_name: string_member(object, "name").filter(|name| !name.is_empty()),
                state_names: Vec::new(),
                initial_state_names: Vec::new(),
                topology_names: Vec::new(),
                incoming_events: Vec::new(),
                ancestor_entry_events: Vec::new(),
                emitted_events: clip_trigger_events(file, object, &event_names),
                looping: int_member(object, "mode").is_some_and(|mode| mode & 1 != 0),
            },
        );
    }
    let mut nested_behaviors = file
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, object)| object.class_name == "hkbBehaviorReferenceGenerator")
        .filter_map(|(index, object)| {
            Some((
                index,
                SkyrimNestedBehaviorEvidence {
                    behavior_path: canonical_runtime_path(&behavior_runtime),
                    behavior_name: string_member(object, "behaviorName")
                        .filter(|name| !name.is_empty())?,
                    initial_state_names: Vec::new(),
                    incoming_events: Vec::new(),
                },
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let mut nearest_states = BTreeMap::<usize, NearestBehaviorState>::new();
    for (state_index, state_name, generator) in states {
        let Some(generator) = generator else {
            continue;
        };
        let mut reached = BTreeMap::new();
        collect_terminal_generators(
            file,
            generator,
            0,
            &mut HashMap::new(),
            &BTreeSet::new(),
            "hkbClipGenerator",
            &mut reached,
        );
        collect_terminal_generators(
            file,
            generator,
            0,
            &mut HashMap::new(),
            &BTreeSet::new(),
            "hkbBehaviorReferenceGenerator",
            &mut reached,
        );
        for (clip_index, reach) in reached {
            nearest_states.entry(clip_index).or_default().merge(
                reach.distance,
                &state_name,
                initial_states.contains(&state_index),
                reach.topology_names,
                incoming.get(&state_index),
            );
        }
    }
    for (clip_index, reference) in &mut references {
        let Some(nearest) = nearest_states.get(clip_index) else {
            continue;
        };
        reference.state_names = nearest.state_names.iter().cloned().collect();
        reference.initial_state_names = nearest.initial_state_names.iter().cloned().collect();
        reference.topology_names = nearest.topology_names.iter().cloned().collect();
        reference.incoming_events = nearest.incoming_events.iter().cloned().collect();
    }
    for (reference_index, reference) in &mut nested_behaviors {
        let Some(nearest) = nearest_states.get(reference_index) else {
            continue;
        };
        reference.initial_state_names = nearest.initial_state_names.iter().cloned().collect();
        reference.incoming_events = nearest.incoming_events.iter().cloned().collect();
    }
    let groups = extract_behavior_generator_groups(
        file,
        &behavior_runtime,
        &event_names,
        &variable_names,
        &variable_types,
        &variable_initial_words,
    );
    let behavior_path = canonical_runtime_path(&behavior_runtime);
    let variable_count = variable_names
        .len()
        .max(variable_types.len())
        .max(variable_initial_words.len());
    let variables = (0..variable_count)
        .map(|variable_index| SkyrimBehaviorVariableEvidence {
            behavior_path: behavior_path.clone(),
            variable_index,
            variable_name: variable_names
                .get(variable_index)
                .filter(|name| !name.is_empty())
                .cloned(),
            variable_type: variable_types.get(variable_index).copied().flatten(),
            initial_word_value: variable_initial_words
                .get(variable_index)
                .copied()
                .flatten(),
        })
        .collect();
    ExtractedSkyrimBehaviorEvidence {
        clips: references.into_values().collect(),
        nested_behaviors: nested_behaviors.into_values().collect(),
        groups,
        variables,
    }
}

fn extract_behavior_generator_groups(
    file: &HkxFile,
    behavior_path: &str,
    event_names: &[String],
    variable_names: &[String],
    variable_types: &[Option<i32>],
    variable_initial_words: &[Option<u32>],
) -> Vec<SkyrimBehaviorGeneratorGroupEvidence> {
    file.objects()
        .iter()
        .enumerate()
        .filter_map(|(object_index, object)| match object.class_name.as_str() {
            "hkbManualSelectorGenerator" => {
                Some(SkyrimBehaviorGeneratorGroupEvidence::ManualSelector(
                    SkyrimManualSelectorEvidence {
                        behavior_path: canonical_runtime_path(behavior_path),
                        selector: behavior_object_evidence(file, object_index)?,
                        selected_generator_index: int_member(object, "selectedGeneratorIndex")
                            .unwrap_or(-1),
                        index_selector: pointer_member(object, "indexSelector")
                            .and_then(|index| behavior_object_evidence(file, index)),
                        selected_index_can_change_after_activate: bool_member(
                            object,
                            "selectedIndexCanChangeAfterActivate",
                        )
                        .unwrap_or(false),
                        transition_effect: pointer_member(
                            object,
                            "generatorChangedTransitionEffect",
                        )
                        .and_then(|index| behavior_object_evidence(file, index)),
                        blending_transition_effect: pointer_member(
                            object,
                            "generatorChangedTransitionEffect",
                        )
                        .and_then(|index| {
                            behavior_blending_transition_effect(
                                file,
                                index,
                                variable_names,
                                variable_types,
                                variable_initial_words,
                            )
                        }),
                        variable_bindings: behavior_variable_bindings(
                            file,
                            object,
                            variable_names,
                            variable_types,
                            variable_initial_words,
                        ),
                        children: behavior_group_children(
                            file,
                            object,
                            "generators",
                            false,
                            event_names,
                            variable_names,
                            variable_types,
                            variable_initial_words,
                        ),
                    },
                ))
            }
            "hkbBlenderGenerator" => Some(SkyrimBehaviorGeneratorGroupEvidence::Blender(
                SkyrimBlenderEvidence {
                    behavior_path: canonical_runtime_path(behavior_path),
                    blender: behavior_object_evidence(file, object_index)?,
                    reference_pose_weight_threshold_bits: float_member(
                        object,
                        "referencePoseWeightThreshold",
                    )
                    .map(f32::to_bits),
                    blend_parameter_bits: float_member(object, "blendParameter").map(f32::to_bits),
                    min_cyclic_blend_parameter_bits: float_member(
                        object,
                        "minCyclicBlendParameter",
                    )
                    .map(f32::to_bits),
                    max_cyclic_blend_parameter_bits: float_member(
                        object,
                        "maxCyclicBlendParameter",
                    )
                    .map(f32::to_bits),
                    index_of_sync_master_child: int_member(object, "indexOfSyncMasterChild")
                        .unwrap_or(-1),
                    flags: int_member(object, "flags").unwrap_or(0),
                    subtract_last_child: bool_member(object, "subtractLastChild").unwrap_or(false),
                    variable_bindings: behavior_variable_bindings(
                        file,
                        object,
                        variable_names,
                        variable_types,
                        variable_initial_words,
                    ),
                    children: behavior_group_children(
                        file,
                        object,
                        "children",
                        true,
                        event_names,
                        variable_names,
                        variable_types,
                        variable_initial_words,
                    ),
                },
            )),
            "hkbStateMachine" => {
                let mut stack = BTreeSet::from([object_index]);
                Some(SkyrimBehaviorGeneratorGroupEvidence::StateMachine(
                    SkyrimStateMachineGeneratorGroupEvidence {
                        behavior_path: canonical_runtime_path(behavior_path),
                        machine: behavior_state_machine_evidence(
                            file,
                            object_index,
                            event_names,
                            variable_names,
                            variable_types,
                            variable_initial_words,
                            &mut stack,
                        ),
                    },
                ))
            }
            _ => None,
        })
        .collect()
}

fn behavior_object_evidence(
    file: &HkxFile,
    object_index: usize,
) -> Option<SkyrimBehaviorObjectEvidence> {
    let object = file.objects().get(object_index)?;
    Some(SkyrimBehaviorObjectEvidence {
        object_index,
        class_name: object.class_name.clone(),
        name: string_member(object, "name").filter(|name| !name.is_empty()),
    })
}

fn behavior_blending_transition_effect(
    file: &HkxFile,
    object_index: usize,
    variable_names: &[String],
    variable_types: &[Option<i32>],
    variable_initial_words: &[Option<u32>],
) -> Option<SkyrimBehaviorBlendingTransitionEffectEvidence> {
    let object = file.objects().get(object_index)?;
    (object.class_name == "hkbBlendingTransitionEffect").then(|| {
        SkyrimBehaviorBlendingTransitionEffectEvidence {
            source: behavior_object_evidence(file, object_index)
                .expect("transition effect source object exists"),
            self_transition_mode: int_member(object, "selfTransitionMode"),
            event_mode: int_member(object, "eventMode"),
            duration_bits: float_member(object, "duration").map(f32::to_bits),
            to_generator_start_time_fraction_bits: float_member(
                object,
                "toGeneratorStartTimeFraction",
            )
            .map(f32::to_bits),
            flags: int_member(object, "flags"),
            end_mode: int_member(object, "endMode"),
            blend_curve: int_member(object, "blendCurve"),
            alignment_bone: int_member(object, "alignmentBone"),
            variable_bindings: behavior_variable_bindings(
                file,
                object,
                variable_names,
                variable_types,
                variable_initial_words,
            ),
        }
    })
}

fn behavior_variable_bindings(
    file: &HkxFile,
    object: &HkxObject,
    variable_names: &[String],
    variable_types: &[Option<i32>],
    variable_initial_words: &[Option<u32>],
) -> Vec<SkyrimBehaviorVariableBindingEvidence> {
    let Some(binding_set) = pointer_member(object, "variableBindingSet")
        .and_then(|index| file.objects().get(index))
        .filter(|object| object.class_name == "hkbVariableBindingSet")
    else {
        return Vec::new();
    };
    let mut bindings = array_member(binding_set, "bindings")
        .into_iter()
        .flatten()
        .filter_map(HkxValue::as_object_members)
        .filter_map(|members| {
            let variable_index = usize::try_from(int_members(members, "variableIndex")?).ok()?;
            Some(SkyrimBehaviorVariableBindingEvidence {
                member_path: string_members(members, "memberPath")?,
                variable_index,
                variable_name: variable_names.get(variable_index).cloned(),
                variable_type: variable_types.get(variable_index).copied().flatten(),
                initial_word_value: variable_initial_words
                    .get(variable_index)
                    .copied()
                    .flatten(),
                bit_index: int_members(members, "bitIndex").unwrap_or(-1),
                binding_type: int_members(members, "bindingType").unwrap_or(0),
            })
        })
        .collect::<Vec<_>>();
    bindings.sort_by(|left, right| {
        left.member_path
            .cmp(&right.member_path)
            .then_with(|| left.variable_index.cmp(&right.variable_index))
            .then_with(|| left.bit_index.cmp(&right.bit_index))
            .then_with(|| left.binding_type.cmp(&right.binding_type))
    });
    bindings.dedup();
    bindings
}

fn behavior_variable_data(file: &HkxFile) -> (Vec<Option<i32>>, Vec<Option<u32>>) {
    let Some(data) = file
        .objects()
        .iter()
        .find(|object| object.class_name == "hkbBehaviorGraphData")
    else {
        return (Vec::new(), Vec::new());
    };
    let variable_types = array_member(data, "variableInfos")
        .into_iter()
        .flatten()
        .map(|value| {
            value
                .as_object_members()
                .and_then(|members| int_members(members, "type"))
        })
        .collect();
    let variable_initial_words = pointer_member(data, "variableInitialValues")
        .and_then(|index| file.objects().get(index))
        .filter(|object| object.class_name == "hkbVariableValueSet")
        .and_then(|values| array_member(values, "wordVariableValues"))
        .map(|values| {
            values
                .iter()
                .map(|value| {
                    value
                        .as_object_members()
                        .and_then(|members| word_value_bits(members, "value"))
                })
                .collect()
        })
        .unwrap_or_default();
    (variable_types, variable_initial_words)
}

fn word_value_bits(members: &[HkxMember], name: &str) -> Option<u32> {
    members.iter().find_map(|member| {
        if member.name != name {
            return None;
        }
        match member.value {
            HkxValue::I8(value) => Some(i32::from(value) as u32),
            HkxValue::U8(value) => Some(u32::from(value)),
            HkxValue::I16(value) => Some(i32::from(value) as u32),
            HkxValue::U16(value) => Some(u32::from(value)),
            HkxValue::I32(value) => Some(value as u32),
            HkxValue::U32(value) => Some(value),
            _ => None,
        }
    })
}

fn behavior_generator_tree(
    file: &HkxFile,
    object_index: usize,
    event_names: &[String],
    variable_names: &[String],
    variable_types: &[Option<i32>],
    variable_initial_words: &[Option<u32>],
    stack: &mut BTreeSet<usize>,
) -> SkyrimBehaviorGeneratorNodeEvidence {
    let Some(source) = behavior_object_evidence(file, object_index) else {
        return SkyrimBehaviorGeneratorNodeEvidence::Missing { object_index };
    };
    if !stack.insert(object_index) {
        return SkyrimBehaviorGeneratorNodeEvidence::Cycle { source };
    }
    let object = &file.objects()[object_index];
    let node = match object.class_name.as_str() {
        "hkbClipGenerator" => {
            if let Some(animation_name) =
                string_member(object, "animationName").filter(|value| !value.is_empty())
            {
                SkyrimBehaviorGeneratorNodeEvidence::Clip {
                    source,
                    animation_name,
                    resolved_clip_path: None,
                }
            } else {
                SkyrimBehaviorGeneratorNodeEvidence::Unsupported {
                    source,
                    child_members: Vec::new(),
                }
            }
        }
        "hkbManualSelectorGenerator" => {
            let children = generator_member_pointers(object, "generators")
                .into_iter()
                .map(|child| {
                    behavior_generator_tree(
                        file,
                        child,
                        event_names,
                        variable_names,
                        variable_types,
                        variable_initial_words,
                        stack,
                    )
                })
                .collect();
            SkyrimBehaviorGeneratorNodeEvidence::ManualSelector {
                source,
                selected_generator_index: int_member(object, "selectedGeneratorIndex"),
                index_selector: pointer_member(object, "indexSelector")
                    .and_then(|index| behavior_object_evidence(file, index)),
                selected_index_can_change_after_activate: bool_member(
                    object,
                    "selectedIndexCanChangeAfterActivate",
                ),
                transition_effect: pointer_member(object, "generatorChangedTransitionEffect")
                    .and_then(|index| behavior_object_evidence(file, index)),
                blending_transition_effect: pointer_member(
                    object,
                    "generatorChangedTransitionEffect",
                )
                .and_then(|index| {
                    behavior_blending_transition_effect(
                        file,
                        index,
                        variable_names,
                        variable_types,
                        variable_initial_words,
                    )
                }),
                variable_bindings: behavior_variable_bindings(
                    file,
                    object,
                    variable_names,
                    variable_types,
                    variable_initial_words,
                ),
                children,
            }
        }
        "hkbBlenderGenerator" => {
            let children = generator_member_pointers(object, "children")
                .into_iter()
                .enumerate()
                .filter_map(|(ordinal, child_index)| {
                    let child = file.objects().get(child_index)?;
                    let generator_index = pointer_member(child, "generator")?;
                    Some(SkyrimBehaviorBlenderChildNodeEvidence {
                        ordinal,
                        source: behavior_object_evidence(file, child_index)?,
                        generator: behavior_generator_tree(
                            file,
                            generator_index,
                            event_names,
                            variable_names,
                            variable_types,
                            variable_initial_words,
                            stack,
                        ),
                        weight_bits: float_member(child, "weight").map(f32::to_bits),
                        world_from_model_weight_bits: float_member(child, "worldFromModelWeight")
                            .map(f32::to_bits),
                        bone_weights: pointer_member(child, "boneWeights")
                            .and_then(|index| behavior_object_evidence(file, index)),
                    })
                })
                .collect();
            SkyrimBehaviorGeneratorNodeEvidence::Blender {
                source,
                reference_pose_weight_threshold_bits: float_member(
                    object,
                    "referencePoseWeightThreshold",
                )
                .map(f32::to_bits),
                blend_parameter_bits: float_member(object, "blendParameter").map(f32::to_bits),
                min_cyclic_blend_parameter_bits: float_member(object, "minCyclicBlendParameter")
                    .map(f32::to_bits),
                max_cyclic_blend_parameter_bits: float_member(object, "maxCyclicBlendParameter")
                    .map(f32::to_bits),
                index_of_sync_master_child: int_member(object, "indexOfSyncMasterChild"),
                flags: int_member(object, "flags"),
                subtract_last_child: bool_member(object, "subtractLastChild"),
                variable_bindings: behavior_variable_bindings(
                    file,
                    object,
                    variable_names,
                    variable_types,
                    variable_initial_words,
                ),
                children,
            }
        }
        "hkbStateMachine" => {
            SkyrimBehaviorGeneratorNodeEvidence::StateMachine(behavior_state_machine_evidence(
                file,
                object_index,
                event_names,
                variable_names,
                variable_types,
                variable_initial_words,
                stack,
            ))
        }
        "hkbModifierGenerator" => SkyrimBehaviorGeneratorNodeEvidence::ModifierGenerator {
            source,
            user_data: uint64_member(object, "userData"),
            variable_bindings: behavior_variable_bindings(
                file,
                object,
                variable_names,
                variable_types,
                variable_initial_words,
            ),
            modifier: pointer_member(object, "modifier").map(|modifier| {
                Box::new(behavior_modifier_tree(
                    file,
                    modifier,
                    variable_names,
                    variable_types,
                    variable_initial_words,
                    &mut BTreeSet::new(),
                ))
            }),
            generator: pointer_member(object, "generator").map(|generator| {
                Box::new(behavior_generator_tree(
                    file,
                    generator,
                    event_names,
                    variable_names,
                    variable_types,
                    variable_initial_words,
                    stack,
                ))
            }),
        },
        _ => SkyrimBehaviorGeneratorNodeEvidence::Unsupported {
            source,
            child_members: object
                .members
                .iter()
                .filter(|member| generator_child_member(&object.class_name, &member.name))
                .map(|member| SkyrimBehaviorNamedGeneratorChildrenEvidence {
                    member_name: member.name.clone(),
                    children: {
                        let mut pointers = Vec::new();
                        collect_pointers(&member.value, &mut pointers);
                        pointers
                            .into_iter()
                            .map(|child| {
                                behavior_generator_tree(
                                    file,
                                    child,
                                    event_names,
                                    variable_names,
                                    variable_types,
                                    variable_initial_words,
                                    stack,
                                )
                            })
                            .collect()
                    },
                })
                .collect(),
        },
    };
    stack.remove(&object_index);
    node
}

fn behavior_modifier_tree(
    file: &HkxFile,
    object_index: usize,
    variable_names: &[String],
    variable_types: &[Option<i32>],
    variable_initial_words: &[Option<u32>],
    stack: &mut BTreeSet<usize>,
) -> SkyrimBehaviorModifierNodeEvidence {
    let Some(source) = behavior_object_evidence(file, object_index) else {
        return SkyrimBehaviorModifierNodeEvidence::Missing { object_index };
    };
    if !stack.insert(object_index) {
        return SkyrimBehaviorModifierNodeEvidence::Cycle { source };
    }
    let object = &file.objects()[object_index];
    let variable_bindings = behavior_variable_bindings(
        file,
        object,
        variable_names,
        variable_types,
        variable_initial_words,
    );
    let node = match object.class_name.as_str() {
        "hkbModifierList" => {
            let mut modifier_indices = Vec::new();
            for value in array_member(object, "modifiers").into_iter().flatten() {
                collect_pointers(value, &mut modifier_indices);
            }
            SkyrimBehaviorModifierNodeEvidence::ModifierList {
                source,
                user_data: uint64_member(object, "userData"),
                enable: bool_member(object, "enable"),
                variable_bindings,
                modifiers: modifier_indices
                    .into_iter()
                    .map(|modifier| {
                        behavior_modifier_tree(
                            file,
                            modifier,
                            variable_names,
                            variable_types,
                            variable_initial_words,
                            stack,
                        )
                    })
                    .collect(),
            }
        }
        "hkbDampingModifier" => SkyrimBehaviorModifierNodeEvidence::Damping {
            source,
            user_data: uint64_member(object, "userData"),
            enable: bool_member(object, "enable"),
            variable_bindings,
            k_p_bits: float_member(object, "kP").map(f32::to_bits),
            k_i_bits: float_member(object, "kI").map(f32::to_bits),
            k_d_bits: float_member(object, "kD").map(f32::to_bits),
            enable_scalar_damping: bool_member(object, "enableScalarDamping"),
            enable_vector_damping: bool_member(object, "enableVectorDamping"),
            raw_value_bits: float_member(object, "rawValue").map(f32::to_bits),
            damped_value_bits: float_member(object, "dampedValue").map(f32::to_bits),
            raw_vector_bits: vector4_member(object, "rawVector")
                .map(|values| values.map(f32::to_bits)),
            damped_vector_bits: vector4_member(object, "dampedVector")
                .map(|values| values.map(f32::to_bits)),
            vector_error_sum_bits: vector4_member(object, "vecErrorSum")
                .map(|values| values.map(f32::to_bits)),
            vector_previous_error_bits: vector4_member(object, "vecPreviousError")
                .map(|values| values.map(f32::to_bits)),
            error_sum_bits: float_member(object, "errorSum").map(f32::to_bits),
            previous_error_bits: float_member(object, "previousError").map(f32::to_bits),
        },
        "hkbEvaluateExpressionModifier" => {
            let expressions = pointer_member(object, "expressions").and_then(|index| {
                let expressions = file.objects().get(index)?;
                (expressions.class_name == "hkbExpressionDataArray").then(|| {
                    SkyrimBehaviorExpressionDataArrayEvidence {
                        source: behavior_object_evidence(file, index)
                            .expect("expression data source object exists"),
                        expressions: array_member(expressions, "expressionsData")
                            .into_iter()
                            .flatten()
                            .enumerate()
                            .filter_map(|(ordinal, value)| {
                                let members = value.as_object_members()?;
                                let expression = string_members(members, "expression");
                                let referenced_variables = expression
                                    .as_deref()
                                    .into_iter()
                                    .flat_map(|expression| {
                                        variable_names.iter().enumerate().filter_map(
                                            move |(variable_index, variable_name)| {
                                                expression_contains_identifier(
                                                    expression,
                                                    variable_name,
                                                )
                                                .then(|| {
                                                    SkyrimBehaviorExpressionVariableReferenceEvidence {
                                                        variable_index,
                                                        variable_name: variable_name.clone(),
                                                        variable_type: variable_types
                                                            .get(variable_index)
                                                            .copied()
                                                            .flatten(),
                                                        initial_word_value: variable_initial_words
                                                            .get(variable_index)
                                                            .copied()
                                                            .flatten(),
                                                    }
                                                })
                                            },
                                        )
                                    })
                                    .collect();
                                Some(SkyrimBehaviorExpressionEvidence {
                                    ordinal,
                                    expression,
                                    referenced_variables,
                                    assignment_variable_index: int_members(
                                        members,
                                        "assignmentVariableIndex",
                                    ),
                                    assignment_event_index: int_members(
                                        members,
                                        "assignmentEventIndex",
                                    ),
                                    event_mode: int_members(members, "eventMode"),
                                })
                            })
                            .collect(),
                    }
                })
            });
            SkyrimBehaviorModifierNodeEvidence::EvaluateExpression {
                source,
                user_data: uint64_member(object, "userData"),
                enable: bool_member(object, "enable"),
                variable_bindings,
                expressions,
            }
        }
        _ => SkyrimBehaviorModifierNodeEvidence::Unsupported {
            source,
            user_data: uint64_member(object, "userData"),
            enable: bool_member(object, "enable"),
            variable_bindings,
        },
    };
    stack.remove(&object_index);
    node
}

fn expression_contains_identifier(expression: &str, identifier: &str) -> bool {
    if identifier.is_empty() {
        return false;
    }
    expression.match_indices(identifier).any(|(start, _)| {
        let end = start + identifier.len();
        let identifier_character = |value: char| value.is_ascii_alphanumeric() || value == '_';
        expression[..start]
            .chars()
            .next_back()
            .is_none_or(|value| !identifier_character(value))
            && expression[end..]
                .chars()
                .next()
                .is_none_or(|value| !identifier_character(value))
    })
}

fn generator_member_pointers(object: &HkxObject, member_name: &str) -> Vec<usize> {
    let mut pointers = Vec::new();
    if let Some(values) = array_member(object, member_name) {
        for value in values {
            collect_pointers(value, &mut pointers);
        }
    }
    pointers
}

fn behavior_state_machine_evidence(
    file: &HkxFile,
    object_index: usize,
    event_names: &[String],
    variable_names: &[String],
    variable_types: &[Option<i32>],
    variable_initial_words: &[Option<u32>],
    stack: &mut BTreeSet<usize>,
) -> SkyrimBehaviorStateMachineEvidence {
    let object = &file.objects()[object_index];
    let return_to_previous_state_event_id = int_member(object, "returnToPreviousStateEventId");
    let random_transition_event_id = int_member(object, "randomTransitionEventId");
    let transition_to_next_higher_state_event_id =
        int_member(object, "transitionToNextHigherStateEventId");
    let transition_to_next_lower_state_event_id =
        int_member(object, "transitionToNextLowerStateEventId");
    SkyrimBehaviorStateMachineEvidence {
        source: behavior_object_evidence(file, object_index).expect("state machine exists"),
        variable_bindings: behavior_variable_bindings(
            file,
            object,
            variable_names,
            variable_types,
            variable_initial_words,
        ),
        event_to_send_when_state_or_transition_changes: member_object(
            &object.members,
            "eventToSendWhenStateOrTransitionChanges",
        )
        .map(|members| behavior_event_property(file, members, event_names)),
        start_state_id_selector: pointer_member(object, "startStateIdSelector")
            .and_then(|index| behavior_object_evidence(file, index)),
        start_state_id: int_member(object, "startStateId"),
        return_to_previous_state_event_id,
        return_to_previous_state_event_name: return_to_previous_state_event_id
            .and_then(|id| behavior_event_name(event_names, id)),
        random_transition_event_id,
        random_transition_event_name: random_transition_event_id
            .and_then(|id| behavior_event_name(event_names, id)),
        transition_to_next_higher_state_event_id,
        transition_to_next_higher_state_event_name: transition_to_next_higher_state_event_id
            .and_then(|id| behavior_event_name(event_names, id)),
        transition_to_next_lower_state_event_id,
        transition_to_next_lower_state_event_name: transition_to_next_lower_state_event_id
            .and_then(|id| behavior_event_name(event_names, id)),
        sync_variable_index: int_member(object, "syncVariableIndex"),
        wrap_around_state_id: bool_member(object, "wrapAroundStateId"),
        max_simultaneous_transitions: int_member(object, "maxSimultaneousTransitions"),
        start_state_mode: int_member(object, "startStateMode"),
        self_transition_mode: int_member(object, "selfTransitionMode"),
        states: generator_member_pointers(object, "states")
            .into_iter()
            .enumerate()
            .filter_map(|(ordinal, state_index)| {
                behavior_state_evidence(
                    file,
                    ordinal,
                    state_index,
                    event_names,
                    variable_names,
                    variable_types,
                    variable_initial_words,
                    stack,
                )
            })
            .collect(),
        wildcard_transitions: pointer_member(object, "wildcardTransitions").and_then(|index| {
            behavior_transition_array(
                file,
                index,
                event_names,
                variable_names,
                variable_types,
                variable_initial_words,
            )
        }),
    }
}

fn behavior_state_evidence(
    file: &HkxFile,
    ordinal: usize,
    object_index: usize,
    event_names: &[String],
    variable_names: &[String],
    variable_types: &[Option<i32>],
    variable_initial_words: &[Option<u32>],
    stack: &mut BTreeSet<usize>,
) -> Option<SkyrimBehaviorStateEvidence> {
    let object = file.objects().get(object_index)?;
    if object.class_name != "hkbStateMachineStateInfo" {
        return None;
    }
    Some(SkyrimBehaviorStateEvidence {
        ordinal,
        source: behavior_object_evidence(file, object_index)?,
        listeners: generator_member_pointers(object, "listeners")
            .into_iter()
            .filter_map(|index| behavior_object_evidence(file, index))
            .collect(),
        enter_notify_events: behavior_event_property_array(
            file,
            object,
            "enterNotifyEvents",
            event_names,
        ),
        exit_notify_events: behavior_event_property_array(
            file,
            object,
            "exitNotifyEvents",
            event_names,
        ),
        transitions: pointer_member(object, "transitions").and_then(|index| {
            behavior_transition_array(
                file,
                index,
                event_names,
                variable_names,
                variable_types,
                variable_initial_words,
            )
        }),
        generator: pointer_member(object, "generator").map(|index| {
            behavior_generator_tree(
                file,
                index,
                event_names,
                variable_names,
                variable_types,
                variable_initial_words,
                stack,
            )
        }),
        name: string_member(object, "name").filter(|name| !name.is_empty()),
        state_id: int_member(object, "stateId"),
        probability_bits: float_member(object, "probability").map(f32::to_bits),
        enable: bool_member(object, "enable"),
        has_eventless_transitions: bool_member(object, "hasEventlessTransitions"),
    })
}

fn behavior_transition_array(
    file: &HkxFile,
    object_index: usize,
    event_names: &[String],
    variable_names: &[String],
    variable_types: &[Option<i32>],
    variable_initial_words: &[Option<u32>],
) -> Option<SkyrimBehaviorTransitionArrayEvidence> {
    let object = file.objects().get(object_index)?;
    let transitions = array_member(object, "transitions")?
        .iter()
        .enumerate()
        .filter_map(|(ordinal, value)| {
            let members = value.as_object_members()?;
            let event_id = int_members(members, "eventId");
            Some(SkyrimBehaviorTransitionEvidence {
                ordinal,
                trigger_interval: member_object(members, "triggerInterval")
                    .map(|interval| behavior_transition_interval(interval, event_names)),
                initiate_interval: member_object(members, "initiateInterval")
                    .map(|interval| behavior_transition_interval(interval, event_names)),
                transition_effect: pointer_members(members, "transition")
                    .and_then(|index| behavior_object_evidence(file, index)),
                blending_transition_effect: pointer_members(members, "transition").and_then(
                    |index| {
                        behavior_blending_transition_effect(
                            file,
                            index,
                            variable_names,
                            variable_types,
                            variable_initial_words,
                        )
                    },
                ),
                condition: pointer_members(members, "condition")
                    .and_then(|index| behavior_object_evidence(file, index)),
                event_id,
                event_name: event_id.and_then(|id| behavior_event_name(event_names, id)),
                to_state_id: int_members(members, "toStateId"),
                from_nested_state_id: int_members(members, "fromNestedStateId"),
                to_nested_state_id: int_members(members, "toNestedStateId"),
                priority: int_members(members, "priority"),
                flags: int_members(members, "flags"),
            })
        })
        .collect();
    Some(SkyrimBehaviorTransitionArrayEvidence {
        source: behavior_object_evidence(file, object_index)?,
        transitions,
        has_eventless_transitions: bool_member(object, "hasEventlessTransitions"),
        has_time_bounded_transitions: bool_member(object, "hasTimeBoundedTransitions"),
    })
}

fn behavior_transition_interval(
    members: &[HkxMember],
    event_names: &[String],
) -> SkyrimBehaviorTransitionIntervalEvidence {
    let enter_event_id = int_members(members, "enterEventId");
    let exit_event_id = int_members(members, "exitEventId");
    SkyrimBehaviorTransitionIntervalEvidence {
        enter_event_id,
        enter_event_name: enter_event_id.and_then(|id| behavior_event_name(event_names, id)),
        exit_event_id,
        exit_event_name: exit_event_id.and_then(|id| behavior_event_name(event_names, id)),
        enter_time_bits: float_members(members, "enterTime").map(f32::to_bits),
        exit_time_bits: float_members(members, "exitTime").map(f32::to_bits),
    }
}

fn behavior_event_property(
    file: &HkxFile,
    members: &[HkxMember],
    event_names: &[String],
) -> SkyrimBehaviorEventPropertyEvidence {
    let event_id = int_members(members, "id");
    SkyrimBehaviorEventPropertyEvidence {
        event_id,
        event_name: event_id.and_then(|id| behavior_event_name(event_names, id)),
        payload: pointer_members(members, "payload")
            .and_then(|index| behavior_object_evidence(file, index)),
    }
}

fn behavior_event_property_array(
    file: &HkxFile,
    object: &HkxObject,
    member_name: &str,
    event_names: &[String],
) -> Vec<SkyrimBehaviorEventPropertyEvidence> {
    let Some(member) = object
        .members
        .iter()
        .find(|member| member.name == member_name)
    else {
        return Vec::new();
    };
    let values = match &member.value {
        HkxValue::Array(values) => Some(values.as_slice()),
        HkxValue::Pointer(Some(index)) => file
            .objects()
            .get(*index)
            .and_then(|array| array_member(array, "events")),
        _ => None,
    };
    values
        .into_iter()
        .flatten()
        .filter_map(HkxValue::as_object_members)
        .map(|members| behavior_event_property(file, members, event_names))
        .collect()
}

fn behavior_event_name(event_names: &[String], event_id: i32) -> Option<String> {
    usize::try_from(event_id)
        .ok()
        .and_then(|index| event_names.get(index))
        .cloned()
}

fn behavior_group_children(
    file: &HkxFile,
    group: &HkxObject,
    member_name: &str,
    blender_children: bool,
    event_names: &[String],
    variable_names: &[String],
    variable_types: &[Option<i32>],
    variable_initial_words: &[Option<u32>],
) -> Vec<SkyrimBehaviorGroupChildEvidence> {
    array_member(group, member_name)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let mut pointers = Vec::new();
            collect_pointers(value, &mut pointers);
            pointers.into_iter().next()
        })
        .enumerate()
        .filter_map(|(ordinal, child_index)| {
            let child = file.objects().get(child_index)?;
            let generator_index = if blender_children {
                pointer_member(child, "generator")?
            } else {
                child_index
            };
            let generator = behavior_object_evidence(file, generator_index)?;
            let generator_tree = behavior_generator_tree(
                file,
                generator_index,
                event_names,
                variable_names,
                variable_types,
                variable_initial_words,
                &mut BTreeSet::new(),
            );
            let mut reached = BTreeMap::new();
            collect_terminal_generators(
                file,
                generator_index,
                0,
                &mut HashMap::new(),
                &BTreeSet::new(),
                "hkbClipGenerator",
                &mut reached,
            );
            let terminal_clips = reached
                .into_iter()
                .filter_map(|(object_index, reached)| {
                    let object = file.objects().get(object_index)?;
                    Some(SkyrimBehaviorTerminalClipEvidence {
                        object_index,
                        animation_name: string_member(object, "animationName")
                            .filter(|name| !name.is_empty())?,
                        resolved_clip_path: None,
                        generator_name: string_member(object, "name")
                            .filter(|name| !name.is_empty()),
                        topology_names: reached.topology_names.into_iter().collect(),
                    })
                })
                .collect();
            Some(SkyrimBehaviorGroupChildEvidence {
                ordinal,
                generator_tree,
                generator,
                terminal_clips,
                weight_bits: blender_children
                    .then(|| float_member(child, "weight").map(f32::to_bits))
                    .flatten(),
                world_from_model_weight_bits: blender_children
                    .then(|| float_member(child, "worldFromModelWeight").map(f32::to_bits))
                    .flatten(),
                bone_weights: blender_children
                    .then(|| pointer_member(child, "boneWeights"))
                    .flatten()
                    .and_then(|index| behavior_object_evidence(file, index)),
            })
        })
        .collect()
}

fn resolve_behavior_group_clip_paths(
    groups: &mut [SkyrimBehaviorGeneratorGroupEvidence],
    actors: &Path,
    family_root: &Path,
    behavior_path: &Path,
) {
    for group in groups {
        match group {
            SkyrimBehaviorGeneratorGroupEvidence::ManualSelector(selector) => {
                for child in &mut selector.children {
                    resolve_behavior_group_child_clip_paths(
                        child,
                        actors,
                        family_root,
                        behavior_path,
                    );
                }
            }
            SkyrimBehaviorGeneratorGroupEvidence::Blender(blender) => {
                for child in &mut blender.children {
                    resolve_behavior_group_child_clip_paths(
                        child,
                        actors,
                        family_root,
                        behavior_path,
                    );
                }
            }
            SkyrimBehaviorGeneratorGroupEvidence::StateMachine(state_machine) => {
                for generator in state_machine
                    .machine
                    .states
                    .iter_mut()
                    .filter_map(|state| state.generator.as_mut())
                {
                    resolve_behavior_generator_tree_clip_paths(
                        generator,
                        actors,
                        family_root,
                        behavior_path,
                    );
                }
            }
        }
    }
}

fn resolve_behavior_group_child_clip_paths(
    child: &mut SkyrimBehaviorGroupChildEvidence,
    actors: &Path,
    family_root: &Path,
    behavior_path: &Path,
) {
    resolve_behavior_generator_tree_clip_paths(
        &mut child.generator_tree,
        actors,
        family_root,
        behavior_path,
    );
    for terminal in &mut child.terminal_clips {
        terminal.resolved_clip_path =
            resolve_reference(actors, family_root, behavior_path, &terminal.animation_name)
                .map(|path| runtime_asset_path(actors, &path));
    }
}

fn resolve_behavior_generator_tree_clip_paths(
    node: &mut SkyrimBehaviorGeneratorNodeEvidence,
    actors: &Path,
    family_root: &Path,
    behavior_path: &Path,
) {
    match node {
        SkyrimBehaviorGeneratorNodeEvidence::Clip {
            animation_name,
            resolved_clip_path,
            ..
        } => {
            *resolved_clip_path =
                resolve_reference(actors, family_root, behavior_path, animation_name)
                    .map(|path| runtime_asset_path(actors, &path));
        }
        SkyrimBehaviorGeneratorNodeEvidence::ManualSelector { children, .. } => {
            for child in children {
                resolve_behavior_generator_tree_clip_paths(
                    child,
                    actors,
                    family_root,
                    behavior_path,
                );
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::Blender { children, .. } => {
            for child in children {
                resolve_behavior_generator_tree_clip_paths(
                    &mut child.generator,
                    actors,
                    family_root,
                    behavior_path,
                );
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine) => {
            for generator in machine
                .states
                .iter_mut()
                .filter_map(|state| state.generator.as_mut())
            {
                resolve_behavior_generator_tree_clip_paths(
                    generator,
                    actors,
                    family_root,
                    behavior_path,
                );
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::ModifierGenerator { generator, .. } => {
            if let Some(generator) = generator {
                resolve_behavior_generator_tree_clip_paths(
                    generator,
                    actors,
                    family_root,
                    behavior_path,
                );
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::Unsupported { child_members, .. } => {
            for child in child_members
                .iter_mut()
                .flat_map(|member| member.children.iter_mut())
            {
                resolve_behavior_generator_tree_clip_paths(
                    child,
                    actors,
                    family_root,
                    behavior_path,
                );
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::Missing { .. }
        | SkyrimBehaviorGeneratorNodeEvidence::Cycle { .. } => {}
    }
}

fn collect_initial_state_chain(
    file: &HkxFile,
    index: usize,
    visited: &mut BTreeSet<usize>,
    initial_states: &mut BTreeSet<usize>,
) {
    if !visited.insert(index) {
        return;
    }
    let Some(object) = file.objects().get(index) else {
        return;
    };
    if object.class_name == "hkbStateMachine" {
        if pointer_member(object, "startStateIdSelector").is_some() {
            return;
        }
        let Some(start_state_id) = int_member(object, "startStateId") else {
            return;
        };
        let mut owned_states = Vec::new();
        if let Some(values) = array_member(object, "states") {
            for value in values {
                collect_pointers(value, &mut owned_states);
            }
        }
        let selected = owned_states
            .into_iter()
            .filter(|state_index| {
                file.objects().get(*state_index).is_some_and(|state| {
                    state.class_name == "hkbStateMachineStateInfo"
                        && int_member(state, "stateId") == Some(start_state_id)
                })
            })
            .collect::<Vec<_>>();
        let [selected] = selected.as_slice() else {
            return;
        };
        initial_states.insert(*selected);
        if let Some(generator) = pointer_member(&file.objects()[*selected], "generator") {
            collect_initial_state_chain(file, generator, visited, initial_states);
        }
        return;
    }
    let mut children = Vec::new();
    for member in &object.members {
        if generator_child_member(&object.class_name, &member.name) {
            collect_pointers(&member.value, &mut children);
        }
    }
    children.sort_unstable();
    children.dedup();
    if let [child] = children.as_slice() {
        collect_initial_state_chain(file, *child, visited, initial_states);
    }
}

#[derive(Default)]
struct NearestBehaviorState {
    distance: Option<usize>,
    state_names: BTreeSet<String>,
    initial_state_names: BTreeSet<String>,
    topology_names: BTreeSet<String>,
    incoming_events: BTreeSet<String>,
}

impl NearestBehaviorState {
    fn merge(
        &mut self,
        distance: usize,
        state_name: &str,
        initial_state: bool,
        topology_names: BTreeSet<String>,
        incoming_events: Option<&BTreeSet<String>>,
    ) {
        if let Some(events) = incoming_events {
            self.incoming_events.extend(events.iter().cloned());
        }
        if self.distance.is_some_and(|current| distance > current) {
            return;
        }
        if self.distance.is_none_or(|current| distance < current) {
            self.distance = Some(distance);
            self.state_names.clear();
            self.initial_state_names.clear();
            self.topology_names.clear();
        }
        if !state_name.is_empty() {
            self.state_names.insert(state_name.to_string());
            if initial_state {
                self.initial_state_names.insert(state_name.to_string());
            }
        }
        self.topology_names.extend(topology_names);
    }
}

struct ReachedBehaviorClip {
    distance: usize,
    topology_names: BTreeSet<String>,
}

fn collect_incoming_transitions(
    file: &HkxFile,
    transitions: usize,
    event_names: &[String],
    incoming: &mut BTreeMap<i32, BTreeSet<String>>,
) {
    let Some(object) = file.objects().get(transitions) else {
        return;
    };
    let Some(values) = array_member(object, "transitions") else {
        return;
    };
    for value in values {
        let Some(members) = value.as_object_members() else {
            continue;
        };
        if int_members(members, "flags").is_some_and(|flags| flags & 32 != 0) {
            continue;
        }
        let Some(to_state) = int_members(members, "toStateId") else {
            continue;
        };
        let Some(event_id) = int_members(members, "eventId") else {
            continue;
        };
        if let Some(event) = event_names.get(event_id as usize) {
            incoming.entry(to_state).or_default().insert(event.clone());
        }
    }
}

fn collect_terminal_generators(
    file: &HkxFile,
    index: usize,
    distance: usize,
    visited: &mut HashMap<usize, usize>,
    ancestor_names: &BTreeSet<String>,
    terminal_class: &str,
    clips: &mut BTreeMap<usize, ReachedBehaviorClip>,
) {
    if visited
        .get(&index)
        .is_some_and(|current| *current <= distance)
    {
        return;
    }
    visited.insert(index, distance);
    let Some(object) = file.objects().get(index) else {
        return;
    };
    let mut topology_names = ancestor_names.clone();
    if object.class_name != terminal_class
        && let Some(name) = string_member(object, "name").filter(|name| !name.is_empty())
    {
        topology_names.insert(name);
    }
    if object.class_name == terminal_class {
        match clips.entry(index) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(ReachedBehaviorClip {
                    distance,
                    topology_names,
                });
            }
            std::collections::btree_map::Entry::Occupied(mut entry)
                if distance < entry.get().distance =>
            {
                entry.insert(ReachedBehaviorClip {
                    distance,
                    topology_names,
                });
            }
            std::collections::btree_map::Entry::Occupied(mut entry)
                if distance == entry.get().distance =>
            {
                entry.get_mut().topology_names.extend(topology_names);
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }
        return;
    }
    for member in &object.members {
        if !generator_child_member(&object.class_name, &member.name) {
            continue;
        }
        let mut pointers = Vec::new();
        collect_pointers(&member.value, &mut pointers);
        for child in pointers {
            collect_terminal_generators(
                file,
                child,
                distance + 1,
                visited,
                &topology_names,
                terminal_class,
                clips,
            );
        }
    }
}

fn generator_child_member(class_name: &str, member_name: &str) -> bool {
    matches!(
        (class_name, member_name),
        ("hkbStateMachine", "states")
            | ("hkbStateMachineStateInfo", "generator")
            | ("hkbModifierGenerator", "generator")
            | ("hkbManualSelectorGenerator", "generators")
            | ("hkbBlenderGenerator", "children")
            | ("hkbBlenderGeneratorChild", "generator")
            | ("BSiStateTaggingGenerator", "pDefaultGenerator")
            | ("BSCyclicBlendTransitionGenerator", "pBlenderGenerator")
            | ("BSOffsetAnimationGenerator", "pDefaultGenerator")
            | ("BSSynchronizedClipGenerator", "pClipGenerator")
    )
}

fn collect_pointers(value: &HkxValue, pointers: &mut Vec<usize>) {
    match value {
        HkxValue::Pointer(Some(index)) => pointers.push(*index),
        HkxValue::Array(values) => {
            for value in values {
                collect_pointers(value, pointers);
            }
        }
        HkxValue::Object(members) | HkxValue::TypedObject { members, .. } => {
            for member in members {
                collect_pointers(&member.value, pointers);
            }
        }
        _ => {}
    }
}

fn clip_trigger_events(file: &HkxFile, clip: &HkxObject, event_names: &[String]) -> Vec<String> {
    let Some(trigger_index) = pointer_member(clip, "triggers") else {
        return Vec::new();
    };
    let Some(trigger_array) = file.objects().get(trigger_index) else {
        return Vec::new();
    };
    let Some(triggers) = array_member(trigger_array, "triggers") else {
        return Vec::new();
    };
    let mut events = BTreeSet::new();
    for trigger in triggers {
        let Some(members) = trigger.as_object_members() else {
            continue;
        };
        let event_id = members
            .iter()
            .find(|member| member.name == "event")
            .and_then(|member| member.value.as_object_members())
            .and_then(|members| int_members(members, "id"));
        if let Some(event) = event_id.and_then(|id| event_names.get(id as usize)) {
            events.insert(event.clone());
        }
    }
    events.into_iter().collect()
}

fn extract_clip_evidence(file: &HkxFile, clip_path: &str) -> SkyrimCreatureClipEvidence {
    let binding = file
        .objects()
        .iter()
        .find(|object| object.class_name == "hkaAnimationBinding");
    let original_skeleton_name =
        binding.and_then(|object| string_member(object, "originalSkeletonName"));
    let blend_hint = binding
        .and_then(|object| int_member(object, "blendHint"))
        .unwrap_or(0);
    let annotations = extract_annotations(file);
    let root_motion = extract_havok_root_motion(file);
    SkyrimCreatureClipEvidence {
        clip_path: canonical_runtime_path(clip_path),
        original_skeleton_name,
        blend_hint,
        behavior: Vec::new(),
        animation_data: Vec::new(),
        annotations,
        root_motion,
    }
}

fn extract_annotations(file: &HkxFile) -> Vec<SkyrimTimedEvent> {
    let mut events = Vec::new();
    for track in file
        .objects()
        .iter()
        .filter(|object| object.class_name == "hkaAnnotationTrack")
    {
        let Some(annotations) = array_member(track, "annotations") else {
            continue;
        };
        for annotation in annotations {
            let Some(members) = annotation.as_object_members() else {
                continue;
            };
            let Some(name) = string_members(members, "text") else {
                continue;
            };
            let Some(time) = float_members(members, "time") else {
                continue;
            };
            events.push(SkyrimTimedEvent { name, time });
        }
    }
    events.sort_by(|left, right| {
        left.time
            .total_cmp(&right.time)
            .then_with(|| left.name.cmp(&right.name))
    });
    events
}

fn extract_havok_root_motion(file: &HkxFile) -> SkyrimRootMotion {
    let frames = file
        .objects()
        .iter()
        .find(|object| object.class_name == "hkaDefaultAnimatedReferenceFrame");
    let Some(frames) = frames else {
        return SkyrimRootMotion::Unknown;
    };
    let samples = array_member(frames, "referenceFrameSamples")
        .map(|values| values.iter().filter_map(vector4).collect::<Vec<_>>())
        .unwrap_or_default();
    root_motion_from_samples(SkyrimRootMotionSource::HavokReferenceFrame, &samples, &[])
}

fn root_motion_from_samples(
    source: SkyrimRootMotionSource,
    translations: &[[f32; 4]],
    rotations: &[[f32; 4]],
) -> SkyrimRootMotion {
    if translations.is_empty() && rotations.is_empty() {
        return SkyrimRootMotion::Unknown;
    }
    let translation_delta = translations
        .first()
        .zip(translations.last())
        .map(|(first, last)| [last[0] - first[0], last[1] - first[1], last[2] - first[2]])
        .unwrap_or([0.0; 3]);
    let moved = translation_delta
        .iter()
        .any(|component| component.abs() > 1.0e-5);
    let rotated = rotations
        .first()
        .zip(rotations.last())
        .is_some_and(|(first, last)| {
            first
                .iter()
                .zip(last)
                .any(|(left, right)| (left - right).abs() > 1.0e-5)
        });
    let sample_count = translations.len().max(rotations.len());
    if !moved && !rotated {
        SkyrimRootMotion::Stationary {
            source,
            sample_count,
        }
    } else {
        SkyrimRootMotion::Sampled {
            source,
            sample_count,
            translation_delta,
            rotation_start: rotations.first().copied(),
            rotation_end: rotations.last().copied(),
        }
    }
}

#[derive(Default)]
struct ProjectAnimationData {
    entries: BTreeMap<String, Vec<SkyrimAnimationDataEvidence>>,
}

fn load_animation_data_catalog(
    root: &Path,
) -> Result<BTreeMap<String, ProjectAnimationData>, SkyrimCreatureMotionError> {
    let single_file = root
        .parent()
        .unwrap_or(root)
        .join("animationdatasinglefile.txt");
    if single_file.exists() {
        return parse_animation_data_single_file(&single_file);
    }
    let mut catalog = BTreeMap::new();
    for (_, project_relative) in RACE_PROJECTS {
        let project_stem = Path::new(project_relative)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        catalog.insert(
            project_stem.clone(),
            load_split_animation_data(root, &project_stem)?,
        );
    }
    Ok(catalog)
}

fn parse_animation_data_single_file(
    path: &Path,
) -> Result<BTreeMap<String, ProjectAnimationData>, SkyrimCreatureMotionError> {
    let text =
        std::fs::read_to_string(path).map_err(|error| SkyrimCreatureMotionError::ReadAsset {
            path: path.display().to_string(),
            detail: error.to_string(),
        })?;
    let lines = text.lines().map(str::trim).collect::<Vec<_>>();
    let project_count = lines
        .first()
        .ok_or_else(|| invalid_animation_data(path, "project count is missing"))
        .and_then(|value| parse_usize(path, value, "project count"))?;
    if lines.len() < project_count + 1 {
        return Err(invalid_animation_data(
            path,
            "project-name table is truncated",
        ));
    }
    let project_names = lines[1..=project_count]
        .iter()
        .map(|name| {
            Path::new(name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase()
        })
        .collect::<Vec<_>>();
    let mut cursor = project_count + 1;
    let mut catalog = BTreeMap::new();
    for project_name in project_names {
        let animation_line_count = lines
            .get(cursor)
            .ok_or_else(|| invalid_animation_data(path, "animation line count is missing"))
            .and_then(|value| parse_usize(path, value, "animation line count"))?;
        cursor += 1;
        if cursor + animation_line_count > lines.len() {
            return Err(invalid_animation_data(
                path,
                format!("{project_name} AnimationData payload is truncated"),
            ));
        }
        let animation_lines = &lines[cursor..cursor + animation_line_count];
        cursor += animation_line_count;
        if animation_lines.is_empty() {
            catalog.insert(project_name, ProjectAnimationData::default());
            continue;
        }
        if animation_lines.len() < 3 {
            return Err(invalid_animation_data(
                path,
                format!("{project_name} AnimationData header is truncated"),
            ));
        }
        let asset_count = parse_usize(path, animation_lines[1], "asset reference count")?;
        let has_motion_index = 2usize
            .checked_add(asset_count)
            .ok_or_else(|| invalid_animation_data(path, "asset count overflow"))?;
        let has_motion = animation_lines
            .get(has_motion_index)
            .is_some_and(|value| *value == "1");
        let bound = if has_motion {
            let motion_line_count = lines
                .get(cursor)
                .ok_or_else(|| invalid_animation_data(path, "motion line count is missing"))
                .and_then(|value| parse_usize(path, value, "motion line count"))?;
            cursor += 1;
            if cursor + motion_line_count > lines.len() {
                return Err(invalid_animation_data(
                    path,
                    format!("{project_name} BoundAnims payload is truncated"),
                ));
            }
            let motion = parse_bound_anims_lines(path, &lines[cursor..cursor + motion_line_count])?;
            cursor += motion_line_count;
            motion
        } else {
            BTreeMap::new()
        };
        let animation_text = animation_lines.join("\n");
        let project = parse_project_animation_data(
            path,
            &animation_text,
            &bound,
            has_motion.then_some(path),
            &project_name,
        )?;
        catalog.insert(project_name, project);
    }
    Ok(catalog)
}

fn load_split_animation_data(
    root: &Path,
    project_stem: &str,
) -> Result<ProjectAnimationData, SkyrimCreatureMotionError> {
    let project_path = root.join(format!("{project_stem}.txt"));
    if !project_path.exists() {
        return Ok(ProjectAnimationData::default());
    }
    let text = std::fs::read_to_string(&project_path).map_err(|error| {
        SkyrimCreatureMotionError::ReadAsset {
            path: project_path.display().to_string(),
            detail: error.to_string(),
        }
    })?;
    let bound_path = root
        .join("boundanims")
        .join(format!("anims_{project_stem}.txt"));
    let bound = if bound_path.exists() {
        parse_bound_anims(&bound_path)?
    } else {
        BTreeMap::new()
    };
    parse_project_animation_data(
        &project_path,
        &text,
        &bound,
        bound_path.exists().then_some(bound_path.as_path()),
        project_stem,
    )
}

fn parse_project_animation_data(
    path: &Path,
    text: &str,
    bound: &BTreeMap<u32, SkyrimRootMotion>,
    root_motion_source_path: Option<&Path>,
    project_stem: &str,
) -> Result<ProjectAnimationData, SkyrimCreatureMotionError> {
    let lines = text.lines().map(str::trim).collect::<Vec<_>>();
    if lines.len() < 3 {
        return Err(invalid_animation_data(path, "header is truncated"));
    }
    let reference_count = parse_usize(path, lines[1], "asset reference count")?;
    let mut cursor = 2usize
        .checked_add(reference_count)
        .and_then(|cursor| cursor.checked_add(1))
        .ok_or_else(|| invalid_animation_data(path, "header count overflow"))?;
    if cursor > lines.len() {
        return Err(invalid_animation_data(
            path,
            "asset reference list is truncated",
        ));
    }
    let mut entries = BTreeMap::new();
    while cursor < lines.len() {
        while cursor < lines.len() && lines[cursor].is_empty() {
            cursor += 1;
        }
        if cursor >= lines.len() {
            break;
        }
        let sequence_name = lines[cursor].to_string();
        cursor += 1;
        if cursor + 4 >= lines.len() {
            return Err(invalid_animation_data(
                path,
                format!("sequence {sequence_name} header is truncated"),
            ));
        }
        let animation_index = parse_u32(path, lines[cursor], "animation index")?;
        let playback_speed = parse_f32(path, lines[cursor + 1], "playback speed")?;
        let crop_start_local_time = parse_f32(path, lines[cursor + 2], "crop start")?;
        let crop_end_local_time = parse_f32(path, lines[cursor + 3], "crop end")?;
        cursor += 4;
        let event_count = parse_usize(path, lines[cursor], "event count")?;
        cursor += 1;
        if cursor + event_count > lines.len() {
            return Err(invalid_animation_data(
                path,
                format!("sequence {sequence_name} event list is truncated"),
            ));
        }
        let mut events = Vec::with_capacity(event_count);
        for event_line in &lines[cursor..cursor + event_count] {
            let Some((name, time)) = event_line.rsplit_once(':') else {
                return Err(invalid_animation_data(
                    path,
                    format!("invalid event row {event_line:?}"),
                ));
            };
            let time = time.parse::<f32>().map_err(|error| {
                invalid_animation_data(path, format!("invalid event time {time:?}: {error}"))
            })?;
            events.push(SkyrimTimedEvent {
                name: name.to_string(),
                time,
            });
        }
        cursor += event_count;
        entries
            .entry(normalized_words(&sequence_name))
            .or_insert_with(Vec::new)
            .push(SkyrimAnimationDataEvidence {
                source_path: canonical_runtime_path(&path.display().to_string()),
                root_motion_source_path: root_motion_source_path
                    .map(|source| canonical_runtime_path(&source.display().to_string())),
                project_stem: project_stem.to_string(),
                sequence_name,
                animation_index,
                playback_speed,
                crop_start_local_time,
                crop_end_local_time,
                events,
                root_motion: bound
                    .get(&animation_index)
                    .cloned()
                    .unwrap_or(SkyrimRootMotion::Unknown),
            });
    }
    Ok(ProjectAnimationData { entries })
}

fn parse_bound_anims(
    path: &Path,
) -> Result<BTreeMap<u32, SkyrimRootMotion>, SkyrimCreatureMotionError> {
    let text =
        std::fs::read_to_string(path).map_err(|error| SkyrimCreatureMotionError::ReadAsset {
            path: path.display().to_string(),
            detail: error.to_string(),
        })?;
    let lines = text.lines().map(str::trim).collect::<Vec<_>>();
    parse_bound_anims_lines(path, &lines)
}

pub(crate) fn load_skyrim_bound_motion_samples(
    source_path: &Path,
    project_path: &str,
    animation_index: u32,
) -> Result<SkyrimBoundMotionSamples, SkyrimCreatureMotionError> {
    let project_stem = Path::new(project_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let samples = if file_name.eq_ignore_ascii_case("animationdatasinglefile.txt") {
        load_single_file_bound_motion_samples(source_path, &project_stem)?
    } else {
        let text = std::fs::read_to_string(source_path).map_err(|error| {
            SkyrimCreatureMotionError::ReadAsset {
                path: source_path.display().to_string(),
                detail: error.to_string(),
            }
        })?;
        let lines = text.lines().map(str::trim).collect::<Vec<_>>();
        parse_bound_motion_samples_lines(source_path, &lines)?
    };
    samples.get(&animation_index).cloned().ok_or_else(|| {
        invalid_animation_data(
            source_path,
            format!("project {project_stem} has no BoundAnims entry {animation_index}"),
        )
    })
}

fn load_single_file_bound_motion_samples(
    path: &Path,
    requested_project: &str,
) -> Result<BTreeMap<u32, SkyrimBoundMotionSamples>, SkyrimCreatureMotionError> {
    let text =
        std::fs::read_to_string(path).map_err(|error| SkyrimCreatureMotionError::ReadAsset {
            path: path.display().to_string(),
            detail: error.to_string(),
        })?;
    let lines = text.lines().map(str::trim).collect::<Vec<_>>();
    let project_count = lines
        .first()
        .ok_or_else(|| invalid_animation_data(path, "project count is missing"))
        .and_then(|value| parse_usize(path, value, "project count"))?;
    if lines.len() < project_count + 1 {
        return Err(invalid_animation_data(
            path,
            "project-name table is truncated",
        ));
    }
    let project_names = lines[1..=project_count]
        .iter()
        .map(|name| {
            Path::new(name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase()
        })
        .collect::<Vec<_>>();
    let mut cursor = project_count + 1;
    for project_name in project_names {
        let animation_line_count = lines
            .get(cursor)
            .ok_or_else(|| invalid_animation_data(path, "animation line count is missing"))
            .and_then(|value| parse_usize(path, value, "animation line count"))?;
        cursor += 1;
        if cursor + animation_line_count > lines.len() {
            return Err(invalid_animation_data(
                path,
                format!("{project_name} AnimationData payload is truncated"),
            ));
        }
        let animation_lines = &lines[cursor..cursor + animation_line_count];
        cursor += animation_line_count;
        if animation_lines.is_empty() {
            if project_name == requested_project {
                return Ok(BTreeMap::new());
            }
            continue;
        }
        if animation_lines.len() < 3 {
            return Err(invalid_animation_data(
                path,
                format!("{project_name} AnimationData header is truncated"),
            ));
        }
        let asset_count = parse_usize(path, animation_lines[1], "asset reference count")?;
        let has_motion_index = 2usize
            .checked_add(asset_count)
            .ok_or_else(|| invalid_animation_data(path, "asset count overflow"))?;
        let has_motion = animation_lines
            .get(has_motion_index)
            .is_some_and(|value| *value == "1");
        if !has_motion {
            if project_name == requested_project {
                return Ok(BTreeMap::new());
            }
            continue;
        }
        let motion_line_count = lines
            .get(cursor)
            .ok_or_else(|| invalid_animation_data(path, "motion line count is missing"))
            .and_then(|value| parse_usize(path, value, "motion line count"))?;
        cursor += 1;
        if cursor + motion_line_count > lines.len() {
            return Err(invalid_animation_data(
                path,
                format!("{project_name} BoundAnims payload is truncated"),
            ));
        }
        let motion_lines = &lines[cursor..cursor + motion_line_count];
        cursor += motion_line_count;
        if project_name == requested_project {
            return parse_bound_motion_samples_lines(path, motion_lines);
        }
    }
    Err(invalid_animation_data(
        path,
        format!("project {requested_project} is absent"),
    ))
}

fn parse_bound_anims_lines(
    path: &Path,
    lines: &[&str],
) -> Result<BTreeMap<u32, SkyrimRootMotion>, SkyrimCreatureMotionError> {
    Ok(parse_bound_motion_samples_lines(path, lines)?
        .into_iter()
        .map(|(index, samples)| {
            let translations = samples
                .translations
                .iter()
                .map(|sample| sample.value)
                .collect::<Vec<_>>();
            let rotations = samples
                .rotations
                .iter()
                .map(|sample| sample.value)
                .collect::<Vec<_>>();
            (
                index,
                root_motion_from_samples(
                    SkyrimRootMotionSource::BoundAnims,
                    &translations,
                    &rotations,
                ),
            )
        })
        .collect())
}

fn parse_bound_motion_samples_lines(
    path: &Path,
    lines: &[&str],
) -> Result<BTreeMap<u32, SkyrimBoundMotionSamples>, SkyrimCreatureMotionError> {
    let lines = lines
        .iter()
        .copied()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let mut cursor = 0usize;
    let mut entries = BTreeMap::new();
    while cursor < lines.len() {
        let index = parse_u32(path, lines[cursor], "bound animation index")?;
        cursor += 1;
        if cursor >= lines.len() {
            return Err(invalid_animation_data(path, "bound duration is missing"));
        }
        let duration = lines[cursor].parse::<f32>().map_err(|error| {
            invalid_animation_data(path, format!("invalid bound duration: {error}"))
        })?;
        cursor += 1;
        let translation_count = lines
            .get(cursor)
            .ok_or_else(|| invalid_animation_data(path, "translation count is missing"))
            .and_then(|value| parse_usize(path, value, "translation count"))?;
        cursor += 1;
        let translations = parse_bound_samples(path, &lines, &mut cursor, translation_count, 3)?;
        let rotation_count = lines
            .get(cursor)
            .ok_or_else(|| invalid_animation_data(path, "rotation count is missing"))
            .and_then(|value| parse_usize(path, value, "rotation count"))?;
        cursor += 1;
        let rotations = parse_bound_samples(path, &lines, &mut cursor, rotation_count, 4)?;
        entries.insert(
            index,
            SkyrimBoundMotionSamples {
                duration,
                translations,
                rotations,
            },
        );
    }
    Ok(entries)
}

fn parse_bound_samples(
    path: &Path,
    lines: &[&str],
    cursor: &mut usize,
    count: usize,
    component_count: usize,
) -> Result<Vec<SkyrimTimedMotionSample>, SkyrimCreatureMotionError> {
    if *cursor + count > lines.len() {
        return Err(invalid_animation_data(
            path,
            "bound sample list is truncated",
        ));
    }
    let mut samples = Vec::with_capacity(count);
    for line in &lines[*cursor..*cursor + count] {
        let values = line
            .split_whitespace()
            .map(|value| value.parse::<f32>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                invalid_animation_data(path, format!("invalid bound sample {line:?}: {error}"))
            })?;
        if values.len() != component_count + 1 {
            return Err(invalid_animation_data(
                path,
                format!("bound sample {line:?} must contain time plus {component_count} values"),
            ));
        }
        samples.push(SkyrimTimedMotionSample {
            time: values[0],
            value: [
                values[1],
                values[2],
                values[3],
                values.get(4).copied().unwrap_or(0.0),
            ],
        });
    }
    *cursor += count;
    Ok(samples)
}

fn animation_data_for_clip(
    clip: &SkyrimCreatureClipEvidence,
    data: &ProjectAnimationData,
) -> Vec<SkyrimAnimationDataEvidence> {
    let mut entries = Vec::new();
    let mut keys = BTreeSet::new();
    for reference in &clip.behavior {
        if let Some(name) = &reference.generator_name {
            keys.insert(normalized_words(name));
        }
        keys.extend(
            reference
                .state_names
                .iter()
                .map(|name| normalized_words(name)),
        );
    }
    keys.insert(normalized_words(
        Path::new(&clip.clip_path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default(),
    ));
    for key in keys {
        if let Some(matches) = data.entries.get(&key) {
            entries.extend(matches.iter().cloned());
        }
    }
    entries.sort_by(|left, right| {
        left.animation_index
            .cmp(&right.animation_index)
            .then_with(|| left.sequence_name.cmp(&right.sequence_name))
    });
    entries
}

fn invalid_animation_data(path: &Path, detail: impl Into<String>) -> SkyrimCreatureMotionError {
    SkyrimCreatureMotionError::InvalidAnimationData {
        path: path.display().to_string(),
        detail: detail.into(),
    }
}

fn parse_usize(path: &Path, value: &str, label: &str) -> Result<usize, SkyrimCreatureMotionError> {
    value.parse::<usize>().map_err(|error| {
        invalid_animation_data(path, format!("invalid {label} {value:?}: {error}"))
    })
}

fn parse_u32(path: &Path, value: &str, label: &str) -> Result<u32, SkyrimCreatureMotionError> {
    value.parse::<u32>().map_err(|error| {
        invalid_animation_data(path, format!("invalid {label} {value:?}: {error}"))
    })
}

fn parse_f32(path: &Path, value: &str, label: &str) -> Result<f32, SkyrimCreatureMotionError> {
    value.parse::<f32>().map_err(|error| {
        invalid_animation_data(path, format!("invalid {label} {value:?}: {error}"))
    })
}

fn ascii_hkx_references(bytes: &[u8]) -> Vec<String> {
    let mut references = Vec::new();
    let mut start = 0usize;
    while start < bytes.len() {
        while start < bytes.len() && !(0x20..=0x7e).contains(&bytes[start]) {
            start += 1;
        }
        let mut end = start;
        while end < bytes.len() && (0x20..=0x7e).contains(&bytes[end]) {
            end += 1;
        }
        if end > start {
            let value = String::from_utf8_lossy(&bytes[start..end]);
            if value.to_ascii_lowercase().ends_with(".hkx") {
                references.push(value.into_owned());
            }
        }
        start = end.saturating_add(1);
    }
    references
}

fn resolve_reference(
    actors: &Path,
    family_root: &Path,
    current: &Path,
    value: &str,
) -> Option<PathBuf> {
    let relative_text = value.replace('\\', "/");
    let relative = Path::new(relative_text.trim_start_matches('/'));
    let mut candidates = vec![family_root.join(relative)];
    let mut ancestor = current.parent();
    while let Some(parent) = ancestor {
        candidates.push(parent.join(relative));
        if parent == actors {
            break;
        }
        ancestor = parent.parent();
    }
    candidates.push(actors.join(relative));
    let actors_relative = relative.components().next().is_some_and(|component| {
        component
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("actors")
    });
    if actors_relative {
        if let Some(meshes) = actors.parent() {
            candidates.push(meshes.join(relative));
        }
    }
    candidates.into_iter().find_map(|candidate| {
        let canonical = candidate.canonicalize().ok()?;
        canonical.starts_with(actors).then_some(canonical)
    })
}

fn string_member(object: &HkxObject, name: &str) -> Option<String> {
    string_members(&object.members, name)
}

fn string_members(members: &[HkxMember], name: &str) -> Option<String> {
    members.iter().find_map(|member| {
        if member.name != name {
            return None;
        }
        match &member.value {
            HkxValue::String {
                value,
                is_null: false,
            } => Some(value.clone()),
            _ => None,
        }
    })
}

fn int_member(object: &HkxObject, name: &str) -> Option<i32> {
    int_members(&object.members, name)
}

fn int_members(members: &[HkxMember], name: &str) -> Option<i32> {
    members.iter().find_map(|member| {
        if member.name != name {
            return None;
        }
        match &member.value {
            HkxValue::I8(value) => Some(i32::from(*value)),
            HkxValue::U8(value) => Some(i32::from(*value)),
            HkxValue::I16(value) => Some(i32::from(*value)),
            HkxValue::U16(value) => Some(i32::from(*value)),
            HkxValue::I32(value) => Some(*value),
            HkxValue::U32(value) => i32::try_from(*value).ok(),
            _ => None,
        }
    })
}

fn uint64_member(object: &HkxObject, name: &str) -> Option<u64> {
    object.members.iter().find_map(|member| {
        if member.name != name {
            return None;
        }
        match member.value {
            HkxValue::I8(value) => u64::try_from(value).ok(),
            HkxValue::U8(value) => Some(u64::from(value)),
            HkxValue::I16(value) => u64::try_from(value).ok(),
            HkxValue::U16(value) => Some(u64::from(value)),
            HkxValue::I32(value) => u64::try_from(value).ok(),
            HkxValue::U32(value) => Some(u64::from(value)),
            HkxValue::I64(value) => u64::try_from(value).ok(),
            HkxValue::U64(value) => Some(value),
            _ => None,
        }
    })
}

fn bool_member(object: &HkxObject, name: &str) -> Option<bool> {
    object.members.iter().find_map(|member| {
        (member.name == name).then(|| match member.value {
            HkxValue::Bool(value) => Some(value),
            HkxValue::I8(value) => Some(value != 0),
            HkxValue::U8(value) => Some(value != 0),
            _ => None,
        })?
    })
}

fn uint_members(members: &[HkxMember], name: &str) -> Option<u32> {
    members.iter().find_map(|member| {
        if member.name != name {
            return None;
        }
        match member.value {
            HkxValue::I8(value) => u32::try_from(value).ok(),
            HkxValue::U8(value) => Some(u32::from(value)),
            HkxValue::I16(value) => u32::try_from(value).ok(),
            HkxValue::U16(value) => Some(u32::from(value)),
            HkxValue::I32(value) => u32::try_from(value).ok(),
            HkxValue::U32(value) => Some(value),
            _ => None,
        }
    })
}

fn float_members(members: &[HkxMember], name: &str) -> Option<f32> {
    members.iter().find_map(|member| {
        (member.name == name).then(|| match &member.value {
            HkxValue::F32(value) | HkxValue::Half(value) => Some(*value),
            _ => None,
        })?
    })
}

fn float_member(object: &HkxObject, name: &str) -> Option<f32> {
    float_members(&object.members, name)
}

fn member_object<'a>(members: &'a [HkxMember], name: &str) -> Option<&'a [HkxMember]> {
    members
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| member.value.as_object_members())
}

fn member_object_with_class<'a>(
    members: &'a [HkxMember],
    name: &str,
) -> Option<(Option<&'a str>, &'a [HkxMember])> {
    members.iter().find_map(|member| {
        if member.name != name {
            return None;
        }
        match &member.value {
            HkxValue::Object(members) => Some((None, members.as_slice())),
            HkxValue::TypedObject {
                class_name,
                members,
            } => Some((Some(class_name.as_str()), members.as_slice())),
            _ => None,
        }
    })
}

fn pointer_member(object: &HkxObject, name: &str) -> Option<usize> {
    pointer_members(&object.members, name)
}

fn pointer_members(members: &[HkxMember], name: &str) -> Option<usize> {
    members.iter().find_map(|member| {
        (member.name == name).then(|| match &member.value {
            HkxValue::Pointer(index) => *index,
            _ => None,
        })?
    })
}

fn array_member<'a>(object: &'a HkxObject, name: &str) -> Option<&'a [HkxValue]> {
    object.members.iter().find_map(|member| {
        if member.name != name {
            return None;
        }
        match &member.value {
            HkxValue::Array(values) => Some(values.as_slice()),
            _ => None,
        }
    })
}

fn string_array(object: &HkxObject, name: &str) -> Vec<String> {
    array_member(object, name)
        .into_iter()
        .flatten()
        .filter_map(|value| match value {
            HkxValue::String {
                value,
                is_null: false,
            } => Some(value.clone()),
            _ => None,
        })
        .collect()
}

fn vector4(value: &HkxValue) -> Option<[f32; 4]> {
    match value {
        HkxValue::F32List(values) if values.len() >= 4 => {
            Some([values[0], values[1], values[2], values[3]])
        }
        _ => None,
    }
}

fn vector4_member(object: &HkxObject, name: &str) -> Option<[f32; 4]> {
    object
        .members
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| vector4(&member.value))
}

fn canonical_runtime_path(path: &str) -> String {
    path.replace('/', "\\")
        .trim_start_matches(".\\")
        .to_string()
}

fn runtime_asset_path(actors: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(actors).unwrap_or(path);
    canonical_runtime_path(&format!("Actors\\{}", relative.display()))
}

fn sort_dedup_paths(paths: &mut Vec<String>) {
    paths.sort_by_key(|path| path_key(path));
    paths.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
}

fn path_key(path: &str) -> String {
    canonical_runtime_path(path).to_ascii_lowercase()
}

fn project_path_key(path: &str) -> String {
    let key = path_key(path);
    key.strip_prefix("actors\\").unwrap_or(&key).to_string()
}

fn clip_id(path: &str) -> String {
    let mut id = String::from("clip_");
    for character in path_key(path).chars() {
        if character.is_ascii_alphanumeric() {
            id.push(character);
        } else if !id.ends_with('_') {
            id.push('_');
        }
    }
    id.trim_end_matches('_').to_string()
}

fn identifier_fragment(value: &str) -> String {
    let value = normalized_words(value).replace(' ', "_");
    if value.is_empty() {
        "creature".to_string()
    } else {
        value
    }
}

fn normalized_words(value: &str) -> String {
    let mut words = String::with_capacity(value.len());
    let mut previous_separator = true;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if character.is_ascii_uppercase() && !previous_separator {
                words.push(' ');
            }
            words.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if !previous_separator {
            words.push(' ');
            previous_separator = true;
        }
    }
    words.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(
        path: &str,
        behavior_name: &str,
        incoming_event: Option<&str>,
        skeleton: &str,
    ) -> SkyrimCreatureClipEvidence {
        SkyrimCreatureClipEvidence {
            clip_path: path.to_string(),
            original_skeleton_name: Some(skeleton.to_string()),
            blend_hint: 0,
            behavior: vec![SkyrimBehaviorClipEvidence {
                behavior_path: "behaviors\\root.hkx".to_string(),
                animation_name: path.to_string(),
                generator_name: Some(behavior_name.to_string()),
                state_names: vec![behavior_name.to_string()],
                initial_state_names: Vec::new(),
                topology_names: Vec::new(),
                incoming_events: incoming_event.into_iter().map(str::to_string).collect(),
                ancestor_entry_events: Vec::new(),
                emitted_events: Vec::new(),
                looping: behavior_name.to_ascii_lowercase().contains("idle"),
            }],
            animation_data: Vec::new(),
            annotations: Vec::new(),
            root_motion: SkyrimRootMotion::Unknown,
        }
    }

    #[test]
    fn selector_and_blender_receipts_preserve_order_bindings_and_weights() {
        let member = |name: &str, value: HkxValue| HkxMember {
            name: name.to_string(),
            value,
        };
        let object = |class_name: &str, members: Vec<HkxMember>| HkxObject {
            name: None,
            offset: 0,
            signature: 0,
            class_name: class_name.to_string(),
            members,
        };
        let text = |value: &str| HkxValue::String {
            value: value.to_string(),
            is_null: false,
        };
        let binding = |member_path: &str, variable_index: i32| {
            HkxValue::Object(vec![
                member("memberPath", text(member_path)),
                member("variableIndex", HkxValue::I32(variable_index)),
                member("bitIndex", HkxValue::I32(-1)),
                member("bindingType", HkxValue::I32(0)),
            ])
        };
        let file = HkxFile::from_tagxml(
            8,
            SKYRIM_CONTENTS_VERSION,
            vec![
                object(
                    "hkbVariableBindingSet",
                    vec![member(
                        "bindings",
                        HkxValue::Array(vec![binding("selectedGeneratorIndex", 0)]),
                    )],
                ),
                object(
                    "hkbVariableBindingSet",
                    vec![member(
                        "bindings",
                        HkxValue::Array(vec![binding("blendParameter", 1)]),
                    )],
                ),
                object(
                    "hkbClipGenerator",
                    vec![
                        member("name", text("IdleState")),
                        member("animationName", text("Animations\\Idle.hkx")),
                    ],
                ),
                object(
                    "hkbClipGenerator",
                    vec![
                        member("name", text("RunState")),
                        member("animationName", text("Animations\\Run.hkx")),
                    ],
                ),
                object("hkbBlendingTransitionEffect", Vec::new()),
                object(
                    "hkbManualSelectorGenerator",
                    vec![
                        member("name", text("IdleSelector")),
                        member("variableBindingSet", HkxValue::Pointer(Some(0))),
                        member(
                            "generators",
                            HkxValue::Array(vec![
                                HkxValue::Pointer(Some(2)),
                                HkxValue::Pointer(Some(3)),
                            ]),
                        ),
                        member("selectedGeneratorIndex", HkxValue::I8(1)),
                        member("indexSelector", HkxValue::Pointer(None)),
                        member("selectedIndexCanChangeAfterActivate", HkxValue::Bool(true)),
                        member(
                            "generatorChangedTransitionEffect",
                            HkxValue::Pointer(Some(4)),
                        ),
                    ],
                ),
                object(
                    "hkbBlenderGeneratorChild",
                    vec![
                        member("generator", HkxValue::Pointer(Some(2))),
                        member("weight", HkxValue::F32(0.25)),
                        member("worldFromModelWeight", HkxValue::F32(0.5)),
                        member("boneWeights", HkxValue::Pointer(None)),
                    ],
                ),
                object(
                    "hkbBlenderGeneratorChild",
                    vec![
                        member("generator", HkxValue::Pointer(Some(3))),
                        member("weight", HkxValue::F32(0.75)),
                        member("worldFromModelWeight", HkxValue::F32(1.0)),
                        member("boneWeights", HkxValue::Pointer(None)),
                    ],
                ),
                object(
                    "hkbBlenderGenerator",
                    vec![
                        member("name", text("SpeedBlend")),
                        member("variableBindingSet", HkxValue::Pointer(Some(1))),
                        member("referencePoseWeightThreshold", HkxValue::F32(0.1)),
                        member("blendParameter", HkxValue::F32(0.0)),
                        member("minCyclicBlendParameter", HkxValue::F32(-1.0)),
                        member("maxCyclicBlendParameter", HkxValue::F32(1.0)),
                        member("indexOfSyncMasterChild", HkxValue::I16(1)),
                        member("flags", HkxValue::I16(3)),
                        member("subtractLastChild", HkxValue::Bool(false)),
                        member(
                            "children",
                            HkxValue::Array(vec![
                                HkxValue::Pointer(Some(6)),
                                HkxValue::Pointer(Some(7)),
                            ]),
                        ),
                    ],
                ),
                object(
                    "hkbVariableValueSet",
                    vec![member(
                        "wordVariableValues",
                        HkxValue::Array(vec![
                            HkxValue::Object(vec![member("value", HkxValue::I32(1))]),
                            HkxValue::Object(vec![member(
                                "value",
                                HkxValue::I32(0.5_f32.to_bits() as i32),
                            )]),
                        ]),
                    )],
                ),
                object(
                    "hkbBehaviorGraphData",
                    vec![
                        member(
                            "variableInfos",
                            HkxValue::Array(vec![
                                HkxValue::Object(vec![member("type", HkxValue::I32(3))]),
                                HkxValue::Object(vec![member("type", HkxValue::I32(4))]),
                            ]),
                        ),
                        member("variableInitialValues", HkxValue::Pointer(Some(9))),
                    ],
                ),
            ],
        );

        let (variable_types, variable_initial_words) = behavior_variable_data(&file);
        let groups = extract_behavior_generator_groups(
            &file,
            "Actors\\Fixture\\Behavior.hkx",
            &[],
            &["SelectorIndex".to_string(), "Speed".to_string()],
            &variable_types,
            &variable_initial_words,
        );
        let [
            SkyrimBehaviorGeneratorGroupEvidence::ManualSelector(selector),
            SkyrimBehaviorGeneratorGroupEvidence::Blender(blender),
        ] = groups.as_slice()
        else {
            panic!("expected one selector followed by one blender: {groups:#?}");
        };
        assert_eq!(selector.selected_generator_index, 1);
        assert!(selector.selected_index_can_change_after_activate);
        assert_eq!(
            selector.variable_bindings[0].variable_name.as_deref(),
            Some("SelectorIndex")
        );
        assert_eq!(selector.variable_bindings[0].variable_type, Some(3));
        assert_eq!(selector.variable_bindings[0].initial_word_value, Some(1));
        assert_eq!(
            selector.children[0].terminal_clips[0].animation_name,
            "Animations\\Idle.hkx"
        );
        assert_eq!(
            selector.children[1].terminal_clips[0].animation_name,
            "Animations\\Run.hkx"
        );
        assert!(matches!(
            &selector.children[0].generator_tree,
            SkyrimBehaviorGeneratorNodeEvidence::Clip { animation_name, .. }
                if animation_name == "Animations\\Idle.hkx"
        ));
        assert_eq!(
            selector
                .transition_effect
                .as_ref()
                .map(|effect| effect.class_name.as_str()),
            Some("hkbBlendingTransitionEffect")
        );
        assert_eq!(blender.index_of_sync_master_child, 1);
        assert_eq!(blender.flags, 3);
        assert_eq!(
            blender.variable_bindings[0].variable_name.as_deref(),
            Some("Speed")
        );
        assert_eq!(blender.variable_bindings[0].variable_type, Some(4));
        assert_eq!(
            blender.variable_bindings[0].initial_word_value,
            Some(0.5_f32.to_bits())
        );
        assert_eq!(blender.children[0].weight_bits, Some(0.25_f32.to_bits()));
        assert_eq!(
            blender.children[1].world_from_model_weight_bits,
            Some(1.0_f32.to_bits())
        );
        assert!(matches!(
            &blender.children[1].generator_tree,
            SkyrimBehaviorGeneratorNodeEvidence::Clip { animation_name, .. }
                if animation_name == "Animations\\Run.hkx"
        ));
    }

    #[test]
    fn recursive_state_machine_receipt_preserves_state_and_transition_fields() {
        let member = |name: &str, value: HkxValue| HkxMember {
            name: name.to_string(),
            value,
        };
        let object = |class_name: &str, members: Vec<HkxMember>| HkxObject {
            name: None,
            offset: 0,
            signature: 0,
            class_name: class_name.to_string(),
            members,
        };
        let text = |value: &str| HkxValue::String {
            value: value.to_string(),
            is_null: false,
        };
        let event_property = |event_id| {
            HkxValue::Object(vec![
                member("id", HkxValue::I32(event_id)),
                member("payload", HkxValue::Pointer(None)),
            ])
        };
        let interval = HkxValue::Object(vec![
            member("enterEventId", HkxValue::I32(1)),
            member("exitEventId", HkxValue::I32(-1)),
            member("enterTime", HkxValue::F32(0.25)),
            member("exitTime", HkxValue::F32(0.75)),
        ]);
        let transition = HkxValue::Object(vec![
            member("triggerInterval", interval.clone()),
            member("initiateInterval", interval),
            member("transition", HkxValue::Pointer(Some(5))),
            member("condition", HkxValue::Pointer(None)),
            member("eventId", HkxValue::I32(0)),
            member("toStateId", HkxValue::I32(7)),
            member("fromNestedStateId", HkxValue::I32(2)),
            member("toNestedStateId", HkxValue::I32(3)),
            member("priority", HkxValue::I16(4)),
            member("flags", HkxValue::I16(5)),
        ]);
        let file = HkxFile::from_tagxml(
            8,
            SKYRIM_CONTENTS_VERSION,
            vec![
                object(
                    "hkbClipGenerator",
                    vec![
                        member("name", text("ExactIdle")),
                        member("animationName", text("Animations\\Idle.hkx")),
                    ],
                ),
                object(
                    "hkbStateMachineTransitionInfoArray",
                    vec![
                        member("transitions", HkxValue::Array(vec![transition])),
                        member("hasEventlessTransitions", HkxValue::Bool(false)),
                        member("hasTimeBoundedTransitions", HkxValue::Bool(true)),
                    ],
                ),
                object(
                    "hkbStateMachineStateInfo",
                    vec![
                        member("listeners", HkxValue::Array(Vec::new())),
                        member(
                            "enterNotifyEvents",
                            HkxValue::Array(vec![event_property(1)]),
                        ),
                        member("exitNotifyEvents", HkxValue::Array(Vec::new())),
                        member("transitions", HkxValue::Pointer(Some(1))),
                        member("generator", HkxValue::Pointer(Some(0))),
                        member("name", text("IdleState")),
                        member("stateId", HkxValue::I32(7)),
                        member("probability", HkxValue::F32(0.5)),
                        member("enable", HkxValue::Bool(true)),
                        member("hasEventlessTransitions", HkxValue::Bool(false)),
                    ],
                ),
                object(
                    "hkbStateMachine",
                    vec![
                        member("name", text("IdleMachine")),
                        member("eventToSendWhenStateOrTransitionChanges", event_property(0)),
                        member("startStateIdSelector", HkxValue::Pointer(None)),
                        member("startStateId", HkxValue::I32(7)),
                        member("returnToPreviousStateEventId", HkxValue::I32(-1)),
                        member("randomTransitionEventId", HkxValue::I32(-1)),
                        member("transitionToNextHigherStateEventId", HkxValue::I32(-1)),
                        member("transitionToNextLowerStateEventId", HkxValue::I32(-1)),
                        member("syncVariableIndex", HkxValue::I32(-1)),
                        member("wrapAroundStateId", HkxValue::Bool(false)),
                        member("maxSimultaneousTransitions", HkxValue::I32(1)),
                        member("startStateMode", HkxValue::I32(0)),
                        member("selfTransitionMode", HkxValue::I32(0)),
                        member("states", HkxValue::Array(vec![HkxValue::Pointer(Some(2))])),
                        member("wildcardTransitions", HkxValue::Pointer(Some(1))),
                    ],
                ),
                object(
                    "hkbManualSelectorGenerator",
                    vec![
                        member("name", text("NestedSelector")),
                        member(
                            "generators",
                            HkxValue::Array(vec![HkxValue::Pointer(Some(3))]),
                        ),
                        member("selectedGeneratorIndex", HkxValue::I32(0)),
                        member("indexSelector", HkxValue::Pointer(None)),
                        member("selectedIndexCanChangeAfterActivate", HkxValue::Bool(false)),
                        member("generatorChangedTransitionEffect", HkxValue::Pointer(None)),
                    ],
                ),
                object(
                    "hkbBlendingTransitionEffect",
                    vec![
                        member("name", text("ExactBlend")),
                        member("selfTransitionMode", HkxValue::I8(2)),
                        member("eventMode", HkxValue::I8(1)),
                        member("duration", HkxValue::F32(0.2)),
                        member("toGeneratorStartTimeFraction", HkxValue::F32(0.4)),
                        member("flags", HkxValue::U16(3)),
                        member("endMode", HkxValue::I8(1)),
                        member("blendCurve", HkxValue::I8(2)),
                        member("alignmentBone", HkxValue::I16(-1)),
                    ],
                ),
            ],
        );
        let groups = extract_behavior_generator_groups(
            &file,
            "Actors\\Fixture\\Behavior.hkx",
            &["enterIdle".to_string(), "intervalStart".to_string()],
            &[],
            &[],
            &[],
        );
        let selector = groups
            .iter()
            .find_map(|group| match group {
                SkyrimBehaviorGeneratorGroupEvidence::ManualSelector(selector) => Some(selector),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected selector group: {groups:#?}"));
        assert!(groups.iter().any(|group| matches!(
            group,
            SkyrimBehaviorGeneratorGroupEvidence::StateMachine(state_machine)
                if state_machine.machine.source == selector.children[0].generator
        )));
        let SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine) =
            &selector.children[0].generator_tree
        else {
            panic!("expected nested state machine");
        };
        assert_eq!(machine.start_state_id, Some(7));
        assert_eq!(machine.states[0].probability_bits, Some(0.5_f32.to_bits()));
        assert_eq!(
            machine.states[0].enter_notify_events[0]
                .event_name
                .as_deref(),
            Some("intervalStart")
        );
        let transition = &machine.states[0].transitions.as_ref().unwrap().transitions[0];
        assert_eq!(transition.event_name.as_deref(), Some("enterIdle"));
        assert_eq!(transition.from_nested_state_id, Some(2));
        assert_eq!(transition.to_nested_state_id, Some(3));
        let blend = transition
            .blending_transition_effect
            .as_ref()
            .expect("exact blending transition receipt");
        assert_eq!(blend.source.object_index, 5);
        assert_eq!(blend.self_transition_mode, Some(2));
        assert_eq!(blend.event_mode, Some(1));
        assert_eq!(blend.duration_bits, Some(0.2_f32.to_bits()));
        assert_eq!(
            blend.to_generator_start_time_fraction_bits,
            Some(0.4_f32.to_bits())
        );
        assert_eq!(blend.flags, Some(3));
        assert_eq!(blend.end_mode, Some(1));
        assert_eq!(blend.blend_curve, Some(2));
        assert_eq!(blend.alignment_bone, Some(-1));
        assert!(blend.variable_bindings.is_empty());
        assert_eq!(
            transition
                .trigger_interval
                .as_ref()
                .and_then(|interval| interval.enter_event_name.as_deref()),
            Some("intervalStart")
        );
        assert!(matches!(
            machine.states[0].generator.as_ref(),
            Some(SkyrimBehaviorGeneratorNodeEvidence::Clip { animation_name, .. })
                if animation_name == "Animations\\Idle.hkx"
        ));
    }

    #[test]
    fn modifier_generator_receipt_preserves_order_expressions_and_variable_references() {
        let member = |name: &str, value: HkxValue| HkxMember {
            name: name.to_string(),
            value,
        };
        let object = |class_name: &str, members: Vec<HkxMember>| HkxObject {
            name: None,
            offset: 0,
            signature: 0,
            class_name: class_name.to_string(),
            members,
        };
        let text = |value: &str| HkxValue::String {
            value: value.to_string(),
            is_null: false,
        };
        let binding = |member_path: &str, variable_index| {
            HkxValue::Object(vec![
                member("memberPath", text(member_path)),
                member("variableIndex", HkxValue::I32(variable_index)),
                member("bitIndex", HkxValue::I8(-1)),
                member("bindingType", HkxValue::I8(0)),
            ])
        };
        let expression = |value: &str| {
            HkxValue::Object(vec![
                member("expression", text(value)),
                member("assignmentVariableIndex", HkxValue::I32(-1)),
                member("assignmentEventIndex", HkxValue::I32(-1)),
                member("eventMode", HkxValue::I8(2)),
            ])
        };
        let zero_vector = HkxValue::F32List(vec![0.0; 4]);
        let file = HkxFile::from_tagxml(
            8,
            SKYRIM_CONTENTS_VERSION,
            vec![
                object(
                    "hkbClipGenerator",
                    vec![
                        member("name", text("Forward")),
                        member("animationName", text("Animations\\Forward.hkx")),
                    ],
                ),
                object(
                    "hkbExpressionDataArray",
                    vec![member(
                        "expressionsData",
                        HkxValue::Array(vec![
                            expression("runStart if (Speed > 420)"),
                            expression("walkStart if (Speed < 420)"),
                        ]),
                    )],
                ),
                object(
                    "hkbEvaluateExpressionModifier",
                    vec![
                        member("name", text("ForwardLocomotion_EEM")),
                        member("userData", HkxValue::U64(2)),
                        member("enable", HkxValue::Bool(false)),
                        member("variableBindingSet", HkxValue::Pointer(None)),
                        member("expressions", HkxValue::Pointer(Some(1))),
                    ],
                ),
                object(
                    "hkbVariableBindingSet",
                    vec![member(
                        "bindings",
                        HkxValue::Array(vec![binding("rawValue", 0), binding("dampedValue", 1)]),
                    )],
                ),
                object(
                    "hkbDampingModifier",
                    vec![
                        member("name", text("TurnDeltaDampingModifier")),
                        member("userData", HkxValue::U64(1)),
                        member("enable", HkxValue::Bool(true)),
                        member("variableBindingSet", HkxValue::Pointer(Some(3))),
                        member("kP", HkxValue::F32(-0.1)),
                        member("kI", HkxValue::F32(f32::from_bits(1))),
                        member("kD", HkxValue::F32(0.0)),
                        member("enableScalarDamping", HkxValue::Bool(false)),
                        member("enableVectorDamping", HkxValue::Bool(false)),
                        member("rawValue", HkxValue::F32(0.0)),
                        member("dampedValue", HkxValue::F32(0.0)),
                        member("rawVector", zero_vector.clone()),
                        member("dampedVector", zero_vector.clone()),
                        member("vecErrorSum", zero_vector.clone()),
                        member("vecPreviousError", zero_vector),
                        member("errorSum", HkxValue::F32(0.0)),
                        member("previousError", HkxValue::F32(0.0)),
                    ],
                ),
                object(
                    "hkbModifierList",
                    vec![
                        member("name", text("ForwardLocomotionModifierList")),
                        member("userData", HkxValue::U64(1)),
                        member("enable", HkxValue::Bool(false)),
                        member("variableBindingSet", HkxValue::Pointer(None)),
                        member(
                            "modifiers",
                            HkxValue::Array(vec![
                                HkxValue::Pointer(Some(4)),
                                HkxValue::Pointer(Some(2)),
                            ]),
                        ),
                    ],
                ),
                object(
                    "hkbModifierGenerator",
                    vec![
                        member("name", text("ForwardLocomotion_MG")),
                        member("userData", HkxValue::U64(1)),
                        member("variableBindingSet", HkxValue::Pointer(None)),
                        member("modifier", HkxValue::Pointer(Some(5))),
                        member("generator", HkxValue::Pointer(Some(0))),
                    ],
                ),
            ],
        );
        let mut node = behavior_generator_tree(
            &file,
            6,
            &[],
            &["Speed".to_string(), "DampedSpeed".to_string()],
            &[Some(4), Some(4)],
            &[Some(0.0_f32.to_bits()), Some(1.0_f32.to_bits())],
            &mut BTreeSet::new(),
        );
        let SkyrimBehaviorGeneratorNodeEvidence::ModifierGenerator {
            generator: Some(generator),
            ..
        } = &mut node
        else {
            panic!("expected exact modifier generator receipt: {node:#?}");
        };
        let SkyrimBehaviorGeneratorNodeEvidence::Clip {
            resolved_clip_path, ..
        } = generator.as_mut()
        else {
            panic!("expected terminal modifier-generator clip");
        };
        *resolved_clip_path = Some("Animations\\Forward.hkx".to_string());
        let family = SkyrimFamilyMotionSet {
            family_id: "fixture".to_string(),
            project_path: "Actors\\Fixture\\FixtureProject.hkx".to_string(),
            race_attacks: Vec::new(),
            race_attack_sources: Vec::new(),
            clip_ids: vec!["forward".to_string()],
        };
        let clip = SkyrimCatalogClip {
            clip_id: "forward".to_string(),
            clip_path: "Animations\\Forward.hkx".to_string(),
            original_skeleton_name: None,
            family_ids: vec!["fixture".to_string()],
            disposition: SkyrimClipDisposition::Unsupported {
                reason: SkyrimUnsupportedMotionReason::NoSemanticRoleEvidence,
            },
            root_motion: SkyrimRootMotion::Unknown,
            events: Vec::new(),
            annotations: Vec::new(),
        };
        let shared =
            capability_generator_node(&family, "Actors\\Fixture\\Behavior.hkx", &node, &[&clip])
                .expect("exact modifier tree should lower to the shared receipt");
        let CapabilityGeneratorNode::ModifierGenerator {
            user_data,
            modifier: shared_modifier,
            generator: shared_generator,
            ..
        } = shared
        else {
            panic!("expected shared modifier-generator receipt");
        };
        assert_eq!(user_data, 1);
        assert!(matches!(
            shared_generator.as_ref(),
            CapabilityGeneratorNode::Clip { clip_name, .. } if clip_name == "forward"
        ));
        let CapabilityModifierNode::List {
            user_data,
            enable,
            modifiers: shared_modifiers,
            ..
        } = shared_modifier.as_ref()
        else {
            panic!("expected shared ordered modifier-list receipt");
        };
        assert_eq!(*user_data, 1);
        assert!(!enable);
        assert!(matches!(
            shared_modifiers.as_slice(),
            [
                CapabilityModifierNode::Damping { .. },
                CapabilityModifierNode::EvaluateExpression { .. }
            ]
        ));
        let SkyrimBehaviorGeneratorNodeEvidence::ModifierGenerator {
            user_data,
            modifier: Some(modifier),
            generator: Some(generator),
            ..
        } = &node
        else {
            panic!("expected exact modifier generator receipt: {node:#?}");
        };
        assert_eq!(*user_data, Some(1));
        assert!(matches!(
            generator.as_ref(),
            SkyrimBehaviorGeneratorNodeEvidence::Clip { animation_name, .. }
                if animation_name == "Animations\\Forward.hkx"
        ));
        let SkyrimBehaviorModifierNodeEvidence::ModifierList {
            user_data,
            enable,
            modifiers,
            ..
        } = modifier.as_ref()
        else {
            panic!("expected exact modifier list receipt");
        };
        assert_eq!(*user_data, Some(1));
        assert_eq!(*enable, Some(false));
        assert_eq!(modifiers.len(), 2);
        let SkyrimBehaviorModifierNodeEvidence::Damping {
            k_p_bits,
            k_i_bits,
            variable_bindings,
            raw_vector_bits,
            ..
        } = &modifiers[0]
        else {
            panic!("expected damping modifier first");
        };
        assert_eq!(*k_p_bits, Some((-0.1_f32).to_bits()));
        assert_eq!(*k_i_bits, Some(1));
        assert_eq!(raw_vector_bits, &Some([0; 4]));
        let raw_binding = variable_bindings
            .iter()
            .find(|binding| binding.member_path == "rawValue")
            .expect("raw-value binding");
        let damped_binding = variable_bindings
            .iter()
            .find(|binding| binding.member_path == "dampedValue")
            .expect("damped-value binding");
        assert_eq!(raw_binding.variable_name.as_deref(), Some("Speed"));
        assert_eq!(damped_binding.variable_index, 1);
        let SkyrimBehaviorModifierNodeEvidence::EvaluateExpression {
            expressions: Some(expressions),
            ..
        } = &modifiers[1]
        else {
            panic!("expected evaluate-expression modifier second");
        };
        assert_eq!(expressions.source.object_index, 1);
        assert_eq!(expressions.expressions[0].event_mode, Some(2));
        assert_eq!(expressions.expressions[0].referenced_variables.len(), 1);
        assert_eq!(
            expressions.expressions[0].referenced_variables[0],
            SkyrimBehaviorExpressionVariableReferenceEvidence {
                variable_index: 0,
                variable_name: "Speed".to_string(),
                variable_type: Some(4),
                initial_word_value: Some(0.0_f32.to_bits()),
            }
        );
    }

    #[test]
    fn malformed_legacy_controller_dimensions_are_a_typed_fatal_disposition() {
        let controller = HkxValue::TypedObject {
            class_name: SKYRIM_CHARACTER_CONTROLLER_INFO_CLASS.to_string(),
            members: vec![
                HkxMember {
                    name: "capsuleHeight".to_string(),
                    value: HkxValue::F32(f32::from_bits(1)),
                },
                HkxMember {
                    name: "capsuleRadius".to_string(),
                    value: HkxValue::F32(0.0),
                },
                HkxMember {
                    name: "collisionFilterInfo".to_string(),
                    value: HkxValue::U32(1),
                },
                HkxMember {
                    name: "characterControllerCinfo".to_string(),
                    value: HkxValue::Pointer(None),
                },
            ],
        };
        let vector = |name: &str, values: [f32; 4]| HkxMember {
            name: name.to_string(),
            value: HkxValue::F32List(values.to_vec()),
        };
        let file = HkxFile::from_tagxml(
            8,
            SKYRIM_CONTENTS_VERSION,
            vec![HkxObject {
                name: None,
                offset: 0,
                signature: SKYRIM_CHARACTER_DATA_SIGNATURE,
                class_name: "hkbCharacterData".to_string(),
                members: vec![
                    HkxMember {
                        name: "characterControllerInfo".to_string(),
                        value: controller,
                    },
                    vector("modelUpMS", [0.0, 0.0, 1.0, 0.0]),
                    vector("modelForwardMS", [0.0, 1.0, 0.0, 0.0]),
                    vector("modelRightMS", [1.0, 0.0, 0.0, 0.0]),
                    HkxMember {
                        name: "scale".to_string(),
                        value: HkxValue::F32(1.0),
                    },
                ],
            }],
        );

        let evidence = extract_character_controller(
            &file,
            &file.objects()[0],
            "Actors\\Malformed\\character.hkx",
        );
        assert!(matches!(
            evidence.disposition,
            SkyrimControllerEvidenceDisposition::InvalidCapsuleDimensions {
                total_height_bits: Some(1),
                radius_bits: Some(0),
            }
        ));
        assert!(evidence.capsule.is_none());
        assert!(evidence.model.is_some());
        assert!(evidence.controller_cinfo_class.is_none());
        assert!(evidence.architecture.is_none());
        assert!(evidence.legacy_controller_recipe_evidence().is_none());
    }

    #[test]
    fn race_and_behavior_evidence_build_source_neutral_motion_inputs() {
        let mut family = SkyrimCreatureFamilyEvidence {
            family_id: "wolf".to_string(),
            project_path: "canine\\wolfproject.hkx".to_string(),
            inventory: SkyrimCreatureFamilyInventory::default(),
            race_attacks: vec![
                SkyrimRaceAttackEvidence {
                    event: "attackStart_Attack1".to_string(),
                    has_attack_spell: false,
                },
                SkyrimRaceAttackEvidence {
                    event: "attackStart_Attack2".to_string(),
                    has_attack_spell: false,
                },
            ],
            race_attack_sources: Vec::new(),
            clips: vec![
                clip("animations\\idle.hkx", "Idle", None, "NPC Root [Root]"),
                clip(
                    "animations\\walk.hkx",
                    "WalkForward",
                    Some("startWalk"),
                    "NPC Root [Root]",
                ),
                clip(
                    "animations\\turn_left.hkx",
                    "TurnLeft",
                    Some("TurnLeft90"),
                    "NPC Root [Root]",
                ),
                clip(
                    "animations\\turn_right.hkx",
                    "TurnRight",
                    Some("TurnRight90"),
                    "NPC Root [Root]",
                ),
                clip(
                    "animations\\attack1.hkx",
                    "Attack1",
                    Some("attackStart_Attack1"),
                    "NPC Root [Root]",
                ),
                clip(
                    "animations\\attack2.hkx",
                    "Attack2",
                    Some("attackStart_Attack2"),
                    "NPC Root [Root]",
                ),
            ],
        };
        for clip in &mut family.clips {
            clip.root_motion = if clip.behavior.iter().any(|evidence| {
                evidence
                    .state_names
                    .iter()
                    .any(|name| name == "WalkForward")
            }) {
                SkyrimRootMotion::Sampled {
                    source: SkyrimRootMotionSource::HavokReferenceFrame,
                    sample_count: 2,
                    translation_delta: [1.0, 0.0, 0.0],
                    rotation_start: None,
                    rotation_end: None,
                }
            } else {
                SkyrimRootMotion::Stationary {
                    source: SkyrimRootMotionSource::HavokReferenceFrame,
                    sample_count: 1,
                }
            };
        }
        let catalog = build_skyrim_creature_motion_catalog(&[family]).unwrap();
        assert_eq!(catalog.accounting.role_clips, 6);
        let motion = catalog.motion_set("wolf").unwrap();
        assert_eq!(motion.attacks.len(), 2);
        let graph = catalog.capability_graph_manifest("wolf").unwrap();
        assert_eq!(graph.template, CreatureGraphTemplate::GroundMelee);
        assert_eq!(
            graph
                .roles
                .iter()
                .filter(|role| role.role == CreatureClipRole::MeleeAttack)
                .count(),
            2
        );
        let living_evidence = catalog.living_family_evidence();
        let [living] = living_evidence.as_slice() else {
            panic!("one fixture family");
        };
        assert_eq!(
            living.disposition,
            SkyrimLivingFamilyDisposition::Ready,
            "{living:#?}"
        );
        let attack = living
            .required_roles
            .iter()
            .find(|disposition| {
                matches!(
                    disposition,
                    SkyrimRequiredRoleDisposition::Ready { alternatives, .. }
                        if alternatives == &[SkyrimCreatureMotionRole::MeleeAttack]
                )
            })
            .expect("melee role receipt");
        let SkyrimRequiredRoleDisposition::Ready { additional, .. } = attack else {
            unreachable!();
        };
        assert_eq!(additional.len(), 1);
    }

    #[test]
    fn attack_roles_do_not_infer_ground_movement_for_pure_swimmers() {
        let candidates = [
            CreatureGraphTemplate::GroundMelee,
            CreatureGraphTemplate::Swim,
        ];
        let pure_swim = BTreeSet::from([
            SkyrimCreatureMotionRole::MeleeAttack,
            SkyrimCreatureMotionRole::SwimIdle,
            SkyrimCreatureMotionRole::SwimLocomotion,
        ]);
        assert_eq!(
            multimodal_template(&candidates, &pure_swim),
            Some(CreatureGraphTemplate::Swim)
        );

        let ground_swim = BTreeSet::from([
            SkyrimCreatureMotionRole::MeleeAttack,
            SkyrimCreatureMotionRole::GroundLocomotion,
            SkyrimCreatureMotionRole::SwimIdle,
            SkyrimCreatureMotionRole::SwimLocomotion,
        ]);
        assert_eq!(
            multimodal_template(&candidates, &ground_swim),
            Some(CreatureGraphTemplate::GroundSwim)
        );
    }

    #[test]
    fn incomplete_multi_attack_set_keeps_the_exact_missing_trigger_blocker() {
        let candidate = |clip_id: &str, triggered: bool| SkyrimLivingClipCandidate {
            role: SkyrimCreatureMotionRole::MeleeAttack,
            clip_id: clip_id.to_string(),
            clip_path: format!("Animations\\{clip_id}.hkx"),
            triggers: triggered
                .then(|| SkyrimMotionTriggerEvidence {
                    event: format!("attackStart_{clip_id}"),
                    locator: SkyrimMotionSourceLocator::BehaviorTransition {
                        behavior_path: "Actors\\Fixture\\Root.hkx".to_string(),
                        animation_name: format!("Animations\\{clip_id}.hkx"),
                        event: format!("attackStart_{clip_id}"),
                    },
                })
                .into_iter()
                .collect(),
            root_motion: SkyrimRootMotion::Stationary {
                source: SkyrimRootMotionSource::HavokReferenceFrame,
                sample_count: 1,
            },
            root_motion_locator: Some(SkyrimRootMotionSourceLocator::HavokReferenceFrame {
                clip_path: format!("Animations\\{clip_id}.hkx"),
            }),
            role_locators: Vec::new(),
        };
        let disposition = unready_required_role_disposition(
            &[SkyrimCreatureMotionRole::MeleeAttack],
            vec![candidate("ready", true), candidate("missing", false)],
        );
        assert!(matches!(
            disposition,
            SkyrimRequiredRoleDisposition::MissingTrigger { candidates, .. }
                if candidates.len() == 2
        ));
    }

    #[test]
    fn referenced_behavior_entry_events_keep_the_parent_locator() {
        let edges = vec![
            ResolvedNestedBehaviorEdge {
                parent_key: "root".to_string(),
                child_key: "middle".to_string(),
                behavior_path: "Actors\\Fixture\\Root.hkx".to_string(),
                initial: true,
                incoming_events: vec!["enterMiddle".to_string()],
            },
            ResolvedNestedBehaviorEdge {
                parent_key: "middle".to_string(),
                child_key: "leaf".to_string(),
                behavior_path: "Actors\\Fixture\\Middle.hkx".to_string(),
                initial: true,
                incoming_events: vec!["enterLeaf".to_string()],
            },
        ];
        let mut events = Vec::new();
        collect_ancestor_behavior_entry_events("leaf", &edges, &mut BTreeSet::new(), &mut events);
        events.sort();
        assert_eq!(
            events,
            [
                SkyrimBehaviorEntryEventEvidence {
                    behavior_path: "Actors\\Fixture\\Middle.hkx".to_string(),
                    event: "enterLeaf".to_string(),
                },
                SkyrimBehaviorEntryEventEvidence {
                    behavior_path: "Actors\\Fixture\\Root.hkx".to_string(),
                    event: "enterMiddle".to_string(),
                },
            ]
        );
    }

    #[test]
    fn idle_roles_are_ready_without_events_and_attack_roles_still_require_one() {
        let candidate =
            |role, triggers: Vec<SkyrimMotionTriggerEvidence>| SkyrimLivingClipCandidate {
                role,
                clip_id: "clip".to_string(),
                clip_path: "actors/fixture/clip.hkx".to_string(),
                triggers,
                root_motion: SkyrimRootMotion::Stationary {
                    source: SkyrimRootMotionSource::HavokReferenceFrame,
                    sample_count: 1,
                },
                root_motion_locator: Some(SkyrimRootMotionSourceLocator::HavokReferenceFrame {
                    clip_path: "actors/fixture/clip.hkx".to_string(),
                }),
                role_locators: Vec::new(),
            };
        let trigger = SkyrimMotionTriggerEvidence {
            event: "attackStart".to_string(),
            locator: SkyrimMotionSourceLocator::BehaviorTransition {
                behavior_path: "actors/fixture/behavior.hkx".to_string(),
                animation_name: "clip".to_string(),
                event: "attackStart".to_string(),
            },
        };

        for role in [
            SkyrimCreatureMotionRole::Idle,
            SkyrimCreatureMotionRole::SwimIdle,
            SkyrimCreatureMotionRole::FlyIdle,
            SkyrimCreatureMotionRole::StationaryIdle,
        ] {
            assert!(candidate_is_ready(&candidate(role, Vec::new())));
            assert!(!candidate_is_ready(&candidate(role, vec![trigger.clone()])));
        }
        assert!(!candidate_is_ready(&candidate(
            SkyrimCreatureMotionRole::MeleeAttack,
            Vec::new(),
        )));
        assert!(candidate_is_ready(&candidate(
            SkyrimCreatureMotionRole::MeleeAttack,
            vec![trigger],
        )));
    }

    #[test]
    fn behavior_role_keeps_corroborating_animation_data_ordinal() {
        let classified_idle = |ordinal: u32| {
            let mut evidence_clip = clip(
                &format!("animations\\idle{ordinal}.hkx"),
                "MainIdle",
                None,
                "NPC Root [Root]",
            );
            evidence_clip.animation_data = vec![SkyrimAnimationDataEvidence {
                source_path: "meshes/animationdata/animationdatasinglefile.txt".to_string(),
                root_motion_source_path: None,
                project_stem: "fixture".to_string(),
                sequence_name: format!("Idle_FullBody{ordinal}"),
                animation_index: ordinal,
                playback_speed: 1.0,
                crop_start_local_time: 0.0,
                crop_end_local_time: 0.0,
                events: Vec::new(),
                root_motion: SkyrimRootMotion::Unknown,
            }];
            let family = SkyrimCreatureFamilyEvidence {
                family_id: "passive_fixture".to_string(),
                project_path: "fixture\\project.hkx".to_string(),
                inventory: SkyrimCreatureFamilyInventory::default(),
                race_attacks: Vec::new(),
                race_attack_sources: Vec::new(),
                clips: vec![evidence_clip.clone()],
            };
            classify_clip(&MergedClipEvidence::new(&family, &evidence_clip))
        };

        let role_locators = |disposition: SkyrimClipDisposition| match disposition {
            SkyrimClipDisposition::Role {
                role: SkyrimCreatureMotionRole::Idle,
                evidence,
                ..
            } => evidence
                .into_iter()
                .map(|item| item.locator)
                .collect::<Vec<_>>(),
            other => panic!("expected Idle role, got {other:?}"),
        };
        let first = role_locators(classified_idle(1));
        let second = role_locators(classified_idle(2));
        assert!(first.iter().any(|locator| matches!(
            locator,
            SkyrimMotionSourceLocator::BehaviorGenerator { node, .. } if node == "MainIdle"
        )));
        assert!(first.iter().any(|locator| matches!(
            locator,
            SkyrimMotionSourceLocator::AnimationDataSequence { sequence_name, animation_index: 1, .. }
                if sequence_name == "Idle_FullBody1"
        )));
        let candidate = |clip_id: &str, role_locators| SkyrimLivingClipCandidate {
            role: SkyrimCreatureMotionRole::Idle,
            clip_id: clip_id.to_string(),
            clip_path: format!("animations/{clip_id}.hkx"),
            triggers: Vec::new(),
            root_motion: SkyrimRootMotion::Stationary {
                source: SkyrimRootMotionSource::HavokReferenceFrame,
                sample_count: 1,
            },
            root_motion_locator: Some(SkyrimRootMotionSourceLocator::HavokReferenceFrame {
                clip_path: format!("animations/{clip_id}.hkx"),
            }),
            role_locators,
        };
        let selected = select_semantic_primary_candidate(&[
            candidate("idle1", first),
            candidate("idle2", second),
        ])
        .expect("AnimationData ordinals select one semantic primary");
        assert_eq!(selected.clip_id, "idle1");
    }

    #[test]
    fn incoming_events_are_scoped_to_the_owning_state_machine() {
        let member = |name: &str, value| HkxMember {
            name: name.to_string(),
            value,
        };
        let object = |class_name: &str, members| HkxObject {
            name: None,
            offset: 0,
            signature: 0,
            class_name: class_name.to_string(),
            members,
        };
        let string = |value: &str| HkxValue::String {
            value: value.to_string(),
            is_null: false,
        };
        let transition = |event_id, flags| {
            HkxValue::Object(vec![
                member("toStateId", HkxValue::I32(1)),
                member("eventId", HkxValue::I32(event_id)),
                member("flags", HkxValue::I16(flags)),
            ])
        };
        let state = |name: &str, generator, transitions| {
            object(
                "hkbStateMachineStateInfo",
                vec![
                    member("name", string(name)),
                    member("stateId", HkxValue::I32(1)),
                    member("generator", HkxValue::Pointer(Some(generator))),
                    member("transitions", HkxValue::Pointer(Some(transitions))),
                ],
            )
        };
        let machine = |state_index| {
            object(
                "hkbStateMachine",
                vec![member(
                    "states",
                    HkxValue::Array(vec![HkxValue::Pointer(Some(state_index))]),
                )],
            )
        };
        let file = HkxFile::from_tagxml(
            8,
            SKYRIM_CONTENTS_VERSION,
            vec![
                object(
                    "hkbBehaviorGraphStringData",
                    vec![member(
                        "eventNames",
                        HkxValue::Array(vec![
                            string("enterFirst"),
                            string("enterSecond"),
                            string("disabledEntry"),
                        ]),
                    )],
                ),
                object(
                    "hkbClipGenerator",
                    vec![member("animationName", string("Animations\\First.hkx"))],
                ),
                object(
                    "hkbClipGenerator",
                    vec![member("animationName", string("Animations\\Second.hkx"))],
                ),
                object(
                    "hkbStateMachineTransitionInfoArray",
                    vec![member(
                        "transitions",
                        HkxValue::Array(vec![transition(0, 0), transition(2, 32)]),
                    )],
                ),
                object(
                    "hkbStateMachineTransitionInfoArray",
                    vec![member(
                        "transitions",
                        HkxValue::Array(vec![transition(1, 0)]),
                    )],
                ),
                state("FirstState", 1, 3),
                state("SecondState", 2, 4),
                machine(5),
                machine(6),
            ],
        );

        let references = extract_behavior_clip_evidence(
            &file,
            Path::new("Actors"),
            Path::new("Actors\\Fixture\\Behavior.hkx"),
        )
        .clips;
        let by_animation = references
            .into_iter()
            .map(|reference| (reference.animation_name.clone(), reference))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            by_animation["Animations\\First.hkx"].incoming_events,
            ["enterFirst"]
        );
        assert_eq!(
            by_animation["Animations\\Second.hkx"].incoming_events,
            ["enterSecond"]
        );
    }

    #[test]
    fn nested_behavior_clip_uses_the_nearest_owning_state() {
        let member = |name: &str, value| HkxMember {
            name: name.to_string(),
            value,
        };
        let object = |class_name: &str, members| HkxObject {
            name: None,
            offset: 0,
            signature: 0,
            class_name: class_name.to_string(),
            members,
        };
        let string = |value: &str| HkxValue::String {
            value: value.to_string(),
            is_null: false,
        };
        let transition = |to_state_id, event_id| {
            HkxValue::Object(vec![
                member("toStateId", HkxValue::I32(to_state_id)),
                member("eventId", HkxValue::I32(event_id)),
            ])
        };
        let state = |name: &str, state_id, generator, transitions| {
            object(
                "hkbStateMachineStateInfo",
                vec![
                    member("name", string(name)),
                    member("stateId", HkxValue::I32(state_id)),
                    member("generator", HkxValue::Pointer(Some(generator))),
                    member("transitions", HkxValue::Pointer(Some(transitions))),
                ],
            )
        };
        let machine = |state_index| {
            object(
                "hkbStateMachine",
                vec![member(
                    "states",
                    HkxValue::Array(vec![HkxValue::Pointer(Some(state_index))]),
                )],
            )
        };
        let file = HkxFile::from_tagxml(
            8,
            SKYRIM_CONTENTS_VERSION,
            vec![
                object(
                    "hkbBehaviorGraphStringData",
                    vec![member(
                        "eventNames",
                        HkxValue::Array(vec![string("idleStart"), string("moveStart")]),
                    )],
                ),
                object(
                    "hkbClipGenerator",
                    vec![
                        member("name", string("ExactIdleGenerator")),
                        member("animationName", string("Animations\\Idle.hkx")),
                    ],
                ),
                object(
                    "hkbStateMachineTransitionInfoArray",
                    vec![member(
                        "transitions",
                        HkxValue::Array(vec![transition(1, 0)]),
                    )],
                ),
                state("InnerIdleState", 1, 1, 2),
                machine(3),
                object(
                    "hkbStateMachineTransitionInfoArray",
                    vec![member(
                        "transitions",
                        HkxValue::Array(vec![transition(2, 1)]),
                    )],
                ),
                state("OuterLocomotionState", 2, 4, 5),
                machine(6),
            ],
        );

        let references = extract_behavior_clip_evidence(
            &file,
            Path::new("Actors"),
            Path::new("Actors\\Fixture\\Behavior.hkx"),
        )
        .clips;
        let [reference] = references.as_slice() else {
            panic!("one clip reference");
        };
        assert_eq!(reference.state_names, ["InnerIdleState"]);
        assert_eq!(reference.incoming_events, ["idleStart", "moveStart"]);
        assert!(
            !reference
                .topology_names
                .iter()
                .any(|name| name == "OuterLocomotionState")
        );
    }

    #[test]
    fn paired_and_overlay_clips_never_enter_the_single_rig_role_catalog() {
        let mut paired = clip(
            "animations\\paired.hkx",
            "KillMove",
            Some("KillMove"),
            "PairedRoot",
        );
        let mut overlay = clip(
            "animations\\face_offset.hkx",
            "FaceOffset",
            Some("FaceStart"),
            "NPC Root [Root]",
        );
        overlay.blend_hint = 1;
        paired.root_motion = SkyrimRootMotion::Sampled {
            source: SkyrimRootMotionSource::HavokReferenceFrame,
            sample_count: 2,
            translation_delta: [1.0, 0.0, 0.0],
            rotation_start: None,
            rotation_end: None,
        };
        let catalog = build_skyrim_creature_motion_catalog(&[SkyrimCreatureFamilyEvidence {
            family_id: "wolf".to_string(),
            project_path: "canine\\wolfproject.hkx".to_string(),
            inventory: SkyrimCreatureFamilyInventory::default(),
            race_attacks: Vec::new(),
            race_attack_sources: Vec::new(),
            clips: vec![paired, overlay],
        }])
        .unwrap();
        assert_eq!(catalog.accounting.paired_clips, 1);
        assert_eq!(catalog.accounting.overlay_clips, 1);
        assert_eq!(catalog.accounting.role_clips, 0);
    }

    #[test]
    fn filename_alone_never_assigns_a_semantic_role() {
        let catalog = build_skyrim_creature_motion_catalog(&[SkyrimCreatureFamilyEvidence {
            family_id: "wolf".to_string(),
            project_path: "canine\\wolfproject.hkx".to_string(),
            inventory: SkyrimCreatureFamilyInventory::default(),
            race_attacks: Vec::new(),
            race_attack_sources: Vec::new(),
            clips: vec![SkyrimCreatureClipEvidence {
                clip_path: "animations\\obvious_attack_and_death.hkx".to_string(),
                original_skeleton_name: Some("NPC Root [Root]".to_string()),
                blend_hint: 0,
                behavior: Vec::new(),
                animation_data: Vec::new(),
                annotations: Vec::new(),
                root_motion: SkyrimRootMotion::Unknown,
            }],
        }])
        .unwrap();
        assert_eq!(catalog.accounting.unsupported_clips, 1);
        assert!(matches!(
            catalog.clips[0].disposition,
            SkyrimClipDisposition::Unsupported {
                reason: SkyrimUnsupportedMotionReason::BehaviorReferenceMissing
            }
        ));
    }

    #[test]
    fn attack_contract_pairs_struct_spell_data_with_the_following_event() {
        let attacks = attack_evidence_from_contract(&[
            "atkd:struct:{attack_spell=form:skyrim.esm:012345,attack_type=form:skyrim.esm:0914e5}"
                .to_string(),
            "atke:string:AttackStartSpell".to_string(),
            "atkd:bytes:0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            "atke:string:attackStartBite".to_string(),
        ]);
        assert_eq!(
            attacks,
            vec![
                SkyrimRaceAttackEvidence {
                    event: "AttackStartSpell".to_string(),
                    has_attack_spell: true,
                },
                SkyrimRaceAttackEvidence {
                    event: "attackStartBite".to_string(),
                    has_attack_spell: false,
                },
            ]
        );
    }

    #[test]
    fn shared_project_race_owners_preserve_conflicting_attack_kinds() {
        let interner = crate::sym::StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let spell = SkyrimRaceAttackEvidence {
            event: "attackStart_Attack1".to_string(),
            has_attack_spell: true,
        };
        let melee = SkyrimRaceAttackEvidence {
            event: "attackStart_Attack1".to_string(),
            has_attack_spell: false,
        };
        let spell_race = FormKey {
            local: 0x013209,
            plugin,
        };
        let melee_race = FormKey {
            local: 0x123456,
            plugin,
        };
        let evidence_clip = SkyrimCreatureClipEvidence {
            clip_path: "Actors\\Witchlight\\Animations\\Attack1.hkx".to_string(),
            original_skeleton_name: Some("NPC Root [Root]".to_string()),
            blend_hint: 0,
            behavior: Vec::new(),
            animation_data: vec![SkyrimAnimationDataEvidence {
                source_path: "meshes/animationdata/animationdatasinglefile.txt".to_string(),
                root_motion_source_path: Some(
                    "meshes/animationdata/animationdatasinglefile.txt".to_string(),
                ),
                project_stem: "witchlight".to_string(),
                sequence_name: "Attack1".to_string(),
                animation_index: 0,
                playback_speed: 1.0,
                crop_start_local_time: 0.0,
                crop_end_local_time: 0.0,
                events: vec![SkyrimTimedEvent {
                    name: "attackStop".to_string(),
                    time: 1.0,
                }],
                root_motion: SkyrimRootMotion::Stationary {
                    source: SkyrimRootMotionSource::BoundAnims,
                    sample_count: 1,
                },
            }],
            annotations: Vec::new(),
            root_motion: SkyrimRootMotion::Stationary {
                source: SkyrimRootMotionSource::BoundAnims,
                sample_count: 1,
            },
        };
        let family = SkyrimCreatureFamilyEvidence {
            family_id: "witchlight".to_string(),
            project_path: "witchlight\\witchlightproject.hkx".to_string(),
            inventory: SkyrimCreatureFamilyInventory::default(),
            race_attacks: vec![spell.clone(), melee.clone()],
            race_attack_sources: vec![
                SkyrimRaceAttackSourceEvidence {
                    source_race: spell_race,
                    source_plugin: "Skyrim.esm".to_string(),
                    attack: spell,
                },
                SkyrimRaceAttackSourceEvidence {
                    source_race: melee_race,
                    source_plugin: "Skyrim.esm".to_string(),
                    attack: melee,
                },
            ],
            clips: vec![evidence_clip.clone()],
        };

        let merged = MergedClipEvidence::new(&family, &evidence_clip);
        assert!(
            merged
                .race_attacks
                .iter()
                .any(|attack| attack.source_race == Some(spell_race))
        );
        assert!(
            merged
                .race_attacks
                .iter()
                .any(|attack| attack.source_race == Some(melee_race))
        );
        let disposition = classify_clip(&merged);
        let SkyrimClipDisposition::Ambiguous { roles, evidence } = disposition else {
            panic!("conflicting source attack kinds must stay typed ambiguous")
        };
        assert_eq!(
            roles,
            [
                SkyrimCreatureMotionRole::MeleeAttack,
                SkyrimCreatureMotionRole::SpellAttack,
            ]
        );
        assert_eq!(
            evidence
                .iter()
                .filter_map(|entry| entry.trigger.as_ref())
                .map(|trigger| trigger.event.to_ascii_lowercase())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["attackstart_attack1".to_string()])
        );
        assert!(
            evidence
                .iter()
                .any(|entry| entry.source == SkyrimMotionEvidenceSource::RaceAttackEvent)
        );
    }

    #[test]
    fn semantic_role_classifier_covers_each_runtime_motion_family() {
        let cases = [
            ("Idle", SkyrimCreatureMotionRole::Idle),
            ("WalkForward", SkyrimCreatureMotionRole::GroundLocomotion),
            ("TurnLeft", SkyrimCreatureMotionRole::TurnLeft),
            ("TurnRight", SkyrimCreatureMotionRole::TurnRight),
            ("TurnAround", SkyrimCreatureMotionRole::Turn),
            ("AttackBite", SkyrimCreatureMotionRole::MeleeAttack),
            ("AttackShootBow", SkyrimCreatureMotionRole::RangedAttack),
            (
                "AttackSpitProjectile",
                SkyrimCreatureMotionRole::ProjectileAttack,
            ),
            ("AttackCastSpell", SkyrimCreatureMotionRole::SpellAttack),
            ("SwimIdle", SkyrimCreatureMotionRole::SwimIdle),
            ("WW_SwimTread", SkyrimCreatureMotionRole::SwimIdle),
            ("SwimForward", SkyrimCreatureMotionRole::SwimLocomotion),
            ("HoverIdle", SkyrimCreatureMotionRole::FlyIdle),
            ("FlyForward", SkyrimCreatureMotionRole::FlyLocomotion),
            ("StationaryIdle", SkyrimCreatureMotionRole::StationaryIdle),
            ("DeployStart", SkyrimCreatureMotionRole::MechanicalStart),
            ("SpinLoop", SkyrimCreatureMotionRole::MechanicalLoop),
            ("RetractStop", SkyrimCreatureMotionRole::MechanicalStop),
            ("HitReact", SkyrimCreatureMotionRole::Hurt),
            ("Death", SkyrimCreatureMotionRole::Death),
        ];
        for (text, expected) in cases {
            assert_eq!(role_from_text(text), Some(expected), "{text}");
        }
        assert_eq!(role_from_text("Idle"), Some(SkyrimCreatureMotionRole::Idle));
        assert_eq!(role_from_text("Start"), None);
    }

    #[test]
    fn swim_tread_semantics_bind_stationary_idle_without_consuming_mode_entry_event() {
        let mut evidence_clip = clip(
            "Animations\\WW_SwimTread.hkx",
            "WW SwimTread.hkx",
            Some("SwimStart"),
            "NPC Root [Root]",
        );
        evidence_clip.animation_data = vec![SkyrimAnimationDataEvidence {
            source_path: "meshes/animationdata/animationdatasinglefile.txt".to_string(),
            root_motion_source_path: Some(
                "meshes/animationdata/animationdatasinglefile.txt".to_string(),
            ),
            project_stem: "werewolfbeastproject".to_string(),
            sequence_name: "WW SwimTread.hkx00".to_string(),
            animation_index: 90,
            playback_speed: 1.0,
            crop_start_local_time: 0.0,
            crop_end_local_time: 0.0,
            events: vec![SkyrimTimedEvent {
                name: "SwimStart".to_string(),
                time: 1.9,
            }],
            root_motion: SkyrimRootMotion::Stationary {
                source: SkyrimRootMotionSource::BoundAnims,
                sample_count: 1,
            },
        }];
        evidence_clip.root_motion = evidence_clip.animation_data[0].root_motion.clone();
        let family = SkyrimCreatureFamilyEvidence {
            family_id: "werewolf".to_string(),
            project_path: "werewolfbeast\\werewolfbeastproject.hkx".to_string(),
            inventory: SkyrimCreatureFamilyInventory::default(),
            race_attacks: Vec::new(),
            race_attack_sources: Vec::new(),
            clips: vec![evidence_clip.clone()],
        };

        let disposition = classify_clip(&MergedClipEvidence::new(&family, &evidence_clip));
        let SkyrimClipDisposition::Role {
            role,
            trigger_event,
            trigger_aliases,
            evidence,
        } = disposition
        else {
            panic!("expected an exact semantic role")
        };
        assert_eq!(role, SkyrimCreatureMotionRole::SwimIdle);
        assert_eq!(trigger_event, None);
        assert!(trigger_aliases.is_empty());
        assert!(evidence.iter().any(|entry| matches!(
            &entry.locator,
            SkyrimMotionSourceLocator::BehaviorGenerator { node, .. }
                if node == "WW SwimTread.hkx"
        )));
        assert!(evidence.iter().any(|entry| matches!(
            &entry.locator,
            SkyrimMotionSourceLocator::AnimationDataSequence {
                sequence_name,
                animation_index: 90,
                ..
            } if sequence_name == "WW SwimTread.hkx00"
        )));
        assert!(
            evidence
                .iter()
                .all(|entry| entry.root_motion_locator.is_some())
        );
    }

    #[test]
    fn transition_roles_reject_outgoing_stop_events() {
        assert_eq!(
            transition_role("moveStart", &[]),
            Some(SkyrimCreatureMotionRole::GroundLocomotion)
        );
        assert_eq!(
            transition_role("attackStart_Attack1", &[]),
            Some(SkyrimCreatureMotionRole::MeleeAttack)
        );
        assert_eq!(transition_role("moveStop", &[]), None);
        assert_eq!(transition_role("attackStop", &[]), None);
        assert_eq!(transition_role("deathEnd", &[]), None);
        assert!(transition_event_matches_node(
            "attackStart_Attack_L1",
            "Attack_L1"
        ));
        assert!(!transition_event_matches_node(
            "attackStart_Attack_R1",
            "Attack_L1"
        ));
        assert!(transition_event_matches_node("moveStart", "ForwardWalk"));
        assert!(transition_event_matches_node(
            "moveStartBackward",
            "Ranged_DrawnBackward"
        ));
        assert!(!transition_event_matches_node(
            "moveStartBackward",
            "Ranged_DrawnForward"
        ));
    }

    #[test]
    fn race_attack_link_requires_the_complete_semantic_token_set() {
        let attacks = [
            LocatedRaceAttack {
                project_path: "cow\\highlandcowproject.hkx".to_string(),
                source_race: None,
                attack: SkyrimRaceAttackEvidence {
                    event: "attackStart_ForwardPower".to_string(),
                    has_attack_spell: false,
                },
            },
            LocatedRaceAttack {
                project_path: "cow\\highlandcowproject.hkx".to_string(),
                source_race: None,
                attack: SkyrimRaceAttackEvidence {
                    event: "attackStart_StandingPower".to_string(),
                    has_attack_spell: false,
                },
            },
        ];

        assert_eq!(
            linked_race_events("AttackPowerForward", &attacks)
                .iter()
                .map(|attack| attack.attack.event.as_str())
                .collect::<Vec<_>>(),
            ["attackStart_ForwardPower"]
        );
        assert!(linked_race_events("RunForward", &attacks).is_empty());
        assert!(linked_race_events("StaggerForward", &attacks).is_empty());
    }

    #[test]
    fn exact_extracted_46_family_corpus_has_one_terminal_disposition_per_clip() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let actors = repo.join("extracted/skyrimse/meshes/actors");
        let animation_data = repo.join("extracted/skyrimse/meshes/animationdata");
        if !actors.exists() {
            return;
        }
        let catalog =
            load_extracted_skyrim_creature_motion_catalog(&actors, &animation_data, &[]).unwrap();
        eprintln!(
            "Skyrim creature motion accounting: {:?}",
            catalog.accounting
        );
        assert_eq!(catalog.accounting.families, 46);
        assert_eq!(catalog.accounting.unique_clips, 2_460);
        assert_eq!(catalog.accounting.paired_clips, 99);
        assert_eq!(
            catalog.accounting.role_clips
                + catalog.accounting.paired_clips
                + catalog.accounting.overlay_clips
                + catalog.accounting.unsupported_clips
                + catalog.accounting.ambiguous_clips,
            2_460
        );
        assert!(catalog.clips.iter().all(|clip| !clip.family_ids.is_empty()));
        assert_eq!(
            catalog
                .clips
                .iter()
                .filter(|clip| {
                    matches!(
                        clip.root_motion,
                        SkyrimRootMotion::Stationary {
                            source: SkyrimRootMotionSource::HavokReferenceFrame,
                            ..
                        } | SkyrimRootMotion::Sampled {
                            source: SkyrimRootMotionSource::HavokReferenceFrame,
                            ..
                        }
                    )
                })
                .count(),
            13
        );
        assert!(
            catalog
                .clips
                .iter()
                .any(|clip| matches!(clip.root_motion, SkyrimRootMotion::Sampled { .. }))
        );
    }

    #[test]
    fn optional_installed_merged_corpus_reports_exact_family_readiness() {
        let Some(data_dir) = std::env::var_os("SKYRIMSE_CREATURE_DATA_DIR").map(PathBuf::from)
        else {
            return;
        };
        let verbose_diagnostics =
            std::env::var_os("SKYRIMSE_CREATURE_DIAGNOSTIC_VERBOSE").is_some();
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let actors = repo.join("extracted/skyrimse/meshes/actors");
        let animation_data = repo.join("extracted/skyrimse/meshes/animationdata");
        if !actors.is_dir() {
            return;
        }
        let interner = crate::sym::StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("skyrimse").unwrap();
        let mut records_by_key = std::collections::HashMap::new();
        for plugin in [
            "Skyrim.esm",
            "Update.esm",
            "Dawnguard.esm",
            "HearthFires.esm",
            "Dragonborn.esm",
        ] {
            let path = data_dir.join(plugin);
            if !path.is_file() {
                continue;
            }
            let handle = esp_authoring_core::plugin_runtime::plugin_handle_load_no_py(
                path.to_str().unwrap(),
                Some("skyrimse"),
                None,
                None,
                true,
            )
            .unwrap();
            for signature in [
                "KYWD", "RACE", "NPC_", "LVLN", "ARMO", "ARMA", "BPTD", "SPEL", "SHOU",
            ] {
                let sig = crate::ids::SigCode::from_str(signature).unwrap();
                for form_key in
                    crate::source_read::iter_form_keys_of_sig(handle, sig, &interner).unwrap()
                {
                    let record = crate::source_read::read_record_relayout_by_form_key(
                        handle, &form_key, &schema, &interner, None,
                    )
                    .unwrap();
                    records_by_key.insert(form_key, record);
                }
            }
            esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle);
        }
        let records = records_by_key.into_values().collect::<Vec<_>>();
        if let Some(wolf) = records.iter().find(|record| {
            record
                .eid
                .and_then(|value| interner.resolve(value))
                .is_some_and(|value| value.eq_ignore_ascii_case("WolfRace"))
        }) {
            eprintln!(
                "Skyrim WolfRace attack_field_count={}",
                wolf.fields
                    .iter()
                    .filter(|field| {
                        matches!(
                            field.sig.as_str(),
                            "ATKD" | "ATKE" | "AttackData" | "AttackEvent"
                        )
                    })
                    .count()
            );
        }
        let plan = super::super::creature_catalog::build_creature_corpus_plan(&records, &interner);
        if let Some(wolf) = plan.races.iter().find(|race| {
            race.editor_id
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("WolfRace"))
        }) {
            eprintln!(
                "Skyrim WolfRace plan project_paths={:?} events={:?} attack_contract_entries={} derived_attacks={:?}",
                wolf.project_paths,
                wolf.attack_events,
                wolf.attack_contract.len(),
                attack_evidence_from_contract(&wolf.attack_contract)
            );
        }
        assert_eq!(plan.summary.candidate_races, 121);
        // 109, not 110: excluding testDraugrRace also removes the catalog family its
        // one-off DLC01\SkeletonWarrior.nif skeleton kept to itself.
        assert_eq!(plan.summary.family_count, 109);
        let race_attacks = race_motion_evidence_from_creature_plan(&plan);
        let catalog =
            load_extracted_skyrim_creature_motion_catalog(&actors, &animation_data, &race_attacks)
                .unwrap();
        assert_eq!(catalog.accounting.families, 46);
        for (family_id, event) in [
            ("benthic_lurker", "attackPowerStart_Stomp"),
            ("giant", "attackPowerStart_Stomp"),
            ("netch", "attackStartRight"),
            ("werewolf", "AttackStartLeftRunningPower"),
            ("werewolf", "AttackStartBackHand"),
        ] {
            let graph = catalog.capability_graph_manifest(family_id).unwrap();
            assert!(
                graph.roles.iter().any(|role| {
                    role.role == CreatureClipRole::MeleeAttack
                        && role
                            .trigger_event
                            .iter()
                            .chain(&role.trigger_aliases)
                            .any(|trigger| trigger.eq_ignore_ascii_case(event))
                }),
                "family {family_id} does not execute source melee event {event:?}"
            );
        }
        if let Ok(requested_family) = std::env::var("SKYRIMSE_CREATURE_DIAGNOSTIC_FAMILY")
            && let Some(family) = catalog.family(&requested_family)
        {
            let family_race_evidence = race_attacks
                .iter()
                .filter(|evidence| {
                    project_path_key(&evidence.project_path)
                        == project_path_key(&family.project_path)
                })
                .collect::<Vec<_>>();
            eprintln!(
                "Skyrim diagnostic family={} clips={} inventory={:?} race_evidence={family_race_evidence:?}",
                family.family_id,
                family.clip_ids.len(),
                catalog.inventory(&requested_family).map(|inventory| (
                    inventory.character_paths.len(),
                    inventory.behavior_paths.len(),
                    inventory.animation_skeleton_paths.len(),
                    inventory.ragdoll_paths.len(),
                ))
            );
            if verbose_diagnostics {
                if let Some(inventory) = catalog.inventory(&requested_family) {
                    eprintln!(
                        "Skyrim diagnostic inventory paths characters={:?} animation_skeletons={:?} controllers={:?}",
                        inventory.character_paths,
                        inventory.animation_skeleton_paths,
                        inventory.controllers
                    );
                }
                let source_races = family_race_evidence
                    .iter()
                    .flat_map(|evidence| evidence.attacks.iter().map(|attack| attack.source_race))
                    .collect::<Vec<_>>();
                for race in plan
                    .races
                    .iter()
                    .filter(|race| source_races.contains(&race.source_race))
                {
                    eprintln!(
                        "Skyrim diagnostic RACE={} editor_id={:?} attack_data={:#?}",
                        race.source_race.format(&interner),
                        race.editor_id,
                        race.attack_data
                    );
                }
            }
            if let Some(inventory) = catalog.inventory(&requested_family) {
                eprintln!(
                    "Skyrim diagnostic behavior group clip sets={:?}",
                    inventory
                        .behavior_groups
                        .iter()
                        .map(|group| (
                            behavior_group_sort_key(group),
                            behavior_group_clip_paths(group)
                        ))
                        .collect::<Vec<_>>()
                );
                eprintln!(
                    "Skyrim diagnostic behavior groups={:#?}",
                    inventory.behavior_groups
                );
                for behavior_path in &inventory.behavior_paths {
                    let relative = behavior_path
                        .strip_prefix("Actors\\")
                        .unwrap_or(behavior_path);
                    let file =
                        HkxFile::read(&std::fs::read(actors.join(relative)).unwrap()).unwrap();
                    if verbose_diagnostics {
                        for (index, object) in
                            file.objects().iter().enumerate().filter(|(_, object)| {
                                matches!(
                                    object.class_name.as_str(),
                                    "hkbModifierGenerator"
                                        | "hkbBehaviorGraphData"
                                        | "hkbVariableValueSet"
                                )
                            })
                        {
                            eprintln!(
                                "Skyrim diagnostic raw behavior object behavior={} index={} class={} members={:?}",
                                behavior_path, index, object.class_name, object.members
                            );
                        }
                    }
                    if let Some(root) = file
                        .objects()
                        .iter()
                        .find(|object| object.class_name == "hkbBehaviorGraph")
                        .and_then(|object| pointer_member(object, "rootGenerator"))
                    {
                        let mut initial_states = BTreeSet::new();
                        collect_initial_state_chain(
                            &file,
                            root,
                            &mut BTreeSet::new(),
                            &mut initial_states,
                        );
                        eprintln!(
                            "Skyrim diagnostic behavior={} root_generator={:?} initial_states={:#?}",
                            behavior_path,
                            file.objects()
                                .get(root)
                                .map(|object| (&object.class_name, string_member(object, "name"))),
                            initial_states
                                .iter()
                                .filter_map(|index| file.objects().get(*index))
                                .map(|state| (
                                    string_member(state, "name"),
                                    pointer_member(state, "generator")
                                        .and_then(|generator| file.objects().get(generator))
                                        .map(|object| (
                                            &object.class_name,
                                            string_member(object, "name")
                                        ))
                                ))
                                .collect::<Vec<_>>()
                        );
                    }
                    for (index, object) in
                        file.objects().iter().enumerate().filter(|(_, object)| {
                            ["name", "animationName"]
                                .iter()
                                .filter_map(|member| string_member(object, member))
                                .any(|value| value.to_ascii_lowercase().contains("idleswim"))
                        })
                    {
                        eprintln!(
                            "Skyrim diagnostic idleswim object behavior={} index={} class={} signature=0x{:08x} members={:?}",
                            behavior_path,
                            index,
                            object.class_name,
                            object.signature,
                            object.members
                        );
                    }
                    for class_name in [
                        "hkbBehaviorGraph",
                        "hkbBehaviorGraphStringData",
                        "hkbStateMachine",
                        "hkbStateMachineStateInfo",
                        "hkbStateMachineTransitionInfoArray",
                        "hkbModifierGenerator",
                        "hkbBehaviorReferenceGenerator",
                        "hkbManualSelectorGenerator",
                        "hkbBlenderGenerator",
                        "hkbVariableBindingSet",
                        "BSiStateTaggingGenerator",
                        "hkbClipGenerator",
                    ] {
                        let objects = file
                            .objects()
                            .iter()
                            .filter(|object| object.class_name == class_name)
                            .collect::<Vec<_>>();
                        eprintln!(
                            "Skyrim diagnostic behavior={} class={} count={} first_named_clip={:?}",
                            behavior_path,
                            class_name,
                            objects.len(),
                            objects
                                .iter()
                                .find(|object| {
                                    string_member(object, "animationName")
                                        .is_some_and(|name| !name.is_empty())
                                })
                                .and_then(|object| string_member(object, "animationName"))
                        );
                        if let Some(object) = objects.first() {
                            let data_section = file.packfile().section("__data__").unwrap();
                            let object_relative = object.offset - data_section.offset;
                            let next_relative = file
                                .objects()
                                .iter()
                                .map(|candidate| candidate.offset - data_section.offset)
                                .filter(|offset| *offset > object_relative)
                                .min()
                                .unwrap_or(data_section.data1 - data_section.offset);
                            let local_fixups = file
                                .packfile()
                                .local_fixups
                                .iter()
                                .filter(|fixup| {
                                    let source = fixup.source as usize;
                                    source >= object_relative && source < next_relative
                                })
                                .map(|fixup| {
                                    let target = data_section.offset + fixup.target as usize;
                                    let tail = &file.source_bytes()[target..];
                                    let end = tail.iter().position(|byte| *byte == 0).unwrap_or(0);
                                    (
                                        fixup.source as usize - object_relative,
                                        String::from_utf8_lossy(&tail[..end]).into_owned(),
                                    )
                                })
                                .collect::<Vec<_>>();
                            let global_fixups = file
                                .packfile()
                                .global_fixups
                                .iter()
                                .filter(|fixup| {
                                    let source = fixup.source as usize;
                                    source >= object_relative && source < next_relative
                                })
                                .map(|fixup| fixup.source as usize - object_relative)
                                .collect::<Vec<_>>();
                            eprintln!(
                                "Skyrim diagnostic layout class={} signature=0x{:08x} local={local_fixups:?} global={global_fixups:?}",
                                object.class_name, object.signature
                            );
                        }
                    }
                }
                for character_path in &inventory.character_paths {
                    let relative = character_path
                        .strip_prefix("Actors\\")
                        .unwrap_or(character_path);
                    let file =
                        HkxFile::read(&std::fs::read(actors.join(relative)).unwrap()).unwrap();
                    if let Some(strings) = file
                        .objects()
                        .iter()
                        .find(|object| object.class_name == "hkbCharacterStringData")
                    {
                        for member_name in [
                            "animationBundleNameData",
                            "animationBundleFilenameData",
                            "behaviorFilename",
                        ] {
                            eprintln!(
                                "Skyrim diagnostic character={} class={} signature=0x{:08x} member={} value={:?}",
                                character_path,
                                strings.class_name,
                                strings.signature,
                                member_name,
                                strings
                                    .members
                                    .iter()
                                    .find(|member| member.name == member_name)
                                    .map(|member| &member.value)
                            );
                        }
                    }
                }
            }
            if verbose_diagnostics {
                for clip_id in &family.clip_ids {
                    eprintln!(
                        "Skyrim diagnostic clip={:#?}",
                        catalog.clip(clip_id).expect("family clip exists")
                    );
                }
            }
            for clip_id in &family.clip_ids {
                let clip = catalog.clip(clip_id).expect("family clip exists");
                if matches!(
                    &clip.disposition,
                    SkyrimClipDisposition::Role {
                        role: SkyrimCreatureMotionRole::SwimIdle
                            | SkyrimCreatureMotionRole::SwimLocomotion,
                        ..
                    }
                ) {
                    eprintln!(
                        "Skyrim diagnostic swim clip family={} clip={} disposition={:?}",
                        family.family_id, clip.clip_id, clip.disposition
                    );
                }
            }
        }
        let readiness = catalog.family_readiness();
        let ready = readiness.iter().filter(|family| family.ready).count();
        eprintln!("Skyrim merged motion ready={ready}/46");
        for family in readiness.iter().filter(|family| !family.ready) {
            eprintln!(
                "Skyrim merged motion blocked family={} template={:?} blockers={:?}",
                family.family_id, family.template, family.blockers
            );
        }
        let living = catalog.living_family_evidence();
        let living_ready = living
            .iter()
            .filter(|family| family.disposition == SkyrimLivingFamilyDisposition::Ready)
            .count();
        eprintln!("Skyrim living motion ready={living_ready}/46");
        for family in &living {
            let (graph, _) = catalog.capability_graph_bundle(&family.family_id).unwrap();
            let mut graph_clips = BTreeSet::new();
            for role in &graph.roles {
                let mut role_clips = BTreeSet::new();
                for clip_name in role.generator.clip_names(&role.clip_name) {
                    let key = clip_name.to_ascii_lowercase();
                    if role_clips.insert(key.clone()) {
                        assert!(
                            graph_clips.insert(key),
                            "family {} assigns clip {clip_name:?} to multiple runtime roles",
                            family.family_id
                        );
                    }
                }
            }
        }
        let template_failures = living
            .iter()
            .filter_map(|family| {
                catalog
                    .capability_graph_bundle(&family.family_id)
                    .err()
                    .map(|error| (family.family_id.clone(), error.to_string()))
            })
            .collect::<Vec<_>>();
        eprintln!(
            "Skyrim authored FO4 template ready={}/46 fatal={}",
            46 - template_failures.len(),
            template_failures.len(),
        );
        for (family_id, error) in &template_failures {
            eprintln!("Skyrim authored FO4 template blocked family={family_id} error={error}");
        }
        if std::env::var_os("SKYRIMSE_CREATURE_SWIM_CENSUS").is_some() {
            for family in living.iter().filter(|family| {
                family.required_roles.iter().any(|role| {
                    matches!(
                        role,
                        SkyrimRequiredRoleDisposition::MissingRole { alternatives }
                            if alternatives.contains(&SkyrimCreatureMotionRole::SwimIdle)
                    )
                })
            }) {
                let inventory = catalog
                    .inventory(&family.family_id)
                    .expect("living family inventory exists");
                let mut bound_swim_assets = inventory
                    .character_paths
                    .iter()
                    .flat_map(|character_path| {
                        let relative = character_path
                            .strip_prefix("Actors\\")
                            .unwrap_or(character_path);
                        let file =
                            HkxFile::read(&std::fs::read(actors.join(relative)).unwrap()).unwrap();
                        file.objects()
                            .iter()
                            .filter(|object| object.class_name == "hkbCharacterStringData")
                            .flat_map(|object| {
                                array_member(object, "animationBundleNameData")
                                    .into_iter()
                                    .flatten()
                            })
                            .filter_map(HkxValue::as_object_members)
                            .filter_map(|members| string_members(members, "bundleName"))
                            .filter(|name| name.to_ascii_lowercase().contains("swim"))
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                bound_swim_assets.sort_by_key(|path| path.to_ascii_lowercase());
                bound_swim_assets.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
                eprintln!(
                    "Skyrim missing SwimIdle family={} bound_swim_assets={bound_swim_assets:?}",
                    family.family_id
                );
            }
        }
        for family in living
            .iter()
            .filter(|family| family.disposition == SkyrimLivingFamilyDisposition::Blocked)
        {
            if std::env::var("SKYRIMSE_CREATURE_DIAGNOSTIC_FAMILY")
                .ok()
                .is_some_and(|requested| requested == family.family_id)
                && verbose_diagnostics
            {
                eprintln!("Skyrim diagnostic living family={family:#?}");
                for role in &family.required_roles {
                    if let SkyrimRequiredRoleDisposition::AmbiguousRole {
                        alternatives,
                        candidates,
                    } = role
                    {
                        eprintln!(
                            "Skyrim diagnostic ambiguous role alternatives={alternatives:?} candidates={:?}",
                            candidates
                                .iter()
                                .map(|candidate| (
                                    &candidate.clip_id,
                                    candidate.role,
                                    &catalog
                                        .clip(&candidate.clip_id)
                                        .expect("living candidate clip exists")
                                        .disposition,
                                    &candidate.triggers,
                                    &candidate.role_locators,
                                ))
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
            if family.family_id == "chicken" && verbose_diagnostics {
                eprintln!("Skyrim chicken passive evidence={family:#?}");
            }
            let template = match &family.template {
                SkyrimLivingTemplateDisposition::Proven { template, .. } => {
                    format!("proven:{template:?}")
                }
                SkyrimLivingTemplateDisposition::Ambiguous { candidates, .. } => {
                    format!("ambiguous:{candidates:?}")
                }
                SkyrimLivingTemplateDisposition::Unsupported { evidenced_roles } => {
                    format!("unsupported:{evidenced_roles:?}")
                }
            };
            for role in &family.required_roles {
                if verbose_diagnostics
                    && let SkyrimRequiredRoleDisposition::AmbiguousTrigger {
                        alternatives,
                        candidates,
                    } = role
                {
                    eprintln!(
                        "Skyrim living trigger aliases family={} alternatives={alternatives:?} candidates={:?}",
                        family.family_id,
                        candidates
                            .iter()
                            .map(|candidate| (&candidate.clip_id, &candidate.triggers))
                            .collect::<Vec<_>>()
                    );
                }
            }
            let required = family
                .required_roles
                .iter()
                .map(|role| match role {
                    SkyrimRequiredRoleDisposition::Ready { alternatives, .. } => {
                        format!("ready:{alternatives:?}")
                    }
                    SkyrimRequiredRoleDisposition::MissingRole { alternatives } => {
                        format!("missing_role:{alternatives:?}")
                    }
                    SkyrimRequiredRoleDisposition::AmbiguousRole { alternatives, .. } => {
                        format!("ambiguous_role:{alternatives:?}")
                    }
                    SkyrimRequiredRoleDisposition::MissingTrigger { alternatives, .. } => {
                        format!("missing_trigger:{alternatives:?}")
                    }
                    SkyrimRequiredRoleDisposition::AmbiguousTrigger { alternatives, .. } => {
                        format!("ambiguous_trigger:{alternatives:?}")
                    }
                    SkyrimRequiredRoleDisposition::UnknownRootMotion { alternatives, .. } => {
                        format!("unknown_root_motion:{alternatives:?}")
                    }
                    SkyrimRequiredRoleDisposition::UnsupportedRootMotion {
                        alternatives, ..
                    } => format!("unsupported_root_motion:{alternatives:?}"),
                })
                .collect::<Vec<_>>();
            eprintln!(
                "Skyrim living motion blocked family={} template={} required={:?}",
                family.family_id, template, required
            );
        }
        assert_eq!(readiness.len(), 46);
        assert_eq!(living.len(), 46);
        assert!(
            template_failures.is_empty(),
            "all installed Skyrim motion families must lower into a deterministic FO4 template"
        );
    }
}
