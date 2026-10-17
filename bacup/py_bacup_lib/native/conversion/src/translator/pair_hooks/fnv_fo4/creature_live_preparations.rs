//! Live, read-only FNV/FO3 candidate preparation for the shared creature MVP.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use havok_native::convert::creature_ragdoll::{
    lower_primitive_shapes_to_fo4_convex_hulls, reconstruct_fo4_creature_ragdoll_packfile,
};
use nif_core_native::convert_file::{ConvertFileOptions, convert_nif_file};
use nif_core_native::creature_closure::{
    CreatureClosureReceipt, CreatureClosureRequest, CreatureNifInput, CreatureNifRole,
    refresh_staged_creature_nif_artifact, seal_embedded_creature_collision,
    stage_creature_nif_closure_with_body_parts,
};
use nif_core_native::creature_ragdoll::{
    extract_fnv_fo3_creature_ragdoll, install_fo4_creature_controller_collision,
    install_fo4_creature_ragdoll_collision,
};
use nif_core_native::model::NifFile;
use thiserror::Error;

#[path = "creature_action_selection.rs"]
mod creature_action_selection;
#[path = "creature_automatic_selection.rs"]
mod creature_automatic_selection;

#[path = "creature_attack_selection.rs"]
mod creature_attack_selection;

#[path = "creature_channel_selection.rs"]
mod creature_channel_selection;

#[path = "creature_aim_selection.rs"]
mod creature_aim_selection;

#[path = "creature_movement_selection.rs"]
mod creature_movement_selection;

#[path = "creature_template_preparations.rs"]
mod creature_template_preparations;

#[path = "creature_actor_projection.rs"]
mod creature_actor_projection;

#[path = "creature_property_sequences.rs"]
mod creature_property_sequences;

pub(super) use creature_automatic_selection::validate_automatic_phase_projection;
pub(super) use creature_aim_selection::candidates as source_aim_candidates;

use crate::formkey_mapper::{FormKeyMapper, MapperState};
use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::run::{
    CreatureActorActionReservationReceipt, CreatureActorActionReservationRequest,
    CreatureAncillaryNpcReservation,
};
use crate::source_rig::{
    BoneDecl, CapabilityClipRole, CapabilityGraphManifest, CapabilityRoleGenerator, Capsule,
    ClipDecl, ClipMotionPolicy, CreatureActorActionRecordPlan, CreatureAttackRecordProjection,
    CreatureAttackRecordVariant, CreatureAttackTargetData, CreatureBodyNifRecordPart,
    CreatureBodyPartProjection, CreatureClipRole, CreatureControllerDecl, CreatureGraphTemplate,
    CreatureManifest, CreatureMeleeRecordKeyPlan, CreatureNpcInventoryEntry,
    CreatureNpcInventoryOwnership, CreatureNpcRecordKeyPlan, CreatureNpcRecordVariant,
    CreaturePrimaryRecordMapping, CreatureRecordEditorIds, CreatureRecordFormKeys,
    CreatureRecordKeyPlan, CreatureRecordManifest, CreatureRecordProjectionManifest,
    CreatureTargetRecordReference, EventDecl, EventUsage, GraphDeclarations, NoRagdollReason,
    OverlayClipRole, PropertyDecl, RaceDataMapping, RagdollDisposition,
    SOURCE_RIG_RUNTIME_MODEL_CLOSURE_VERSION, ScaffoldPaths, SkeletonDecl, SourceCreatureIdentity,
    SourceOwnedRagdollReceipt, SourceRigArtifactProvenance, SourceRigArtifactReceipt,
    SourceRigArtifactRole, SourceRigConvertedArtifactRequestReceipt, SourceRigExecutableRecipe,
    SourceRigFieldDecision, SourceRigRecordBatchIntent, SourceRigRuntimeModelArtifactKind,
    SourceRigRuntimeModelArtifactReceipt, SourceRigRuntimeModelClosureReceipt,
    SourceRigRuntimeModelRowExpectation, SourceRigRuntimeModelRowKey,
    SourceRigRuntimeModelRowReceipt, TargetFormKey, VariableDecl, VariableType, VariableValue,
    deterministic_standard_event, required_actor_action_records,
    validate_source_rig_runtime_model_closure,
};
use crate::source_rig::CreatureTemplateProjection;
use crate::sym::StringInterner;

use super::creature_catalog::{
    LegacyCreatureGame, LegacyRecordSource, RigFamilyKey, StableFormKey, game_data_root,
};
use super::creature_dependencies::{
    CreatureDependencyCandidateReceipt, DependencyReferenceResolution,
    FnvFo3CreatureDependencyLedger, RuntimeDependencyRecordReceipt,
};
use super::creature_live_builder::{
    FnvFo3LiveRecipeBuildError, FnvFo3LiveRecipeBuildInput, FnvFo3LiveRecipeEvidence,
    build_live_fnv_fo3_creature_recipe_evidence,
};
use super::creature_motion::{
    KfParseEvidence, MotionCandidate, MotionEventKind, MotionRole, RootMotionEvidence,
    stage_request_from_motion_candidate,
};
use super::creature_templates::{LegacyTemplateBranchReceipt, LegacyTemplateProfile};
use super::creature_mvp_adapter::{
    FnvFo3MvpAdapterError, FnvFo3MvpCandidateBlocker, FnvFo3MvpCandidateDestination,
    FnvFo3MvpConvertedArtifact, FnvFo3MvpPreparationLedger,
    build_fnv_fo3_mvp_candidate_preparations_with_live_blockers, canonical_recipe_source_key,
};
use super::creature_recipe::{
    CanonicalAssetClaim, CreatureFamilyRecipeJob, CreatureFamilyRecipeLedger,
    CreatureGraphCapability, CreatureRecordRecipeDisposition, FamilyRecipeDisposition,
    IndexedAssetKind, RagdollCapabilityEvidence,
};
use super::legacy_debris::{classify_legacy_debris, legacy_debris_model_rows};
use super::legacy_npc_appearance::{
    build_legacy_ancillary_npc_appearance_preparations,
    build_legacy_ancillary_npc_candidate_requests,
};
use super::legacy_race_appearance::{
    build_legacy_ancillary_race_face_assets, validate_legacy_target_appearance_closure,
};
use super::source_rig_bridge::{
    FnvFo3SourceRigAdapterInput, build_fnv_fo3_source_rig_bridge_input_prevalidated,
    canonical_fnv_fo3_family_id,
};

#[derive(Clone, Copy)]
pub struct FnvFo3LiveMvpPreparationBuildInput<'a> {
    pub winning_records: &'a [LegacyRecordSource<'a>],
    pub ancillary_npc_race_records: &'a [LegacyRecordSource<'a>],
    pub ancillary_npc_target_appearance_records: &'a [Record],
    pub dependency_ledger: &'a FnvFo3CreatureDependencyLedger,
    pub source_data_root: &'a Path,
    pub target_data_root: &'a Path,
    pub private_staging_root: &'a Path,
    pub mapper_state: &'a MapperState,
    pub compiled_script: &'a dyn Fn(FormKey, FormKey) -> Result<crate::source_rig::CreatureNpcScriptAttachment, String>,
    pub ancillary_npc_reservations: &'a [CreatureAncillaryNpcReservation],
    pub actor_action_reservations: &'a [CreatureActorActionReservationReceipt],
    pub destinations: &'a [FnvFo3MvpCandidateDestination],
    pub expected_creature_winners: Option<usize>,
    pub editor_id_prefix: &'a str,
}

