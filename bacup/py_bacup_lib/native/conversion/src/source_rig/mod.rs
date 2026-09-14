//! Source-rig creature contracts and deterministic FO4 Havok scaffolds.
//!
//! This module emits and packs the project, character, root behavior, and core
//! behavior files for capability-driven locomotion/combat slices, with an
//! explicit source-owned ragdoll or no-ragdoll runtime disposition.

mod admission;
mod ancillary_npc;
mod bridge;
mod corpus;
mod emit;
mod execution;
mod manifest;
mod pack;
pub mod race_data;
mod recipe;
mod record_batch;
mod record_builder;
mod records;
mod runtime_model_closure;
mod xml;

pub use admission::{
    ActorActionKind, ActorActionRequirement, BehaviorAdmissionIssue, BehaviorAdmissionReport,
    BehaviorAdmissionSeverity, CreatureActorActionRecordPlan, CreatureDegradationReceipt,
    CreatureFallbackDisposition, actor_action_admission_report, deterministic_standard_event,
    required_actor_action_records,
};
pub use bridge::{
    SOURCE_RIG_BRIDGE_VERSION, SourceRigBridgeError, SourceRigConvertedArtifactRequestReceipt,
    SourceRigPairLedgerEvidence, SourceRigRecipeBridgeInput, SourceRigRecipeBridgeReceipt,
    SourceRigSelectedFamilyEvidence, build_source_rig_executable_recipe,
};
pub use corpus::{
    CapabilityLedger, CreatureCapability, CreatureCorpusCandidate, CreatureCorpusPlan,
    CreatureCorpusPlanError, CreatureReadiness, CreatureRejectionReason, Fo4RaceDataField,
    Fo4RaceDataTarget, Fo4RaceFlag, Fo4RaceFlag2, Fo4RaceSize, MotionAttack, MotionAttackKind,
    MotionOverlayEvidence, MotionRigEvidence, MotionSet, PlannedCreature, RaceDataMapping,
    RaceDataValidationError, RecordVariant, RejectedCreature, RigFamily, SourceCreatureIdentity,
};
pub use emit::{
    ScaffoldArtifact, SourceRigScaffold, emit_capability_scaffold, emit_idle_scaffold,
    emit_mvp_scaffold, emit_mvp_scaffold_with_motion,
};
pub use execution::{
    PREPARED_SOURCE_RIG_EXECUTION_VERSION, PreparedRecordFamilyIntentReceipt,
    PreparedScaffoldArtifactReceipt, PreparedSourceRigExecution, PreparedSourceRigExecutionReceipt,
    SourceRigArtifactInput, SourceRigArtifactProvenance, SourceRigArtifactReceipt,
    SourceRigArtifactRole, SourceRigCreatureClosureInput, SourceRigExecutionError,
    SourceRigRuntimeModelArtifactInput, prepare_source_rig_execution,
    prepare_source_rig_execution_with_runtime_models,
};
pub use manifest::{
    BoneDecl, CapabilityBlenderChild, CapabilityCandidateAttackBinding, CapabilityClipRole,
    CapabilityGeneratorAnonymousSourceObject, CapabilityGeneratorBlenderChild,
    CapabilityGeneratorBlendingTransitionEffect, CapabilityGeneratorEventProperty,
    CapabilityGeneratorEventRef, CapabilityGeneratorExpression, CapabilityGeneratorExpressionArray,
    CapabilityGeneratorExpressionVariableReference, CapabilityGeneratorInterval,
    CapabilityGeneratorNode, CapabilityGeneratorSourceObject, CapabilityGeneratorState,
    CapabilityGeneratorTransition, CapabilityGeneratorTransitionArray,
    CapabilityGeneratorVariableBinding, CapabilityGraphManifest, CapabilityModifierNode,
    CapabilityRoleGenerator, CapabilitySourceVariableType, CapabilityVariableBindingType, Capsule,
    ClipBinding, ClipDecl, ClipMotionPolicy, CreatureClipRole, CreatureControllerDecl,
    CreatureGraphTemplate, CreatureManifest, EventDecl, EventUsage, GraphDeclarations,
    MvpGraphManifest, MvpMotionManifest, NoRagdollReason, OverlayClipRole,
    POWERED_RAGDOLL_INERT_PROOF_SCHEMA, PoweredRagdollInertFieldReceipt, PoweredRagdollSourceField,
    PoweredRagdollTargetControlPolicy, PropertyDecl, RagdollDisposition, ScaffoldPaths,
    SkeletonDecl, SourceOwnedPoweredRagdollConfig, SourceOwnedRagdollReceipt,
    SourcePoweredRagdollEvidence, SourcePoweredRagdollFeedbackEvidence,
    SourcePoweredRagdollPoseEvidence, ValidationError, ValidationErrors, VariableDecl,
    VariableType, VariableValue,
};
pub(crate) use manifest::{RAGDOLL_ENTER_EVENTS, RAGDOLL_TRANSITION_EVENTS};
pub use pack::{
    PackedScaffoldArtifact, SourceRigPackError, SourceRigPackReport, SourceRigRuntimeManifest,
    pack_capability_scaffold, pack_idle_scaffold, pack_mvp_scaffold, pack_mvp_scaffold_with_motion,
};
pub(crate) use recipe::projection_dependencies;
pub use recipe::{
    SOURCE_RIG_RECIPE_VERSION, SourceRigExecutableRecipe, SourceRigFieldDecision,
    SourceRigFieldReceipt, SourceRigRecipeError, SourceRigRecordBatchIntent,
};
pub use record_batch::{
    CREATURE_RECORD_COMMIT_LEDGER_RELATIVE_PATH, CREATURE_RECORD_COMMIT_LEDGER_VERSION,
    CommittedCreatureRecordBatch, CreatureEncodedRecordCommitment, CreaturePrimaryRecordMapping,
    CreatureProjectedRecordCommitment, CreatureRaceRecordReceipt, CreatureRecordBatchError,
    CreatureRecordBatchReceipt, CreatureRecordBatchStage, CreatureRecordCommitIntent,
    CreatureRecordCommitLedger, CreatureRecordCommitLedgerError, CreatureRecordFamilyBatch,
    CreatureRecordFamilyReceipt, CreatureRecordRecipeBinding, CreatureTargetDependencyCommitment,
    PreparedCreatureRecordBatch, PreparedCreatureRecordCommit,
    merge_creature_record_family_batches, prepare_creature_record_commit,
    prepare_creature_record_commit_batch, prepare_creature_record_families_batch,
    publish_creature_record_families_batch, publish_creature_record_families_batch_for_target,
    publish_creature_record_family_batch, read_and_validate_creature_record_commit_ledger,
    stage_creature_record_commit_ledger,
};
pub use record_builder::{
    CreatureMeleeRecordKeyPlan, CreatureNpcRecordKeyPlan, CreatureRecordBatchBuildError,
    CreatureRecordKeyPlan, CreatureRecordKeyPlanRequest, CreatureRecordProjectionBuildInput,
    build_creature_record_family_batches, plan_creature_record_form_keys,
};
pub use records::{
    ATTACK_SHOUT_TO_SPELL_PROOF_SCHEMA, ATTACK_STAGGER_OFFSET_ZERO_PROOF_SCHEMA,
    ATTACK_STAMINA_TO_ACTION_POINTS_PROOF_SCHEMA, ATTACK_TYPE_RUNTIME_INERT_PROOF_SCHEMA,
    CreatureAttackRecordProjection, CreatureAttackRecordVariant, CreatureAttackSourceDataReceipt,
    CreatureAttackSpellPolicy, CreatureAttackStaggerOffsetPolicy, CreatureAttackStaminaPolicy,
    CreatureAttackTargetData, CreatureAttackTypePolicy, CreatureBodyNifRecordPart,
    CreatureBodyPartProjection, CreatureNpcInventoryEntry, CreatureNpcInventoryOwnership,
    CreatureNpcRecordVariant, CreatureRecordClosure, CreatureRecordEditorIds, CreatureRecordError,
    CreatureRecordFormKeys, CreatureRecordManifest, CreatureRecordProfile,
    CreatureRecordProjectionClosure, CreatureRecordProjectionManifest,
    CreatureSemanticProofReceipt, CreatureSourceRecordReference, CreatureTargetRecordReference,
    ProjectedRecordIdentity, RootBodyPartProfile, TargetFormKey,
    append_creature_actor_action_records, emit_creature_capability_record_projection,
    emit_creature_record_closure, emit_creature_record_projection,
};
pub use runtime_model_closure::{
    SOURCE_RIG_RUNTIME_MODEL_CLOSURE_VERSION, SourceRigRuntimeModelArtifactKind,
    SourceRigRuntimeModelArtifactReceipt, SourceRigRuntimeModelClosureError,
    SourceRigRuntimeModelClosureReceipt, SourceRigRuntimeModelRowExpectation,
    SourceRigRuntimeModelRowKey, SourceRigRuntimeModelRowReceipt,
    validate_source_rig_runtime_model_closure,
};
pub use xml::{HavokXmlFacts, validate_fo4_havok_xml_signatures, validate_havok_xml};