#[derive(Debug, Error)]
pub enum FnvFo3LiveMvpPreparationBuildError {
    #[error(transparent)]
    Recipe(#[from] FnvFo3LiveRecipeBuildError),
    #[error(transparent)]
    Adapter(#[from] FnvFo3MvpAdapterError),
    #[error("invalid live FNV/FO3 preparation input: {0}")]
    Input(String),
}

pub(crate) fn fnv_fo3_actor_action_reservation_requests(
    evidence: &FnvFo3LiveRecipeEvidence,
    dependency_ledger: &FnvFo3CreatureDependencyLedger,
    editor_id_prefix: &str,
) -> Result<Vec<CreatureActorActionReservationRequest>, FnvFo3LiveMvpPreparationBuildError> {
    if editor_id_prefix.trim().is_empty()
        || !editor_id_prefix
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(FnvFo3LiveMvpPreparationBuildError::Input(
            "editor_id_prefix must be a nonempty ASCII EditorID prefix".to_string(),
        ));
    }
    let owners = actor_action_owners_by_family(evidence, dependency_ledger);
    let mut requests = Vec::new();
    let mut editor_ids = BTreeSet::new();
    for family in evidence
        .recipe_ledger
        .families
        .iter()
        .filter(|family| matches!(family.disposition, FamilyRecipeDisposition::Ready))
    {
        let family_id = canonical_fnv_fo3_family_id(&family.rig);
        if !owners.contains_key(&family_id) {
            continue;
        }
        let selected = select_family_graph(family).map_err(|error| {
            FnvFo3LiveMvpPreparationBuildError::Input(format!("family {family_id}: {error}"))
        })?;
        let graph = build_capability_graph(&selected, None)
            .map_err(FnvFo3LiveMvpPreparationBuildError::Input)?;
        let family_hash = blake3::hash(family_id.as_bytes()).to_hex().to_string();
        let root_behavior = super::creature_source_idles::source_family_behavior_path(&family.rig);
        for (ordinal, requirement) in required_actor_action_records(&graph, &root_behavior)
            .into_iter()
            .enumerate()
        {
            let editor_id = format!(
                "{editor_id_prefix}FnvFo3Action_{}_{}",
                &family_hash[..16],
                ordinal + 1
            );
            if !editor_ids.insert(editor_id.to_ascii_lowercase()) {
                return Err(FnvFo3LiveMvpPreparationBuildError::Input(format!(
                    "Actor Action EditorID collision for family {family_id}"
                )));
            }
            requests.push(CreatureActorActionReservationRequest {
                family_id: family_id.clone(),
                requirement,
                editor_id,
            });
        }
    }
    requests.sort_by(|left, right| {
        left.family_id
            .to_ascii_lowercase()
            .cmp(&right.family_id.to_ascii_lowercase())
            .then_with(|| left.requirement.cmp(&right.requirement))
            .then_with(|| left.editor_id.cmp(&right.editor_id))
    });
    Ok(requests)
}

fn actor_action_record_plans_by_family(
    evidence: &FnvFo3LiveRecipeEvidence,
    dependency_ledger: &FnvFo3CreatureDependencyLedger,
    reservations: &[CreatureActorActionReservationReceipt],
    editor_id_prefix: &str,
    interner: &StringInterner,
) -> Result<BTreeMap<String, Vec<CreatureActorActionRecordPlan>>, FnvFo3LiveMvpPreparationBuildError>
{
    let expected =
        fnv_fo3_actor_action_reservation_requests(evidence, dependency_ledger, editor_id_prefix)?;
    let mut receipts = BTreeMap::new();
    for receipt in reservations {
        let key = (
            receipt.family_id.clone(),
            receipt.requirement.clone(),
            receipt.editor_id.clone(),
        );
        if receipts.insert(key, receipt).is_some() {
            return Err(FnvFo3LiveMvpPreparationBuildError::Input(format!(
                "duplicate Actor Action reservation receipt for family {}",
                receipt.family_id
            )));
        }
    }
    if receipts.len() != expected.len() {
        return Err(FnvFo3LiveMvpPreparationBuildError::Input(format!(
            "Actor Action reservation count mismatch: expected {} received {}",
            expected.len(),
            receipts.len()
        )));
    }
    let mut plans = BTreeMap::<String, Vec<CreatureActorActionRecordPlan>>::new();
    for request in expected {
        let key = (
            request.family_id.clone(),
            request.requirement.clone(),
            request.editor_id.clone(),
        );
        let receipt = receipts.remove(&key).ok_or_else(|| {
            FnvFo3LiveMvpPreparationBuildError::Input(format!(
                "missing Actor Action reservation for family {} event {}",
                request.family_id, request.requirement.animation_event
            ))
        })?;
        let plan = receipt
            .record_plan(interner)
            .map_err(FnvFo3LiveMvpPreparationBuildError::Input)?;
        plans.entry(request.family_id).or_default().push(plan);
    }
    if let Some((key, _)) = receipts.into_iter().next() {
        return Err(FnvFo3LiveMvpPreparationBuildError::Input(format!(
            "unexpected Actor Action reservation for family {}",
            key.0
        )));
    }
    for plans in plans.values_mut() {
        plans.sort_by_key(|plan| {
            (
                plan.requirement.clone(),
                plan.editor_id.to_ascii_lowercase(),
                plan.form_key.local,
            )
        });
    }
    Ok(plans)
}

fn actor_action_owners_by_family(
    evidence: &FnvFo3LiveRecipeEvidence,
    dependency_ledger: &FnvFo3CreatureDependencyLedger,
) -> BTreeMap<String, StableFormKey> {
    let record_by_source = evidence
        .recipe_ledger
        .records
        .iter()
        .map(|record| (canonical_recipe_source_key(&record.source), record))
        .collect::<BTreeMap<_, _>>();
    let family_by_rig = evidence
        .recipe_ledger
        .families
        .iter()
        .map(|family| (family.rig.clone(), family))
        .collect::<BTreeMap<_, _>>();
    let mut owners = BTreeMap::<String, StableFormKey>::new();
    for candidate in dependency_ledger
        .candidates
        .iter()
        .filter(|candidate| candidate.blockers.is_empty())
    {
        let Some(record) = record_by_source.get(&canonical_recipe_source_key(&candidate.source))
        else {
            continue;
        };
        let rig = match &record.disposition {
            CreatureRecordRecipeDisposition::VisualOwner { rig, .. }
            | CreatureRecordRecipeDisposition::Proxy { rig, .. } => rig,
            CreatureRecordRecipeDisposition::RejectedProxy { .. }
            | CreatureRecordRecipeDisposition::Special { .. } => continue,
        };
        if !matches!(
            family_by_rig.get(rig).map(|family| &family.disposition),
            Some(FamilyRecipeDisposition::Ready)
        ) {
            continue;
        }
        owners
            .entry(canonical_fnv_fo3_family_id(rig))
            .and_modify(|owner| *owner = owner.clone().min(candidate.source.clone()))
            .or_insert_with(|| candidate.source.clone());
    }
    owners
}

#[derive(Clone)]
struct PreparedLiveFamily {
    replacement_runtime_paths: BTreeMap<String, String>,
    body_part_segments: Vec<nif_core_native::skin::creature_parts::CreatureBodyPartSegment>,
    closure: CreatureClosureReceipt,
    closure_staged_data_root: PathBuf,
    rig: CreatureManifest,
    graph: CapabilityGraphManifest,
    attack_weapon_sources: BTreeMap<String, Vec<StableFormKey>>,
    power_attack_events: BTreeSet<String>,
    natural_attack_seconds: BTreeMap<String, f32>,
    artifact_requests: Vec<SourceRigConvertedArtifactRequestReceipt>,
    artifact_receipts: Vec<SourceRigArtifactReceipt>,
    converted_artifacts: Vec<FnvFo3MvpConvertedArtifact>,
    body_runtime_paths: BTreeMap<String, String>,
}

#[derive(Clone)]
struct PreparedLegacyDebrisRecord {
    expectations: Vec<SourceRigRuntimeModelRowExpectation>,
    rows: Vec<SourceRigRuntimeModelRowReceipt>,
}

#[derive(Clone)]
struct SelectedClip {
    thrown_attack: bool,
    candidate: MotionCandidate,
    role: CreatureClipRole,
    name: String,
    state_name: String,
    trigger_event: Option<String>,
    target_looping: bool,
    locomotion_sample: Option<(f32, f32)>,
    source_idle: Option<CapabilityRoleGenerator>,
    source_aim: Option<CapabilityRoleGenerator>,
    source_attack: Option<crate::source_rig::SourceAttackContract>,
    attack_blend_only: bool,
    source_movement: Option<crate::source_rig::SourceMovementClip>,
    automatic_attack: Option<creature_automatic_selection::SelectedAutomaticAttack>,
    source_weapon: Option<StableFormKey>,
    equipment_index: Option<u8>,
}

#[derive(Clone)]
struct SelectedOverlay {
    candidate: MotionCandidate,
    name: String,
    clip_name: String,
    start_event: String,
    stop_event: String,
}

#[derive(Clone)]
struct SelectedGraph {
    source_channels: Vec<crate::source_rig::SourceAnimationClip>,
    template: CreatureGraphTemplate,
    clips: Vec<SelectedClip>,
    overlays: Vec<SelectedOverlay>,
    idle_event: String,
}

#[derive(Clone, Copy)]
struct ActionPointSettings {
    base: f32,
    multiplier: f32,
}

#[derive(Clone, Copy)]
struct LegacyCreatureStats {
    level: u16,
    health: u16,
    action_points: u16,
    damage: u16,
    reach: f32,
}

#[derive(Clone, Debug)]
struct LegacyAttackProjectionPolicy {
    damage_multiplier: f32,
    chance: f32,
    strike_angle: f32,
    action_point_cost: f32,
    target_data: CreatureAttackTargetData,
}

const TEMPLATE_TRAITS: u16 = 0x0001;
const TEMPLATE_STATS: u16 = 0x0002;
const TEMPLATE_SPELL_LIST: u16 = 0x0008;
const TEMPLATE_MODEL_ANIMATION: u16 = 0x0040;
const TEMPLATE_BASE_DATA: u16 = 0x0080;
const TEMPLATE_INVENTORY: u16 = 0x0100;

pub fn build_live_fnv_fo3_mvp_candidate_preparations(
    input: FnvFo3LiveMvpPreparationBuildInput<'_>,
    interner: &StringInterner,
) -> Result<FnvFo3MvpPreparationLedger, FnvFo3LiveMvpPreparationBuildError> {
    if input.editor_id_prefix.trim().is_empty() {
        return Err(FnvFo3LiveMvpPreparationBuildError::Input(
            "editor_id_prefix is empty".to_string(),
        ));
    }
    if input.private_staging_root == input.source_data_root
        || input.private_staging_root == input.target_data_root
        || [LegacyCreatureGame::Fnv, LegacyCreatureGame::Fo3]
            .into_iter()
            .any(|game| input.private_staging_root == game_data_root(input.source_data_root, game))
    {
        return Err(FnvFo3LiveMvpPreparationBuildError::Input(
            "private staging root aliases an immutable source or target data root".to_string(),
        ));
    }

    let evidence = build_live_fnv_fo3_creature_recipe_evidence(
        FnvFo3LiveRecipeBuildInput {
            winning_records: input.winning_records,
            dependency_ledger: input.dependency_ledger,
            source_data_root: input.source_data_root,
            expected_creature_winners: input.expected_creature_winners,
        },
        interner,
    )?;
    let shared_recipe_ledger = Arc::new(evidence.recipe_ledger.clone());
    let record_by_source = evidence
        .recipe_ledger
        .records
        .iter()
        .map(|record| (canonical_recipe_source_key(&record.source), record))
        .collect::<BTreeMap<_, _>>();
    let family_by_rig = evidence
        .recipe_ledger
        .families
        .iter()
        .map(|family| (family.rig.clone(), family))
        .collect::<BTreeMap<_, _>>();
    let winning_record_by_source = winning_creature_records(input.winning_records, interner)?;
    let winning_runtime_record_by_source =
        winning_runtime_records(input.winning_records, interner)?;
    let template_records_by_game = [LegacyCreatureGame::Fnv, LegacyCreatureGame::Fo3]
        .into_iter().map(|game| (game, winning_runtime_record_by_source.iter()
            .filter(|((source_game, _), record)| *source_game == game && matches!(record.sig.as_str(), "CREA" | "LVLC" | "LVLN"))
            .map(|((_, key), record)| (key.clone(), *record)).collect::<BTreeMap<_, _>>()))
        .collect::<BTreeMap<_, _>>();
    let ancillary_npc_appearances = build_legacy_ancillary_npc_appearance_preparations(
        input.ancillary_npc_reservations,
        &winning_runtime_record_by_source,
        interner,
    )
    .map_err(FnvFo3LiveMvpPreparationBuildError::Input)?;
    let expected_ancillary_npcs = input
        .dependency_ledger
        .records
        .iter()
        .filter(|record| record.signature == "NPC_")
        .count();
    if ancillary_npc_appearances.len() != expected_ancillary_npcs {
        return Err(FnvFo3LiveMvpPreparationBuildError::Input(format!(
            "ancillary NPC reservation accounting mismatch: expected={expected_ancillary_npcs} actual={}",
            ancillary_npc_appearances.len()
        )));
    }
    let _ancillary_npc_candidate_requests =
        build_legacy_ancillary_npc_candidate_requests(input.dependency_ledger)
            .map_err(FnvFo3LiveMvpPreparationBuildError::Input)?;
    let expected_ancillary_races = ancillary_npc_appearances
        .iter()
        .filter_map(|preparation| {
            preparation
                .evidence
                .as_ref()
                .ok()
                .map(|evidence| (preparation.game, evidence.race.clone()))
        })
        .collect::<BTreeSet<_>>();
    let _ancillary_race_face_assets = build_legacy_ancillary_race_face_assets(
        &expected_ancillary_races,
        input.ancillary_npc_race_records,
        interner,
    )
    .map_err(FnvFo3LiveMvpPreparationBuildError::Input)?;
    let expected_target_appearance_races = expected_ancillary_races
        .iter()
        .map(|(_, source)| {
            let source_key = FormKey {
                local: source.local,
                plugin: interner.intern(&source.plugin),
            };
            let target = input
                .mapper_state
                .source_to_target
                .get(&source_key)
                .copied()
                .ok_or_else(|| {
                    FnvFo3LiveMvpPreparationBuildError::Input(format!(
                        "ancillary NPC source RACE {source} has no target mapping"
                    ))
                })?;
            let plugin = interner.resolve(target.plugin).ok_or_else(|| {
                FnvFo3LiveMvpPreparationBuildError::Input(format!(
                    "ancillary NPC target RACE plugin for {source} is unresolved"
                ))
            })?;
            Ok(StableFormKey {
                local: target.local,
                plugin: plugin.to_string(),
            })
        })
        .collect::<Result<BTreeSet<_>, FnvFo3LiveMvpPreparationBuildError>>()?;
    let _target_appearance_closure = validate_legacy_target_appearance_closure(
        &expected_target_appearance_races,
        input.ancillary_npc_target_appearance_records,
        interner,
    )
    .map_err(FnvFo3LiveMvpPreparationBuildError::Input)?;
    let action_point_settings = load_action_point_settings(input.winning_records, interner);
    let dependency_record_by_source = input
        .dependency_ledger
        .records
        .iter()
        .map(|record| (record.source.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mut next_mapper_state = input.mapper_state.clone();
    let mut allocator = FormKeyMapper::from_state(&mut next_mapper_state, interner);
    let target_plugin = input.mapper_state.options.output_plugin_name.clone();
    let mut family_cache = BTreeMap::<RigFamilyKey, Result<PreparedLiveFamily, String>>::new();
    let mut runtime_model_cache = BTreeMap::<
        (LegacyCreatureGame, StableFormKey),
        Result<PreparedLegacyDebrisRecord, String>,
    >::new();
    let actor_action_owners = actor_action_owners_by_family(&evidence, input.dependency_ledger);
    let actor_action_plans = actor_action_record_plans_by_family(
        &evidence,
        input.dependency_ledger,
        input.actor_action_reservations,
        input.editor_id_prefix,
        interner,
    )?;
    if actor_action_owners.len() != actor_action_plans.len()
        || actor_action_owners
            .keys()
            .any(|family_id| !actor_action_plans.contains_key(family_id))
    {
        return Err(FnvFo3LiveMvpPreparationBuildError::Input(
            "Ready Actor Action family reservation has no deterministic creature owner".to_string(),
        ));
    }
    let mut candidate_inputs = Vec::new();

    let mut live_blockers = BTreeMap::<StableFormKey, Vec<FnvFo3MvpCandidateBlocker>>::new();
    for candidate in input
        .dependency_ledger
        .candidates
        .iter()
        .filter(|candidate| candidate.blockers.is_empty())
    {
        let recipe_source = canonical_recipe_source_key(&candidate.source);
        let Some(record) = record_by_source.get(&recipe_source) else {
            continue;
        };
        match creature_template_preparations::prepare(
            input, candidate, record, &evidence, &shared_recipe_ledger,
            &winning_record_by_source, &winning_runtime_record_by_source,
            &template_records_by_game[&record.provenance.game],
            &dependency_record_by_source, action_point_settings.get(&record.provenance.game).copied(),
            &actor_action_owners, &actor_action_plans,
            &mut family_cache, &mut runtime_model_cache, &mut allocator, interner,
        ) {
            Ok(Some(preparations)) => {
                candidate_inputs.extend(preparations);
                continue;
            }
            Ok(None) => {}
            Err(detail) => {
                live_blockers.entry(candidate.source.clone()).or_default()
                    .push(FnvFo3MvpCandidateBlocker::RecordProjection { detail });
                continue;
            }
        }
        let rig = match &record.disposition {
            CreatureRecordRecipeDisposition::VisualOwner { rig, .. }
            | CreatureRecordRecipeDisposition::Proxy { rig, .. } => rig,
            CreatureRecordRecipeDisposition::RejectedProxy { .. }
            | CreatureRecordRecipeDisposition::Special { .. } => continue,
        };
        if !matches!(
            family_by_rig.get(rig).map(|family| &family.disposition),
            Some(FamilyRecipeDisposition::Ready)
        ) {
            continue;
        }
        let reservation = match primary_reservation(&candidate.source, input.mapper_state, interner)
        {
            Ok(reservation) => reservation,
            Err(blocker) => {
                live_blockers
                    .entry(candidate.source.clone())
                    .or_default()
                    .push(blocker);
                continue;
            }
        };
        let Some(source_record) = winning_record_by_source.get(&candidate.source).copied() else {
            live_blockers
                .entry(candidate.source.clone())
                .or_default()
                .push(FnvFo3MvpCandidateBlocker::RecordProjection {
                    detail: "winning CREA source record is unavailable to the live builder"
                        .to_string(),
                });
            continue;
        };
        let Some(settings) = action_point_settings.get(&record.provenance.game).copied() else {
            live_blockers
                .entry(candidate.source.clone())
                .or_default()
                .push(FnvFo3MvpCandidateBlocker::RecordProjection {
                    detail: format!(
                        "missing exact {:?} fAVDActionPointsBase/fAVDActionPointsMult winners",
                        record.provenance.game
                    ),
                });
            continue;
        };
        let proxy_owner = singular_proxy_owner(record);
        let inherited_stats = match resolve_template_group_record(
                &candidate.source,
                TEMPLATE_STATS,
                &winning_record_by_source,
                proxy_owner,
                interner,
            ) {
            Ok((_, stats)) => stats,
            Err(detail) => {
                live_blockers
                    .entry(candidate.source.clone())
                    .or_default()
                    .push(FnvFo3MvpCandidateBlocker::RecordProjection { detail });
                continue;
            }
        };
        let stats = match legacy_creature_stats(
            inherited_stats,
            settings,
            interner,
        ) {
            Ok(stats) => stats,
            Err(detail) => {
                live_blockers
                    .entry(candidate.source.clone())
                    .or_default()
                    .push(FnvFo3MvpCandidateBlocker::RecordProjection { detail });
                continue;
            }
        };
        let family_job = *family_by_rig
            .get(rig)
            .expect("Ready family was resolved above");
        let family_id = canonical_fnv_fo3_family_id(&family_job.rig);
        let actor_action_records = if actor_action_owners.get(&family_id) == Some(&candidate.source)
        {
            actor_action_plans.get(&family_id).cloned().ok_or_else(|| {
                FnvFo3LiveMvpPreparationBuildError::Input(format!(
                    "missing Actor Action reservations for Ready family {family_id}"
                ))
            })?
        } else {
            Vec::new()
        };
        let family_assets = family_cache.entry(rig.clone()).or_insert_with(|| {
            prepare_live_family(
                family_job,
                &evidence,
                input.source_data_root,
                input.private_staging_root,
                &winning_runtime_record_by_source,
                interner,
            )
        });
        let family_assets = match family_assets {
            Ok(assets) => assets,
            Err(detail) => {
                live_blockers
                    .entry(candidate.source.clone())
                    .or_default()
                    .push(FnvFo3MvpCandidateBlocker::AssetPreparation {
                        detail: detail.clone(),
                    });
                continue;
            }
        };
        match prepare_live_candidate(
            candidate,
            record,
            family_job,
            source_record,
            stats,
            reservation,
            family_assets,
            &evidence,
            &shared_recipe_ledger,
            &winning_record_by_source,
            &dependency_record_by_source,
            input.dependency_ledger,
            &winning_runtime_record_by_source,
            &mut runtime_model_cache,
            input.source_data_root,
            input.private_staging_root,
            input.mapper_state,
            &mut allocator,
            &target_plugin,
            input.editor_id_prefix,
            actor_action_records,
            input.compiled_script,
            None,
            None,
            None,
            interner,
        ) {
            Ok(preparation) => candidate_inputs.push(preparation),
            Err(detail) => live_blockers
                .entry(candidate.source.clone())
                .or_default()
                .push(FnvFo3MvpCandidateBlocker::RecordProjection { detail }),
        }
    }

    Ok(build_fnv_fo3_mvp_candidate_preparations_with_live_blockers(
        input.dependency_ledger,
        &evidence.recipe_ledger,
        input.destinations.to_vec(),
        candidate_inputs,
        live_blockers,
    )?)
}

fn primary_reservation(
    source: &StableFormKey,
    mapper_state: &MapperState,
    interner: &StringInterner,
) -> Result<FormKey, FnvFo3MvpCandidateBlocker> {
    let source = FormKey {
        local: source.local,
        plugin: interner.intern(&source.plugin),
    };
    let Some(target) = mapper_state.source_to_target.get(&source).copied() else {
        return Err(FnvFo3MvpCandidateBlocker::MissingPrimaryNpcReservation);
    };
    if target.plugin != interner.intern(&mapper_state.options.output_plugin_name)
        || target.local == 0
        || target.local > 0x00ff_ffff
    {
        return Err(FnvFo3MvpCandidateBlocker::InvalidPrimaryNpcReservation {
            detail: format!(
                "reserved target {:06X} is not a generated key in {}",
                target.local, mapper_state.options.output_plugin_name
            ),
        });
    }
    Ok(target)
}

#[allow(clippy::too_many_arguments)]
fn prepare_live_candidate(
    candidate: &CreatureDependencyCandidateReceipt,
    record: &super::creature_recipe::CreatureRecordRecipeEntry,
    family: &CreatureFamilyRecipeJob,
    source_record: &Record,
    stats: LegacyCreatureStats,
    reservation: FormKey,
    family_assets: &PreparedLiveFamily,
    evidence: &FnvFo3LiveRecipeEvidence,
    shared_recipe_ledger: &Arc<CreatureFamilyRecipeLedger>,
    winning_records: &BTreeMap<StableFormKey, &Record>,
    dependency_records: &BTreeMap<StableFormKey, &RuntimeDependencyRecordReceipt>,
    dependency_ledger: &FnvFo3CreatureDependencyLedger,
    winning_runtime_records: &BTreeMap<(LegacyCreatureGame, StableFormKey), &Record>,
    runtime_model_cache: &mut BTreeMap<
        (LegacyCreatureGame, StableFormKey),
        Result<PreparedLegacyDebrisRecord, String>,
    >,
    source_data_root: &Path,
    private_staging_root: &Path,
    mapper_state: &MapperState,
    allocator: &mut FormKeyMapper<'_>,
    target_plugin: &str,
    editor_id_prefix: &str,
    mut actor_action_records: Vec<CreatureActorActionRecordPlan>,
    compiled_script: &dyn Fn(FormKey, FormKey) -> Result<crate::source_rig::CreatureNpcScriptAttachment, String>,
    template_branch: Option<LegacyTemplateBranchReceipt>,
    templates: Option<CreatureTemplateProjection>,
    template_leaf: Option<TargetFormKey>,
    interner: &StringInterner,
) -> Result<super::creature_mvp_adapter::FnvFo3MvpCandidatePreparationInput, String> {
    let direct_profile = if template_branch.is_none() {
        match super::creature_templates::resolve_template_plan(&candidate.source, record.provenance.game, winning_records, interner)? {
            super::creature_templates::LegacyTemplatePlan::Creature(profile) => Some(profile),
            super::creature_templates::LegacyTemplatePlan::Leveled { .. } => return Err("leveled template bypassed its branch projection".to_string()),
        }
    } else { None };
    let template_profile = template_branch.as_ref().map(|branch| branch.profile()).transpose()?.or(direct_profile.as_ref());
    let body = match &record.disposition {
        CreatureRecordRecipeDisposition::VisualOwner { body, .. } => body,
        CreatureRecordRecipeDisposition::Proxy {
            terminal_visual_owners,
            ..
        } => {
            let [owner] = terminal_visual_owners.as_slice() else {
                return Err(format!(
                    "CREA proxy resolves {} visual owners; a singular NPC cannot choose one",
                    terminal_visual_owners.len()
                ));
            };
            let owner_recipe = evidence
                .recipe_ledger
                .records
                .iter()
                .find(|entry| &entry.source == owner)
                .ok_or_else(|| {
                    format!("proxy visual owner {owner} is absent from recipe ledger")
                })?;
            match &owner_recipe.disposition {
                CreatureRecordRecipeDisposition::VisualOwner { body, .. } => body,
                _ => return Err(format!("proxy visual owner {owner} is not a body owner")),
            }
        }
        CreatureRecordRecipeDisposition::RejectedProxy { .. }
        | CreatureRecordRecipeDisposition::Special { .. } => {
            return Err("non-executable recipe disposition reached stage2".to_string());
        }
    };
    if body.body_paths.is_empty() {
        return Err("body variant has no NIF parts".to_string());
    }
    let mut source_identity = SourceCreatureIdentity {
        namespace: legacy_game_name(record.provenance.game).to_string(),
        plugin: candidate.source.plugin.clone(),
        local_form_id: candidate.source.local,
    };
    if let Some(branch) = &template_branch {
        if !branch.wrapper { source_identity = branch.generated_identity.clone(); }
    }
    let mut editor_id_stem = candidate_editor_id_stem(
        editor_id_prefix,
        record.provenance.game,
        candidate.source.local,
    )?;
    if template_branch.is_some() { editor_id_stem.push_str(&format!("_T{:06X}", reservation.local)); }
    let target = |form_key: FormKey| TargetFormKey::new(form_key.local, target_plugin);
    let allocate = |allocator: &mut FormKeyMapper<'_>| target(allocator.allocate_generated());
    let mut form_keys = CreatureRecordFormKeys {
        race: allocate(allocator),
        npc: target(reservation),
        skin: allocate(allocator),
        armor_addon: allocate(allocator),
        body_part_data: allocate(allocator),
        unarmed_weapon: TargetFormKey::new(0, target_plugin),
    };
    let editor_ids = CreatureRecordEditorIds {
        race: format!("{editor_id_stem}_Race"),
        npc: format!("{editor_id_stem}_Npc"),
        skin: format!("{editor_id_stem}_Skin"),
        armor_addon: format!("{editor_id_stem}_Arma"),
        body_part_data: format!("{editor_id_stem}_BodyParts"),
        unarmed_weapon: format!("{editor_id_stem}_Unarmed"),
    };
    let available_body_paths = body
        .body_paths
        .iter()
        .filter_map(|body_path| {
            family_assets
                .body_runtime_paths
                .get(&canonical_mesh_path(body_path))
                .cloned()
        })
        .collect::<Vec<_>>();
    if available_body_paths.is_empty() {
        return Err("prepared family contains no available body NIF".to_string());
    }
    let body_nif_parts = available_body_paths
        .into_iter()
        .enumerate()
        .map(|(index, body_nif)| {
            Ok(CreatureBodyNifRecordPart {
                body_nif,
                armor_addon_form_key: if index == 0 {
                    form_keys.armor_addon.clone()
                } else {
                    allocate(allocator)
                },
                armor_addon_editor_id: if index == 0 {
                    editor_ids.armor_addon.clone()
                } else {
                    format!("{editor_id_stem}_Arma_{index:02}")
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let body_nif = body_nif_parts[0].body_nif.clone();
    let (_, base_record) = candidate_template_group_record(
        &candidate.source,
        TEMPLATE_BASE_DATA,
        winning_records,
        singular_proxy_owner(record),
        template_profile,
        interner,
    )?;
    let display_name = source_display_name(base_record, record, interner);
    let (traits_source, traits_record) = candidate_template_group_record(
        &candidate.source,
        TEMPLATE_TRAITS,
        winning_records,
        singular_proxy_owner(record),
        template_profile,
        interner,
    )?;
    let traits_dependencies = dependency_records
        .get(&traits_source)
        .copied()
        .ok_or_else(|| {
            format!("dependency ledger omitted inherited traits owner {traits_source}")
        })?;
    let source_death_item = exact_optional_record_form_key(traits_record, "INAM", interner)?;
    let followed_death_items =
        direct_followed_sources_at_locator(traits_dependencies, "LVLI", "INAM[");
    let npc_death_item = match (source_death_item.as_ref(), followed_death_items.as_slice()) {
        (None, []) => None,
        (Some(source), [followed]) if source == followed => Some(mapped_target_reference(
            source,
            "LVLI",
            mapper_state,
            interner,
        )?),
        _ => {
            return Err(format!(
                "inherited CREA traits owner {traits_source} has INAM {source_death_item:?} but {} followed LVLI receipts",
                followed_death_items.len()
            ));
        }
    };
    let (spell_source, _) = candidate_template_group_record(
        &candidate.source,
        TEMPLATE_SPELL_LIST,
        winning_records,
        singular_proxy_owner(record),
        template_profile,
        interner,
    )?;
    let spell_dependencies = dependency_records
        .get(&spell_source)
        .copied()
        .ok_or_else(|| format!("dependency ledger omitted inherited spell owner {spell_source}"))?;
    let source_spells = direct_followed_sources_at_locator(spell_dependencies, "SPEL", "SPLO[");
    let npc_spells = source_spells
        .iter()
        .map(|source| mapped_target_reference(source, "SPEL", mapper_state, interner))
        .collect::<Result<Vec<_>, _>>()?;
    let (inventory_source, inventory_record) = candidate_template_group_record(
        &candidate.source,
        TEMPLATE_INVENTORY,
        winning_records,
        singular_proxy_owner(record),
        template_profile,
        interner,
    )?;
    let inventory_dependencies = dependency_records
        .get(&inventory_source)
        .copied()
        .ok_or_else(|| {
            format!("dependency ledger omitted inherited inventory owner {inventory_source}")
        })?;
    let direct_inventory_weapons =
        direct_followed_sources_at_locator(inventory_dependencies, "WEAP", "CNTO[");
    let npc_inventory = exact_inventory_entries(
        inventory_record,
        inventory_dependencies,
        mapper_state,
        interner,
    )?;
    let inventory_weapons = exact_inventory_weapon_sources(
        inventory_dependencies,
        &direct_inventory_weapons,
        dependency_records,
    )?;
    let inventory_weapons = if family.rig.game == LegacyCreatureGame::Fnv {
        exact_usable_inventory_weapon_sources(&inventory_weapons, traits_dependencies, dependency_records)?
    } else { inventory_weapons };
    let melee_weapons = exact_weapon_sources_by_kind(
        &inventory_weapons,
        false, family.rig.game, winning_runtime_records, interner,
    )?;
    let owned_equipment = inventory_weapons.clone();
    let inventory_weapons = exact_weapon_sources_by_kind(
        &inventory_weapons, true, family.rig.game, winning_runtime_records, interner,
    )?;
    let attack_policy = exact_legacy_attack_projection_policy(&[source_record, traits_record])?;

    let melee_events = graph_events(&family_assets.graph, CreatureClipRole::MeleeAttack);
    let projectile_events = graph_events(&family_assets.graph, CreatureClipRole::ProjectileAttack);
    let continuous_events = graph_events(
        &family_assets.graph,
        CreatureClipRole::ContinuousAttackStart,
    );
    let mut attacks = Vec::new();
    let mut melee_key_plan = Vec::new();
    for (index, event) in melee_events.iter().enumerate() {
        let event_weapons = if family_assets.attack_weapon_sources.get(&event.to_ascii_lowercase()).is_some_and(|weapons| !weapons.is_empty()) {
            let weapons = attack_weapons_for_event(&family_assets.attack_weapon_sources, event, &melee_weapons)?;
            if weapons.is_empty() { continue; }
            weapons
        } else {
            Vec::new()
        };
        let ordinal = index + 1;
        let attack_id = format!("source_unarmed_{ordinal:02}");
        let projection = if event_weapons.is_empty() {
            let weapon_form_key = allocate(allocator);
            let primary_unarmed = melee_key_plan.is_empty();
            if primary_unarmed {
                form_keys.unarmed_weapon = weapon_form_key.clone();
            }
            melee_key_plan.push(CreatureMeleeRecordKeyPlan {
                attack_id: attack_id.clone(),
                form_key: weapon_form_key.clone(),
                primary: primary_unarmed,
            });
            CreatureAttackRecordProjection::MeleeUnarmed {
                weapon_form_key,
                weapon_editor_id: if primary_unarmed {
                    editor_ids.unarmed_weapon.clone()
                } else {
                    format!("{}_{}", editor_ids.unarmed_weapon, ordinal)
                },
                damage: stats.damage,
                reach: stats.reach,
                attack_seconds: *family_assets.natural_attack_seconds.get(&event.to_ascii_lowercase())
                    .ok_or_else(|| format!("natural melee event {event} has no exact source clip duration"))?,
            }
        } else {
            CreatureAttackRecordProjection::MeleeEquipment {
                equipment: event_weapons
                    .iter()
                    .map(|source| mapped_target_reference(source, "WEAP", mapper_state, interner))
                    .collect::<Result<Vec<_>, _>>()?,
            }
        };
        let mut target_data = attack_policy.target_data.clone();
        if family_assets.power_attack_events.contains(&event.to_ascii_lowercase()) {
            target_data.attack_flags |= 4;
        }
        attacks.push(CreatureAttackRecordVariant {
            id: attack_id,
            event: event.clone(),
            primary: attacks.is_empty(),
            projection,
            damage_multiplier: attack_policy.damage_multiplier,
            chance: attack_policy.chance,
            strike_angle: attack_policy.strike_angle,
            action_point_cost: attack_policy.action_point_cost,
            target_data,
        });
    }
    for (index, event) in projectile_events.iter().enumerate() {
        let event_weapons = attack_weapons_for_event(&family_assets.attack_weapon_sources, event, &inventory_weapons)?;
        if event_weapons.is_empty() {
            continue;
        }
        let (projectile, ammunition) = exact_weapon_set_runtime_dependencies(
            &event_weapons,
            candidate,
            dependency_records,
            mapper_state,
            interner,
        )?;
        attacks.push(CreatureAttackRecordVariant {
            id: format!("source_projectile_{:02}", index + 1),
            event: event.clone(),
            primary: attacks.is_empty(),
            projection: CreatureAttackRecordProjection::RangedEquipment {
                equipment: event_weapons.iter().map(|source| mapped_target_reference(source, "WEAP", mapper_state, interner)).collect::<Result<Vec<_>, _>>()?,
                projectile,
                ammunition,
            },
            damage_multiplier: attack_policy.damage_multiplier,
            chance: attack_policy.chance,
            strike_angle: attack_policy.strike_angle,
            action_point_cost: attack_policy.action_point_cost,
            target_data: attack_policy.target_data.clone(),
        });
    }
    for (index, event) in continuous_events.iter().enumerate() {
        let event_weapons = attack_weapons_for_event(&family_assets.attack_weapon_sources, event, &inventory_weapons)?;
        if event_weapons.is_empty() {
            continue;
        }
        let (projectile, ammunition) = exact_weapon_set_runtime_dependencies(
            &event_weapons,
            candidate,
            dependency_records,
            mapper_state,
            interner,
        )?;
        attacks.push(CreatureAttackRecordVariant {
            id: format!("source_continuous_{:02}", index + 1),
            event: event.clone(),
            primary: attacks.is_empty(),
            projection: CreatureAttackRecordProjection::RangedEquipment {
                equipment: event_weapons.iter().map(|source| mapped_target_reference(source, "WEAP", mapper_state, interner)).collect::<Result<Vec<_>, _>>()?,
                projectile,
                ammunition,
            },
            damage_multiplier: attack_policy.damage_multiplier,
            chance: attack_policy.chance,
            strike_angle: attack_policy.strike_angle,
            action_point_cost: attack_policy.action_point_cost,
            target_data: attack_policy.target_data.clone(),
        });
    }
    let profile = template_profile.ok_or("creature projection has no resolved source profile")?;
    let (_, stats_record) = stable_map_entry(winning_records, &profile.owners[1])
        .ok_or("creature profile Stats owner is absent")?;
    let profile_race_data = super::creature_profile_race_data::prepare(
        family, &profile.owners[1], stats_record, interner,
    )?;
    let race_data = match &profile_race_data.derivation.mapping {
        RaceDataMapping::Mapped { target } => target.clone(),
        RaceDataMapping::Missing { .. } => return Err("creature profile has no complete FO4 RACE.DATA mapping".into()),
    };
    let root = family
        .actual_root_node
        .clone()
        .ok_or_else(|| "Ready family has no exact root node".to_string())?;
    let base = CreatureRecordManifest {
        target_plugin: target_plugin.to_string(),
        editor_id_prefix: editor_id_prefix.to_string(),
        form_keys: form_keys.clone(),
        editor_ids: editor_ids.clone(),
        display_name: display_name.clone(),
        body_nif,
    };
    let armor_addon_keys = body_nif_parts
        .iter()
        .map(|part| part.armor_addon_form_key.clone())
        .collect();
    let mut projection = CreatureRecordProjectionManifest {
        race_keywords: vec![super::creature_profile_race_data::controller_scale_keyword()],
        templates,
        equipment_slots: super::creature_muzzle::family_equipment_slots(
            family.indexed_motion_evidence.as_ref().map(|motion| motion.weapon_attacks.as_slice()).unwrap_or_default(),
            &owned_equipment, mapper_state, interner,
        )?,
        base,
        source_primary_identity: source_identity.clone(),
        race_data,
        body_parts: if let Some(source_key) = source_body_part_key(template_profile.map(|profile| &profile.owners[6]).unwrap_or(&candidate.source), winning_records, singular_proxy_owner(record), interner)? {
            let body_record = winning_runtime_records
                .get(&(record.provenance.game, source_key.clone()))
                .copied()
                .ok_or("source BPTD is absent from winning records")?;
            let dependency = dependency_records
                .get(&source_key)
                .ok_or("source BPTD dependency is absent")?;
            crate::source_rig::body_parts::project_source_body_parts(
                body_record,
                legacy_game_name(record.provenance.game),
                &root,
                &family_assets.body_part_segments,
                interner,
                |row, field, _raw| {
                    let locator = format!("BPND[{row}].{field}@");
                    let reference = dependency
                        .references
                        .iter()
                        .find(|reference| {
                            reference
                                .source_locators
                                .iter()
                                .any(|value| value.starts_with(&locator))
                        })
                        .ok_or_else(|| format!("BPTD dependency missing {locator}"))?;
                    match &reference.resolution {
                        DependencyReferenceResolution::Followed { signature } => mapped_target_reference(&reference.target, signature, mapper_state, interner),
                        DependencyReferenceResolution::TargetIntrinsic { signature, mapped_target } => Ok(CreatureTargetRecordReference::new(signature, TargetFormKey::new(mapped_target.local, &mapped_target.plugin))),
                        _ => Err(format!("BPTD dependency {locator} is not translated")),
                    }
                },
                |path| {
                    if path.is_empty() {
                        Ok(String::new())
                    } else {
                        family_assets.replacement_runtime_paths.get(&canonical_mesh_path(path)).cloned().ok_or_else(|| format!("unstaged BPTD replacement {path}"))
                    }
                },
            )?
        } else {
            CreatureBodyPartProjection::root_only_32(root)
        },
        body_nif_parts,
        npc_inventory: Vec::new(),
        npc_spells: Vec::new(),
        npc_death_item: None,
        npc_equipment: Vec::new(),
        variants: vec![CreatureNpcRecordVariant {
            actor_data: None,
            source_identity: source_identity.clone(),
            form_key: form_keys.npc.clone(),
            editor_id: editor_ids.npc.clone(),
            display_name,
            primary: true,
            level: stats.level,
            health: stats.health,
            action_points: stats.action_points,
            npc_inventory: Some(npc_inventory),
            npc_equipment: Some(Vec::new()),
            npc_spells: Some(npc_spells),
            npc_perks: Vec::new(),
            npc_death_item: candidate_death_item_override(npc_death_item),
        }],
        attacks,
    };
    let mut key_plan = CreatureRecordKeyPlan {
        planned_source_identity: source_identity.clone(),
        base: form_keys,
        armor_addons: armor_addon_keys,
        npc_variants: vec![CreatureNpcRecordKeyPlan {
            source_identity: source_identity.clone(),
            form_key: target(reservation),
            primary: true,
        }],
        melee_attacks: melee_key_plan,
    };
    projection.variants[0].actor_data = Some(creature_actor_projection::prepare_candidate_actor_data(
        template_profile.ok_or("creature projection has no resolved source profile")?,
        winning_records, dependency_records, mapper_state, interner,
    )?);
    let profile = template_profile.ok_or("creature script has no resolved source profile")?;
    let (_, script_owner) = stable_map_entry(winning_records, &profile.owners[9])
        .ok_or("creature Script template owner is missing")?;
    if let Some(source_script) = exact_optional_record_form_key(script_owner, "SCRI", interner)? {
        projection.variants[0].actor_data.as_mut().unwrap().script = Some(compiled_script(FormKey { local: source_script.local, plugin: interner.intern(&source_script.plugin) }, reservation)?);
    }
    if let Some(leaf_key) = template_leaf {
        let branch = template_branch.as_ref().ok_or("template leaf has no source branch receipt")?;
        if !branch.wrapper { return Err("secondary template leaf is only valid on the wrapper recipe".to_string()); }
        let mut leaf = projection.variants[0].clone();
        leaf.source_identity = branch.generated_identity.clone();
        leaf.form_key = leaf_key.clone();
        leaf.editor_id.push_str("_Selected");
        leaf.primary = false;
        projection.variants.push(leaf);
        key_plan.npc_variants.push(CreatureNpcRecordKeyPlan {
            source_identity: branch.generated_identity.clone(),
            form_key: leaf_key,
            primary: false,
        });
    }
    bind_attack_actor_action_conditions(&mut actor_action_records, family_assets, family, mapper_state, interner)?;
    let required_target_records = projection_target_dependencies(&projection);
    let batch_intent = SourceRigRecordBatchIntent {
        family_id: canonical_fnv_fo3_family_id(&family.rig),
        primary_mapping: CreaturePrimaryRecordMapping {
            source: source_identity,
            target: target(reservation),
        },
        required_target_records,
    }
    .with_actor_action_dependencies(&actor_action_records);
    let source_game = legacy_game_name(record.provenance.game);
    let initial_receipts =
        SourceRigExecutableRecipe::required_field_receipts_from_source_for_intent_with_actor_actions(
            &family_assets.rig,
            &family_assets.graph,
            &projection,
            &key_plan,
            &actor_action_records,
            &batch_intent,
            None,
            source_game,
        );
    let runtime_staged_data_root = private_staging_root.join("runtime_models").join("data");
    let runtime_model_closure = prepare_candidate_runtime_model_closure(
        candidate,
        dependency_ledger,
        winning_runtime_records,
        runtime_model_cache,
        source_data_root,
        &runtime_staged_data_root,
        interner,
    )?;
    let mut converted_artifacts = family_assets.converted_artifacts.clone();
    if let Some(receipt) = &runtime_model_closure {
        converted_artifacts.extend(
            receipt
                .canonical_target_artifacts()
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|artifact| FnvFo3MvpConvertedArtifact {
                    source_path: runtime_staged_data_root
                        .join(path_from_runtime(&artifact.target_data_path)),
                    runtime_path: artifact.target_data_path,
                }),
        );
    }
    let mut source_rig = FnvFo3SourceRigAdapterInput {
        template_branch,
        profile_race_data: Some(profile_race_data),
        ledger: Arc::clone(shared_recipe_ledger),
        family: family.rig.clone(),
        creature_closure: family_assets.closure.clone(),
        creature_closure_request_blake3: family_assets.closure.request_blake3.clone(),
        artifact_requests: family_assets.artifact_requests.clone(),
        artifact_receipts: family_assets.artifact_receipts.clone(),
        runtime_model_closure,
        rig: family_assets.rig.clone(),
        graph: family_assets.graph.clone(),
        projection,
        key_plan,
        actor_action_records,
        batch_intent,
        field_receipts: initial_receipts,
    };
    let bridge = build_fnv_fo3_source_rig_bridge_input_prevalidated(
        source_rig.clone(),
        &evidence.recipe_ledger.content_hash_blake3,
    )
    .map_err(|error| error.to_string())?;
    let bridge_receipt = bridge.bridge_receipt();
    source_rig.field_receipts =
        SourceRigExecutableRecipe::required_field_receipts_from_source_for_intent_with_actor_actions(
            &source_rig.rig,
            &source_rig.graph,
            &source_rig.projection,
            &source_rig.key_plan,
            &source_rig.actor_action_records,
            &source_rig.batch_intent,
            Some(&bridge_receipt),
            source_game,
        );
    for receipt in &mut source_rig.field_receipts {
        receipt.decision = if receipt.field.starts_with("projection.attacks[") && receipt.field.ends_with(".target_data.attack_flags") {
            SourceRigFieldDecision::Derived {
                source_games: vec![source_game.to_string()],
                source_fields: vec!["NiControllerSequence.name:source_attack_group".to_string(), "WEAP.DNAM.animation_type".to_string()],
                policy_id: "fnv_fo3_source_power_groups_92_96_to_fo4_power_attack_v1".to_string(),
            }
        } else if is_legacy_inert_attack_policy_field(&receipt.field) {
            SourceRigFieldDecision::Derived {
                source_games: vec![source_game.to_string()],
                source_fields: vec![
                    "effective_CREA_negative_proof:ATKD".to_string(),
                    "effective_CREA_negative_proof:ATKE".to_string(),
                ],
                policy_id: "fnv_fo3_legacy_no_attack_surface_to_inert_fo4_atkd_v1".to_string(),
            }
        } else {
            SourceRigFieldDecision::Derived {
                source_games: vec![source_game.to_string()],
                source_fields: vec![
                    "CREA".to_string(),
                    "GMST:fAVDActionPointsBase+fAVDActionPointsMult".to_string(),
                    "pair_recipe_and_dependency_receipts".to_string(),
                ],
                policy_id: "fnv_fo3_exact_creature_projection_v1".to_string(),
            }
        };
    }
    Ok(
        super::creature_mvp_adapter::FnvFo3MvpCandidatePreparationInput {
            source: candidate.source.clone(),
            family: family.rig.clone(),
            source_rig,
            closure_staged_data_root: family_assets.closure_staged_data_root.clone(),
            converted_artifacts,
        },
    )
}

fn candidate_death_item_override(
    death_item: Option<CreatureTargetRecordReference>,
) -> Option<Option<CreatureTargetRecordReference>> {
    death_item.map(Some)
}

fn prepare_candidate_runtime_model_closure(
    candidate: &CreatureDependencyCandidateReceipt,
    dependency_ledger: &FnvFo3CreatureDependencyLedger,
    winning_runtime_records: &BTreeMap<(LegacyCreatureGame, StableFormKey), &Record>,
    cache: &mut BTreeMap<
        (LegacyCreatureGame, StableFormKey),
        Result<PreparedLegacyDebrisRecord, String>,
    >,
    source_data_root: &Path,
    staged_data_root: &Path,
    interner: &StringInterner,
) -> Result<Option<SourceRigRuntimeModelClosureReceipt>, String> {
    let debris = dependency_ledger
        .records
        .iter()
        .filter(|record| {
            record.signature == "DEBR"
                && record.provenance.game == candidate.provenance.game
                && candidate.closure_form_keys.contains(&record.source)
        })
        .collect::<Vec<_>>();
    if debris.is_empty() {
        return Ok(None);
    }

    let mut expectations = Vec::new();
    let mut rows = Vec::new();
    for dependency in debris {
        let cache_key = (dependency.provenance.game, dependency.source.clone());
        let prepared = if let Some(prepared) = cache.get(&cache_key) {
            prepared.clone()
        } else {
            let prepared = winning_runtime_records
                .get(&cache_key)
                .copied()
                .ok_or_else(|| {
                    format!(
                        "winning {:?} DEBR source {} is unavailable to stage2",
                        dependency.provenance.game, dependency.source
                    )
                })
                .and_then(|record| {
                    prepare_legacy_debris_record(
                        dependency.provenance.game,
                        record,
                        source_data_root,
                        staged_data_root,
                        interner,
                    )
                });
            cache.insert(cache_key, prepared.clone());
            prepared
        }?;
        expectations.extend(prepared.expectations);
        rows.extend(prepared.rows);
    }
    expectations.sort_by(|left, right| left.key.cmp(&right.key));
    rows.sort_by(|left, right| left.key.cmp(&right.key));
    let receipt = SourceRigRuntimeModelClosureReceipt {
        version: SOURCE_RIG_RUNTIME_MODEL_CLOSURE_VERSION,
        rows,
    };
    let source_game = legacy_game_name(candidate.provenance.game).to_string();
    let source_roots = BTreeMap::from([(
        source_game,
        game_data_root(source_data_root, candidate.provenance.game),
    )]);
    validate_source_rig_runtime_model_closure(
        &expectations,
        &receipt,
        &source_roots,
        staged_data_root,
    )
    .map_err(|error| error.to_string())?;
    Ok(Some(receipt))
}

fn prepare_legacy_debris_record(
    game: LegacyCreatureGame,
    record: &Record,
    source_data_root: &Path,
    staged_data_root: &Path,
    interner: &StringInterner,
) -> Result<PreparedLegacyDebrisRecord, String> {
    let support = classify_legacy_debris(record);
    if !support.is_ready() {
        return Err(format!(
            "DEBR runtime-model staging rejected source shape: {}",
            support.reason_codes.join(",")
        ));
    }
    let source_game = legacy_game_name(game);
    let source_root = game_data_root(source_data_root, game);
    let source_plugin = interner
        .resolve(record.form_key.plugin)
        .ok_or_else(|| "DEBR runtime-model source plugin is unresolved".to_string())?;
    let source_record = SourceCreatureIdentity {
        namespace: source_game.to_string(),
        plugin: source_plugin.to_string(),
        local_form_id: record.form_key.local,
    };
    let model_rows = legacy_debris_model_rows(record, source_game, interner)?;
    let mut expectations = Vec::with_capacity(model_rows.len());
    let mut receipts = Vec::with_capacity(model_rows.len());
    for (row_index, row) in model_rows.into_iter().enumerate() {
        let key = SourceRigRuntimeModelRowKey {
            source_game: source_game.to_string(),
            source_record: source_record.clone(),
            source_signature: "DEBR".to_string(),
            row_index: row_index as u32,
        };
        expectations.push(SourceRigRuntimeModelRowExpectation {
            key: key.clone(),
            percentage: row.percentage,
            has_collision: row.has_collision,
            source_model_filename: row.source_model_filename.clone(),
        });

        let source_data_path = format!("meshes\\{}", row.source_model_filename);
        let target_data_path = format!("meshes\\{}", row.target_model_filename);
        let source_path = source_root.join(path_from_runtime(&source_data_path));
        let target_path = staged_data_root.join(path_from_runtime(&target_data_path));
        let source_bytes = fs::read(&source_path).map_err(|error| {
            format!(
                "failed to read DEBR source model {}: {error}",
                source_path.display()
            )
        })?;
        let source_nif = NifFile::from_bytes(&source_bytes, Some(source_path.clone()))
            .map_err(|error| format!("failed to parse DEBR source model: {error}"))?;
        let namespace = row
            .target_model_filename
            .split('\\')
            .next()
            .ok_or_else(|| "DEBR target model namespace is empty".to_string())?;
        let report = convert_nif_file(
            &source_path,
            &target_path,
            source_game,
            "fo4",
            None,
            &ConvertFileOptions {
                asset_prefix: Some(namespace.to_string()),
                material_namespace: Some(namespace.to_string()),
                source_material_dir: Some(source_root.clone()),
                ..ConvertFileOptions::default()
            },
        )
        .map_err(|error| format!("DEBR NIF conversion failed: {error}"))?;
        if !report.supported || !report.errors.is_empty() {
            return Err(format!(
                "DEBR NIF conversion was not target-valid: {}",
                report.errors.join(";")
            ));
        }
        if !report.emitted_bgsms.is_empty() || !report.emitted_textures.is_empty() {
            return Err("DEBR NIF conversion emitted unreceipted ancillary assets".to_string());
        }
        let target_bytes = fs::read(&target_path).map_err(|error| {
            format!(
                "failed to read converted DEBR model {}: {error}",
                target_path.display()
            )
        })?;
        let target_nif = NifFile::from_bytes(&target_bytes, Some(target_path.clone()))
            .map_err(|error| format!("failed to reread converted DEBR model: {error}"))?;
        let mut artifacts = BTreeMap::<String, SourceRigRuntimeModelArtifactReceipt>::new();
        insert_runtime_model_artifact(
            &mut artifacts,
            SourceRigRuntimeModelArtifactKind::Nif,
            vec![source_data_path.clone()],
            target_data_path.clone(),
            &target_bytes,
        )?;
        stage_debris_referenced_assets(
            &source_nif,
            &target_nif,
            namespace,
            &source_root,
            staged_data_root,
            &mut artifacts,
        )?;
        receipts.push(SourceRigRuntimeModelRowReceipt {
            key,
            percentage: row.percentage,
            has_collision: row.has_collision,
            source_model_filename: row.source_model_filename,
            source_data_path,
            source_byte_len: source_bytes.len() as u64,
            source_blake3: blake3::hash(&source_bytes).to_hex().to_string(),
            target_model_filename: row.target_model_filename,
            target_data_path,
            target_byte_len: target_bytes.len() as u64,
            target_blake3: blake3::hash(&target_bytes).to_hex().to_string(),
            artifacts: artifacts.into_values().collect(),
        });
    }
    Ok(PreparedLegacyDebrisRecord {
        expectations,
        rows: receipts,
    })
}

fn stage_debris_referenced_assets(
    source_nif: &NifFile,
    target_nif: &NifFile,
    namespace: &str,
    source_root: &Path,
    staged_data_root: &Path,
    artifacts: &mut BTreeMap<String, SourceRigRuntimeModelArtifactReceipt>,
) -> Result<(), String> {
    let source_references = source_nif.referenced_asset_paths();
    let source_textures = source_references
        .textures
        .iter()
        .map(|path| canonical_data_path(path))
        .collect::<BTreeSet<_>>();
    let source_materials = source_references
        .materials
        .iter()
        .map(|path| canonical_data_path(path))
        .collect::<BTreeSet<_>>();
    let target_references = target_nif.referenced_asset_paths();
    for target in &target_references.textures {
        let source = source_path_for_namespaced_target(target, "textures", namespace)?;
        if !source_textures.contains(&canonical_data_path(&source)) {
            return Err(format!(
                "converted DEBR NIF texture {target:?} lacks exact source reference evidence"
            ));
        }
        stage_debris_runtime_asset(
            SourceRigRuntimeModelArtifactKind::Dds,
            &source,
            target,
            source_root,
            staged_data_root,
            artifacts,
        )?;
    }
    for target in &target_references.materials {
        let source = source_path_for_namespaced_target(target, "materials", namespace)?;
        if !source_materials.contains(&canonical_data_path(&source)) {
            return Err(format!(
                "converted DEBR NIF material {target:?} lacks exact source reference evidence"
            ));
        }
        let kind = if target.to_ascii_lowercase().ends_with(".bgsm") {
            SourceRigRuntimeModelArtifactKind::Bgsm
        } else if target.to_ascii_lowercase().ends_with(".bgem") {
            SourceRigRuntimeModelArtifactKind::Bgem
        } else {
            return Err(format!("unsupported DEBR material path {target:?}"));
        };
        stage_debris_runtime_asset(
            kind,
            &source,
            target,
            source_root,
            staged_data_root,
            artifacts,
        )?;
        let staged_material = staged_data_root.join(path_from_runtime(target));
        for texture in crate::relocation::read_material_texture_paths(&staged_material) {
            stage_debris_runtime_asset(
                SourceRigRuntimeModelArtifactKind::Dds,
                &texture,
                &texture,
                source_root,
                staged_data_root,
                artifacts,
            )?;
        }
    }
    Ok(())
}

fn stage_debris_runtime_asset(
    kind: SourceRigRuntimeModelArtifactKind,
    source_data_path: &str,
    target_data_path: &str,
    source_root: &Path,
    staged_data_root: &Path,
    artifacts: &mut BTreeMap<String, SourceRigRuntimeModelArtifactReceipt>,
) -> Result<(), String> {
    let source_data_path = runtime_backslashes(source_data_path)?;
    let target_data_path = runtime_backslashes(target_data_path)?;
    let source_path = source_root.join(path_from_runtime(&source_data_path));
    let target_path = staged_data_root.join(path_from_runtime(&target_data_path));
    let bytes = fs::read(&source_path).map_err(|error| {
        format!(
            "failed to read DEBR runtime asset {}: {error}",
            source_path.display()
        )
    })?;
    write_exact_staged_asset(&target_path, &bytes)?;
    insert_runtime_model_artifact(
        artifacts,
        kind,
        vec![source_data_path],
        target_data_path,
        &bytes,
    )
}

fn insert_runtime_model_artifact(
    artifacts: &mut BTreeMap<String, SourceRigRuntimeModelArtifactReceipt>,
    kind: SourceRigRuntimeModelArtifactKind,
    mut source_paths: Vec<String>,
    target_data_path: String,
    bytes: &[u8],
) -> Result<(), String> {
    source_paths.sort_by_key(|path| canonical_data_path(path));
    source_paths.dedup_by(|left, right| canonical_data_path(left) == canonical_data_path(right));
    let key = canonical_data_path(&target_data_path);
    let hash = blake3::hash(bytes).to_hex().to_string();
    match artifacts.get_mut(&key) {
        Some(existing)
            if existing.kind != kind
                || existing.byte_len != bytes.len() as u64
                || !existing.blake3.eq_ignore_ascii_case(&hash) =>
        {
            Err(format!(
                "DEBR runtime target {target_data_path:?} has conflicting artifacts"
            ))
        }
        Some(existing) => {
            existing.source_paths.extend(source_paths);
            existing
                .source_paths
                .sort_by_key(|path| canonical_data_path(path));
            existing
                .source_paths
                .dedup_by(|left, right| canonical_data_path(left) == canonical_data_path(right));
            Ok(())
        }
        None => {
            artifacts.insert(
                key,
                SourceRigRuntimeModelArtifactReceipt {
                    kind,
                    source_paths,
                    target_data_path,
                    byte_len: bytes.len() as u64,
                    blake3: hash,
                },
            );
            Ok(())
        }
    }
}

fn source_path_for_namespaced_target(
    target: &str,
    root: &str,
    namespace: &str,
) -> Result<String, String> {
    let canonical = canonical_data_path(target);
    let mut parts = canonical.split('/').collect::<Vec<_>>();
    if parts.first().copied() != Some(root) || parts.len() < 2 {
        return Err(format!("invalid DEBR referenced asset path {target:?}"));
    }
    if parts
        .get(1)
        .is_some_and(|part| part.eq_ignore_ascii_case(namespace))
    {
        parts.remove(1);
    }
    Ok(parts.join("\\"))
}

fn write_exact_staged_asset(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.is_file() {
        let existing = fs::read(path).map_err(|error| {
            format!("failed to reread staged asset {}: {error}", path.display())
        })?;
        if existing != bytes {
            return Err(format!(
                "staged DEBR runtime path {} has conflicting bytes",
                path.display()
            ));
        }
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("staged DEBR asset {} has no parent", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create staged DEBR asset directory {}: {error}",
            parent.display()
        )
    })?;
    fs::write(path, bytes)
        .map_err(|error| format!("failed to stage DEBR asset {}: {error}", path.display()))
}

fn runtime_backslashes(path: &str) -> Result<String, String> {
    let path = path.trim().replace('/', "\\");
    if path.is_empty()
        || path.starts_with('\\')
        || path.contains(':')
        || path
            .split('\\')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("invalid DEBR runtime asset path {path:?}"));
    }
    Ok(path)
}

fn canonical_data_path(path: &str) -> String {
    path.trim().replace('\\', "/").to_ascii_lowercase()
}

fn path_from_runtime(path: &str) -> PathBuf {
    path.replace('/', "\\").split('\\').collect()
}

fn candidate_editor_id_stem(
    prefix: &str,
    game: LegacyCreatureGame,
    local: u32,
) -> Result<String, String> {
    let prefix = prefix.trim().trim_end_matches('_');
    if prefix.is_empty()
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(
            "editor-id prefix must contain only ASCII letters, digits, and '_'".to_string(),
        );
    }
    Ok(format!(
        "{prefix}_Creature_{}_{local:06X}",
        legacy_game_name(game)
    ))
}

fn source_display_name(
    record: &Record,
    recipe: &super::creature_recipe::CreatureRecordRecipeEntry,
    interner: &StringInterner,
) -> String {
    record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "FULL")
        .and_then(|field| match field.value {
            FieldValue::String(value) => interner.resolve(value),
            _ => None,
        })
        .filter(|value| !value.trim().is_empty())
        .or(recipe.editor_id.as_deref())
        .unwrap_or("Legacy Creature")
        .to_string()
}

pub(super) fn source_body_part_key(
    source: &StableFormKey,
    records: &BTreeMap<StableFormKey, &Record>,
    visual_owner: Option<&StableFormKey>,
    interner: &StringInterner,
) -> Result<Option<StableFormKey>, String> {
    let (_, owner) = resolve_template_group_record(
        source, TEMPLATE_MODEL_ANIMATION, records, visual_owner, interner,
    )?;
    exact_optional_record_form_key(owner, "PNAM", interner)
}

pub(super) fn resolve_template_group_record<'a>(
    source: &StableFormKey,
    group_flag: u16,
    records: &'a BTreeMap<StableFormKey, &'a Record>,
    singular_proxy_owner: Option<&StableFormKey>,
    interner: &StringInterner,
) -> Result<(StableFormKey, &'a Record), String> {
    let mut current = source.clone();
    let mut last_creature = None;
    let mut visited = BTreeSet::new();
    loop {
        if !visited.insert(canonical_recipe_source_key(&current)) {
            return Err(format!(
                "template group 0x{group_flag:04X} cycles at {current}"
            ));
        }
        let Some((resolved_source, record)) = stable_map_entry(records, &current) else {
            if let Some(owner) = singular_proxy_owner
                && &current != owner
            {
                current = owner.clone();
                continue;
            }
            if group_flag != TEMPLATE_MODEL_ANIMATION
                && let Some(owner) = last_creature
            {
                return Ok(owner);
            }
            return Err(format!(
                "template group 0x{group_flag:04X} target {current} is not a winning CREA"
            ));
        };
        current = resolved_source.clone();
        let record = *record;
        last_creature = Some((current.clone(), record));
        let template = legacy_template_target(record, interner)?;
        if template.is_none() {
            return Ok((current, record));
        }
        let flags = legacy_template_flags(record, interner)?;
        if flags & group_flag == 0 {
            return Ok((current, record));
        }
        current = template.expect("template presence was checked");
    }
}

fn candidate_template_group_record<'a>(
    source: &StableFormKey,
    group_flag: u16,
    records: &'a BTreeMap<StableFormKey, &'a Record>,
    singular_proxy_owner: Option<&StableFormKey>,
    profile: Option<&LegacyTemplateProfile>,
    interner: &StringInterner,
) -> Result<(StableFormKey, &'a Record), String> {
    if let Some(profile) = profile {
        let owner = &profile.owners[group_flag.trailing_zeros() as usize];
        return stable_map_entry(records, owner)
            .map(|(key, record)| (key.clone(), *record))
            .ok_or_else(|| format!("selected creature template owner {owner} is unavailable"));
    }
    resolve_template_group_record(source, group_flag, records, singular_proxy_owner, interner)
}

fn singular_proxy_owner(
    record: &super::creature_recipe::CreatureRecordRecipeEntry,
) -> Option<&StableFormKey> {
    match &record.disposition {
        CreatureRecordRecipeDisposition::Proxy {
            terminal_visual_owners,
            ..
        } => terminal_visual_owners
            .as_slice()
            .first()
            .filter(|_| terminal_visual_owners.len() == 1),
        _ => None,
    }
}

fn legacy_template_flags(record: &Record, interner: &StringInterner) -> Result<u16, String> {
    let value = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "ACBS")
        .map(|field| &field.value)
        .ok_or_else(|| "CREA.ACBS is absent".to_string())?;
    let flags = match value {
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("template_flags"))
            .and_then(|(_, value)| integer_value(value)),
        FieldValue::Bytes(bytes) if bytes.len() >= 24 => {
            Some(i64::from(u16::from_le_bytes([bytes[22], bytes[23]])))
        }
        _ => None,
    }
    .and_then(|value| u16::try_from(value).ok())
    .ok_or_else(|| "CREA.ACBS.template_flags is undecodable".to_string())?;
    Ok(flags)
}

fn legacy_template_target(
    record: &Record,
    interner: &StringInterner,
) -> Result<Option<StableFormKey>, String> {
    let targets = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "TPLT")
        .flat_map(|field| form_keys_in_value(&field.value))
        .map(|target| {
            interner
                .resolve(target.plugin)
                .map(|plugin| StableFormKey {
                    local: target.local,
                    plugin: plugin.to_string(),
                })
                .ok_or_else(|| "CREA.TPLT plugin symbol is unresolved".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    match targets.as_slice() {
        [] => Ok(None),
        [target] => Ok(Some(target.clone())),
        _ => Err(format!("CREA has {} decoded TPLT targets", targets.len())),
    }
}

fn form_keys_in_value(value: &FieldValue) -> Vec<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => vec![*form_key],
        FieldValue::List(values) => values.iter().flat_map(form_keys_in_value).collect(),
        FieldValue::Struct(fields) => fields
            .iter()
            .flat_map(|(_, value)| form_keys_in_value(value))
            .collect(),
        _ => Vec::new(),
    }
}

fn exact_optional_record_form_key(
    record: &Record,
    signature: &str,
    interner: &StringInterner,
) -> Result<Option<StableFormKey>, String> {
    let fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .collect::<Vec<_>>();
    let field = match fields.as_slice() {
        [] => return Ok(None),
        [field] => *field,
        _ => return Err(format!("CREA has {} {signature} fields", fields.len())),
    };
    match &field.value {
        FieldValue::FormKey(form_key) if form_key.local == 0 => Ok(None),
        FieldValue::FormKey(form_key) => interner
            .resolve(form_key.plugin)
            .map(|plugin| {
                Some(StableFormKey {
                    local: form_key.local,
                    plugin: plugin.to_string(),
                })
            })
            .ok_or_else(|| format!("CREA.{signature} plugin symbol is unresolved")),
        FieldValue::Bytes(bytes) if bytes.len() == 4 && bytes.iter().all(|byte| *byte == 0) => {
            Ok(None)
        }
        FieldValue::Uint(0) | FieldValue::Int(0) => Ok(None),
        _ => Err(format!(
            "CREA.{signature} is not one decoded FormKey or null"
        )),
    }
}