#[cfg(test)]
pub(crate) mod tests;
pub use ancillary_npc::{
    CREATURE_ANCILLARY_NPC_COMMIT_LEDGER_VERSION, CREATURE_ANCILLARY_NPC_FACEGEN_PIPELINE_ID,
    CREATURE_ANCILLARY_NPC_FAMILY_ID, CREATURE_ANCILLARY_NPC_LEDGER_VERSION,
    CreatureAncillaryNpcAppearancePartKind, CreatureAncillaryNpcAppearancePartReceipt,
    CreatureAncillaryNpcAppearanceProjection, CreatureAncillaryNpcArtifactKind,
    CreatureAncillaryNpcArtifactReceipt, CreatureAncillaryNpcCommitLedger,
    CreatureAncillaryNpcEffectiveField, CreatureAncillaryNpcEffectiveFieldReceipt,
    CreatureAncillaryNpcError, CreatureAncillaryNpcFacegenDisposition,
    CreatureAncillaryNpcFacegenGenerationReceipt, CreatureAncillaryNpcProjectionLedger,
    CreatureAncillaryNpcProjectionReceipt, CreatureAncillaryNpcRequestId,
    CreatureAncillaryNpcReservedIdentity, CreatureAncillaryNpcSex,
    CreatureAncillaryNpcSourceArtifactKind, CreatureAncillaryNpcSourceArtifactReceipt,
    CreatureAncillaryNpcTemplateReceipt, PreparedCreatureAncillaryNpcBatch,
    build_creature_ancillary_npc_family_batch,
    creature_ancillary_npc_assetless_facegen_proof_blake3,
    creature_ancillary_npc_facegen_artifacts_blake3,
    creature_ancillary_npc_facegen_generation_receipt, creature_ancillary_npc_morph_inputs_blake3,
    prepare_creature_ancillary_npc_batch, validate_creature_ancillary_npc_artifacts,
};