fn direct_followed_sources(
    record: &RuntimeDependencyRecordReceipt,
    signature: &str,
) -> Vec<StableFormKey> {
    record
        .references
        .iter()
        .filter_map(|reference| match &reference.resolution {
            DependencyReferenceResolution::Followed { signature: actual }
                if actual == signature =>
            {
                Some(reference.target.clone())
            }
            _ => None,
        })
        .collect()
}

fn direct_followed_sources_at_locator(
    record: &RuntimeDependencyRecordReceipt,
    signature: &str,
    locator_prefix: &str,
) -> Vec<StableFormKey> {
    let mut sources = record
        .references
        .iter()
        .filter_map(|reference| {
            let order = reference
                .source_locators
                .iter()
                .filter_map(|locator| locator_index(locator, locator_prefix))
                .min()?;
            match &reference.resolution {
                DependencyReferenceResolution::Followed { signature: actual }
                    if actual == signature =>
                {
                    Some((order, reference.target.clone()))
                }
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    sources.sort_by_key(|(order, source)| (*order, source.clone()));
    sources.into_iter().map(|(_, source)| source).collect()
}

pub(super) fn exact_inventory_weapon_sources(
    inventory: &RuntimeDependencyRecordReceipt,
    direct_weapons: &[StableFormKey],
    records: &BTreeMap<StableFormKey, &RuntimeDependencyRecordReceipt>,
) -> Result<Vec<StableFormKey>, String> {
    let mut weapons = direct_weapons.iter().cloned().collect::<BTreeSet<_>>();
    let mut pending_lists = direct_followed_sources_at_locator(inventory, "LVLI", "CNTO[");
    let mut visited_lists = BTreeSet::new();
    while let Some(list) = pending_lists.pop() {
        if !visited_lists.insert(canonical_recipe_source_key(&list)) {
            continue;
        }
        let (_, receipt) = stable_map_entry(records, &list)
            .ok_or_else(|| format!("inventory LVLI dependency receipt {list} is absent"))?;
        if receipt.signature != "LVLI" {
            return Err(format!(
                "inventory LVLI dependency {list} resolved as {}",
                receipt.signature
            ));
        }
        weapons.extend(direct_followed_sources(receipt, "WEAP"));
        pending_lists.extend(direct_followed_sources(receipt, "LVLI"));
    }
    Ok(weapons.into_iter().collect())
}

pub(super) fn exact_usable_inventory_weapon_sources(
    inventory_weapons: &[StableFormKey],
    traits: &RuntimeDependencyRecordReceipt,
    records: &BTreeMap<StableFormKey, &RuntimeDependencyRecordReceipt>,
) -> Result<Vec<StableFormKey>, String> {
    let lists = direct_followed_sources_at_locator(traits, "FLST", "LNAM[");
    let [list] = lists.as_slice() else {
        if lists.is_empty() { return Ok(Vec::new()); }
        return Err(format!("creature traits {} has {} weapon admissibility lists", traits.source, lists.len()));
    };
    let (_, receipt) = stable_map_entry(records, list)
        .ok_or_else(|| format!("creature weapon FLST dependency receipt {list} is absent"))?;
    if receipt.signature != "FLST" { return Err(format!("creature weapon list {list} is not FLST")); }
    let admitted = direct_followed_sources_at_locator(receipt, "WEAP", "LNAM[")
        .iter().map(canonical_recipe_source_key).collect::<BTreeSet<_>>();
    Ok(inventory_weapons.iter().filter(|weapon| admitted.contains(&canonical_recipe_source_key(weapon))).cloned().collect())
}

fn stable_map_entry<'a, T>(
    records: &'a BTreeMap<StableFormKey, T>,
    source: &StableFormKey,
) -> Option<(&'a StableFormKey, &'a T)> {
    records.get_key_value(source).or_else(|| {
        records.iter().find(|(candidate, _)| {
            candidate.local == source.local && candidate.plugin.eq_ignore_ascii_case(&source.plugin)
        })
    })
}

fn locator_index(locator: &str, prefix: &str) -> Option<usize> {
    let suffix = locator.strip_prefix(prefix)?;
    suffix.split_once(']')?.0.parse().ok()
}

fn exact_inventory_entries(
    source_record: &Record,
    dependency_record: &RuntimeDependencyRecordReceipt,
    mapper_state: &MapperState,
    interner: &StringInterner,
) -> Result<Vec<CreatureNpcInventoryEntry>, String> {
    let mut entries = Vec::new();
    let mut conto_index = 0usize;
    let mut coed_index = 0usize;
    for (field_index, field) in source_record.fields.iter().enumerate() {
        if field.sig.as_str() != "CNTO" {
            if field.sig.as_str() == "COED" {
                coed_index += 1;
            }
            continue;
        }
        let index = conto_index;
        conto_index += 1;
        let references = dependency_record
            .references
            .iter()
            .filter(|reference| {
                reference
                    .source_locators
                    .iter()
                    .any(|locator| locator_index(locator, "CNTO[") == Some(index))
            })
            .collect::<Vec<_>>();
        let [reference] = references.as_slice() else {
            return Err(format!(
                "CNTO[{index}] resolved {} dependency references",
                references.len()
            ));
        };
        let signature = match &reference.resolution {
            DependencyReferenceResolution::Followed { signature } => signature.as_str(),
            resolution => {
                return Err(format!(
                    "CNTO[{index}] dependency {} is not runtime-followed: {resolution:?}",
                    reference.target
                ));
            }
        };
        let count = inventory_count(&field.value, interner)
            .ok_or_else(|| format!("CNTO[{index}].count is undecodable"))?;
        if let Some(item) = inventory_item(&field.value, interner) {
            let plugin = interner
                .resolve(item.plugin)
                .ok_or_else(|| format!("CNTO[{index}].item plugin symbol is unresolved"))?;
            if item.local != reference.target.local
                || !plugin.eq_ignore_ascii_case(&reference.target.plugin)
            {
                return Err(format!(
                    "CNTO[{index}].item differs from dependency locator receipt"
                ));
            }
        }
        entries.push(CreatureNpcInventoryEntry {
            target_record: mapped_target_reference(
                &reference.target,
                signature,
                mapper_state,
                interner,
            )?,
            count,
            ownership: source_record
                .fields
                .get(field_index + 1)
                .filter(|field| field.sig.as_str() == "COED")
                .map(|field| {
                    exact_inventory_ownership(
                        &field.value,
                        coed_index,
                        dependency_record,
                        mapper_state,
                        interner,
                    )
                })
                .transpose()?,
        });
    }
    Ok(entries)
}

fn exact_inventory_ownership(
    value: &FieldValue,
    coed_index: usize,
    dependency_record: &RuntimeDependencyRecordReceipt,
    mapper_state: &MapperState,
    interner: &StringInterner,
) -> Result<CreatureNpcInventoryOwnership, String> {
    if let FieldValue::Bytes(bytes) = value {
        if bytes.len() != 12 {
            return Err(format!(
                "COED[{coed_index}] is not the exact 12-byte layout"
            ));
        }
        let owner_raw = u32::from_le_bytes(bytes[0..4].try_into().expect("COED size checked"));
        let rank_or_global_raw =
            u32::from_le_bytes(bytes[4..8].try_into().expect("COED size checked"));
        let condition = f32::from_le_bytes(bytes[8..12].try_into().expect("COED size checked"));
        if !condition.is_finite() || condition < 0.0 {
            return Err(format!(
                "COED[{coed_index}].item_condition is invalid: {condition}"
            ));
        }
        let owner_reference = ownership_reference_at_locator(
            owner_raw,
            dependency_record,
            &format!("COED[{coed_index}].owner"),
            mapper_state,
            interner,
        )?;
        if owner_reference
            .as_ref()
            .is_some_and(|owner| owner.signature == "FACT")
        {
            return Ok(CreatureNpcInventoryOwnership::FactionRank {
                faction: owner_reference.expect("FACT owner was checked"),
                required_rank: rank_or_global_raw as i32,
                condition,
            });
        }
        let global = ownership_reference_at_locator(
            rank_or_global_raw,
            dependency_record,
            &format!("COED[{coed_index}].global_variable_required_rank"),
            mapper_state,
            interner,
        )?;
        return Ok(CreatureNpcInventoryOwnership::OwnerGlobal {
            owner: owner_reference,
            global,
            condition,
        });
    }
    let FieldValue::Struct(fields) = value else {
        return Err(format!(
            "COED[{coed_index}] is not a decoded struct or 12-byte layout"
        ));
    };
    let owner = named_value(fields, "owner", interner)
        .ok_or_else(|| format!("COED[{coed_index}].owner is absent"))?;
    let owner_reference = ownership_reference(
        owner,
        dependency_record,
        &format!("COED[{coed_index}].owner"),
        mapper_state,
        interner,
    )?;
    let rank_or_global = named_value(fields, "global_variable_required_rank", interner)
        .ok_or_else(|| format!("COED[{coed_index}].global_variable_required_rank is absent"))?;
    let condition = named_value(fields, "item_condition", interner)
        .and_then(numeric_f32)
        .ok_or_else(|| format!("COED[{coed_index}].item_condition is undecodable"))?;
    if owner_reference
        .as_ref()
        .is_some_and(|owner| owner.signature == "FACT")
    {
        let required_rank = integer_value(rank_or_global)
            .and_then(|rank| i32::try_from(rank).ok())
            .ok_or_else(|| format!("COED[{coed_index}].required_rank is undecodable"))?;
        return Ok(CreatureNpcInventoryOwnership::FactionRank {
            faction: owner_reference.expect("FACT owner was checked"),
            required_rank,
            condition,
        });
    }
    let global = ownership_reference(
        rank_or_global,
        dependency_record,
        &format!("COED[{coed_index}].global_variable_required_rank"),
        mapper_state,
        interner,
    )?;
    Ok(CreatureNpcInventoryOwnership::OwnerGlobal {
        owner: owner_reference,
        global,
        condition,
    })
}

fn ownership_reference_at_locator(
    raw: u32,
    dependency_record: &RuntimeDependencyRecordReceipt,
    locator: &str,
    mapper_state: &MapperState,
    interner: &StringInterner,
) -> Result<Option<CreatureTargetRecordReference>, String> {
    if raw == 0 {
        return Ok(None);
    }
    let references = dependency_record
        .references
        .iter()
        .filter(|reference| {
            reference
                .source_locators
                .iter()
                .any(|source_locator| source_locator == locator)
        })
        .collect::<Vec<_>>();
    let [reference] = references.as_slice() else {
        return Err(format!(
            "{locator} resolved {} dependency references",
            references.len()
        ));
    };
    let DependencyReferenceResolution::Followed { signature } = &reference.resolution else {
        return Err(format!(
            "{locator} dependency {} is not runtime-followed: {:?}",
            reference.target, reference.resolution
        ));
    };
    Ok(Some(mapped_target_reference(
        &reference.target,
        signature,
        mapper_state,
        interner,
    )?))
}

fn ownership_reference(
    value: &FieldValue,
    dependency_record: &RuntimeDependencyRecordReceipt,
    locator: &str,
    mapper_state: &MapperState,
    interner: &StringInterner,
) -> Result<Option<CreatureTargetRecordReference>, String> {
    if integer_value(value) == Some(0) {
        return Ok(None);
    }
    let FieldValue::FormKey(source_key) = value else {
        return Err(format!("{locator} is neither a FormKey nor null"));
    };
    let plugin = interner
        .resolve(source_key.plugin)
        .ok_or_else(|| format!("{locator} plugin symbol is unresolved"))?;
    let source = StableFormKey {
        local: source_key.local,
        plugin: plugin.to_string(),
    };
    let references = dependency_record
        .references
        .iter()
        .filter(|reference| {
            reference.target == source
                && reference
                    .source_locators
                    .iter()
                    .any(|source_locator| source_locator == locator)
        })
        .collect::<Vec<_>>();
    let [reference] = references.as_slice() else {
        return Err(format!(
            "{locator} resolved {} dependency references",
            references.len()
        ));
    };
    let DependencyReferenceResolution::Followed { signature } = &reference.resolution else {
        return Err(format!(
            "{locator} dependency {} is not runtime-followed: {:?}",
            reference.target, reference.resolution
        ));
    };
    Ok(Some(mapped_target_reference(
        &reference.target,
        signature,
        mapper_state,
        interner,
    )?))
}

fn inventory_item(value: &FieldValue, interner: &StringInterner) -> Option<FormKey> {
    let FieldValue::Struct(fields) = value else {
        return None;
    };
    fields.iter().find_map(|(name, value)| {
        (interner.resolve(*name) == Some("item"))
            .then_some(value)
            .and_then(|value| match value {
                FieldValue::FormKey(item) => Some(*item),
                _ => None,
            })
    })
}

fn inventory_count(value: &FieldValue, interner: &StringInterner) -> Option<i32> {
    match value {
        FieldValue::Struct(fields) => fields.iter().find_map(|(name, value)| {
            (interner.resolve(*name) == Some("count"))
                .then_some(value)
                .and_then(integer_value)
                .and_then(|count| i32::try_from(count).ok())
        }),
        FieldValue::Bytes(bytes) if bytes.len() == 8 => {
            Some(i32::from_le_bytes(bytes[4..8].try_into().ok()?))
        }
        _ => None,
    }
}

fn exact_weapon_sources_by_kind(
    sources: &[StableFormKey],
    ranged: bool,
    game: LegacyCreatureGame,
    records: &BTreeMap<(LegacyCreatureGame, StableFormKey), &Record>,
    interner: &StringInterner,
) -> Result<Vec<StableFormKey>, String> {
    let mut weapons = Vec::new();
    for source in sources {
        let record = records.iter().find_map(|((record_game, key), record)| {
            (*record_game == game && key.local == source.local && key.plugin.eq_ignore_ascii_case(&source.plugin)).then_some(*record)
        }).ok_or_else(|| format!("creature attack WEAP {source} is absent from source records"))?;
        let attack = super::creature_weapon_attacks::weapon_attack_evidence(record, interner)?;
        if attack.is_ranged() == ranged && !weapons.contains(source) {
            weapons.push(source.clone());
        }
    }
    Ok(weapons)
}

fn graph_events(graph: &CapabilityGraphManifest, role: CreatureClipRole) -> Vec<String> {
    let mut events = graph
        .roles
        .iter()
        .filter(|entry| entry.role == role && entry.generator.is_attack_entry())
        .filter_map(|entry| entry.trigger_event.clone())
        .collect::<Vec<_>>();
    events.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    events
}

fn attack_weapons_for_event(
    event_weapons: &BTreeMap<String, Vec<StableFormKey>>,
    event: &str,
    inventory_weapons: &[StableFormKey],
) -> Result<Vec<StableFormKey>, String> {
    let sources = event_weapons.get(&event.to_ascii_lowercase())
        .ok_or_else(|| format!("creature attack event {event} lacks a source WEAP binding"))?;
    Ok(inventory_weapons.iter().filter(|weapon| sources.iter().any(|source| {
        source.local == weapon.local && source.plugin.eq_ignore_ascii_case(&weapon.plugin)
    })).cloned().collect())
}

fn selected_attack_weapon_sources(
    selected: &SelectedGraph,
    family: &CreatureFamilyRecipeJob,
) -> BTreeMap<String, Vec<StableFormKey>> {
    let mut entries = BTreeMap::new();
    for clip in &selected.clips {
        if clip.attack_blend_only { continue; }
        if !matches!(clip.role, CreatureClipRole::MeleeAttack | CreatureClipRole::ProjectileAttack | CreatureClipRole::ContinuousAttackStart) {
            continue;
        }
        let Some(event) = &clip.trigger_event else { continue; };
        if clip.automatic_attack.as_ref().is_some_and(|automatic| !automatic.generator.is_attack_entry()) {
            continue;
        }
        let weapons = if let Some(source) = &clip.source_weapon {
            vec![source.clone()]
        } else if clip.role == CreatureClipRole::MeleeAttack {
            Vec::new()
        } else {
            family.indexed_motion_evidence.iter().flat_map(|motion| &motion.weapon_attacks)
                .filter(|weapon| (weapon.is_ranged() != (clip.role == CreatureClipRole::MeleeAttack)) && weapon.matches_clip(&clip.candidate.source_kf, &clip.candidate.sequence_name))
                .map(|weapon| weapon.source.clone()).collect()
        };
        entries.insert(event.to_ascii_lowercase(), weapons);
    }
    entries
}

fn bind_attack_actor_action_conditions(
    plans: &mut [CreatureActorActionRecordPlan],
    assets: &PreparedLiveFamily,
    family: &CreatureFamilyRecipeJob,
    mapper: &MapperState,
    interner: &StringInterner,
) -> Result<(), String> {
    for plan in plans.iter_mut().filter(|plan| matches!(plan.requirement.kind, crate::source_rig::ActorActionKind::Melee | crate::source_rig::ActorActionKind::FireSingle | crate::source_rig::ActorActionKind::FireAuto | crate::source_rig::ActorActionKind::Throw)) {
        let event = plan.requirement.animation_event.to_ascii_lowercase();
        plan.graph_condition = assets.graph.roles.iter().find(|role| {
            role.trigger_event.as_ref().is_some_and(|value| value.eq_ignore_ascii_case(&event))
                || role.trigger_aliases.iter().any(|value| value.eq_ignore_ascii_case(&event))
        }).and_then(|role| role.generator.source_attack()).map(|contract| crate::source_rig::CreatureActorActionGraphCondition {
            name: contract.gate_variable.clone(), value: contract.group,
        });
        let sources = assets.attack_weapon_sources.get(&event)
            .ok_or_else(|| format!("projectile Actor Action {event} lacks exact source WEAP"))?;
        if sources.is_empty() && plan.requirement.kind == crate::source_rig::ActorActionKind::Melee {
            plan.weapon_exclusions = family.indexed_motion_evidence.iter().flat_map(|motion| &motion.weapon_attacks)
                .filter(|weapon| !weapon.is_ranged())
                .map(|weapon| mapped_target_reference(&weapon.source, "WEAP", mapper, interner).map(|target| target.form_key))
                .collect::<Result<Vec<_>, _>>()?;
            continue;
        }
        let [source] = sources.as_slice() else {
            return Err(format!("projectile Actor Action {event} has {} source weapons", sources.len()));
        };
        let weapon = family.indexed_motion_evidence.iter().flat_map(|motion| &motion.weapon_attacks)
            .find(|weapon| weapon.source.local == source.local && weapon.source.plugin.eq_ignore_ascii_case(&source.plugin))
            .ok_or_else(|| format!("projectile Actor Action {event} lacks source weapon evidence"))?;
        if weapon.equipment_index.is_none() {
            return Err(format!("weapon Actor Action {event} lacks an exact equipment index"));
        }
        plan.weapon_condition = Some(crate::source_rig::CreatureActorActionWeaponCondition {
            weapon: mapped_target_reference(source, "WEAP", mapper, interner)?.form_key,
            equipment_index: weapon.equipment_index,
        });
    }
    Ok(())
}

fn exact_legacy_attack_projection_policy(
    effective_records: &[&Record],
) -> Result<LegacyAttackProjectionPolicy, String> {
    for record in effective_records {
        if let Some(field) = record
            .fields
            .iter()
            .find(|field| matches!(field.sig.as_str(), "ATKD" | "ATKE"))
        {
            return Err(format!(
                "effective legacy CREA {:06X} contains unsupported per-attack {} data",
                record.form_key.local,
                field.sig.as_str()
            ));
        }
    }
    Ok(LegacyAttackProjectionPolicy {
        damage_multiplier: 1.0,
        chance: 1.0,
        strike_angle: 0.0,
        action_point_cost: 0.0,
        target_data: CreatureAttackTargetData::default(),
    })
}

fn is_legacy_inert_attack_policy_field(field: &str) -> bool {
    field.starts_with("projection.attacks[")
        && [
            ".damage_multiplier",
            ".chance",
            ".strike_angle",
            ".action_point_cost",
            ".target_data.attack_angle",
            ".target_data.knockdown",
            ".target_data.recovery_time",
            ".target_data.action_points_multiplier",
            ".target_data.stagger_offset",
        ]
        .iter()
        .any(|suffix| field.ends_with(suffix))
}

fn exact_melee_attack_seconds(family: &CreatureFamilyRecipeJob, candidate: &MotionCandidate) -> Result<f32, String> {
    let motion = family
        .indexed_motion_evidence
        .as_ref()
        .ok_or_else(|| "Ready family has no indexed KF timing evidence".to_string())?;
    let sequence = motion
        .kf_evidence
        .iter()
        .filter(|entry| {
            canonical_mesh_path(&entry.source_kf) == canonical_mesh_path(&candidate.source_kf)
        })
        .filter_map(|entry| match &entry.sequence {
            KfParseEvidence::Parsed(sequence)
                if sequence.sequence_index == candidate.sequence_index =>
            {
                Some(sequence)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [sequence] = sequence.as_slice() else {
        return Err(format!(
            "melee KF timing selection resolved {} exact sequences",
            sequence.len()
        ));
    };
    let seconds = (sequence.stop_time - sequence.start_time) / sequence.frequency;
    if !seconds.is_finite() || seconds <= 0.0 || seconds > f64::from(f32::MAX) {
        return Err(format!("invalid source melee sequence duration {seconds}"));
    }
    Ok(seconds as f32)
}

fn mapped_target_reference(
    source: &StableFormKey,
    signature: &str,
    mapper_state: &MapperState,
    interner: &StringInterner,
) -> Result<CreatureTargetRecordReference, String> {
    let source_key = FormKey {
        local: source.local,
        plugin: interner.intern(&source.plugin),
    };
    let target = mapper_state
        .source_to_target
        .get(&source_key)
        .copied()
        .ok_or_else(|| {
            format!("{signature} dependency {source} has no translated target mapping")
        })?;
    let plugin = interner
        .resolve(target.plugin)
        .ok_or_else(|| format!("target plugin symbol for {source} is unresolved"))?;
    Ok(CreatureTargetRecordReference::new(
        signature,
        TargetFormKey::new(target.local, plugin),
    ))
}

fn exact_weapon_set_runtime_dependencies(
    source_weapons: &[StableFormKey],
    candidate: &CreatureDependencyCandidateReceipt,
    records: &BTreeMap<StableFormKey, &RuntimeDependencyRecordReceipt>,
    mapper_state: &MapperState,
    interner: &StringInterner,
) -> Result<
    (
        Option<CreatureTargetRecordReference>,
        Option<CreatureTargetRecordReference>,
    ),
    String,
> {
    let mut ammunition = BTreeSet::new();
    let mut projectiles = BTreeSet::new();
    for source_weapon in source_weapons {
        let weapon = records
            .get(source_weapon)
            .copied()
            .ok_or_else(|| format!("WEAP dependency receipt {source_weapon} is absent"))?;
        ammunition.extend(
            direct_followed_sources(weapon, "AMMO")
                .into_iter()
                .filter(|source| candidate.closure_form_keys.contains(source)),
        );
        projectiles.extend(
            direct_followed_sources(weapon, "PROJ")
                .into_iter()
                .filter(|source| candidate.closure_form_keys.contains(source)),
        );
    }
    for source_ammo in &ammunition {
        if let Some(ammo) = records.get(source_ammo) {
            projectiles.extend(
                direct_followed_sources(ammo, "PROJ")
                    .into_iter()
                    .filter(|source| candidate.closure_form_keys.contains(source)),
            );
        }
    }
    let projectile = (projectiles.len() == 1)
        .then(|| {
            projectiles
                .iter()
                .next()
                .expect("single projectile checked")
        })
        .map(|source| mapped_target_reference(source, "PROJ", mapper_state, interner))
        .transpose()?;
    let ammunition = (ammunition.len() == 1)
        .then(|| ammunition.iter().next().expect("single ammunition checked"))
        .map(|source| mapped_target_reference(source, "AMMO", mapper_state, interner))
        .transpose()?;
    Ok((projectile, ammunition))
}

fn projection_target_dependencies(
    projection: &CreatureRecordProjectionManifest,
) -> Vec<CreatureTargetRecordReference> {
    let mut dependencies = projection.npc_spells.clone();
    dependencies.extend(projection.race_keywords.iter().cloned());
    dependencies.extend(projection.templates.iter().flat_map(|templates| templates.dependencies()));
    dependencies.extend(projection.equipment_slots.iter().map(|binding| binding.equip_slot.clone()));
    dependencies.extend(projection.body_parts.dependencies());
    dependencies.extend(projection.npc_equipment.clone());
    dependencies.extend(projection.npc_death_item.iter().cloned());
    extend_inventory_dependencies(&mut dependencies, &projection.npc_inventory);
    for variant in &projection.variants {
        dependencies.extend(variant.actor_data.iter().flat_map(|data| data.dependencies()));
        dependencies.extend(
            variant
                .npc_spells
                .as_ref()
                .unwrap_or(&projection.npc_spells)
                .clone(),
        );
        dependencies.extend(
            variant
                .npc_equipment
                .as_ref()
                .unwrap_or(&projection.npc_equipment)
                .clone(),
        );
        dependencies.extend(
            variant
                .npc_death_item
                .as_ref()
                .map_or_else(|| projection.npc_death_item.as_ref(), |item| item.as_ref())
                .cloned(),
        );
        extend_inventory_dependencies(
            &mut dependencies,
            variant
                .npc_inventory
                .as_ref()
                .unwrap_or(&projection.npc_inventory),
        );
    }
    for attack in &projection.attacks {
        match &attack.projection {
            CreatureAttackRecordProjection::MeleeUnarmed { .. } => {}
            CreatureAttackRecordProjection::MeleeEquipment { equipment } => {
                dependencies.extend(equipment.clone());
            }
            CreatureAttackRecordProjection::RangedProjectile {
                attack_spell,
                projectile,
                equipment,
            }
            | CreatureAttackRecordProjection::Stationary {
                attack_spell,
                projectile,
                equipment,
            } => dependencies.extend([attack_spell.clone(), projectile.clone(), equipment.clone()]),
            CreatureAttackRecordProjection::RangedEquipment {
                equipment,
                projectile,
                ammunition,
            } => {
                dependencies.extend(equipment.clone());
                dependencies.extend(projectile.iter().cloned());
                dependencies.extend(ammunition.iter().cloned());
            }
            CreatureAttackRecordProjection::SpellAbility { spell } => {
                dependencies.push(spell.clone());
            }
            CreatureAttackRecordProjection::ContinuousRobot {
                attack_spell,
                equipment,
            } => dependencies.extend([attack_spell.clone(), equipment.clone()]),
        }
    }
    dependencies.sort_by_key(|reference| {
        (
            reference.signature.clone(),
            reference.form_key.plugin.to_ascii_lowercase(),
            reference.form_key.local,
        )
    });
    dependencies.dedup();
    dependencies
}

fn extend_inventory_dependencies(
    dependencies: &mut Vec<CreatureTargetRecordReference>,
    inventory: &[CreatureNpcInventoryEntry],
) {
    for entry in inventory {
        dependencies.push(entry.target_record.clone());
        match &entry.ownership {
            Some(CreatureNpcInventoryOwnership::FactionRank { faction, .. }) => {
                dependencies.push(faction.clone());
            }
            Some(CreatureNpcInventoryOwnership::OwnerGlobal { owner, global, .. }) => {
                dependencies.extend(owner.iter().cloned());
                dependencies.extend(global.iter().cloned());
            }
            None => {}
        }
    }
}

fn prepare_live_family(
    family: &CreatureFamilyRecipeJob,
    _evidence: &FnvFo3LiveRecipeEvidence,
    source_data_root: &Path,
    private_staging_root: &Path,
    source_records: &BTreeMap<(LegacyCreatureGame, StableFormKey), &Record>,
    interner: &StringInterner,
) -> Result<PreparedLiveFamily, String> {
    let source_data_root = game_data_root(source_data_root, family.rig.game);
    let selected = select_family_graph(family)?;
    let family_id = canonical_fnv_fo3_family_id(&family.rig);
    let namespace_hash = blake3::hash(family_id.as_bytes()).to_hex().to_string();
    let staging_namespace = format!("b21_fnvfo3_{}", &namespace_hash[..16]);
    let creature_name = format!("FnvFo3Creature_{}", &namespace_hash[..16]);
    let target_namespace = format!("actors/{creature_name}");
    let runtime_root = format!("Actors\\{creature_name}");
    let source_game = legacy_game_name(family.rig.game);
    let source_owner = family_id.clone();
    let member_body_parts = family.members.iter().filter_map(|member| source_records.get(&(family.rig.game, member.clone())).copied()).map(|record| exact_optional_record_form_key(record, "PNAM", interner)).collect::<Result<Vec<_>,_>>()?.into_iter().flatten().collect::<BTreeSet<_>>();
    let source_body_parts = source_records.iter().filter(|((game, _), record)| *game == family.rig.game && record.sig.as_str() == "BPTD")
        .filter(|((_, key), record)| member_body_parts.contains(key) || record.fields.iter().any(|field| field.sig.as_str() == "MODL" && matches!(&field.value, FieldValue::String(value) if interner.resolve(*value).is_some_and(|path| canonical_mesh_path(&path) == canonical_mesh_path(&family.rig.skeleton_path)))))
        .map(|(_, record)| *record)
        .collect::<Vec<_>>();
    let body_part_segments = crate::source_rig::body_parts::creature_body_part_segments(
        &source_body_parts,
        source_game,
        interner,
    )?;

    let mut closure_inputs = vec![CreatureNifInput {
        role: CreatureNifRole::Skeleton,
        source_data_relative_path: data_relative_mesh_path(&family.rig.skeleton_path),
        source_owner: source_owner.clone(),
        body_variant: None,
    }];
    closure_inputs.extend(crate::source_rig::body_parts::creature_body_part_replacement_models(&source_body_parts, interner).into_iter().map(|path| CreatureNifInput {
        role: CreatureNifRole::BodyPartReplacement,
        source_data_relative_path: data_relative_mesh_path(&path),
        source_owner: source_owner.clone(),
        body_variant: None,
    }));
    let mut body_paths = family
        .body_variants
        .iter()
        .flat_map(|body| body.body_paths.iter())
        .map(|path| canonical_mesh_path(path))
        .filter(|path| source_mesh_path(&source_data_root, path).is_file())
        .collect::<BTreeSet<_>>();
    if body_paths.is_empty() {
        return Err("Ready family contains no source body NIF".to_string());
    }
    closure_inputs.extend(
        body_paths
            .iter()
            .enumerate()
            .map(|(index, path)| CreatureNifInput {
                role: CreatureNifRole::Body,
                source_data_relative_path: data_relative_mesh_path(path),
                source_owner: source_owner.clone(),
                body_variant: Some(format!("body_{index:04}")),
            }),
    );
    let family_staging_root = private_staging_root
        .join("families")
        .join(&staging_namespace);
    let closure_staging_root = family_staging_root.join("closure");
    let mut closure = stage_creature_nif_closure_with_body_parts(
        &CreatureClosureRequest {
            source_game: source_game.to_string(),
            source_data_root: source_data_root.clone(),
            private_staging_root: closure_staging_root.clone(),
            target_namespace,
            texture_fallbacks: BTreeMap::from([(
                "textures/creatures/deathclaw/deathclaw_alphamale.dds".to_string(),
                "textures/creatures/deathclaw/deathclaw.dds".to_string(),
            )]),
            inputs: closure_inputs,
        },
        &body_part_segments,
    )
    .map_err(|error| format!("creature NIF closure failed: {error}"))?;
    let closure_staged_data_root = closure_staging_root.join("data");
    let replacement_runtime_paths = closure.inputs.iter().filter(|input| input.role == CreatureNifRole::BodyPartReplacement).map(|input| (canonical_mesh_path(&input.source_data_relative_path), runtime_nif_path(&input.target_data_relative_path))).collect();
    let visual_skeleton_input = closure
        .inputs
        .iter()
        .find(|input| input.role == CreatureNifRole::Skeleton)
        .ok_or_else(|| "creature closure omitted the visual skeleton NIF".to_string())?;
    let visual_skeleton_target = visual_skeleton_input.target_data_relative_path.clone();
    let visual_skeleton_nif = runtime_nif_path(&visual_skeleton_target);
    let visual_skeleton_path = closure_staged_data_root
        .join(visual_skeleton_target.replace('/', std::path::MAIN_SEPARATOR_STR));
    let mut body_runtime_paths = BTreeMap::new();
    for input in closure
        .inputs
        .iter()
        .filter(|input| input.role == CreatureNifRole::Body)
    {
        body_runtime_paths.insert(
            canonical_mesh_path(&input.source_data_relative_path),
            runtime_nif_path(&input.target_data_relative_path),
        );
    }
    if body_paths
        .iter()
        .any(|path| !body_runtime_paths.contains_key(path))
    {
        return Err("creature closure did not retain every body variant".to_string());
    }

    let skeleton_claim = exact_asset_claim(
        family,
        &family.rig.skeleton_path,
        IndexedAssetKind::Skeleton,
    )?;
    let skeleton_runtime_path = format!("{runtime_root}\\CharacterAssets\\Skeleton.hkx");
    let skeleton_source_path = source_mesh_path(&source_data_root, &family.rig.skeleton_path);
    let skeleton_runtime_name = skeleton_claim
        .skeleton_runtime_name
        .as_deref()
        .ok_or_else(|| "source skeleton runtime name evidence is absent".to_string())?;
    let skeleton = crate::phase::skeleton::convert_source_owned_skeleton_artifact(
        &skeleton_source_path,
        &skeleton_runtime_path,
        skeleton_runtime_name,
        &[],
    )
    .map_err(|error| format!("source-owned skeleton conversion failed: {error}"))?;

    let converted_root = family_staging_root.join("converted");
    let mut artifact_requests = Vec::new();
    let mut artifact_receipts = Vec::new();
    let mut converted_artifacts = Vec::new();
    stage_converted_artifact(
        &converted_root,
        SourceRigArtifactRole::AnimationSkeleton,
        SourceRigArtifactProvenance::ReconstructedSourceSkeleton,
        &skeleton_runtime_path,
        skeleton_claim.content_hash_blake3.clone(),
        &skeleton.hkx_bytes,
        source_game,
        &mut artifact_requests,
        &mut artifact_receipts,
        &mut converted_artifacts,
    )?;

    let mut clip_declarations = Vec::new();
    let mut property_clips = Vec::new();
    let mut clip_motion_policies = BTreeMap::new();
    let mut natural_attack_seconds = BTreeMap::new();
    for selection in &selected.clips {
        let runtime_path = format!("{runtime_root}\\Animations\\{}.hkx", selection.name);
        let source_path = source_mesh_path(&source_data_root, &selection.candidate.source_kf);
        let source_bytes = fs::read(&source_path)
            .map_err(|error| format!("read source KF {}: {error}", source_path.display()))?;
        let event_map = HashMap::new();
        let mut request = stage_request_from_motion_candidate(
            &selection.candidate,
            &family.rig.skeleton_path,
            &skeleton,
            &runtime_path,
            &event_map,
            None,
        )
        .map_err(|error| {
            format!(
                "KF request {} failed: {error}",
                selection.candidate.source_kf
            )
        })?;
        creature_action_selection::configure_attack_request(family, selection, &mut request)?;
        creature_property_sequences::capture(&mut property_clips, &source_bytes, &request,
            &selection.candidate, &selection.name, selection.target_looping, selection.source_idle.as_ref())?;
        let staged = crate::phase::animations::stage_fnv_creature_kf(&source_bytes, request)
            .map_err(|error| {
                format!(
                    "KF conversion {} failed: {error}",
                    selection.candidate.source_kf
                )
            })?;
        if selection.role == CreatureClipRole::MeleeAttack && selection.source_weapon.is_none() && !selection.attack_blend_only
            && let Some(event) = &selection.trigger_event
        {
            natural_attack_seconds.insert(event.to_ascii_lowercase(), exact_melee_attack_seconds(family, &selection.candidate)?);
        }
        clip_motion_policies.insert(
            selection.name.clone(),
            staged_clip_motion_policy(&staged.receipt.motion),
        );
        let binding = staged.receipt.binding.clone();
        clip_declarations.push(ClipDecl {
            name: selection.name.clone(),
            path: runtime_path.clone(),
            binding,
            looping: selection.target_looping,
        });
        stage_converted_artifact(
            &converted_root,
            SourceRigArtifactRole::AnimationClip {
                clip_name: selection.name.clone(),
            },
            SourceRigArtifactProvenance::ConvertedSourceClip,
            &runtime_path,
            motion_candidate_hash(&selection.candidate),
            &staged.hkx_bytes,
            source_game,
            &mut artifact_requests,
            &mut artifact_receipts,
            &mut converted_artifacts,
        )?;
    }
    for overlay in &selected.overlays {
        let runtime_path = format!("{runtime_root}\\Animations\\{}.hkx", overlay.clip_name);
        let source_path = source_mesh_path(&source_data_root, &overlay.candidate.source_kf);
        let source_bytes = fs::read(&source_path)
            .map_err(|error| format!("read source KF {}: {error}", source_path.display()))?;
        let event_map = HashMap::new();
        let request = stage_request_from_motion_candidate(
            &overlay.candidate,
            &family.rig.skeleton_path,
            &skeleton,
            &runtime_path,
            &event_map,
            None,
        )
        .map_err(|error| format!("overlay KF request failed: {error}"))?;
        creature_property_sequences::capture(&mut property_clips, &source_bytes, &request,
            &overlay.candidate, &overlay.clip_name, overlay.candidate.looping, None)?;
        let staged = crate::phase::animations::stage_fnv_creature_kf(&source_bytes, request)
            .map_err(|error| format!("overlay KF conversion failed: {error}"))?;
        clip_motion_policies.insert(
            overlay.clip_name.clone(),
            staged_clip_motion_policy(&staged.receipt.motion),
        );
        let binding = staged.receipt.binding.clone();
        clip_declarations.push(ClipDecl {
            name: overlay.clip_name.clone(),
            path: runtime_path.clone(),
            binding,
            looping: overlay.candidate.looping,
        });
        stage_converted_artifact(
            &converted_root,
            SourceRigArtifactRole::AnimationClip {
                clip_name: overlay.clip_name.clone(),
            },
            SourceRigArtifactProvenance::ConvertedSourceClip,
            &runtime_path,
            motion_candidate_hash(&overlay.candidate),
            &staged.hkx_bytes,
            source_game,
            &mut artifact_requests,
            &mut artifact_receipts,
            &mut converted_artifacts,
        )?;
    }

    let property_sequences = creature_property_sequences::attach(&property_clips, &mut closure, &closure_staged_data_root)?;
    let mut embedded_ragdoll_body_count = None;
    let ragdoll = match family.ragdoll.as_ref() {
        Some(RagdollCapabilityEvidence::Supported { owner_asset, .. }) => {
            let source = source_mesh_path(&source_data_root, owner_asset);
            let nif = NifFile::load(&source)
                .map_err(|error| format!("load ragdoll owner {}: {error}", source.display()))?;
            let ir = extract_fnv_fo3_creature_ragdoll(&nif)
                .map_err(|error| format!("extract source ragdoll: {error}"))?;
            let lowered = lower_primitive_shapes_to_fo4_convex_hulls(&ir, 12)
                .map_err(|error| format!("lower source ragdoll shapes: {error}"))?;
            let mut visual_skeleton = NifFile::load(&visual_skeleton_path).map_err(|error| {
                format!(
                    "load staged visual skeleton {}: {error}",
                    visual_skeleton_path.display()
                )
            })?;
            let body_count = install_fo4_creature_ragdoll_collision(&mut visual_skeleton, &lowered)
                .map_err(|error| format!("install embedded FO4 ragdoll collision: {error}"))?;
            visual_skeleton
                .save(Some(visual_skeleton_path.clone()))
                .map_err(|error| format!("save ragdoll-bearing visual skeleton: {error}"))?;
            embedded_ragdoll_body_count = Some(body_count);
            let bytes = reconstruct_fo4_creature_ragdoll_packfile(&lowered)
                .map_err(|error| format!("reconstruct FO4 ragdoll: {error}"))?;
            let runtime_path = format!("{runtime_root}\\CharacterAssets\\Ragdoll.hkx");
            let claim = exact_asset_claim(family, owner_asset, IndexedAssetKind::Skeleton)?;
            stage_converted_artifact(
                &converted_root,
                SourceRigArtifactRole::Ragdoll,
                SourceRigArtifactProvenance::ReconstructedSourceRagdoll,
                &runtime_path,
                claim.content_hash_blake3.clone(),
                &bytes,
                source_game,
                &mut artifact_requests,
                &mut artifact_receipts,
                &mut converted_artifacts,
            )?;
            RagdollDisposition::SourceOwned {
                runtime_path,
                receipt: SourceOwnedRagdollReceipt {
                    byte_len: bytes.len() as u64,
                    blake3: blake3::hash(&bytes).to_hex().to_string(),
                    powered_ragdoll: None,
                },
            }
        }
        Some(RagdollCapabilityEvidence::ExplicitlyUnsupported { .. }) => {
            RagdollDisposition::NoRagdoll {
                reason: NoRagdollReason::UnsupportedSourceRagdoll,
            }
        }
        None => return Err("Ready family has no ragdoll disposition".to_string()),
    };

    let graph = build_capability_graph(&selected, Some(&clip_motion_policies))?;
    let mut root_declarations = graph_declarations(&graph, &ragdoll);
    let mut core_declarations = graph_declarations(
        &graph,
        &RagdollDisposition::NoRagdoll {
            reason: NoRagdollReason::UnsupportedSourceRagdoll,
        },
    );
    if graph.template == CreatureGraphTemplate::StationaryTurret {
        add_stationary_aim_declarations(&mut root_declarations);
        add_stationary_aim_declarations(&mut core_declarations);
    }
    let race = family
        .race_data
        .as_ref()
        .ok_or_else(|| "Ready family has no RACE.DATA evidence".to_string())?;
    let capsule = race
        .measured_evidence
        .capsule
        .as_ref()
        .ok_or_else(|| "Ready family has no source controller capsule".to_string())?;
    let mut visual_skeleton = NifFile::load(&visual_skeleton_path).map_err(|error| {
        format!(
            "reload staged visual skeleton {}: {error}",
            visual_skeleton_path.display()
        )
    })?;
    install_fo4_creature_controller_collision(
        &mut visual_skeleton,
        capsule.total_height / 69.99125,
        capsule.radius / 69.99125,
        [0.0, 0.0, 1.0],
        1,
    )
    .map_err(|error| format!("install FO4 CharacterController collision: {error}"))?;
    super::creature_muzzle::validate_weapon_attachment_nodes(
        &visual_skeleton,
        family.indexed_motion_evidence.as_ref().map(|motion| motion.weapon_attacks.as_slice()).unwrap_or_default(),
    )?;
    super::creature_muzzle::install_projectile_node_aliases(
        &mut visual_skeleton,
        &super::creature_muzzle::family_muzzle_nodes(family.indexed_motion_evidence.as_ref().map(|motion| motion.weapon_attacks.as_slice()).unwrap_or_default()),
    )?;
    visual_skeleton
        .save(Some(visual_skeleton_path.clone()))
        .map_err(|error| format!("save controller-bearing visual skeleton: {error}"))?;
    if let Some(body_count) = embedded_ragdoll_body_count {
        seal_embedded_creature_collision(
            &mut closure,
            &closure_staged_data_root,
            &visual_skeleton_target,
            body_count,
        )
        .map_err(|error| format!("seal embedded ragdoll NIF receipt: {error}"))?;
    } else {
        refresh_staged_creature_nif_artifact(
            &mut closure,
            &closure_staged_data_root,
            &visual_skeleton_target,
        )
        .map_err(|error| format!("refresh controller-bearing NIF receipt: {error}"))?;
    }
    let rig = CreatureManifest {
        property_sequences,
        creature_name,
        visual_skeleton_nif,
        animation_skeleton: SkeletonDecl {
            path: skeleton_runtime_path,
            runtime_name: skeleton.skeleton_name.clone(),
            bones: skeleton
                .ordered_bone_names
                .iter()
                .zip(&skeleton.parent_indices)
                .map(|(name, parent)| BoneDecl {
                    name: name.clone(),
                    parent_index: usize::try_from(*parent).ok(),
                })
                .collect(),
            float_slots: skeleton.float_slot_names.clone(),
        },
        controller: CreatureControllerDecl {
            collision_filter_info: 1,
            rigid_body_type: 255,
            model_up_ms: [0.0, 0.0, 1.0, 0.0],
            model_forward_ms: [1.0, 0.0, 0.0, 0.0],
            model_right_ms: [0.0, -1.0, 0.0, 0.0],
            model_scale: 1.0,
        },
        ragdoll,
        clips: clip_declarations,
        idle_clip: selected
            .clips
            .iter()
            .find(|clip| is_primary_idle(selected.template, clip.role))
            .map(|clip| clip.name.clone())
            .ok_or_else(|| "selected graph has no primary idle clip".to_string())?,
        capsule: Capsule {
            height: capsule.total_height,
            radius: capsule.radius,
        },
        paths: ScaffoldPaths {
            project: format!("{runtime_root}\\FnvFo3Project.hkx"),
            character: format!("{runtime_root}\\Characters\\FnvFo3Character.hkx"),
            root_behavior: format!("{runtime_root}\\Behaviors\\FnvFo3RootBehavior.hkx"),
            core_behavior: format!("{runtime_root}\\Behaviors\\FnvFo3CoreBehavior.hkx"),
        },
        root: root_declarations,
        core: core_declarations,
    };
    rig.validate_capability_graph(&graph)
        .map_err(|error| format!("live source-rig graph validation failed: {error}"))?;
    creature_property_sequences::instantiate_graph_sequences(&rig, &graph, &mut closure, &closure_staged_data_root)?;
    artifact_requests.sort_by_key(|request| {
        (
            request.runtime_path.to_ascii_lowercase(),
            request.role.clone(),
        )
    });
    artifact_receipts.sort_by_key(|receipt| {
        (
            receipt.runtime_path.to_ascii_lowercase(),
            receipt.role.clone(),
        )
    });
    converted_artifacts.sort();
    body_paths.clear();
    let attack_weapon_sources = selected_attack_weapon_sources(&selected, family);
    let power_attack_events = selected.clips.iter().filter(|clip| !clip.attack_blend_only && clip.role == CreatureClipRole::MeleeAttack)
        .filter(|clip| super::creature_weapon_attacks::exact_source_attack_group(&clip.candidate.source_kf, &clip.candidate.sequence_name)
            .is_some_and(|(group, _)| matches!(group, 92..=96)))
        .filter_map(|clip| clip.trigger_event.as_ref().map(|event| event.to_ascii_lowercase())).collect();
    Ok(PreparedLiveFamily {
        replacement_runtime_paths,
        body_part_segments,
        closure,
        closure_staged_data_root,
        rig,
        graph,
        attack_weapon_sources,
        power_attack_events,
        natural_attack_seconds,
        artifact_requests,
        artifact_receipts,
        converted_artifacts,
        body_runtime_paths,
    })
}

fn select_family_graph(family: &CreatureFamilyRecipeJob) -> Result<SelectedGraph, String> {
    let contract = family
        .graph_contract
        .as_ref()
        .ok_or_else(|| "Ready family has no graph contract".to_string())?;
    let has = |role| {
        family
            .required_motion
            .iter()
            .any(|entry| entry.role == role)
    };
    let has_ranged =
        || has(MotionRole::RangedAttack) || has(MotionRole::Fire) || has(MotionRole::Projectile);
    let template = match contract.capability {
        CreatureGraphCapability::PassiveGround => CreatureGraphTemplate::PassiveGround,
        CreatureGraphCapability::GroundMelee => CreatureGraphTemplate::GroundMelee,
        CreatureGraphCapability::GroundRangedProjectile => {
            CreatureGraphTemplate::GroundRangedProjectile
        }
        CreatureGraphCapability::GroundMeleeRanged => CreatureGraphTemplate::GroundMeleeRanged,
        CreatureGraphCapability::GroundSwim => CreatureGraphTemplate::GroundSwim,
        CreatureGraphCapability::GroundFly => CreatureGraphTemplate::GroundFly,
        CreatureGraphCapability::Swim => CreatureGraphTemplate::Swim,
        CreatureGraphCapability::Fly => CreatureGraphTemplate::Fly,
        CreatureGraphCapability::StationaryTurret => CreatureGraphTemplate::StationaryTurret,
        CreatureGraphCapability::RobotContinuousAttack => {
            CreatureGraphTemplate::RobotContinuousAttack
        }
        CreatureGraphCapability::HumanoidWeaponOverlay
            if has(MotionRole::MeleeAttack) && has_ranged() =>
        {
            CreatureGraphTemplate::GroundMeleeRanged
        }
        CreatureGraphCapability::HumanoidWeaponOverlay if has_ranged() => {
            CreatureGraphTemplate::GroundRangedProjectile
        }
        CreatureGraphCapability::HumanoidWeaponOverlay if has(MotionRole::MeleeAttack) => {
            CreatureGraphTemplate::GroundMelee
        }
        CreatureGraphCapability::HumanoidWeaponOverlay => CreatureGraphTemplate::PassiveGround,
    };
    let mut clips = Vec::new();
    let mut overlays = Vec::new();
    let mut used_events = BTreeSet::new();
    let idle_event = if matches!(
        template,
        CreatureGraphTemplate::StationaryTurret | CreatureGraphTemplate::RobotContinuousAttack
    ) {
        "g_defaultState".to_string()
    } else {
        "Idle".to_string()
    };
    used_events.insert(idle_event.to_ascii_lowercase());

    match template {
        CreatureGraphTemplate::GroundMelee
        | CreatureGraphTemplate::GroundRangedProjectile
        | CreatureGraphTemplate::GroundMeleeRanged
        | CreatureGraphTemplate::PassiveGround => {
            add_first_motion_clip(
                family,
                &mut clips,
                &mut used_events,
                CreatureClipRole::Idle,
                MotionRole::Idle,
                "idle",
                "Idle",
                false,
                false,
            )?;
            if has(MotionRole::GroundLocomotion) {
                add_first_motion_clip(
                    family,
                    &mut clips,
                    &mut used_events,
                    CreatureClipRole::GroundForward,
                    MotionRole::GroundLocomotion,
                    "ground_forward",
                    "GroundForward",
                    true,
                    false,
                )?;
            }
        }
        CreatureGraphTemplate::GroundSwim | CreatureGraphTemplate::GroundFly => {
            add_first_motion_clip(
                family,
                &mut clips,
                &mut used_events,
                CreatureClipRole::Idle,
                MotionRole::Idle,
                "idle",
                "GroundIdle",
                false,
                false,
            )?;
            add_first_motion_clip(
                family,
                &mut clips,
                &mut used_events,
                CreatureClipRole::GroundForward,
                MotionRole::GroundLocomotion,
                "ground_forward",
                "GroundForward",
                true,
                false,
            )?;
            let motion_role = if template == CreatureGraphTemplate::GroundSwim {
                MotionRole::Swim
            } else {
                MotionRole::Fly
            };
            let (idle_candidate, forward_candidate) = secondary_motion_pair(family, motion_role)?;
            add_selected_clip(
                &mut clips,
                &mut used_events,
                idle_candidate,
                if template == CreatureGraphTemplate::GroundSwim {
                    CreatureClipRole::SwimIdle
                } else {
                    CreatureClipRole::FlyIdle
                },
                "secondary_idle",
                "SecondaryIdle",
                true,
                false,
            )?;
            add_selected_clip(
                &mut clips,
                &mut used_events,
                forward_candidate,
                if template == CreatureGraphTemplate::GroundSwim {
                    CreatureClipRole::SwimForward
                } else {
                    CreatureClipRole::FlyForward
                },
                "secondary_forward",
                "SecondaryForward",
                true,
                false,
            )?;
        }
        CreatureGraphTemplate::Swim | CreatureGraphTemplate::Fly => {
            add_first_motion_clip(
                family,
                &mut clips,
                &mut used_events,
                if template == CreatureGraphTemplate::Swim {
                    CreatureClipRole::SwimIdle
                } else {
                    CreatureClipRole::FlyIdle
                },
                MotionRole::Idle,
                "idle",
                "Idle",
                false,
                false,
            )?;
            add_first_motion_clip(
                family,
                &mut clips,
                &mut used_events,
                if template == CreatureGraphTemplate::Swim {
                    CreatureClipRole::SwimForward
                } else {
                    CreatureClipRole::FlyForward
                },
                if template == CreatureGraphTemplate::Swim {
                    MotionRole::Swim
                } else {
                    MotionRole::Fly
                },
                "secondary_forward",
                "SecondaryForward",
                true,
                false,
            )?;
        }
        CreatureGraphTemplate::StationaryTurret => {
            let idle_role = if has(MotionRole::StationaryOrTurret) {
                MotionRole::StationaryOrTurret
            } else {
                MotionRole::Idle
            };
            add_first_motion_clip(
                family,
                &mut clips,
                &mut used_events,
                CreatureClipRole::StationaryIdle,
                idle_role,
                "stationary_idle",
                "StationaryIdle",
                false,
                false,
            )?;
        }
        CreatureGraphTemplate::RobotContinuousAttack => {
            add_first_motion_clip(
                family,
                &mut clips,
                &mut used_events,
                CreatureClipRole::Idle,
                MotionRole::Idle,
                "idle",
                "RobotIdle",
                false,
                false,
            )?;
            let continuous = motion_candidates(family, MotionRole::ContinuousOrRobot)?;
            let start = named_motion_candidate(continuous, &["start", "begin"])?;
            let looped = continuous
                .iter()
                .find(|candidate| candidate.looping)
                .cloned()
                .ok_or_else(|| "continuous robot family has no looping attack phase".to_string())?;
            let stop = named_motion_candidate(continuous, &["stop", "end", "release"])?;
            if motion_candidate_hash(&start) == motion_candidate_hash(&looped)
                || motion_candidate_hash(&start) == motion_candidate_hash(&stop)
                || motion_candidate_hash(&looped) == motion_candidate_hash(&stop)
            {
                return Err(
                    "continuous robot start/loop/stop phases are not three distinct source sequences"
                        .to_string(),
                );
            }
            for (candidate, role, name, state) in [
                (
                    start,
                    CreatureClipRole::ContinuousAttackStart,
                    "continuous_start",
                    "ContinuousStart",
                ),
                (
                    looped,
                    CreatureClipRole::ContinuousAttackLoop,
                    "continuous_loop",
                    "ContinuousLoop",
                ),
                (
                    stop,
                    CreatureClipRole::ContinuousAttackStop,
                    "continuous_stop",
                    "ContinuousStop",
                ),
            ] {
                add_selected_clip(
                    &mut clips,
                    &mut used_events,
                    candidate,
                    role,
                    name,
                    state,
                    true,
                    false,
                )?;
            }
        }
    }

    creature_action_selection::add_lifecycle_clips(family, &mut clips, &mut used_events)?;
    creature_action_selection::add_source_idle_clips(family, &mut clips, &mut used_events)?;

    if has(MotionRole::MeleeAttack)
        && matches!(
            template,
            CreatureGraphTemplate::GroundMelee
                | CreatureGraphTemplate::GroundMeleeRanged
                | CreatureGraphTemplate::GroundSwim
                | CreatureGraphTemplate::GroundFly
                | CreatureGraphTemplate::Swim
                | CreatureGraphTemplate::Fly
        )
    {
        add_motion_attack_clips(
            family,
            &mut clips,
            &mut used_events,
            CreatureClipRole::MeleeAttack,
            MotionRole::MeleeAttack,
            "melee_attack",
            "MeleeAttack",
            true,
            false,
        )?;
    }
    let has_projectile_clip = has_ranged()
        || (template == CreatureGraphTemplate::StationaryTurret && has(MotionRole::MeleeAttack));
    if has_projectile_clip
        && matches!(
            template,
            CreatureGraphTemplate::GroundRangedProjectile
                | CreatureGraphTemplate::GroundMeleeRanged
                | CreatureGraphTemplate::GroundSwim
                | CreatureGraphTemplate::GroundFly
                | CreatureGraphTemplate::Swim
                | CreatureGraphTemplate::Fly
                | CreatureGraphTemplate::StationaryTurret
        )
    {
        add_motion_attack_clips(
            family,
            &mut clips,
            &mut used_events,
            CreatureClipRole::ProjectileAttack,
            if has(MotionRole::RangedAttack) {
                MotionRole::RangedAttack
            } else if has(MotionRole::Fire) {
                MotionRole::Fire
            } else if has(MotionRole::Projectile) {
                MotionRole::Projectile
            } else {
                MotionRole::MeleeAttack
            },
            "projectile_attack",
            "ProjectileAttack",
            false,
            template == CreatureGraphTemplate::StationaryTurret,
        )?;
    }
    if contract.capability == CreatureGraphCapability::HumanoidWeaponOverlay {
        let candidate =
            first_stationary_motion_candidate_with_looping(family, MotionRole::Overlay, true)?
                .clone();
        let start_event = exact_overlay_event(&candidate, true)?;
        let stop_event = exact_overlay_event(&candidate, false)?;
        if !used_events.insert(start_event.to_ascii_lowercase())
            || !used_events.insert(stop_event.to_ascii_lowercase())
        {
            return Err("overlay start/stop events collide with graph events".to_string());
        }
        overlays.push(SelectedOverlay {
            candidate,
            name: "weapon_overlay".to_string(),
            clip_name: "weapon_overlay".to_string(),
            start_event,
            stop_event,
        });
    }
    creature_automatic_selection::expand(family, &mut clips)?;
    creature_attack_selection::attach(family, &mut clips)?;
    creature_aim_selection::attach(family, &mut clips)?;
    creature_movement_selection::attach(family, &mut clips)?;
    if let Some(motion) = &family.indexed_motion_evidence {
        for clip in &mut clips {
            clip.equipment_index = clip.source_weapon.as_ref().and_then(|source| {
                motion.weapon_attacks.iter().find(|weapon| &weapon.source == source)
                    .and_then(|weapon| weapon.equipment_index)
            });
        }
    }
    let source_channels = creature_channel_selection::collect(family, &clips, &overlays)?;
    Ok(SelectedGraph {
        source_channels,
        template,
        clips,
        overlays,
        idle_event,
    })
}

#[allow(clippy::too_many_arguments)]
fn add_motion_attack_clips(
    family: &CreatureFamilyRecipeJob,
    clips: &mut Vec<SelectedClip>,
    used_events: &mut BTreeSet<String>,
    role: CreatureClipRole,
    motion_role: MotionRole,
    name_prefix: &str,
    state_prefix: &str,
    melee_event: bool,
    project_looping_as_one_shot: bool,
) -> Result<(), String> {
    let motion_candidates = motion_candidates(family, motion_role)?;
    let mut candidates = motion_candidates
        .iter()
        .filter(|candidate| !candidate.looping || (role == CreatureClipRole::ProjectileAttack && creature_automatic_selection::is_automatic(family, candidate)))
        .cloned()
        .collect::<Vec<_>>();
    if candidates.is_empty() && project_looping_as_one_shot {
        candidates.extend(
            motion_candidates
                .iter()
                .filter(|candidate| candidate.looping)
                .cloned(),
        );
    }
    if candidates.is_empty() {
        return Err(format!(
            "Ready family has no non-looping {motion_role:?} attack candidate"
        ));
    }
    for (index, candidate) in candidates.into_iter().enumerate() {
        let automatic = role == CreatureClipRole::ProjectileAttack && creature_automatic_selection::is_automatic(family, &candidate);
        let ordinal = index + 1;
        add_selected_clip_with_cycle_projection(
            clips,
            used_events,
            candidate,
            role,
            &format!("{name_prefix}_{ordinal:02}"),
            &format!("{state_prefix}{ordinal}"),
            true,
            melee_event,
            project_looping_as_one_shot || automatic,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn add_first_motion_clip(
    family: &CreatureFamilyRecipeJob,
    clips: &mut Vec<SelectedClip>,
    used_events: &mut BTreeSet<String>,
    role: CreatureClipRole,
    motion_role: MotionRole,
    name: &str,
    state_name: &str,
    trigger: bool,
    melee_event: bool,
) -> Result<(), String> {
    if role == CreatureClipRole::GroundForward {
        return creature_action_selection::add_locomotion_clips(
            family,
            clips,
            used_events,
            name,
            state_name,
        );
    }
    let candidate =
        first_motion_candidate_with_looping(family, motion_role, clip_role_expects_looping(role))?
            .clone();
    add_selected_clip(
        clips,
        used_events,
        candidate,
        role,
        name,
        state_name,
        trigger,
        melee_event,
    )
}

fn add_selected_clip(
    clips: &mut Vec<SelectedClip>,
    used_events: &mut BTreeSet<String>,
    candidate: MotionCandidate,
    role: CreatureClipRole,
    name: &str,
    state_name: &str,
    trigger: bool,
    melee_event: bool,
) -> Result<(), String> {
    add_selected_clip_with_cycle_projection(
        clips,
        used_events,
        candidate,
        role,
        name,
        state_name,
        trigger,
        melee_event,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn add_selected_clip_with_cycle_projection(
    clips: &mut Vec<SelectedClip>,
    used_events: &mut BTreeSet<String>,
    candidate: MotionCandidate,
    role: CreatureClipRole,
    name: &str,
    state_name: &str,
    trigger: bool,
    melee_event: bool,
    project_looping_as_one_shot: bool,
) -> Result<(), String> {
    let expected_looping = clip_role_expects_looping(role);
    let cycle_projection_is_valid =
        project_looping_as_one_shot && candidate.looping && !expected_looping;
    if candidate.looping != expected_looping && !cycle_projection_is_valid {
        return Err(format!(
            "source sequence {} looping={} cannot satisfy {:?} looping={expected_looping}",
            candidate.source_kf, candidate.looping, role
        ));
    }
    let ordinal = clips.iter().filter(|clip| clip.role == role).count() + 1;
    let trigger_event = trigger
        .then(|| selected_trigger_event(&candidate, role, ordinal, melee_event, used_events))
        .transpose()?;
    clips.push(SelectedClip {
        thrown_attack: false,
        candidate,
        role,
        name: name.to_string(),
        state_name: state_name.to_string(),
        trigger_event,
        target_looping: expected_looping,
        locomotion_sample: None,
        source_idle: None,
        source_aim: None,
        source_attack: None,
        attack_blend_only: false,
        source_movement: None,
        automatic_attack: None,
        source_weapon: None,
        equipment_index: None,
    });
    Ok(())
}

fn clip_role_expects_looping(role: CreatureClipRole) -> bool {
    matches!(
        role,
        CreatureClipRole::Idle
            | CreatureClipRole::WeaponAim
            | CreatureClipRole::GroundForward
            | CreatureClipRole::SwimIdle
            | CreatureClipRole::SwimForward
            | CreatureClipRole::FlyIdle
            | CreatureClipRole::FlyForward
            | CreatureClipRole::StationaryIdle
            | CreatureClipRole::ContinuousAttackLoop
            | CreatureClipRole::TurnLeft
            | CreatureClipRole::TurnRight
            | CreatureClipRole::ScriptedLoop
    )
}

fn selected_trigger_event(
    candidate: &MotionCandidate,
    role: CreatureClipRole,
    ordinal: usize,
    melee: bool,
    used_events: &mut BTreeSet<String>,
) -> Result<String, String> {
    let event = candidate.events.iter().find(|event| {
        if !melee {
            return false;
        }
        let normalized = event.raw_text.to_ascii_lowercase();
        let unused = !used_events.contains(&normalized);
        unused && event.raw_text.starts_with("melee") && event.raw_text.len() > "melee".len()
    });
    if let Some(event) = event {
        used_events.insert(event.raw_text.to_ascii_lowercase());
        return Ok(event.raw_text.clone());
    }
    let event = deterministic_standard_event(role, "fnvfo3", ordinal);
    if !used_events.insert(event.to_ascii_lowercase()) {
        return Err(format!(
            "deterministic fallback event {event:?} collides in the generated graph"
        ));
    }
    Ok(event)
}

fn exact_overlay_event(candidate: &MotionCandidate, start: bool) -> Result<String, String> {
    let event = candidate.events.iter().find(|event| {
        let text = event.raw_text.to_ascii_lowercase();
        if start {
            event.kind == MotionEventKind::Begin || text.contains("start") || text.ends_with("on")
        } else {
            event.kind == MotionEventKind::End
                || text.contains("stop")
                || text.contains("release")
                || text.ends_with("off")
        }
    });
    event.map(|event| event.raw_text.clone()).ok_or_else(|| {
        format!(
            "overlay sequence {} lacks an exact {} event",
            candidate.source_kf,
            if start { "start" } else { "stop" }
        )
    })
}

fn first_motion_candidate_with_looping(
    family: &CreatureFamilyRecipeJob,
    role: MotionRole,
    looping: bool,
) -> Result<&MotionCandidate, String> {
    motion_candidates(family, role)?
        .iter()
        .filter(|candidate| candidate.looping == looping)
        .min_by_key(|candidate| {
            role == MotionRole::Idle && !candidate.sequence_name.eq_ignore_ascii_case("Idle")
        })
        .ok_or_else(|| {
            format!("Ready family has no {role:?} motion candidate with looping={looping}")
        })
}

fn first_stationary_motion_candidate_with_looping(
    family: &CreatureFamilyRecipeJob,
    role: MotionRole,
    looping: bool,
) -> Result<&MotionCandidate, String> {
    motion_candidates(family, role)?
        .iter()
        .filter(|candidate| {
            candidate.looping == looping
                && matches!(
                    candidate.root_motion,
                    RootMotionEvidence::None | RootMotionEvidence::Stationary { .. }
                )
        })
        .min_by_key(|candidate| {
            role == MotionRole::Idle && !candidate.sequence_name.eq_ignore_ascii_case("Idle")
        })
        .ok_or_else(|| {
            format!(
                "Ready family has no stationary {role:?} motion candidate with looping={looping}"
            )
        })
}

fn motion_candidates(
    family: &CreatureFamilyRecipeJob,
    role: MotionRole,
) -> Result<&[MotionCandidate], String> {
    family
        .required_motion
        .iter()
        .find(|required| required.role == role)
        .map(|required| required.candidates.as_slice())
        .filter(|candidates| !candidates.is_empty())
        .ok_or_else(|| format!("Ready family has no {role:?} motion evidence"))
}

fn secondary_motion_pair(
    family: &CreatureFamilyRecipeJob,
    role: MotionRole,
) -> Result<(MotionCandidate, MotionCandidate), String> {
    let candidates = motion_candidates(family, role)?;
    let idle = candidates
        .iter()
        .find(|candidate| {
            candidate.looping
                && matches!(
                    candidate.root_motion,
                    RootMotionEvidence::None | RootMotionEvidence::Stationary { .. }
                )
        })
        .cloned()
        .ok_or_else(|| format!("{role:?} motion has no looping stationary idle sequence"))?;
    let forward = candidates
        .iter()
        .find(|candidate| {
            candidate.looping
                && matches!(candidate.root_motion, RootMotionEvidence::Planar { .. })
                && motion_candidate_hash(candidate) != motion_candidate_hash(&idle)
        })
        .cloned()
        .ok_or_else(|| format!("{role:?} motion has no distinct looping planar sequence"))?;
    Ok((idle, forward))
}

fn named_motion_candidate(
    candidates: &[MotionCandidate],
    tokens: &[&str],
) -> Result<MotionCandidate, String> {
    candidates
        .iter()
        .find(|candidate| {
            let name = candidate.sequence_name.to_ascii_lowercase();
            let path = candidate.source_kf.to_ascii_lowercase();
            tokens
                .iter()
                .any(|token| name.contains(token) || path.contains(token))
        })
        .cloned()
        .ok_or_else(|| format!("motion set has no sequence matching {tokens:?}"))
}

fn build_capability_graph(
    selected: &SelectedGraph,
    clip_motion_policies: Option<&BTreeMap<String, ClipMotionPolicy>>,
) -> Result<CapabilityGraphManifest, String> {
    let mut explicit_events = BTreeMap::<String, EventDecl>::new();
    let insert_event = |events: &mut BTreeMap<String, EventDecl>, name: &str, usage| {
        events.insert(
            name.to_ascii_lowercase(),
            EventDecl {
                name: name.to_string(),
                usage,
                flags: 0,
            },
        );
    };
    insert_event(
        &mut explicit_events,
        &selected.idle_event,
        EventUsage::Generic,
    );
    if selected.clips.iter().any(|clip| clip.automatic_attack.is_some()) {
        insert_event(&mut explicit_events, "attackStop", EventUsage::Generic);
        insert_event(&mut explicit_events, "attackRelease", EventUsage::Generic);
    }
    if selected.clips.iter().any(|clip| clip.thrown_attack) {
        insert_event(&mut explicit_events, "weaponAttach", EventUsage::Generic);
        insert_event(&mut explicit_events, "weaponDetach", EventUsage::Generic);
    }
    if selected.clips.iter().any(|clip| matches!(clip.role, CreatureClipRole::ScriptedAction | CreatureClipRole::ScriptedLoop)) {
        insert_event(&mut explicit_events, "IdleStop", EventUsage::Generic);
    }
    for clip in &selected.clips {
        if let Some(CapabilityRoleGenerator::SourceIdle { complete_event, .. }) = &clip.source_idle {
            insert_event(&mut explicit_events, complete_event, EventUsage::Generic);
        }
    }
    if selected.clips.iter().any(|clip| clip.role == CreatureClipRole::MeleeAttack) {
        for name in ["preHitFrame", "weaponSwing", "HitFrame", "attackWinStart", "attackStop"] {
            insert_event(&mut explicit_events, name, EventUsage::Generic);
        }
    }
    if selected.clips.iter().any(|clip| matches!(clip.role,
        CreatureClipRole::ProjectileAttack | CreatureClipRole::ContinuousAttackStart
        | CreatureClipRole::ContinuousAttackLoop | CreatureClipRole::ContinuousAttackStop
    )) {
        insert_event(&mut explicit_events, "weaponFire", EventUsage::Generic);
        for clip in &selected.clips {
            let name = clip.equipment_index.map(|index| format!("attackState{index}"))
                .unwrap_or_else(|| "attackState".to_string());
            insert_event(&mut explicit_events, &name, EventUsage::Generic);
        }
    }
    let roles = selected
        .clips
        .iter()
        .filter(|clip| !clip.attack_blend_only && (clip.locomotion_sample.is_none() || clip.trigger_event.is_some()))
        .map(|clip| {
            if let Some(event) = &clip.trigger_event {
                insert_event(
                    &mut explicit_events,
                    event,
                    if clip.role == CreatureClipRole::MeleeAttack {
                        EventUsage::MeleeAttack
                    } else {
                        EventUsage::Generic
                    },
                );
            }
            CapabilityClipRole {
                equipment_index: clip.equipment_index,
                role: clip.role,
                state_name: clip.state_name.clone(),
                clip_name: clip.name.clone(),
                generator: {
                    let base = clip.source_aim.clone().or_else(|| clip.thrown_attack.then_some(CapabilityRoleGenerator::ThrownAttack)).or_else(|| clip.automatic_attack.as_ref().map(|attack| attack.generator.clone()))
                        .unwrap_or_else(|| creature_action_selection::role_generator(selected, clip));
                    let base = creature_movement_selection::wrap(selected, clip, base);
                    clip.source_attack.as_ref().map(|contract| CapabilityRoleGenerator::SourceAttack { contract:contract.clone(), generator:Box::new(base.clone()) }).unwrap_or(base)
                },
                trigger_event: clip.trigger_event.clone(),
                trigger_aliases: Vec::new(),
                motion: clip_motion_policies
                    .and_then(|policies| policies.get(&clip.name))
                    .copied()
                    .unwrap_or_else(|| motion_policy(&clip.candidate)),
            }
        })
        .collect::<Vec<_>>();
    if roles
        .iter()
        .any(|role| role.role == CreatureClipRole::Death)
    {
        insert_event(
            &mut explicit_events,
            "B21_DeathAnimationComplete",
            EventUsage::Generic,
        );
    }
    for role in &roles {
        if role.generator.source_attack().is_some_and(|contract| contract.window.is_some()) {
            for event in crate::source_rig::SourceAttackContract::window_events(&role.state_name) {
                insert_event(&mut explicit_events, &event, EventUsage::Generic);
            }
        }
    }
    let overlays = selected
        .overlays
        .iter()
        .map(|overlay| {
            insert_event(
                &mut explicit_events,
                &overlay.start_event,
                EventUsage::Generic,
            );
            insert_event(
                &mut explicit_events,
                &overlay.stop_event,
                EventUsage::Generic,
            );
            OverlayClipRole {
                name: overlay.name.clone(),
                clip_name: overlay.clip_name.clone(),
                start_event: overlay.start_event.clone(),
                stop_event: overlay.stop_event.clone(),
                motion: clip_motion_policies
                    .and_then(|policies| policies.get(&overlay.clip_name))
                    .copied()
                    .unwrap_or_else(|| motion_policy(&overlay.candidate)),
            }
        })
        .collect();
    Ok(CapabilityGraphManifest {
        source_channels: selected.source_channels.clone(),
        template: selected.template,
        roles,
        idle_event: selected.idle_event.clone(),
        explicit_events: explicit_events.into_values().collect(),
        candidate_attack_bindings: Vec::new(),
        source_inert_attack_receipts: Vec::new(),
        overlays,
    })
}

fn motion_policy(candidate: &MotionCandidate) -> ClipMotionPolicy {
    if matches!(candidate.root_motion, RootMotionEvidence::Planar { .. }) {
        ClipMotionPolicy {
            animation_driven: true,
            extracted_planar_reference_frames: 1,
        }
    } else {
        ClipMotionPolicy::default()
    }
}

fn staged_clip_motion_policy(motion: &crate::phase::animations::FnvClipMotion) -> ClipMotionPolicy {
    let extracted_planar_reference_frames = match motion {
        crate::phase::animations::FnvClipMotion::InPlace => 0,
        crate::phase::animations::FnvClipMotion::ExtractedPlanar { sample_count, .. }
        | crate::phase::animations::FnvClipMotion::ExtractedPlanarYaw { sample_count, .. } => {
            *sample_count
        }
    };
    ClipMotionPolicy {
        animation_driven: extracted_planar_reference_frames > 0,
        extracted_planar_reference_frames,
    }
}

fn graph_declarations(
    graph: &CapabilityGraphManifest,
    ragdoll: &RagdollDisposition,
) -> GraphDeclarations {
    let mut events = graph.explicit_events.clone();
    if graph.any_animation_driven() {
        events.push(EventDecl {
            name: "startAnimationDriven".to_string(),
            usage: EventUsage::Generic,
            flags: 0,
        });
    }
    if matches!(ragdoll, RagdollDisposition::SourceOwned { .. }) {
        for name in [
            "Ragdoll",
            "RagdollInstant",
            "g_InitializeGraph",
            "g_InitializeGraphInstant",
            "AddRagdollToWorld",
            "RemoveCharacterControllerFromWorld",
        ] {
            events.push(EventDecl {
                name: name.to_string(),
                usage: EventUsage::Generic,
                flags: 0,
            });
        }
    }
    events.sort_by_key(|event| event.name.to_ascii_lowercase());
    events.dedup_by(|left, right| left.name.eq_ignore_ascii_case(&right.name));
    let mut variables = vec![VariableDecl {
        name: "bGraphDriven".to_string(),
        variable_type: VariableType::Bool,
        initial_value: VariableValue::Bool(true),
    }];
    for (name, opening) in graph.roles.iter().filter_map(|role| role.generator.source_attack())
        .map(|contract| (contract.gate_variable.clone(), contract.opening_group)).collect::<BTreeMap<_, _>>()
    {
        variables.push(VariableDecl { name, variable_type:VariableType::Int32, initial_value:VariableValue::Int32(opening) });
    }
    if graph.roles.iter().any(|role| role.generator.source_attack().is_some()) {
        variables.push(VariableDecl { name:"B21SourceAttackActive".to_string(), variable_type:VariableType::Int32, initial_value:VariableValue::Int32(0) });
        variables.push(VariableDecl { name:"B21SourceMovementActive".to_string(), variable_type:VariableType::Int32, initial_value:VariableValue::Int32(0) });
    }
    let has_source_aim = graph.roles.iter().any(|role| matches!(role.generator, CapabilityRoleGenerator::SourceAim { .. }));
    if has_source_aim {
        variables.push(VariableDecl { name:"B21SourceWeaponBank".to_string(), variable_type:VariableType::Int32, initial_value:VariableValue::Int32(0) });
    }
    if has_source_aim || graph.roles.iter().any(|role| role.generator.source_attack().is_some_and(|contract| !contract.aim_children.is_empty())) {
        variables.push(VariableDecl { name:"B21SourceWeaponPitch".to_string(), variable_type:VariableType::Real, initial_value:VariableValue::Real(0.0) });
    }
    for name in graph.roles.iter().filter_map(|role| role.generator.source_attack()).filter_map(|contract| contract.opening_variable.clone()).collect::<BTreeSet<_>>() {
        variables.push(VariableDecl { name, variable_type:VariableType::Int32, initial_value:VariableValue::Int32(32) });
    }
    if graph
        .roles
        .iter()
        .any(|role| matches!(role.generator.base_generator(), CapabilityRoleGenerator::Locomotion { .. }))
    {
        for name in ["Speed", "Direction"] {
            variables.push(VariableDecl {
                name: name.to_string(),
                variable_type: VariableType::Real,
                initial_value: VariableValue::Real(0.0),
            });
        }
    }
    if graph
        .roles
        .iter()
        .any(|role| role.role == CreatureClipRole::Death)
    {
        variables.push(VariableDecl {
            name: "bSupportedDeathAnim".to_string(),
            variable_type: VariableType::Bool,
            initial_value: VariableValue::Bool(true),
        });
    }
    if graph.any_animation_driven() {
        variables.push(VariableDecl {
            name: "bAnimationDriven".to_string(),
            variable_type: VariableType::Bool,
            initial_value: VariableValue::Bool(false),
        });
    }
    GraphDeclarations {
        events,
        variables,
        character_properties: Vec::new(),
    }
}

fn add_stationary_aim_declarations(declarations: &mut GraphDeclarations) {
    for (name, value) in [
        ("AimHeadingMaxCCW", 90.0),
        ("AimHeadingMaxCW", 90.0),
        ("fDirectAtHeadingSavedGain", 0.0),
        ("AimHeadingCurrent", 0.0),
        ("AimPitchCurrent", 0.0),
        ("fAimOnGain", 0.05),
        ("camerafromx", 0.0),
        ("camerafromy", 0.0),
        ("camerafromz", 0.0),
    ] {
        declarations.variables.push(VariableDecl {
            name: name.to_string(),
            variable_type: VariableType::Real,
            initial_value: VariableValue::Real(value),
        });
    }
    declarations.variables.push(VariableDecl {
        name: "bAimActive".to_string(),
        variable_type: VariableType::Bool,
        initial_value: VariableValue::Bool(false),
    });
    for (name, value) in [
        ("DirectAtHeadingSourceBoneIndex", 0),
        ("DirectAtHeadingBoneIndex", 2),
    ] {
        declarations.character_properties.push(PropertyDecl {
            name: name.to_string(),
            variable_type: VariableType::Int32,
            initial_value: VariableValue::Int32(value),
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn stage_converted_artifact(
    converted_root: &Path,
    role: SourceRigArtifactRole,
    provenance: SourceRigArtifactProvenance,
    runtime_path: &str,
    source_evidence_blake3: String,
    bytes: &[u8],
    source_game: &str,
    requests: &mut Vec<SourceRigConvertedArtifactRequestReceipt>,
    receipts: &mut Vec<SourceRigArtifactReceipt>,
    artifacts: &mut Vec<FnvFo3MvpConvertedArtifact>,
) -> Result<(), String> {
    let path = runtime_path
        .split('\\')
        .fold(converted_root.to_path_buf(), |path, part| path.join(part));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create converted artifact directory: {error}"))?;
    }
    fs::write(&path, bytes)
        .map_err(|error| format!("write converted artifact {}: {error}", path.display()))?;
    let request_bytes = serde_json::to_vec(&(
        &role,
        runtime_path,
        source_game,
        &source_evidence_blake3,
        "fnv_fo3_source_owned_conversion_v1",
    ))
    .map_err(|error| format!("serialize converted artifact request: {error}"))?;
    requests.push(SourceRigConvertedArtifactRequestReceipt {
        role: role.clone(),
        runtime_path: runtime_path.to_string(),
        source_game: source_game.to_string(),
        source_evidence_blake3,
        conversion_request_blake3: blake3::hash(&request_bytes).to_hex().to_string(),
    });
    receipts.push(SourceRigArtifactReceipt {
        role,
        provenance,
        runtime_path: runtime_path.to_string(),
        byte_len: bytes.len() as u64,
        blake3: blake3::hash(bytes).to_hex().to_string(),
    });
    artifacts.push(FnvFo3MvpConvertedArtifact {
        runtime_path: runtime_path.to_string(),
        source_path: path,
    });
    Ok(())
}

fn exact_asset_claim<'a>(
    family: &'a CreatureFamilyRecipeJob,
    path: &str,
    kind: IndexedAssetKind,
) -> Result<&'a CanonicalAssetClaim, String> {
    let claims = family
        .assets
        .iter()
        .filter(|claim| {
            claim.game == family.rig.game
                && canonical_mesh_path(&claim.path) == canonical_mesh_path(path)
                && claim.kind == kind
        })
        .collect::<Vec<_>>();
    match claims.as_slice() {
        [claim] if claim.content_hash_blake3.len() == 64 => Ok(claim),
        _ => Err(format!(
            "expected one hash-bound {kind:?} claim for {path}, got {}",
            claims.len()
        )),
    }
}

fn is_primary_idle(template: CreatureGraphTemplate, role: CreatureClipRole) -> bool {
    match template {
        CreatureGraphTemplate::Swim => role == CreatureClipRole::SwimIdle,
        CreatureGraphTemplate::Fly => role == CreatureClipRole::FlyIdle,
        CreatureGraphTemplate::StationaryTurret => role == CreatureClipRole::StationaryIdle,
        _ => role == CreatureClipRole::Idle,
    }
}

fn motion_candidate_hash(candidate: &MotionCandidate) -> String {
    blake3::hash(
        &serde_json::to_vec(candidate).expect("MotionCandidate serialization is infallible"),
    )
    .to_hex()
    .to_string()
}

fn source_mesh_path(source_data_root: &Path, path: &str) -> PathBuf {
    data_relative_mesh_path(path)
        .split('/')
        .fold(source_data_root.to_path_buf(), |root, part| root.join(part))
}

fn data_relative_mesh_path(path: &str) -> String {
    format!("meshes/{}", canonical_mesh_path(path))
}

fn canonical_mesh_path(path: &str) -> String {
    let path = path
        .trim()
        .replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase();
    path.strip_prefix("meshes/").unwrap_or(&path).to_string()
}

fn runtime_nif_path(data_relative_path: &str) -> String {
    data_relative_path
        .replace('/', "\\")
        .strip_prefix("meshes\\")
        .unwrap_or(&data_relative_path.replace('/', "\\"))
        .to_string()
}

fn legacy_game_name(game: LegacyCreatureGame) -> &'static str {
    match game {
        LegacyCreatureGame::Fnv => "fnv",
        LegacyCreatureGame::Fo3 => "fo3",
    }
}

fn winning_creature_records<'a>(
    sources: &'a [LegacyRecordSource<'a>],
    interner: &StringInterner,
) -> Result<BTreeMap<StableFormKey, &'a Record>, FnvFo3LiveMvpPreparationBuildError> {
    let mut winners = BTreeMap::<StableFormKey, (&LegacyRecordSource<'a>, &'a Record)>::new();
    for source in sources
        .iter()
        .filter(|source| source.record.sig.as_str() == "CREA")
    {
        let Some(plugin) = interner.resolve(source.record.form_key.plugin) else {
            continue;
        };
        let key = StableFormKey {
            local: source.record.form_key.local,
            plugin: plugin.to_string(),
        };
        match winners.get(&key) {
            Some((current, _)) if current.provenance.precedence > source.provenance.precedence => {}
            Some((current, _)) if current.provenance.precedence == source.provenance.precedence => {
                return Err(FnvFo3LiveMvpPreparationBuildError::Input(format!(
                    "winning CREA collision for {key} at precedence {}",
                    source.provenance.precedence
                )));
            }
            _ => {
                winners.insert(key, (source, source.record));
            }
        }
    }
    Ok(winners
        .into_iter()
        .map(|(key, (_, record))| (key, record))
        .collect())
}

fn winning_runtime_records<'a>(
    sources: &'a [LegacyRecordSource<'a>],
    interner: &StringInterner,
) -> Result<
    BTreeMap<(LegacyCreatureGame, StableFormKey), &'a Record>,
    FnvFo3LiveMvpPreparationBuildError,
> {
    let mut winners = BTreeMap::<
        (LegacyCreatureGame, StableFormKey),
        (&LegacyRecordSource<'a>, &'a Record),
    >::new();
    for source in sources {
        let Some(plugin) = interner.resolve(source.record.form_key.plugin) else {
            continue;
        };
        let key = (
            source.provenance.game,
            StableFormKey {
                local: source.record.form_key.local,
                plugin: plugin.to_string(),
            },
        );
        match winners.get(&key) {
            Some((current, _)) if current.provenance.precedence > source.provenance.precedence => {}
            Some((current, _)) if current.provenance.precedence == source.provenance.precedence => {
                return Err(FnvFo3LiveMvpPreparationBuildError::Input(format!(
                    "winning {:?} record collision for {} at precedence {}",
                    key.0, key.1, source.provenance.precedence
                )));
            }
            _ => {
                winners.insert(key, (source, source.record));
            }
        }
    }
    Ok(winners
        .into_iter()
        .map(|(key, (_, record))| (key, record))
        .collect())
}

fn load_action_point_settings(
    sources: &[LegacyRecordSource<'_>],
    interner: &StringInterner,
) -> BTreeMap<LegacyCreatureGame, ActionPointSettings> {
    const BASE: &str = "favdactionpointsbase";
    const MULTIPLIER: &str = "favdactionpointsmult";
    let mut winners = BTreeMap::<(LegacyCreatureGame, String), (u32, Option<f32>, bool)>::new();
    for source in sources
        .iter()
        .filter(|source| source.record.sig.as_str() == "GMST")
    {
        let Some(editor_id) = source.record.eid.and_then(|eid| interner.resolve(eid)) else {
            continue;
        };
        let editor_id = editor_id.to_ascii_lowercase();
        if !matches!(editor_id.as_str(), BASE | MULTIPLIER) {
            continue;
        }
        let value = scalar_number(source.record, "DATA", "float_float", interner);
        let key = (source.provenance.game, editor_id);
        match winners.get_mut(&key) {
            Some((precedence, current, collision))
                if *precedence == source.provenance.precedence =>
            {
                *current = None;
                *collision = true;
            }
            Some((precedence, _, _)) if *precedence > source.provenance.precedence => {}
            entry => {
                if let Some(entry) = entry {
                    *entry = (source.provenance.precedence, value, false);
                } else {
                    winners.insert(key, (source.provenance.precedence, value, false));
                }
            }
        }
    }
    [LegacyCreatureGame::Fnv, LegacyCreatureGame::Fo3]
        .into_iter()
        .filter_map(|game| {
            let base = winners.get(&(game, BASE.to_string()))?;
            let multiplier = winners.get(&(game, MULTIPLIER.to_string()))?;
            let (Some(base), Some(multiplier)) = (base.1, multiplier.1) else {
                return None;
            };
            (!base.is_finite() || base < 0.0 || !multiplier.is_finite() || multiplier <= 0.0)
                .then_some(())
                .map_or_else(
                    || Some((game, ActionPointSettings { base, multiplier })),
                    |_| None,
                )
        })
        .collect()
}

fn legacy_creature_stats(
    stats_record: &Record,
    action_points: ActionPointSettings,
    interner: &StringInterner,
) -> Result<LegacyCreatureStats, String> {
    let level = struct_integer(stats_record, "ACBS", "level", interner)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| "CREA.ACBS.level is missing or outside u16".to_string())?;
    let health = struct_integer_or_bytes(stats_record, "DATA", "health", 4, true, interner)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| "CREA.DATA.health is missing, negative, or outside u16".to_string())?;
    let damage = struct_integer_or_bytes(stats_record, "DATA", "damage", 8, true, interner)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| "CREA.DATA.damage is missing, negative, or outside u16".to_string())?;
    let agility = struct_integer_or_bytes(
        stats_record,
        "DATA",
        "attribute_agility",
        15,
        false,
        interner,
    )
    .and_then(|value| u16::try_from(value).ok())
    .ok_or_else(|| "CREA.DATA.attribute_agility is missing or outside u16".to_string())?;
    let computed_action_points = action_points.base + f32::from(agility) * action_points.multiplier;
    let rounded_action_points = computed_action_points.round();
    if !computed_action_points.is_finite()
        || computed_action_points <= 0.0
        || (computed_action_points - rounded_action_points).abs() > f32::EPSILON
        || rounded_action_points > f32::from(u16::MAX)
    {
        return Err(format!(
            "legacy action-point formula produced nonintegral/out-of-range value {computed_action_points}"
        ));
    }
    let reach_percent = scalar_integer(stats_record, "RNAM", interner)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| "CREA.RNAM attack reach is missing or outside u16".to_string())?;
    Ok(LegacyCreatureStats {
        level,
        health,
        action_points: rounded_action_points as u16,
        damage,
        reach: f32::from(reach_percent) / 100.0,
    })
}

fn scalar_number(
    record: &Record,
    signature: &str,
    member: &str,
    interner: &StringInterner,
) -> Option<f32> {
    let value = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == signature)
        .map(|field| &field.value)?;
    match value {
        FieldValue::Float(value) => Some(*value as f32),
        FieldValue::Int(value) => Some(*value as f32),
        FieldValue::Uint(value) => Some(*value as f32),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            Some(f32::from_le_bytes(bytes.as_slice().try_into().ok()?))
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some(member))
            .and_then(|(_, value)| numeric_f32(value)),
        _ => None,
    }
}

fn numeric_f32(value: &FieldValue) -> Option<f32> {
    match value {
        FieldValue::Float(value) => Some(*value as f32),
        FieldValue::Int(value) => Some(*value as f32),
        FieldValue::Uint(value) => Some(*value as f32),
        _ => None,
    }
}

fn struct_integer(
    record: &Record,
    signature: &str,
    member: &str,
    interner: &StringInterner,
) -> Option<i64> {
    let value = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == signature)
        .map(|field| &field.value)?;
    match value {
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some(member))
            .and_then(|(_, value)| integer_value(value)),
        FieldValue::Bytes(bytes) if signature == "ACBS" && bytes.len() == 24 => {
            let offset = match member {
                "level" => 8,
                "speed_multiplier" => 14,
                _ => return None,
            };
            Some(i64::from(u16::from_le_bytes(
                bytes[offset..offset + 2].try_into().ok()?,
            )))
        }
        _ => None,
    }
}

fn struct_integer_or_bytes(
    record: &Record,
    signature: &str,
    member: &str,
    byte_offset: usize,
    signed_i16: bool,
    interner: &StringInterner,
) -> Option<i64> {
    let value = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == signature)
        .map(|field| &field.value)?;
    match value {
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some(member))
            .and_then(|(_, value)| integer_value(value)),
        FieldValue::Bytes(bytes) if signed_i16 && bytes.len() >= byte_offset + 2 => {
            Some(i64::from(i16::from_le_bytes([
                bytes[byte_offset],
                bytes[byte_offset + 1],
            ])))
        }
        FieldValue::Bytes(bytes) if !signed_i16 && bytes.len() > byte_offset => {
            Some(i64::from(bytes[byte_offset]))
        }
        _ => None,
    }
}

fn scalar_integer(record: &Record, signature: &str, _interner: &StringInterner) -> Option<i64> {
    record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == signature)
        .and_then(|field| integer_value(&field.value))
}

fn integer_value(value: &FieldValue) -> Option<i64> {
    match value {
        FieldValue::Int(value) => Some(*value),
        FieldValue::Uint(value) => i64::try_from(*value).ok(),
        _ => None,
    }
}

fn named_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields.iter().find_map(|(field_name, value)| {
        (interner.resolve(*field_name) == Some(name)).then_some(value)
    })
}

#[cfg(test)]
mod tests {
    use super::super::creature_catalog::CreatureProvenance;
    use super::super::creature_dependencies::{
        DependencyReferenceReceipt, PairRecordLoweringDisposition,
    };
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use tempfile::TempDir;

    fn stable(local: u32) -> StableFormKey {
        StableFormKey {
            local,
            plugin: "FalloutNV.esm".to_string(),
        }
    }

    #[test]
    fn leveled_visual_owner_replaces_placeholder_body_parts_only_for_model_inheritance() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let make = |local, template_flags, parent, body| {
            let mut record = Record::new(SigCode(*b"CREA"), FormKey { local, plugin });
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"ACBS"),
                value: FieldValue::Struct(vec![(interner.intern("template_flags"), FieldValue::Uint(template_flags))]),
            });
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"PNAM"),
                value: FieldValue::FormKey(FormKey { local: body, plugin }),
            });
            if let Some(local) = parent {
                record.fields.push(FieldEntry { sig: SubrecordSig(*b"TPLT"), value: FieldValue::FormKey(FormKey { local, plugin }) });
            }
            record
        };
        let owner = make(0x1cf93, 0, None, 0x2026e);
        let inherited = make(0x1f67c, u64::from(TEMPLATE_MODEL_ANIMATION), Some(0x2359f), 0x1d);
        let local = make(0x1cf72, u64::from(TEMPLATE_TRAITS), Some(0x2359f), 0x8445e);
        let records = BTreeMap::from([(stable(0x1cf93), &owner), (stable(0x1f67c), &inherited), (stable(0x1cf72), &local)]);
        assert_eq!(source_body_part_key(&stable(0x1f67c), &records, Some(&stable(0x1cf93)), &interner).unwrap(), Some(stable(0x2026e)));
        assert_eq!(source_body_part_key(&stable(0x1cf72), &records, Some(&stable(0x1cf93)), &interner).unwrap(), Some(stable(0x8445e)));
        assert!(source_body_part_key(&stable(0x1f67c), &records, None, &interner).is_err());
    }

    #[test]
    #[ignore = "requires source FalloutNV.esm and CLI-derived B21_CREATURE_BODY_PART_TEMPLATE_AUDIT"]
    fn real_fnv_leveled_body_parts_follow_each_selected_visual_owner() {
        use esp_authoring_core::plugin_runtime::{plugin_handle_close_native, plugin_handle_load_lazy_records_no_py};
        let audit_path = PathBuf::from(std::env::var("B21_CREATURE_BODY_PART_TEMPLATE_AUDIT").unwrap());
        let audit: serde_json::Value = serde_json::from_slice(&std::fs::read(&audit_path).unwrap()).unwrap();
        let plugins: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(std::env::var("B21_CREATURE_BODY_PART_PLUGINS").unwrap()).unwrap()).unwrap();
        let plugin = plugins.iter().find(|entry| entry["game"] == "fnv" && entry["path"].as_str().unwrap().ends_with("FalloutNV.esm")).unwrap();
        let handle = plugin_handle_load_lazy_records_no_py(plugin["path"].as_str().unwrap(), "fnv", None, &[*b"CREA"]).unwrap();
        let keys = crate::session::open_session(handle, None).unwrap().handle_raw_scan(handle).unwrap().form_keys_of_signature("CREA");
        let interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("fnv").unwrap();
        let owned = keys.iter().map(|key| crate::source_read::read_record(handle, key, &schema, &interner).unwrap()).collect::<Vec<_>>();
        let records = owned.iter().map(|record| (stable(record.form_key.local), record)).collect::<BTreeMap<_, _>>();
        let key = |value: &serde_json::Value| {
            let (plugin, local) = value.as_str().unwrap().split_once(':').unwrap();
            StableFormKey { plugin: plugin.to_string(), local: u32::from_str_radix(local, 16).unwrap() }
        };
        let mut checked = Vec::new();
        for row in audit["records_detail"].as_array().unwrap() {
            let source = key(&row["source"]);
            assert!(row["errors"].as_array().unwrap().is_empty());
            assert!(!row["owners"].as_array().unwrap().is_empty());
            for branch in row["owners"].as_array().unwrap() {
                let owner = key(&branch["owner"]);
                let expected = key(&branch["bodypart"]);
                let projected = source_body_part_key(&source, &records, Some(&owner), &interner).unwrap();
                assert_eq!(projected, Some(expected), "{source} selected {owner}");
                checked.push(serde_json::json!({"source":source,"owner":owner,"bodypart":projected}));
            }
        }
        assert!(!checked.is_empty());
        assert!(plugin_handle_close_native(handle));
        let report = serde_json::json!({"sources":audit["records"],"branches":checked.len(),"checked":checked});
        std::fs::write(audit_path.with_file_name("fnv_creature_bodypart_effective_projection.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        println!("FNV body-part template projections: {} source proxies, {} selected branches", audit["records"], checked.len());
    }

    #[test]
    fn raw_gmst_float_is_decoded() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("GMST").unwrap(),
            FormKey {
                local: 0x100,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(12.5_f32.to_le_bytes().to_vec().into()),
        });

        assert_eq!(
            scalar_number(&record, "DATA", "float_float", &interner),
            Some(12.5)
        );
    }

    #[test]
    fn staged_planar_motion_declares_exact_reference_frame_count() {
        let policy =
            staged_clip_motion_policy(&crate::phase::animations::FnvClipMotion::ExtractedPlanar {
                root_track_index: 0,
                root_bone_index: 0,
                sample_count: 51,
                end_displacement: [1.0, 0.0],
            });

        assert!(policy.animation_driven);
        assert_eq!(policy.extracted_planar_reference_frames, 51);
    }

    #[test]
    fn raw_crea_acbs_integer_is_decoded() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("CREA").unwrap(),
            FormKey {
                local: 0x100,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        let mut acbs = vec![0_u8; 24];
        acbs[8..10].copy_from_slice(&23_u16.to_le_bytes());
        acbs[14..16].copy_from_slice(&125_u16.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ACBS").unwrap(),
            value: FieldValue::Bytes(acbs.into()),
        });

        assert_eq!(
            struct_integer(&record, "ACBS", "level", &interner),
            Some(23)
        );
        assert_eq!(
            struct_integer(&record, "ACBS", "speed_multiplier", &interner),
            Some(125)
        );
    }

    #[test]
    fn stats_template_owns_health_damage_agility_and_reach_independently_of_base_data() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let mut root = Record::new(SigCode(*b"CREA"), FormKey { local: 0x100, plugin });
        root.fields.extend([
            FieldEntry { sig: SubrecordSig(*b"ACBS"), value: FieldValue::Struct(vec![
                (interner.intern("template_flags"), FieldValue::Uint(TEMPLATE_STATS.into())),
                (interner.intern("level"), FieldValue::Uint(1)),
            ]) },
            FieldEntry { sig: SubrecordSig(*b"TPLT"), value: FieldValue::FormKey(FormKey { local: 0x200, plugin }) },
            FieldEntry { sig: SubrecordSig(*b"DATA"), value: FieldValue::Struct(vec![
                (interner.intern("health"), FieldValue::Int(1)),
                (interner.intern("damage"), FieldValue::Int(2)),
                (interner.intern("attribute_agility"), FieldValue::Uint(1)),
            ]) },
            FieldEntry { sig: SubrecordSig(*b"RNAM"), value: FieldValue::Uint(10) },
        ]);
        let mut parent = root.clone();
        parent.form_key.local = 0x200;
        parent.fields.retain(|field| field.sig.0 != *b"TPLT");
        parent.fields[0].value = FieldValue::Struct(vec![
            (interner.intern("template_flags"), FieldValue::Uint(0)),
            (interner.intern("level"), FieldValue::Uint(7)),
        ]);
        parent.fields[1].value = FieldValue::Struct(vec![
            (interner.intern("health"), FieldValue::Int(321)),
            (interner.intern("damage"), FieldValue::Int(27)),
            (interner.intern("attribute_agility"), FieldValue::Uint(9)),
        ]);
        parent.fields[2].value = FieldValue::Uint(150);
        let source = StableFormKey { local: 0x100, plugin: "FalloutNV.esm".into() };
        let records = BTreeMap::from([
            (source.clone(), &root),
            (StableFormKey { local: 0x200, plugin: source.plugin.clone() }, &parent),
        ]);
        let (_, stats) = resolve_template_group_record(&source, TEMPLATE_STATS, &records, None, &interner).unwrap();
        let (base_owner, _) = resolve_template_group_record(&source, TEMPLATE_BASE_DATA, &records, None, &interner).unwrap();
        assert_eq!(base_owner, source);
        let result = legacy_creature_stats(stats, ActionPointSettings { base: 50.0, multiplier: 1.0 }, &interner).unwrap();
        assert_eq!((result.level, result.health, result.damage, result.action_points), (7, 321, 27, 59));
        assert_eq!(result.reach, 1.5);
    }

    #[test]
    fn zero_legacy_level_and_health_are_preserved() {
        let interner = StringInterner::new();
        let mut stats = Record::new(
            SigCode::from_str("CREA").unwrap(),
            FormKey {
                local: 0x100,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        stats.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ACBS").unwrap(),
            value: FieldValue::Struct(vec![(interner.intern("level"), FieldValue::Uint(0))]),
        });
        stats.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Struct(vec![
                (interner.intern("health"), FieldValue::Int(0)),
                (interner.intern("damage"), FieldValue::Int(0)),
                (interner.intern("attribute_agility"), FieldValue::Uint(5)),
            ]),
        });
        stats.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RNAM").unwrap(),
            value: FieldValue::Uint(100),
        });

        let result = legacy_creature_stats(
            &stats,
            ActionPointSettings {
                base: 50.0,
                multiplier: 1.0,
            },
            &interner,
        )
        .unwrap();

        assert_eq!(result.level, 0);
        assert_eq!(result.health, 0);
        assert_eq!(result.action_points, 55);
    }

    fn trigger_fixture(events: Vec<&str>) -> MotionCandidate {
        MotionCandidate {
            source_game: LegacyCreatureGame::Fnv,
            source_kf: "creatures\\fixture\\attack.kf".to_string(),
            sequence_index: 0,
            sequence_name: "Attack".to_string(),
            cycle: super::super::creature_motion::SequenceCycle::Clamp,
            looping: false,
            events: events
                .into_iter()
                .map(
                    |raw_text| super::super::creature_motion::MotionEventProvenance {
                        time: 0.0,
                        raw_text: raw_text.to_string(),
                        kind: MotionEventKind::Other,
                    },
                )
                .collect(),
            idle_claims: Vec::new(),
            binding: super::super::creature_motion::BindingSummary {
                source_skeleton_path: "creatures\\fixture\\skeleton.nif".to_string(),
                transform_track_count: 1,
                ordered_transform_targets: vec!["Bip01".to_string()],
                float_track_count: 0,
                controller_types: Vec::new(),
                interpolator_types: Vec::new(),
                overlay_targets: Vec::new(),
                required_float_slots: Vec::new(),
                compatibility: super::super::creature_motion::BindingCompatibility::Verified,
                compatibility_detail: None,
            },
            root_motion: RootMotionEvidence::None,
            confidence: super::super::creature_motion::CandidateConfidence::Low,
            evidence: Vec::new(),
            ambiguous_with: Vec::new(),
        }
    }

    #[test]
    fn missing_or_non_fo4_melee_events_use_distinct_standard_events() {
        let mut used = BTreeSet::new();
        let first = selected_trigger_event(
            &trigger_fixture(Vec::new()),
            CreatureClipRole::MeleeAttack,
            1,
            true,
            &mut used,
        )
        .unwrap();
        let second = selected_trigger_event(
            &trigger_fixture(vec!["AttackStart"]),
            CreatureClipRole::MeleeAttack,
            2,
            true,
            &mut used,
        )
        .unwrap();
        assert_eq!(first, "melee_fnvfo3_01");
        assert_eq!(second, "melee_fnvfo3_02");
    }

    #[test]
    fn stationary_turret_looping_attack_projects_to_one_shot_clip() {
        let mut candidate = trigger_fixture(vec!["start", "end"]);
        candidate.looping = true;
        candidate.cycle = super::super::creature_motion::SequenceCycle::Loop;
        let mut clips = Vec::new();
        let mut used = BTreeSet::new();

        add_selected_clip_with_cycle_projection(
            &mut clips,
            &mut used,
            candidate,
            CreatureClipRole::ProjectileAttack,
            "projectile_attack_01",
            "ProjectileAttack1",
            true,
            false,
            true,
        )
        .unwrap();

        assert!(clips[0].candidate.looping);
        assert!(!clips[0].target_looping);
    }

    #[test]
    fn graph_triggers_do_not_claim_overlay_lifecycle_events() {
        let mut candidate = trigger_fixture(vec!["start", "end"]);
        candidate.looping = true;
        candidate.cycle = super::super::creature_motion::SequenceCycle::Loop;
        candidate.events[0].kind = MotionEventKind::Begin;
        candidate.events[1].kind = MotionEventKind::End;
        let mut used = BTreeSet::new();

        let trigger = selected_trigger_event(
            &candidate,
            CreatureClipRole::GroundForward,
            1,
            false,
            &mut used,
        )
        .unwrap();
        assert_eq!(trigger, "moveStart");

        let start = exact_overlay_event(&candidate, true).unwrap();
        let stop = exact_overlay_event(&candidate, false).unwrap();
        assert!(used.insert(start.to_ascii_lowercase()));
        assert!(used.insert(stop.to_ascii_lowercase()));
    }

    fn receipt(
        source: StableFormKey,
        signature: &str,
        references: Vec<DependencyReferenceReceipt>,
    ) -> RuntimeDependencyRecordReceipt {
        RuntimeDependencyRecordReceipt {
            source,
            signature: signature.to_string(),
            provenance: CreatureProvenance {
                game: LegacyCreatureGame::Fnv,
                source_plugin: "FalloutNV.esm".to_string(),
                precedence: 0,
            },
            lowering: PairRecordLoweringDisposition::Ready {
                mechanism: "fixture".to_string(),
            },
            degradations: Vec::new(),
            references,
        }
    }

    fn followed(
        target: StableFormKey,
        signature: &str,
        locator: &str,
    ) -> DependencyReferenceReceipt {
        DependencyReferenceReceipt {
            target,
            source_locators: vec![locator.to_string()],
            resolution: DependencyReferenceResolution::Followed {
                signature: signature.to_string(),
            },
        }
    }

    #[test]
    fn ranged_weapon_sources_follow_leveled_inventory_without_inventing_ammo_only_weapons() {
        let inventory_source = stable(0x100);
        let list_source = stable(0x200);
        let ammunition = stable(0x300);
        let other_ammunition = stable(0x301);
        let leveled_weapon = stable(0x400);
        let matching_embedded_weapon = stable(0x500);
        let other_embedded_weapon = stable(0x501);
        let inventory = self::receipt(
            inventory_source,
            "CREA",
            vec![
                followed(list_source.clone(), "LVLI", "CNTO[0].item"),
                followed(ammunition.clone(), "AMMO", "CNTO[1].item"),
            ],
        );
        let list = self::receipt(
            list_source.clone(),
            "LVLI",
            vec![followed(leveled_weapon.clone(), "WEAP", "LVLO[0].item")],
        );
        let matching = self::receipt(
            matching_embedded_weapon.clone(),
            "WEAP",
            vec![followed(ammunition.clone(), "AMMO", "DATA.ammo")],
        );
        let other = self::receipt(
            other_embedded_weapon.clone(),
            "WEAP",
            vec![followed(other_ammunition, "AMMO", "DATA.ammo")],
        );
        let records = BTreeMap::from([
            (list_source, &list),
            (matching_embedded_weapon.clone(), &matching),
            (other_embedded_weapon.clone(), &other),
        ]);

        assert_eq!(
            exact_inventory_weapon_sources(&inventory, &[], &records).unwrap(),
            vec![leveled_weapon]
        );

        let ammo_only_inventory = self::receipt(
            stable(0x101),
            "CREA",
            vec![followed(ammunition, "AMMO", "CNTO[0].item")],
        );
        assert_eq!(
            exact_inventory_weapon_sources(
                &ammo_only_inventory,
                &[],
                &records,
            )
            .unwrap(),
            Vec::<StableFormKey>::new()
        );
    }

    #[test]
    fn creature_weapon_admission_intersects_inventory_with_flat_traits_list() {
        let list_key = stable(0x200);
        let nested_key = stable(0x201);
        let usable = stable(0x300);
        let loot = stable(0x301);
        let not_owned = stable(0x302);
        let traits = receipt(stable(0x100), "CREA", vec![followed(list_key.clone(), "FLST", "LNAM[0]")]);
        let list = receipt(list_key.clone(), "FLST", vec![
            followed(usable.clone(), "WEAP", "LNAM[0]"),
            followed(not_owned.clone(), "WEAP", "LNAM[1]"),
            followed(nested_key.clone(), "FLST", "LNAM[2]"),
        ]);
        let nested = receipt(nested_key.clone(), "FLST", vec![followed(loot.clone(), "WEAP", "LNAM[0]")]);
        let records = BTreeMap::from([(list_key, &list), (nested_key, &nested)]);
        assert_eq!(exact_usable_inventory_weapon_sources(&[usable.clone(), loot.clone()], &traits, &records).unwrap(), [usable]);
        let no_list = receipt(stable(0x101), "CREA", Vec::new());
        assert!(exact_usable_inventory_weapon_sources(&[loot, not_owned], &no_list, &records).unwrap().is_empty());
    }

    #[test]
    fn singular_proxy_owner_lookup_is_plugin_case_insensitive() {
        let interner = StringInterner::new();
        let source = stable(0x100);
        let leveled_template = stable(0x200);
        let owner = stable(0x300);
        let mut proxy = Record::new(
            SigCode::from_str("CREA").unwrap(),
            FormKey {
                local: source.local,
                plugin: interner.intern(&source.plugin),
            },
        );
        proxy.fields.extend([
            FieldEntry {
                sig: SubrecordSig::from_str("ACBS").unwrap(),
                value: FieldValue::Struct(vec![(
                    interner.intern("template_flags"),
                    FieldValue::Uint(u64::from(TEMPLATE_INVENTORY)),
                )]),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("TPLT").unwrap(),
                value: FieldValue::FormKey(FormKey {
                    local: leveled_template.local,
                    plugin: interner.intern(&leveled_template.plugin),
                }),
            },
        ]);
        let mut owner_record = Record::new(
            SigCode::from_str("CREA").unwrap(),
            FormKey {
                local: owner.local,
                plugin: interner.intern(&owner.plugin),
            },
        );
        owner_record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ACBS").unwrap(),
            value: FieldValue::Struct(vec![(
                interner.intern("template_flags"),
                FieldValue::Uint(0),
            )]),
        });
        let records = BTreeMap::from([(source.clone(), &proxy), (owner.clone(), &owner_record)]);
        let lowercase_owner = StableFormKey {
            local: owner.local,
            plugin: owner.plugin.to_ascii_lowercase(),
        };

        let (resolved, record) = resolve_template_group_record(
            &source,
            TEMPLATE_INVENTORY,
            &records,
            Some(&lowercase_owner),
            &interner,
        )
        .unwrap();

        assert_eq!(resolved, owner);
        assert!(std::ptr::eq(record, &owner_record));
    }

    #[test]
    fn stages_and_validates_exact_debris_runtime_model_closure() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let mut record = Record::new(
            SigCode::from_str("DEBR").unwrap(),
            FormKey {
                local: 0x800,
                plugin,
            },
        );
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(interner.intern("DebrisFixture")),
            },
            FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(
                    [50_u8]
                        .into_iter()
                        .chain(b"Gore\\DebrisFixture.nif\0".iter().copied())
                        .chain([0])
                        .collect::<Vec<_>>()
                        .into(),
                ),
            },
            FieldEntry {
                sig: SubrecordSig(*b"MODT"),
                value: FieldValue::Bytes(vec![1, 2, 3].into()),
            },
        ]);
        let temp = TempDir::new().unwrap();
        let source_parent = temp.path().join("source");
        let source_root = source_parent.join("fnv");
        let source_model = source_root.join("meshes/Gore/DebrisFixture.nif");
        fs::create_dir_all(source_model.parent().unwrap()).unwrap();
        let mut nif = NifFile::new("fnv");
        nif.rebuild_header();
        nif.save(Some(source_model)).unwrap();
        let staged_root = temp.path().join("stage/data");

        let prepared = prepare_legacy_debris_record(
            LegacyCreatureGame::Fnv,
            &record,
            &source_parent,
            &staged_root,
            &interner,
        )
        .unwrap();
        let receipt = SourceRigRuntimeModelClosureReceipt {
            version: SOURCE_RIG_RUNTIME_MODEL_CLOSURE_VERSION,
            rows: prepared.rows,
        };
        validate_source_rig_runtime_model_closure(
            &prepared.expectations,
            &receipt,
            &BTreeMap::from([("fnv".to_string(), source_root)]),
            &staged_root,
        )
        .unwrap();
        assert_eq!(receipt.rows.len(), 1);
        assert!(
            receipt.rows[0]
                .target_data_path
                .starts_with("meshes\\b21_fnvfo3_debr_")
        );
        assert_eq!(receipt.rows[0].artifacts.len(), 1);
        assert_eq!(
            receipt.rows[0].artifacts[0].kind,
            SourceRigRuntimeModelArtifactKind::Nif
        );
    }

    #[test]
    fn legacy_attack_policy_requires_negative_per_attack_evidence() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let mut record = Record::new(
            SigCode::from_str("CREA").unwrap(),
            FormKey {
                local: 0x100,
                plugin,
            },
        );
        let policy = exact_legacy_attack_projection_policy(&[&record]).unwrap();
        assert_eq!(policy.damage_multiplier.to_bits(), 1.0_f32.to_bits());
        assert_eq!(policy.chance.to_bits(), 1.0_f32.to_bits());
        assert_eq!(policy.strike_angle.to_bits(), 0.0_f32.to_bits());
        assert_eq!(policy.action_point_cost.to_bits(), 0.0_f32.to_bits());
        assert_eq!(policy.target_data, CreatureAttackTargetData::default());
        assert!(is_legacy_inert_attack_policy_field(
            "projection.attacks[0].target_data.action_points_multiplier"
        ));
        assert!(!is_legacy_inert_attack_policy_field(
            "projection.attacks[0].event"
        ));

        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ATKD").unwrap(),
            value: FieldValue::Bytes(vec![0; 44].into()),
        });
        assert!(
            exact_legacy_attack_projection_policy(&[&record])
                .unwrap_err()
                .contains("unsupported per-attack ATKD")
        );
    }

    #[test]
    fn template_group_resolution_follows_only_the_requested_slot() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let template = FormKey {
            local: 0x200,
            plugin,
        };
        let mut proxy = Record::new(
            SigCode::from_str("CREA").unwrap(),
            FormKey {
                local: 0x100,
                plugin,
            },
        );
        proxy.fields.extend([
            FieldEntry {
                sig: SubrecordSig::from_str("ACBS").unwrap(),
                value: FieldValue::Struct(vec![(
                    interner.intern("template_flags"),
                    FieldValue::Uint(u64::from(TEMPLATE_STATS | TEMPLATE_BASE_DATA)),
                )]),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("TPLT").unwrap(),
                value: FieldValue::FormKey(template),
            },
        ]);
        let mut owner = Record::new(SigCode::from_str("CREA").unwrap(), template);
        owner.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ACBS").unwrap(),
            value: FieldValue::Struct(vec![(
                interner.intern("template_flags"),
                FieldValue::Uint(0),
            )]),
        });
        let records = BTreeMap::from([(stable(0x100), &proxy), (stable(0x200), &owner)]);

        assert_eq!(
            resolve_template_group_record(
                &stable(0x100),
                TEMPLATE_STATS,
                &records,
                None,
                &interner,
            )
            .unwrap()
            .0,
            stable(0x200)
        );
        assert_eq!(
            resolve_template_group_record(
                &stable(0x100),
                TEMPLATE_TRAITS,
                &records,
                None,
                &interner,
            )
            .unwrap()
            .0,
            stable(0x100)
        );
        assert_eq!(
            resolve_template_group_record(
                &stable(0x100),
                TEMPLATE_BASE_DATA,
                &records,
                None,
                &interner,
            )
            .unwrap()
            .0,
            stable(0x200)
        );
    }

    #[test]
    fn template_flags_without_a_template_target_remain_local() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let mut creature = Record::new(
            SigCode::from_str("CREA").unwrap(),
            FormKey {
                local: 0x100,
                plugin,
            },
        );
        creature.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ACBS").unwrap(),
            value: FieldValue::Struct(vec![(
                interner.intern("template_flags"),
                FieldValue::Uint(u64::from(TEMPLATE_BASE_DATA)),
            )]),
        });
        let records = BTreeMap::from([(stable(0x100), &creature)]);

        assert_eq!(
            resolve_template_group_record(
                &stable(0x100),
                TEMPLATE_BASE_DATA,
                &records,
                None,
                &interner,
            )
            .unwrap()
            .0,
            stable(0x100)
        );
    }

    #[test]
    fn non_creature_template_targets_preserve_local_nonvisual_groups() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let template = FormKey {
            local: 0x200,
            plugin,
        };
        let mut creature = Record::new(
            SigCode::from_str("CREA").unwrap(),
            FormKey {
                local: 0x100,
                plugin,
            },
        );
        creature.fields.extend([
            FieldEntry {
                sig: SubrecordSig::from_str("ACBS").unwrap(),
                value: FieldValue::Struct(vec![(
                    interner.intern("template_flags"),
                    FieldValue::Uint(u64::from(TEMPLATE_STATS | TEMPLATE_MODEL_ANIMATION)),
                )]),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("TPLT").unwrap(),
                value: FieldValue::FormKey(template),
            },
        ]);
        let records = BTreeMap::from([(stable(0x100), &creature)]);

        assert_eq!(
            resolve_template_group_record(
                &stable(0x100),
                TEMPLATE_STATS,
                &records,
                None,
                &interner,
            )
            .unwrap()
            .0,
            stable(0x100)
        );
        assert!(
            resolve_template_group_record(
                &stable(0x100),
                TEMPLATE_MODEL_ANIMATION,
                &records,
                None,
                &interner,
            )
            .unwrap_err()
            .contains("is not a winning CREA")
        );
    }

    #[test]
    fn inherited_death_item_requires_one_decoded_lvli_form_key() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let mut record = Record::new(
            SigCode::from_str("CREA").unwrap(),
            FormKey {
                local: 0x100,
                plugin,
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("INAM").unwrap(),
            value: FieldValue::FormKey(FormKey {
                local: 0x300,
                plugin,
            }),
        });

        assert_eq!(
            exact_optional_record_form_key(&record, "INAM", &interner).unwrap(),
            Some(stable(0x300))
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("INAM").unwrap(),
            value: FieldValue::Uint(0),
        });
        assert!(
            exact_optional_record_form_key(&record, "INAM", &interner)
                .unwrap_err()
                .contains("2 INAM fields")
        );
    }

    #[test]
    fn candidate_death_item_override_round_trips_without_ambiguous_null() {
        let absent = candidate_death_item_override(None);
        let absent_json = serde_json::to_string(&absent).unwrap();
        let absent_roundtrip: Option<Option<CreatureTargetRecordReference>> =
            serde_json::from_str(&absent_json).unwrap();
        assert_eq!(absent_roundtrip, absent);

        let present = candidate_death_item_override(Some(CreatureTargetRecordReference {
            signature: "LVLI".to_string(),
            form_key: TargetFormKey::new(0x300, "FalloutNV.esm"),
        }));
        let present_json = serde_json::to_string(&present).unwrap();
        let present_roundtrip: Option<Option<CreatureTargetRecordReference>> =
            serde_json::from_str(&present_json).unwrap();
        assert_eq!(present_roundtrip, present);
    }

    #[test]
    fn creature_attack_profiles_only_bind_the_candidates_owned_weapon() {
        let laser = stable(0x100);
        let missile = stable(0x200);
        let entries = BTreeMap::from([
            ("laserattack".to_string(), vec![laser.clone()]),
            ("missileattack".to_string(), vec![missile.clone()]),
        ]);
        assert_eq!(attack_weapons_for_event(&entries, "LaserAttack", std::slice::from_ref(&laser)).unwrap(), vec![laser]);
        assert!(attack_weapons_for_event(&entries, "LaserAttack", std::slice::from_ref(&missile)).unwrap().is_empty());
        assert!(attack_weapons_for_event(&entries, "MissingAttack", &[missile]).is_err());
    }

    #[test]
    fn inventory_preserves_cnto_order_counts_and_coed_union_semantics() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let target_plugin = interner.intern("Converted.esp");
        let source_record_key = stable(0x100);
        let source_item_a = stable(0x200);
        let source_item_b = stable(0x201);
        let source_faction = stable(0x300);
        let source_owner = stable(0x301);
        let source_global = stable(0x302);
        let source_form_key = |source: &StableFormKey| FormKey {
            local: source.local,
            plugin: source_plugin,
        };
        let mut source = Record::new(
            SigCode::from_str("CREA").unwrap(),
            source_form_key(&source_record_key),
        );
        let struct_value = |fields: Vec<(&str, FieldValue)>| {
            FieldValue::Struct(
                fields
                    .into_iter()
                    .map(|(name, value)| (interner.intern(name), value))
                    .collect(),
            )
        };
        source.fields.extend([
            FieldEntry {
                sig: SubrecordSig::from_str("CNTO").unwrap(),
                value: struct_value(vec![
                    ("item", FieldValue::FormKey(source_form_key(&source_item_a))),
                    ("count", FieldValue::Int(2)),
                ]),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("COED").unwrap(),
                value: struct_value(vec![
                    (
                        "owner",
                        FieldValue::FormKey(source_form_key(&source_faction)),
                    ),
                    ("global_variable_required_rank", FieldValue::Int(-1)),
                    ("item_condition", FieldValue::Float(0.75)),
                ]),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("CNTO").unwrap(),
                value: struct_value(vec![
                    ("item", FieldValue::FormKey(source_form_key(&source_item_b))),
                    ("count", FieldValue::Int(-3)),
                ]),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("COED").unwrap(),
                value: struct_value(vec![
                    ("owner", FieldValue::FormKey(source_form_key(&source_owner))),
                    (
                        "global_variable_required_rank",
                        FieldValue::FormKey(source_form_key(&source_global)),
                    ),
                    ("item_condition", FieldValue::Float(0.5)),
                ]),
            },
        ]);
        let dependencies = self::receipt(
            source_record_key,
            "CREA",
            vec![
                followed(source_item_a.clone(), "WEAP", "CNTO[0].item"),
                followed(source_faction.clone(), "FACT", "COED[0].owner"),
                followed(source_item_b.clone(), "ARMO", "CNTO[1].item"),
                followed(source_owner.clone(), "NPC_", "COED[1].owner"),
                followed(
                    source_global.clone(),
                    "GLOB",
                    "COED[1].global_variable_required_rank",
                ),
            ],
        );
        let mut mapper_state = MapperState::new(
            std::iter::empty(),
            crate::formkey_mapper::MapperOptions {
                output_plugin_name: "Converted.esp".to_string(),
                ..Default::default()
            },
        );
        for (source, local) in [
            (&source_item_a, 0x800),
            (&source_item_b, 0x801),
            (&source_faction, 0x802),
            (&source_owner, 0x803),
            (&source_global, 0x804),
        ] {
            mapper_state.source_to_target.insert(
                source_form_key(source),
                FormKey {
                    local,
                    plugin: target_plugin,
                },
            );
        }

        let entries = exact_inventory_entries(&source, &dependencies, &mapper_state, &interner)
            .expect("inventory projection");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].count, 2);
        assert_eq!(entries[0].target_record.signature, "WEAP");
        assert!(matches!(
            &entries[0].ownership,
            Some(CreatureNpcInventoryOwnership::FactionRank {
                faction,
                required_rank: -1,
                condition,
            }) if faction.signature == "FACT" && condition.to_bits() == 0.75_f32.to_bits()
        ));
        assert_eq!(entries[1].count, -3);
        assert_eq!(entries[1].target_record.signature, "ARMO");
        assert!(matches!(
            &entries[1].ownership,
            Some(CreatureNpcInventoryOwnership::OwnerGlobal {
                owner: Some(owner),
                global: Some(global),
                condition,
            }) if owner.signature == "NPC_"
                && global.signature == "GLOB"
                && condition.to_bits() == 0.5_f32.to_bits()
        ));
        let mut projected_dependencies = Vec::new();
        extend_inventory_dependencies(&mut projected_dependencies, &entries);
        assert_eq!(
            projected_dependencies
                .iter()
                .map(|dependency| dependency.signature.as_str())
                .collect::<Vec<_>>(),
            ["WEAP", "FACT", "ARMO", "NPC_", "GLOB"]
        );

        for (index, field) in source
            .fields
            .iter_mut()
            .filter(|field| field.sig.as_str() == "COED")
            .enumerate()
        {
            let (owner, union, condition) = if index == 0 {
                (1_u32, u32::MAX, 0.75_f32)
            } else {
                (1_u32, 1_u32, 0.5_f32)
            };
            let mut bytes = Vec::with_capacity(12);
            bytes.extend_from_slice(&owner.to_le_bytes());
            bytes.extend_from_slice(&union.to_le_bytes());
            bytes.extend_from_slice(&condition.to_le_bytes());
            field.value = FieldValue::Bytes(bytes.into());
        }
        let raw_entries = exact_inventory_entries(&source, &dependencies, &mapper_state, &interner)
            .expect("raw COED inventory projection");
        assert!(matches!(
            &raw_entries[0].ownership,
            Some(CreatureNpcInventoryOwnership::FactionRank {
                required_rank: -1,
                condition,
                ..
            }) if condition.to_bits() == 0.75_f32.to_bits()
        ));
        assert!(matches!(
            &raw_entries[1].ownership,
            Some(CreatureNpcInventoryOwnership::OwnerGlobal {
                owner: Some(_),
                global: Some(_),
                condition,
            }) if condition.to_bits() == 0.5_f32.to_bits()
        ));
    }
}
