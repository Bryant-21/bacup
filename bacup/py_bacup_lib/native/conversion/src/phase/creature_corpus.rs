//! Transactional orchestration for broad source-rig creature batches.
//!
//! `DiscoverCreatureCorpusPhase` turns the source-neutral corpus catalogs plus artifact declarations into
//! `debug/creature_corpus/plan.json` and a capability/preflight ledger.
//! `ExecuteCreatureCorpusPhase` reads that plan, stages complete rig families,
//! audits their recursive closure, and only then publishes assets and registers
//! them with the output sink. Record-bearing families fail closed until their
//! prepared record batch can commit after asset promotion. No path borrows RACE
//! DATA from a donor creature.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::run::{
    CreatureDependencyAdmission, SkyrimCreatureReservationOwner, SkyrimCreatureReservationSource,
    SkyrimCreatureReservedRecordKind,
};
use crate::skyrimse_fo4_runtime::creature_catalog as skyrim_catalog;
use crate::skyrimse_fo4_runtime::creature_motion as skyrim_motion;
use crate::skyrimse_fo4_runtime::creature_mvp_adapter as skyrim_dependencies;
use crate::skyrimse_fo4_runtime::creature_mvp_live as skyrim_live;
use crate::skyrimse_fo4_runtime::creature_race_data as skyrim_race_data;
use crate::skyrimse_fo4_runtime::creature_recipe as skyrim_recipe;
use crate::skyrimse_fo4_runtime::wolf_creature::canonical_skyrim_wolf_runtime_contract;
use crate::source_read::{
    decode_record_from_parsed, iter_form_keys_of_sig, snapshot_records_by_form_keys,
};
use crate::source_rig::{
    CreatureCapability, CreatureCorpusCandidate, CreatureCorpusPlan, CreatureGraphTemplate,
    CreatureRejectionReason, Fo4RaceDataField, Fo4RaceDataTarget, MotionAttack, MotionAttackKind,
    MotionSet, RaceDataMapping, RecordVariant, RigFamily, SourceCreatureIdentity,
};
use crate::sym::StringInterner;
use crate::translator::Game;
use crate::translator::pair_hooks::fnv_fo4::creature_catalog as fnv_catalog;
use crate::translator::pair_hooks::fnv_fo4::creature_dependencies as fnv_dependencies;
use crate::translator::pair_hooks::fnv_fo4::creature_live_builder as fnv_live_builder;
use crate::translator::pair_hooks::fnv_fo4::creature_live_preparations as fnv_live;
use crate::translator::pair_hooks::fnv_fo4::creature_motion as fnv_motion;
use crate::translator::pair_hooks::fnv_fo4::creature_mvp_adapter as fnv_mvp;
use crate::translator::pair_hooks::fnv_fo4::creature_race_data as fnv_race_data;
use crate::translator::pair_hooks::fnv_fo4::creature_recipe as fnv_recipe;
use crate::translator::pair_hooks::fnv_fo4::gecko_creature::canonical_gecko_asset_evidence;

const PLAN_VERSION: u32 = 1;
const DEFAULT_DEBUG_DIR: &str = "debug/creature_corpus";
const PLAN_FILE: &str = "plan.json";
const DISCOVERY_LEDGER_FILE: &str = "capability_ledger.json";
const EXECUTION_LEDGER_FILE: &str = "execution_ledger.json";
const DEPENDENCY_LEDGER_FILE: &str = "dependency_ledger.json";
const LIVE_PREPARATION_LEDGER_FILE: &str = "live_preparation_ledger.json";
const LIVE_RECIPE_DIAGNOSTICS_FILE: &str = "live_recipe_diagnostics.json";
const ANCILLARY_NPC_COMMIT_LEDGER_FILE: &str = "ancillary_npc_commit_ledger.json";
const LIVE_PREPARED_DIR: &str = "live_prepared";
const LIVE_PREPARED_WORK_DIR: &str = "live_prepared.work";

pub struct PlanCreatureDependenciesPhase;
pub struct DiscoverCreatureCorpusPhase;

pub struct ExecuteCreatureCorpusPhase;

struct LiveCorpusDiscovery {
    corpus: CreatureCorpusPlan,
    native_catalog: NativeCatalogBridgeLedger,
    jobs: Vec<CreatureCorpusJob>,
    preparation: CreatureLivePreparationLedger,
}

#[derive(Clone, Debug, Serialize)]
struct CreatureLivePreparationLedger {
    version: u32,
    source_game: String,
    candidate_count: usize,
    family_count: usize,
    ready_candidate_count: usize,
    accounted_candidate_count: usize,
    blocked_candidate_count: usize,
    ready_family_count: usize,
    accounted_family_count: usize,
    blocked_family_count: usize,
    terminals: Vec<CreatureLiveFamilyTerminal>,
}

#[derive(Serialize)]
struct FnvFo3LiveRecipeDiagnostics<'a> {
    motion_families: &'a [fnv_motion::CreatureMotionFamilyEvidence],
    motion_sets: &'a [fnv_motion::CreatureFamilyMotionSet],
    movement_issues: &'a [fnv_race_data::LegacyMovementMvpIssue],
    graph_contracts: &'a [fnv_recipe::CreatureGraphContractEvidence],
    recipe_ledger: &'a fnv_recipe::CreatureFamilyRecipeLedger,
}

#[derive(Clone, Debug, Serialize)]
struct CreatureLiveFamilyTerminal {
    family_id: String,
    member_source_keys: Vec<String>,
    disposition: String,
    blockers: Vec<CreatureLivePreparationBlocker>,
    warnings: Vec<crate::source_rig::CreatureDegradationReceipt>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
struct CreatureLivePreparationBlocker {
    code: String,
    detail: String,
}

struct LivePreparedFamilyBundle {
    family_id: String,
    member_source_keys: Vec<String>,
    asset_recipe: crate::source_rig::SourceRigExecutableRecipe,
    candidate_recipes: Vec<crate::source_rig::SourceRigExecutableRecipe>,
    closure: nif_core_native::creature_closure::CreatureClosureReceipt,
    closure_staged_data_root: PathBuf,
    converted_artifacts: Vec<LivePreparedConvertedArtifact>,
}

struct LivePreparedConvertedArtifact {
    runtime_path: String,
    source_path: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
struct CreatureDependencyPhaseLedger {
    version: u32,
    source_game: String,
    candidate_count: usize,
    curated_exclusion_count: usize,
    ready_candidate_count: usize,
    blocked_candidate_count: usize,
    dependency_record_count: usize,
    admitted_translation_count: usize,
    blocker_counts: BTreeMap<String, usize>,
    reservation_blockers: Vec<CreatureReservationBlockerLedgerEntry>,
    admissions: Vec<CreatureDependencyAdmissionLedgerEntry>,
}

#[derive(Clone, Debug, Serialize)]
struct CreatureReservationBlockerLedgerEntry {
    source_key: String,
    code: String,
    detail: String,
}

enum CreatureReservationPlan {
    Skyrim(Vec<SkyrimCreatureReservationSource>),
    Legacy {
        primary_creature_sources: Vec<FormKey>,
        ancillary_npc_sources: Vec<FormKey>,
    },
}

struct LoadedLegacyPluginRecords {
    handle_id: u64,
    records: Vec<Record>,
    provenance: fnv_catalog::CreatureProvenance,
    source_master_names: Vec<String>,
}

struct LoadedLegacyAppearanceRaceRecord {
    record: Record,
    provenance: fnv_catalog::CreatureProvenance,
}

#[derive(Clone, Debug, Serialize)]
struct CreatureDependencyAdmissionLedgerEntry {
    source_key: String,
    signature: String,
    mechanism: String,
    owner_family_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DiscoveryProfile {
    #[default]
    AllCreaturesV1,
    InjectedFixtureV1,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiscoveryParams {
    #[serde(default)]
    profile: DiscoveryProfile,
    #[serde(default)]
    jobs: Vec<JobDeclaration>,
    #[serde(default = "default_debug_dir")]
    debug_dir: String,
    #[cfg(test)]
    #[serde(default)]
    fixture: Option<InjectedCorpusFixture>,
}

#[cfg(test)]
#[derive(Clone, Debug, Deserialize)]
struct InjectedCorpusFixture {
    rig_families: Vec<RigFamily>,
    motion_sets: Vec<MotionSet>,
    candidates: Vec<CreatureCorpusCandidate>,
}

#[derive(Clone, Debug, Deserialize)]
struct ExecutionParams {
    #[serde(default = "default_plan_path")]
    plan_path: String,
    #[serde(default)]
    mode: ExecutionMode,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    #[default]
    Strict,
    BestEffort,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureAdapterProfile {
    EvidenceBoundRecipe,
    SkyrimWolf,
    FnvGecko,
    UnsupportedCapability,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactSourceRoot {
    #[default]
    Mod,
    SourceExtracted,
    TargetExtracted,
    TargetData,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    ProjectHkx,
    CharacterHkx,
    RootBehaviorHkx,
    CoreBehaviorHkx,
    SkeletonHkx,
    IdleHkx,
    LocomotionHkx,
    MeleeHkx,
    SkeletonNif,
    VisualNif,
    Material,
    Texture,
    Asset,
    Record,
}

impl ArtifactKind {
    fn is_record(self) -> bool {
        self == Self::Record
    }

    fn is_hkx(self) -> bool {
        matches!(
            self,
            Self::ProjectHkx
                | Self::CharacterHkx
                | Self::RootBehaviorHkx
                | Self::CoreBehaviorHkx
                | Self::SkeletonHkx
                | Self::IdleHkx
                | Self::LocomotionHkx
                | Self::MeleeHkx
        )
    }

    fn is_nif(self) -> bool {
        matches!(self, Self::SkeletonNif | Self::VisualNif)
    }
}

#[derive(Clone, Debug, Deserialize)]
struct JobDeclaration {
    output_slug: String,
    #[serde(default)]
    adapter: Option<CreatureAdapterProfile>,
    #[serde(default)]
    unsupported_capability: Option<String>,
    publish_root: String,
    #[serde(default)]
    artifacts: Vec<ArtifactDeclaration>,
    #[serde(default)]
    executable_recipe: Option<ExecutableRecipeDeclaration>,
}

#[derive(Clone, Debug, Deserialize)]
struct ExecutableRecipeDeclaration {
    #[serde(default)]
    recipe_source_root: ArtifactSourceRoot,
    recipe_path: String,
    #[serde(default)]
    creature_closure_receipt_source_root: ArtifactSourceRoot,
    creature_closure_receipt_path: String,
    #[serde(default)]
    creature_closure_staged_data_source_root: ArtifactSourceRoot,
    creature_closure_staged_data_root: String,
    converted_artifacts: Vec<ConvertedArtifactDeclaration>,
}

#[derive(Clone, Debug, Deserialize)]
struct ConvertedArtifactDeclaration {
    runtime_path: String,
    #[serde(default)]
    source_root: ArtifactSourceRoot,
    source_path: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ArtifactDeclaration {
    #[serde(default)]
    source_root: ArtifactSourceRoot,
    source_path: String,
    target_path: String,
    kind: ArtifactKind,
    #[serde(default)]
    record_signature: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    root: bool,
    #[serde(default)]
    skeleton_name: Option<String>,
    #[serde(default)]
    float_slot_names: Vec<String>,
    #[serde(default)]
    sequence_index: Option<usize>,
    #[serde(default)]
    event_map: BTreeMap<String, String>,
    #[serde(default)]
    target_sample_rate_hz: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatureCorpusExecutionPlan {
    pub version: u32,
    pub debug_dir: String,
    pub native_catalog: NativeCatalogBridgeLedger,
    pub corpus: CreatureCorpusPlan,
    pub jobs: Vec<CreatureCorpusJob>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeBridgeSubject {
    RaceCandidate,
    CreatureCandidate,
    RecordVariant,
    TemplateDependency,
    RejectedDependent,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct NativeBridgeEntry {
    pub source_key: String,
    pub subject: NativeBridgeSubject,
    pub disposition: String,
    pub reason_codes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeCatalogBridgeLedger {
    pub catalog: String,
    pub winner_count: usize,
    pub candidate_count: usize,
    pub dependent_count: usize,
    pub rejected_dependent_count: usize,
    pub entries_blake3: String,
    pub entries: Vec<NativeBridgeEntry>,
    #[serde(default)]
    pub motion: NativeMotionBridgeLedger,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeMotionDisposition {
    ReadyGroundMelee,
    ReadyRangedCreature,
    ReadyMixedCreature,
    Incomplete,
    Ambiguous,
    Unclassified,
    EvidenceIncomplete,
    ConversionIncomplete,
    UnsupportedOverlay,
    #[default]
    AdapterUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct NativeMotionKfEntry {
    pub source_kf: String,
    pub disposition: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct NativeMotionFamilyEntry {
    pub family_id: String,
    pub disposition: NativeMotionDisposition,
    pub motion_set_emitted: bool,
    pub has_melee: bool,
    pub has_ranged: bool,
    pub has_overlay: bool,
    pub kfs: Vec<NativeMotionKfEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NativeMotionBridgeLedger {
    pub catalog: String,
    pub family_count: usize,
    pub terminal_family_count: usize,
    pub referenced_kf_count: usize,
    pub emitted_motion_set_count: usize,
    pub entries_blake3: String,
    pub entries: Vec<NativeMotionFamilyEntry>,
}

impl Default for NativeMotionBridgeLedger {
    fn default() -> Self {
        Self {
            catalog: "adapter_unavailable".to_string(),
            family_count: 0,
            terminal_family_count: 0,
            referenced_kf_count: 0,
            emitted_motion_set_count: 0,
            entries_blake3: blake3::hash(b"[]").to_hex().to_string(),
            entries: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatureCorpusJob {
    pub job_id: String,
    pub source_key: String,
    pub output_slug: String,
    pub family_id: String,
    pub adapter: CreatureAdapterProfile,
    pub unsupported_capability: Option<String>,
    pub publish_root: String,
    #[serde(default)]
    pub graph_paths: Vec<String>,
    pub artifacts: Vec<PlannedArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_recipe: Option<PlannedExecutableRecipe>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub member_source_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidate_recipes: Vec<PlannedCandidateRecipe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family_bundle_blake3: Option<String>,
    pub preflight_issues: Vec<PreflightIssue>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct PlannedCandidateRecipe {
    pub source_key: String,
    pub recipe_source_root: ArtifactSourceRoot,
    pub recipe_path: String,
    pub recipe_blake3: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlannedExecutableRecipe {
    pub source_key: String,
    pub family_id: String,
    pub publish_root: String,
    pub recipe_source_root: ArtifactSourceRoot,
    pub recipe_path: String,
    pub recipe_blake3: String,
    pub creature_closure_receipt_source_root: ArtifactSourceRoot,
    pub creature_closure_receipt_path: String,
    pub creature_closure_receipt_blake3: String,
    pub creature_closure_staged_data_source_root: ArtifactSourceRoot,
    pub creature_closure_staged_data_root: String,
    pub graph_paths: Vec<String>,
    pub converted_artifacts: Vec<PlannedConvertedArtifact>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlannedConvertedArtifact {
    pub runtime_path: String,
    pub source_root: ArtifactSourceRoot,
    pub source_path: String,
    pub source_blake3: String,
}

impl PlannedCandidateRecipe {
    fn from_asset_recipe(recipe: &PlannedExecutableRecipe) -> Self {
        Self {
            source_key: recipe.source_key.clone(),
            recipe_source_root: recipe.recipe_source_root,
            recipe_path: recipe.recipe_path.clone(),
            recipe_blake3: recipe.recipe_blake3.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlannedArtifact {
    pub source_root: ArtifactSourceRoot,
    pub source_path: String,
    pub target_path: String,
    pub kind: ArtifactKind,
    pub record_signature: Option<String>,
    pub dependencies: Vec<String>,
    pub root: bool,
    pub source_blake3: Option<String>,
    #[serde(default)]
    pub skeleton_name: Option<String>,
    #[serde(default)]
    pub float_slot_names: Vec<String>,
    #[serde(default)]
    pub sequence_index: Option<usize>,
    #[serde(default)]
    pub event_map: BTreeMap<String, String>,
    #[serde(default)]
    pub target_sample_rate_hz: Option<f64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum PreflightIssue {
    UnsupportedCapability { capability: String },
    MissingSourceRoot { root: ArtifactSourceRoot },
    MissingSourceArtifact { path: String },
    SourceArtifactNotFile { path: String },
    SourceArtifactRead { path: String, message: String },
    MissingAdapterArtifact { kind: ArtifactKind },
    MissingRecordSignature { signature: String },
    CanonicalAdapterPathMissing { path: String },
    UnconvertedSourceArtifact { path: String, kind: ArtifactKind },
    MissingExecutableRecipe,
    InvalidExecutableRecipe { message: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveryLedger {
    pub version: u32,
    pub candidate_count: usize,
    pub planned_count: usize,
    pub rejected_count: usize,
    pub job_count: usize,
    pub jobs_preflight_ready: usize,
    pub jobs_preflight_failed: usize,
    pub native_catalog: String,
    pub native_winner_count: usize,
    pub native_dependent_count: usize,
    pub native_rejected_dependent_count: usize,
    pub native_entries_blake3: String,
    pub native_motion_family_count: usize,
    pub native_motion_referenced_kf_count: usize,
    pub native_motion_emitted_set_count: usize,
    pub native_motion_entries_blake3: String,
    pub capability_available: BTreeMap<String, usize>,
    pub capability_missing: BTreeMap<String, usize>,
    pub rejection_reason_counts: BTreeMap<String, usize>,
    pub preflight_issue_counts: BTreeMap<String, usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalDisposition {
    Published,
    Failed,
    UnsupportedCapability,
    AbortedStrict,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobExecutionLedger {
    pub job_id: String,
    pub family_id: String,
    pub disposition: TerminalDisposition,
    pub assets_published: usize,
    pub records_deferred: usize,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FamilyExecutionLedger {
    pub family_id: String,
    pub staging_dir: String,
    pub publish_root: String,
    pub disposition: TerminalDisposition,
    pub job_ids: Vec<String>,
    pub assets_published: usize,
    pub records_deferred: usize,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandidateExecutionLedger {
    pub source_key: String,
    pub owner_family_id: String,
    pub disposition: TerminalDisposition,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchExecutionLedger {
    pub version: u32,
    pub mode: ExecutionMode,
    pub source_rejected_count: usize,
    pub strict_aborted: bool,
    pub candidate_disposition_counts: BTreeMap<String, usize>,
    pub job_disposition_counts: BTreeMap<String, usize>,
    pub family_disposition_counts: BTreeMap<String, usize>,
    pub candidates: Vec<CandidateExecutionLedger>,
    pub jobs: Vec<JobExecutionLedger>,
    pub families: Vec<FamilyExecutionLedger>,
}

impl Phase for PlanCreatureDependenciesPhase {
    fn name(&self) -> &'static str {
        "plan_creature_dependencies"
    }

    fn requires_source_plugin(&self) -> bool {
        true
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        if ctx.run.target != Game::Fo4 {
            return Err(PhaseError::BadParams(
                "creature dependency planning requires Fallout 4 as the target".to_string(),
            ));
        }
        let debug_dir = canonical_debug_dir(
            ctx.params
                .get("debug_dir")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(DEFAULT_DEBUG_DIR),
        )?;
        let (ledger, admissions, reservation_plan) = match ctx.run.source {
            Game::SkyrimSe => plan_skyrim_creature_dependencies(ctx)?,
            Game::Fnv | Game::Fo3 => plan_legacy_creature_dependencies(ctx)?,
            source => {
                return Err(PhaseError::BadParams(format!(
                    "creature dependency planning does not support {} -> fo4",
                    source.as_str()
                )));
            }
        };
        ctx.run
            .prepare_mapper_state_for_creature_dependency_plan()
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let ledger_path = ctx
            .mod_path
            .join(path_from_canonical(&debug_dir))
            .join(DEPENDENCY_LEDGER_FILE);
        write_json(&ledger_path, &ledger)?;
        match reservation_plan {
            CreatureReservationPlan::Skyrim(sources) => ctx
                .run
                .install_skyrim_creature_dependency_plan(
                    admissions,
                    sources,
                    ledger.candidate_count,
                )
                .map_err(|error| PhaseError::Internal(error.to_string()))?,
            CreatureReservationPlan::Legacy {
                primary_creature_sources,
                ancillary_npc_sources,
            } => ctx
                .run
                .install_legacy_creature_dependency_plan(
                    admissions,
                    &primary_creature_sources,
                    &ancillary_npc_sources,
                    ledger.candidate_count,
                )
                .map_err(|error| PhaseError::Internal(error.to_string()))?,
        }
        let admitted_translation_count = ledger.admitted_translation_count as u32;
        Ok(PhaseReport {
            records_deferred: admitted_translation_count,
            warnings: ledger.blocked_candidate_count as u32,
            ..Default::default()
        })
    }
}

fn plan_skyrim_creature_dependencies(
    ctx: &PhaseCtx<'_>,
) -> Result<
    (
        CreatureDependencyPhaseLedger,
        Vec<CreatureDependencyAdmission>,
        CreatureReservationPlan,
    ),
    PhaseError,
> {
    let records = read_live_records(
        ctx,
        skyrim_dependencies::required_skyrim_creature_dependency_signatures(),
    )?;
    let catalog = skyrim_catalog::build_creature_corpus_plan(&records, &ctx.run.interner);
    let dependency = skyrim_dependencies::build_skyrim_creature_dependency_ledger(
        &records,
        &catalog,
        &ctx.run.interner,
    )
    .map_err(|error| PhaseError::Internal(error.to_string()))?;

    let mut owners_by_record = BTreeMap::<String, BTreeSet<String>>::new();
    let mut blocker_counts = BTreeMap::new();
    for candidate in &dependency.candidates {
        match &candidate.disposition {
            skyrim_dependencies::SkyrimCreatureDependencyCandidateDisposition::Ready => {
                let owner = candidate.family_id.as_ref().ok_or_else(|| {
                    PhaseError::Internal(format!(
                        "ready Skyrim creature candidate {} has no family",
                        stable_form_key_string(candidate.source_race, &ctx.run.interner)
                            .unwrap_or_else(|_| format!("{:06X}", candidate.source_race.local))
                    ))
                })?;
                for form_key in &candidate.closure {
                    owners_by_record
                        .entry(stable_form_key_string(*form_key, &ctx.run.interner)?)
                        .or_default()
                        .insert(owner.clone());
                }
            }
            skyrim_dependencies::SkyrimCreatureDependencyCandidateDisposition::Blocked {
                blockers,
            } => {
                for blocker in blockers {
                    *blocker_counts
                        .entry(skyrim_dependency_blocker_code(blocker))
                        .or_insert(0) += 1;
                }
            }
            skyrim_dependencies::SkyrimCreatureDependencyCandidateDisposition::CuratedExclusion => {
            }
        }
    }

    let mut admissions = Vec::new();
    let mut ledger_admissions = Vec::new();
    for record in &dependency.records {
        let skyrim_dependencies::SkyrimRecordLoweringDisposition::Ready { kind } =
            &record.disposition
        else {
            continue;
        };
        if *kind == skyrim_dependencies::SkyrimRecordLoweringKind::SourceRigProjection {
            continue;
        }
        let source_key = stable_form_key_string(record.source.form_key, &ctx.run.interner)?;
        let Some(owner_family_ids) = ready_owner_ids(&owners_by_record, &source_key) else {
            continue;
        };
        let source_signature = SigCode::from_str(&record.source.signature)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let mechanism = skyrim_lowering_mechanism(*kind).to_string();
        admissions.push(CreatureDependencyAdmission {
            source_form_key: record.source.form_key,
            source_signature,
            owner_family_ids: owner_family_ids.clone(),
        });
        ledger_admissions.push(CreatureDependencyAdmissionLedgerEntry {
            source_key,
            signature: record.source.signature.clone(),
            mechanism,
            owner_family_ids,
        });
    }
    sort_dependency_admissions(&mut admissions, &ctx.run.interner)?;
    ledger_admissions.sort_by(|left, right| {
        left.source_key
            .to_ascii_lowercase()
            .cmp(&right.source_key.to_ascii_lowercase())
            .then_with(|| left.signature.cmp(&right.signature))
    });
    let (reservation_sources, mut reservation_blockers) =
        build_skyrim_creature_reservation_sources(ctx, &catalog, &dependency)?;
    reservation_blockers.sort_by(|left, right| {
        left.source_key
            .to_ascii_lowercase()
            .cmp(&right.source_key.to_ascii_lowercase())
            .then_with(|| left.code.cmp(&right.code))
    });
    for blocker in &reservation_blockers {
        *blocker_counts.entry(blocker.code.clone()).or_insert(0) += 1;
    }
    let ready_candidate_keys = dependency
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.disposition,
                skyrim_dependencies::SkyrimCreatureDependencyCandidateDisposition::Ready
            )
        })
        .map(|candidate| stable_form_key_string(candidate.source_race, &ctx.run.interner))
        .collect::<Result<BTreeSet<_>, PhaseError>>()?;
    let ready_reservation_blocker_count = reservation_blockers
        .iter()
        .filter(|blocker| ready_candidate_keys.contains(&blocker.source_key))
        .count();
    let ready_candidate_count = dependency
        .accounting
        .ready_candidates
        .checked_sub(ready_reservation_blocker_count)
        .ok_or_else(|| {
            PhaseError::Internal(
                "Skyrim creature reservation blockers exceed ready dependency candidates"
                    .to_string(),
            )
        })?;
    let blocked_candidate_count =
        dependency.accounting.blocked_candidates + ready_reservation_blocker_count;

    Ok((
        CreatureDependencyPhaseLedger {
            version: 1,
            source_game: "skyrimse".to_string(),
            candidate_count: dependency.accounting.candidate_races,
            curated_exclusion_count: dependency.accounting.curated_exclusions,
            ready_candidate_count,
            blocked_candidate_count,
            dependency_record_count: dependency.records.len(),
            admitted_translation_count: admissions.len(),
            blocker_counts,
            reservation_blockers,
            admissions: ledger_admissions,
        },
        admissions,
        CreatureReservationPlan::Skyrim(reservation_sources),
    ))
}

fn build_skyrim_creature_reservation_sources(
    ctx: &PhaseCtx<'_>,
    catalog: &skyrim_catalog::CreatureCorpusPlan,
    dependency: &skyrim_dependencies::SkyrimCreatureDependencyLedger,
) -> Result<
    (
        Vec<SkyrimCreatureReservationSource>,
        Vec<CreatureReservationBlockerLedgerEntry>,
    ),
    PhaseError,
> {
    let candidates = dependency
        .candidates
        .iter()
        .filter(|candidate| {
            !matches!(
                candidate.disposition,
                skyrim_dependencies::SkyrimCreatureDependencyCandidateDisposition::CuratedExclusion
            )
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let mesh_root = extracted_mesh_root(ctx.source_extracted_dir);
    let race_evidence = skyrim_motion::race_motion_evidence_from_creature_plan(catalog);
    let motion = match skyrim_motion::load_extracted_skyrim_creature_motion_catalog(
        &mesh_root.join("actors"),
        &mesh_root.join("animationdata"),
        &race_evidence,
    ) {
        Ok(motion) => motion,
        Err(error) => {
            return Ok((
                Vec::new(),
                candidates
                    .into_iter()
                    .map(|candidate| {
                        Ok(CreatureReservationBlockerLedgerEntry {
                            source_key: stable_form_key_string(
                                candidate.source_race,
                                &ctx.run.interner,
                            )?,
                            code: "skyrim_motion_catalog_unavailable".to_string(),
                            detail: error.to_string(),
                        })
                    })
                    .collect::<Result<Vec<_>, PhaseError>>()?,
            ));
        }
    };
    if motion.families.len() != skyrim_dependencies::SKYRIM_CREATURE_FAMILY_COUNT {
        return Ok((
            Vec::new(),
            candidates
                .into_iter()
                .map(|candidate| {
                    Ok(CreatureReservationBlockerLedgerEntry {
                        source_key: stable_form_key_string(
                            candidate.source_race,
                            &ctx.run.interner,
                        )?,
                        code: "skyrim_motion_family_census_mismatch".to_string(),
                        detail: format!(
                            "expected {} exact motion families, found {}",
                            skyrim_dependencies::SKYRIM_CREATURE_FAMILY_COUNT,
                            motion.families.len()
                        ),
                    })
                })
                .collect::<Result<Vec<_>, PhaseError>>()?,
        ));
    }

    let mut motion_by_project = BTreeMap::<String, Vec<(String, String)>>::new();
    for family in &motion.families {
        let project_path = normalize_skyrim_motion_project_path(&family.project_path);
        motion_by_project
            .entry(project_path.clone())
            .or_default()
            .push((family.family_id.clone(), project_path));
    }
    let race_by_source = catalog
        .races
        .iter()
        .map(|race| (race.source_race, race))
        .collect::<HashMap<_, _>>();
    let dependency_record_by_source = dependency
        .records
        .iter()
        .map(|record| (record.source.form_key, record))
        .collect::<HashMap<_, _>>();

    let mut sources = Vec::new();
    let mut blockers = Vec::new();
    for candidate in candidates {
        let source_key = stable_form_key_string(candidate.source_race, &ctx.run.interner)?;
        let Some(race) = race_by_source.get(&candidate.source_race).copied() else {
            blockers.push(CreatureReservationBlockerLedgerEntry {
                source_key,
                code: "skyrim_reservation_missing_catalog_race".to_string(),
                detail: "ready dependency candidate is absent from the live creature catalog"
                    .to_string(),
            });
            continue;
        };
        let matches = race
            .project_paths
            .first()
            .into_iter()
            .filter_map(|path| motion_by_project.get(&normalize_skyrim_motion_project_path(path)))
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>();
        let [(motion_family_id, normalized_project_path)] =
            matches.iter().collect::<Vec<_>>().as_slice()
        else {
            blockers.push(CreatureReservationBlockerLedgerEntry {
                source_key,
                code: if matches.is_empty() {
                    "skyrim_reservation_motion_project_missing".to_string()
                } else {
                    "skyrim_reservation_motion_project_ambiguous".to_string()
                },
                detail: format!(
                    "primary decoded RACE project from {:?} resolved to {} exact motion families: {:?}",
                    race.project_paths,
                    matches.len(),
                    matches
                ),
            });
            continue;
        };
        let owner = SkyrimCreatureReservationOwner {
            source_race: candidate.source_race,
            motion_family_id: (*motion_family_id).clone(),
            normalized_project_path: (*normalized_project_path).clone(),
        };

        let mut candidate_sources = Vec::new();
        let mut invalid_projection = None;
        for form_key in &candidate.closure {
            let Some(record) = dependency_record_by_source.get(form_key).copied() else {
                invalid_projection = Some(format!(
                    "closure record {} has no dependency record receipt",
                    stable_form_key_string(*form_key, &ctx.run.interner)?
                ));
                break;
            };
            let Some(kind) =
                skyrim_reservation_kind(&record.source.signature, *form_key, candidate.source_race)
            else {
                continue;
            };
            candidate_sources.push(SkyrimCreatureReservationSource {
                source: *form_key,
                kind,
                owners: vec![owner.clone()],
            });
        }
        if invalid_projection.is_none()
            && !candidate_sources.iter().any(|source| {
                source.source == candidate.source_race
                    && source.kind == SkyrimCreatureReservedRecordKind::Race
            })
        {
            invalid_projection =
                Some("candidate closure has no exact source RACE projection".to_string());
        }
        if let Some(detail) = invalid_projection {
            blockers.push(CreatureReservationBlockerLedgerEntry {
                source_key,
                code: "skyrim_reservation_projection_invalid".to_string(),
                detail,
            });
            continue;
        }
        sources.extend(candidate_sources);
    }
    Ok((sources, blockers))
}

fn normalize_skyrim_motion_project_path(path: &str) -> String {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    normalized
        .strip_prefix("actors/")
        .unwrap_or(&normalized)
        .to_string()
}

fn skyrim_reservation_kind(
    signature: &str,
    source: FormKey,
    candidate_race: FormKey,
) -> Option<SkyrimCreatureReservedRecordKind> {
    match signature {
        "RACE" if source == candidate_race => Some(SkyrimCreatureReservedRecordKind::Race),
        "NPC_" => Some(SkyrimCreatureReservedRecordKind::Npc),
        "ARMO" => Some(SkyrimCreatureReservedRecordKind::Armor),
        "ARMA" => Some(SkyrimCreatureReservedRecordKind::ArmorAddon),
        "BPTD" => Some(SkyrimCreatureReservedRecordKind::BodyPartData),
        _ => None,
    }
}

fn plan_legacy_creature_dependencies(
    ctx: &PhaseCtx<'_>,
) -> Result<
    (
        CreatureDependencyPhaseLedger,
        Vec<CreatureDependencyAdmission>,
        CreatureReservationPlan,
    ),
    PhaseError,
> {
    let loaded = load_live_legacy_plugin_records(ctx)?;
    let sources = loaded_legacy_record_sources(&loaded);
    let source_load_orders = loaded_legacy_source_load_orders(&loaded);
    let excluded_signatures = ctx
        .run
        .config
        .skip_record_signatures
        .iter()
        .map(|signature| signature.trim().to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    let dependency =
        fnv_dependencies::build_creature_dependency_ledger_with_load_orders_and_exclusions(
            &sources,
            fnv_dependencies::CreatureDependencyOptions {
                expected_creature_winners: (ctx.run.source == Game::Fnv)
                    .then_some(fnv_catalog::EXPECTED_FULL_MERGED_CREA_WINNERS),
            },
            &source_load_orders,
            &excluded_signatures,
            &ctx.run.interner,
        )
        .map_err(|error| PhaseError::Internal(error.to_string()))?;

    let mut owners_by_record = BTreeMap::<fnv_catalog::StableFormKey, BTreeSet<String>>::new();
    let mut blocker_counts = BTreeMap::new();
    for candidate in &dependency.candidates {
        if candidate.blockers.is_empty() {
            let owner = format!("candidate:{}", candidate.source);
            for form_key in &candidate.closure_form_keys {
                owners_by_record
                    .entry(form_key.clone())
                    .or_default()
                    .insert(owner.clone());
            }
        } else {
            for blocker in &candidate.blockers {
                *blocker_counts
                    .entry(legacy_dependency_blocker_code(blocker))
                    .or_insert(0) += 1;
            }
        }
    }

    let mut admissions = Vec::new();
    let mut ledger_admissions = Vec::new();
    for record in &dependency.records {
        let fnv_dependencies::PairRecordLoweringDisposition::Ready { mechanism } = &record.lowering
        else {
            continue;
        };
        if mechanism == "source_rig_creature_projection" {
            continue;
        }
        let Some(owner_family_ids) = ready_owner_ids(&owners_by_record, &record.source) else {
            continue;
        };
        let source_signature = SigCode::from_str(&record.signature)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let source_form_key = FormKey {
            local: record.source.local,
            plugin: ctx.run.interner.intern(&record.source.plugin),
        };
        admissions.push(CreatureDependencyAdmission {
            source_form_key,
            source_signature,
            owner_family_ids: owner_family_ids.clone(),
        });
        ledger_admissions.push(CreatureDependencyAdmissionLedgerEntry {
            source_key: record.source.to_string(),
            signature: record.signature.clone(),
            mechanism: mechanism.clone(),
            owner_family_ids,
        });
    }
    sort_dependency_admissions(&mut admissions, &ctx.run.interner)?;
    ledger_admissions.sort_by(|left, right| {
        left.source_key
            .to_ascii_lowercase()
            .cmp(&right.source_key.to_ascii_lowercase())
            .then_with(|| left.signature.cmp(&right.signature))
    });
    let primary_creature_sources = dependency
        .candidates
        .iter()
        .map(|candidate| FormKey {
            local: candidate.source.local,
            plugin: ctx.run.interner.intern(&candidate.source.plugin),
        })
        .collect::<Vec<_>>();
    let ancillary_npc_sources = dependency
        .records
        .iter()
        .filter(|record| record.signature == "NPC_")
        .map(|record| FormKey {
            local: record.source.local,
            plugin: ctx.run.interner.intern(&record.source.plugin),
        })
        .collect::<Vec<_>>();

    Ok((
        CreatureDependencyPhaseLedger {
            version: 1,
            source_game: ctx.run.source.as_str().to_string(),
            candidate_count: dependency.winning_creatures,
            curated_exclusion_count: 0,
            ready_candidate_count: dependency.ready_candidates,
            blocked_candidate_count: dependency.blocked_candidates,
            dependency_record_count: dependency.records.len(),
            admitted_translation_count: admissions.len(),
            blocker_counts,
            reservation_blockers: Vec::new(),
            admissions: ledger_admissions,
        },
        admissions,
        CreatureReservationPlan::Legacy {
            primary_creature_sources,
            ancillary_npc_sources,
        },
    ))
}

fn load_live_legacy_plugin_records(
    ctx: &PhaseCtx<'_>,
) -> Result<Vec<LoadedLegacyPluginRecords>, PhaseError> {
    // ConversionRun master handles belong to the FO4 target. The merged FNV/FO3
    // plugin is the sole legacy source owned by this run.
    let source_handle = ctx.run.require_source_handle().map_err(|error| {
        PhaseError::Internal(format!("legacy creature source unavailable: {error}"))
    })?;
    let mut loaded = Vec::with_capacity(1);
    for (precedence, handle) in [source_handle].into_iter().enumerate() {
        let source_plugin = crate::source_read::plugin_name_for_handle(handle)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let source_game = esp_authoring_core::plugin_runtime::plugin_handle_game_no_py(handle)
            .map_err(PhaseError::Internal)?
            .ok_or_else(|| {
                PhaseError::Internal(format!(
                    "legacy creature source handle {source_plugin} has no recorded game"
                ))
            })?;
        let (game, schema_game) = match source_game.to_ascii_lowercase().as_str() {
            "fnv" | "falloutnv" | "fallout_new_vegas" => {
                (fnv_catalog::LegacyCreatureGame::Fnv, "fnv")
            }
            "fo3" | "fallout3" => (fnv_catalog::LegacyCreatureGame::Fo3, "fo3"),
            other => {
                return Err(PhaseError::Internal(format!(
                    "legacy creature source handle {source_plugin} records unsupported game {other}"
                )));
            }
        };
        let schema = crate::schema::AuthoringSchema::for_game(schema_game)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let mut records =
            read_legacy_dependency_records_from_handle(handle, &schema, &ctx.run.interner)?;
        let source_master_names =
            esp_authoring_core::plugin_runtime::plugin_handle_master_names_no_py(handle)
                .map_err(PhaseError::Internal)?;
        for record in &mut records {
            crate::translator::pair_hooks::fnv_fo4::decode_legacy_lvlc_entries(
                record,
                &source_plugin,
                &source_master_names,
                &ctx.run.interner,
            )
            .map_err(|error| {
                PhaseError::Internal(format!(
                    "decode live {source_plugin} LVLC {}: {error}",
                    stable_form_key_string(record.form_key, &ctx.run.interner)
                        .unwrap_or_else(|_| format!("{:06X}", record.form_key.local))
                ))
            })?;
        }
        loaded.push(LoadedLegacyPluginRecords {
            handle_id: handle,
            records,
            provenance: fnv_catalog::CreatureProvenance {
                game,
                source_plugin,
                precedence: u32::try_from(precedence).map_err(|_| {
                    PhaseError::Internal("legacy creature precedence exceeds u32".to_string())
                })?,
            },
            source_master_names,
        });
    }
    Ok(loaded)
}

fn ready_owner_ids<K: Ord>(
    owners_by_record: &BTreeMap<K, BTreeSet<String>>,
    record_key: &K,
) -> Option<Vec<String>> {
    owners_by_record
        .get(record_key)
        .map(|owners| owners.iter().cloned().collect())
}

fn loaded_legacy_record_sources(
    loaded: &[LoadedLegacyPluginRecords],
) -> Vec<fnv_catalog::LegacyRecordSource<'_>> {
    loaded
        .iter()
        .flat_map(|plugin| {
            plugin
                .records
                .iter()
                .map(|record| fnv_catalog::LegacyRecordSource {
                    record,
                    provenance: plugin.provenance.clone(),
                })
        })
        .collect()
}

fn loaded_legacy_appearance_race_sources(
    loaded: &[LoadedLegacyAppearanceRaceRecord],
) -> Vec<fnv_catalog::LegacyRecordSource<'_>> {
    loaded
        .iter()
        .map(|record| fnv_catalog::LegacyRecordSource {
            record: &record.record,
            provenance: record.provenance.clone(),
        })
        .collect()
}

fn collect_form_keys(value: &FieldValue, form_keys: &mut Vec<FormKey>) {
    match value {
        FieldValue::FormKey(form_key) => form_keys.push(*form_key),
        FieldValue::List(values) => {
            for value in values {
                collect_form_keys(value, form_keys);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_form_keys(value, form_keys);
            }
        }
        FieldValue::None
        | FieldValue::Bool(_)
        | FieldValue::Int(_)
        | FieldValue::Uint(_)
        | FieldValue::Float(_)
        | FieldValue::String(_)
        | FieldValue::Bytes(_) => {}
    }
}

fn load_legacy_ancillary_npc_target_appearance_records(
    ctx: &PhaseCtx<'_>,
    source_races: &[LoadedLegacyAppearanceRaceRecord],
) -> Result<Vec<Record>, PhaseError> {
    let mapper_state = ctx.run.mapper_state.as_ref().ok_or_else(|| {
        PhaseError::Internal(
            "ancillary NPC target appearance requires initialized mapper state".to_string(),
        )
    })?;
    let mut roots = source_races
        .iter()
        .map(|source| {
            mapper_state
                .source_to_target
                .get(&source.record.form_key)
                .copied()
                .ok_or_else(|| {
                    PhaseError::Internal(format!(
                        "ancillary source RACE {} has no mapped target RACE",
                        stable_form_key_string(source.record.form_key, &ctx.run.interner)
                            .unwrap_or_else(|_| format!("{:06X}", source.record.form_key.local))
                    ))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    sort_form_keys(&mut roots, &ctx.run.interner)?;
    roots.dedup();
    if roots.iter().any(|root| root.local == 0) {
        return Err(PhaseError::Internal(
            "ancillary source RACE maps to the null target FormKey".to_string(),
        ));
    }
    let root_set = roots.iter().copied().collect::<HashSet<_>>();
    let mut pending = roots;
    let mut visited = HashSet::new();
    let mut records = Vec::new();
    while let Some(form_key) = pending.pop() {
        if !visited.insert(form_key) {
            continue;
        }
        let record = ctx
            .run
            .read_target_record_if_available(form_key)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let Some(record) = record else {
            if root_set.contains(&form_key) {
                return Err(PhaseError::Internal(format!(
                    "mapped ancillary target RACE {} is unavailable from target record handles",
                    stable_form_key_string(form_key, &ctx.run.interner)?
                )));
            }
            continue;
        };
        let is_root = root_set.contains(&form_key);
        if is_root && record.sig.as_str() != "RACE" {
            return Err(PhaseError::Internal(format!(
                "mapped ancillary target RACE {} resolves as {}",
                stable_form_key_string(form_key, &ctx.run.interner)?,
                record.sig.as_str()
            )));
        }
        if !is_root && !matches!(record.sig.as_str(), "HDPT" | "TXST" | "FLST") {
            continue;
        }
        let mut references = record
            .fields
            .iter()
            .flat_map(|field| {
                let mut form_keys = Vec::new();
                collect_form_keys(&field.value, &mut form_keys);
                form_keys
            })
            .filter(|reference| !visited.contains(reference))
            .collect::<Vec<_>>();
        sort_form_keys(&mut references, &ctx.run.interner)?;
        references.dedup();
        pending.extend(references.into_iter().rev());
        records.push(record);
    }
    records.sort_by(|left, right| {
        let left_key = stable_form_key_string(left.form_key, &ctx.run.interner)
            .unwrap_or_else(|_| format!("{:06X}", left.form_key.local));
        let right_key = stable_form_key_string(right.form_key, &ctx.run.interner)
            .unwrap_or_else(|_| format!("{:06X}", right.form_key.local));
        left_key
            .cmp(&right_key)
            .then_with(|| left.sig.cmp(&right.sig))
    });
    Ok(records)
}

fn loaded_legacy_source_load_orders(
    loaded: &[LoadedLegacyPluginRecords],
) -> Vec<fnv_dependencies::LegacyPluginLoadOrder> {
    loaded
        .iter()
        .map(|plugin| fnv_dependencies::LegacyPluginLoadOrder {
            game: plugin.provenance.game,
            source_plugin: plugin.provenance.source_plugin.clone(),
            source_master_names: plugin.source_master_names.clone(),
        })
        .collect()
}

fn sort_dependency_admissions(
    admissions: &mut [CreatureDependencyAdmission],
    interner: &StringInterner,
) -> Result<(), PhaseError> {
    let mut keys = admissions
        .iter()
        .map(|admission| admission.source_form_key)
        .collect::<Vec<_>>();
    sort_form_keys(&mut keys, interner)?;
    let order = keys
        .into_iter()
        .enumerate()
        .map(|(index, key)| (key, index))
        .collect::<HashMap<_, _>>();
    admissions.sort_by_key(|admission| order[&admission.source_form_key]);
    Ok(())
}

fn skyrim_lowering_mechanism(kind: skyrim_dependencies::SkyrimRecordLoweringKind) -> &'static str {
    match kind {
        skyrim_dependencies::SkyrimRecordLoweringKind::SourceRigProjection => {
            "source_rig_projection"
        }
        skyrim_dependencies::SkyrimRecordLoweringKind::SkyrimPairHook => "skyrim_pair_hook",
        skyrim_dependencies::SkyrimRecordLoweringKind::SkyrimMagicContract => {
            "skyrim_magic_contract"
        }
        skyrim_dependencies::SkyrimRecordLoweringKind::SkyrimTargetProjection => {
            "skyrim_target_projection"
        }
        skyrim_dependencies::SkyrimRecordLoweringKind::SchemaIdentical => "schema_identical",
    }
}

fn skyrim_dependency_blocker_code(
    blocker: &skyrim_dependencies::SkyrimCreatureDependencyBlocker,
) -> String {
    match blocker {
        skyrim_dependencies::SkyrimCreatureDependencyBlocker::CuratedNonCreatureRace => {
            "curated_non_creature_race".to_string()
        }
        skyrim_dependencies::SkyrimCreatureDependencyBlocker::CatalogIssue(code) => {
            format!("catalog_issue:{code}")
        }
        skyrim_dependencies::SkyrimCreatureDependencyBlocker::MissingSourceRecord { .. } => {
            "missing_source_record".to_string()
        }
        skyrim_dependencies::SkyrimCreatureDependencyBlocker::TargetRecordUnsupported {
            signature,
        } => format!("target_record_unsupported:{signature}"),
        skyrim_dependencies::SkyrimCreatureDependencyBlocker::TargetFieldUnsupported {
            signature,
            field,
        } => format!("target_field_unsupported:{signature}.{field}"),
        skyrim_dependencies::SkyrimCreatureDependencyBlocker::TargetFieldLayoutMismatch {
            signature,
            field,
        } => format!("target_field_layout_mismatch:{signature}.{field}"),
        skyrim_dependencies::SkyrimCreatureDependencyBlocker::UnsupportedMagicLowering {
            signature,
        } => format!("unsupported_magic_lowering:{signature}"),
        skyrim_dependencies::SkyrimCreatureDependencyBlocker::UnsupportedCreatureWeapon => {
            "unsupported_creature_weapon".to_string()
        }
        skyrim_dependencies::SkyrimCreatureDependencyBlocker::SourceScriptLoweringUnavailable => {
            "source_script_lowering_unavailable".to_string()
        }
        skyrim_dependencies::SkyrimCreatureDependencyBlocker::MissingFamily => {
            "missing_family".to_string()
        }
    }
}

fn legacy_dependency_blocker_code(blocker: &fnv_dependencies::CreatureDependencyBlocker) -> String {
    match blocker {
        fnv_dependencies::CreatureDependencyBlocker::MissingSourceRecord { .. } => {
            "missing_source_record".to_string()
        }
        fnv_dependencies::CreatureDependencyBlocker::LoweringUnavailable {
            signature,
            reason_code,
            ..
        } => format!("lowering_unavailable:{signature}:{reason_code}"),
    }
}

impl Phase for DiscoverCreatureCorpusPhase {
    fn name(&self) -> &'static str {
        "discover_creature_corpus"
    }

    fn requires_source_plugin(&self) -> bool {
        true
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        let params: DiscoveryParams = serde_json::from_value(ctx.params.clone())
            .map_err(|error| PhaseError::BadParams(error.to_string()))?;
        let debug_dir = canonical_debug_dir(&params.debug_dir)?;
        let (corpus, native_catalog, live_jobs, preparation) = match params.profile {
            DiscoveryProfile::AllCreaturesV1 => {
                let discovery = discover_live_corpus(ctx, &debug_dir)?;
                (
                    discovery.corpus,
                    discovery.native_catalog,
                    Some(discovery.jobs),
                    Some(discovery.preparation),
                )
            }
            DiscoveryProfile::InjectedFixtureV1 => {
                #[cfg(test)]
                {
                    let fixture = params.fixture.ok_or_else(|| {
                        PhaseError::BadParams(
                            "injected fixture profile requires fixture".to_string(),
                        )
                    })?;
                    let corpus = CreatureCorpusPlan::build(
                        fixture.rig_families,
                        fixture.motion_sets,
                        fixture.candidates,
                    )
                    .map_err(|error| PhaseError::BadParams(error.to_string()))?;
                    let native_catalog = fixture_bridge_ledger(&corpus)?;
                    (corpus, native_catalog, None, None)
                }
                #[cfg(not(test))]
                {
                    return Err(PhaseError::BadParams(
                        "injected creature corpus fixtures are test-only".to_string(),
                    ));
                }
            }
        };

        let declarations = match params.profile {
            DiscoveryProfile::AllCreaturesV1 if !params.jobs.is_empty() => {
                return Err(PhaseError::BadParams(
                    "all_creatures_v1 jobs are produced from live pair evidence; injected job declarations are forbidden"
                        .to_string(),
                ));
            }
            DiscoveryProfile::AllCreaturesV1 => BTreeMap::new(),
            DiscoveryProfile::InjectedFixtureV1 => declarations_by_slug(params.jobs)?,
        };
        let roots = SourceRoots::from_ctx(ctx);
        let jobs = match live_jobs {
            Some(jobs) => jobs,
            None => build_jobs(&corpus, &declarations, &roots)?,
        };
        let plan = CreatureCorpusExecutionPlan {
            version: PLAN_VERSION,
            debug_dir: debug_dir.clone(),
            native_catalog,
            corpus,
            jobs,
        };
        let output_dir = ctx.mod_path.join(path_from_canonical(&debug_dir));
        if let Some(preparation) = preparation {
            write_json(&output_dir.join(LIVE_PREPARATION_LEDGER_FILE), &preparation)?;
            if preparation.blocked_candidate_count != 0 || preparation.blocked_family_count != 0 {
                return Err(PhaseError::Internal(format!(
                    "live creature preparation has {} blocked candidate(s) across {} blocked family/families; see {}",
                    preparation.blocked_candidate_count,
                    preparation.blocked_family_count,
                    output_dir.join(LIVE_PREPARATION_LEDGER_FILE).display()
                )));
            }
        }
        validate_execution_plan(&plan)?;

        let ledger = discovery_ledger(&plan);
        write_json(&output_dir.join(PLAN_FILE), &plan)?;
        write_json(&output_dir.join(DISCOVERY_LEDGER_FILE), &ledger)?;

        Ok(PhaseReport {
            warnings: ledger.jobs_preflight_failed as u32 + ledger.rejected_count as u32,
            items_failed: ledger.jobs_preflight_failed as u32,
            records_deferred: ledger.job_count as u32,
            ..Default::default()
        })
    }
}

impl Phase for ExecuteCreatureCorpusPhase {
    fn name(&self) -> &'static str {
        "execute_creature_corpus"
    }

    fn requires_source_plugin(&self) -> bool {
        false
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        let params: ExecutionParams = serde_json::from_value(ctx.params.clone())
            .map_err(|error| PhaseError::BadParams(error.to_string()))?;
        let plan_rel = canonical_debug_file(&params.plan_path)?;
        let plan_path = ctx.mod_path.join(path_from_canonical(&plan_rel));
        let plan: CreatureCorpusExecutionPlan = read_json(&plan_path)?;
        validate_execution_plan(&plan)?;

        let roots = SourceRoots::from_ctx(ctx);
        let mut ledger = if plan.jobs.iter().any(|job| job.executable_recipe.is_some()) {
            execute_evidence_bound_batch(&plan, params.mode, ctx.mod_path, ctx.run, &roots)?
        } else {
            execute_batch(
                &plan,
                params.mode,
                ctx.mod_path,
                &roots,
                ctx.run.output_sink.as_deref(),
                &FilesystemStager,
            )?
        };
        normalize_execution_ledger(&mut ledger);
        let ledger_path = ctx
            .mod_path
            .join(path_from_canonical(&plan.debug_dir))
            .join(EXECUTION_LEDGER_FILE);
        write_json(&ledger_path, &ledger)?;

        let assets_written = ledger
            .families
            .iter()
            .map(|family| family.assets_published)
            .sum::<usize>();
        let records_deferred = ledger
            .families
            .iter()
            .map(|family| family.records_deferred)
            .sum::<usize>();
        let failures = ledger
            .jobs
            .iter()
            .filter(|job| job.disposition != TerminalDisposition::Published)
            .count();

        if ledger.strict_aborted {
            return Err(PhaseError::Internal(format!(
                "strict creature corpus batch aborted; see {}",
                ledger_path.display()
            )));
        }

        Ok(PhaseReport {
            assets_written: assets_written as u32,
            records_deferred: records_deferred as u32,
            warnings: failures as u32,
            items_failed: failures as u32,
            ..Default::default()
        })
    }
}

fn default_debug_dir() -> String {
    DEFAULT_DEBUG_DIR.to_string()
}

fn default_plan_path() -> String {
    format!("{DEFAULT_DEBUG_DIR}/{PLAN_FILE}")
}

#[derive(Clone, Copy)]
struct SourceRoots<'a> {
    mod_root: &'a Path,
    source_extracted: &'a Path,
    target_extracted: Option<&'a Path>,
    target_data: Option<&'a Path>,
}

impl<'a> SourceRoots<'a> {
    fn from_ctx(ctx: &PhaseCtx<'a>) -> Self {
        Self {
            mod_root: ctx.mod_path,
            source_extracted: ctx.source_extracted_dir,
            target_extracted: ctx.target_extracted_dir,
            target_data: ctx.target_data_dir,
        }
    }

    fn resolve(&self, root: ArtifactSourceRoot, relative: &str) -> Option<PathBuf> {
        let base = match root {
            ArtifactSourceRoot::Mod => Some(self.mod_root),
            ArtifactSourceRoot::SourceExtracted => Some(self.source_extracted),
            ArtifactSourceRoot::TargetExtracted => self.target_extracted,
            ArtifactSourceRoot::TargetData => self.target_data,
        }?;
        Some(base.join(path_from_canonical(relative)))
    }
}

#[derive(Default)]
struct LegacyBridgeAdapters {
    motion_evidence: Vec<fnv_motion::CreatureMotionFamilyEvidence>,
    race_data: BTreeMap<fnv_catalog::RigFamilyKey, Fo4RaceDataTarget>,
    converted_rig_roots: BTreeMap<fnv_catalog::RigFamilyKey, String>,
    converted_body_nifs: BTreeMap<fnv_catalog::BodyVariantKey, String>,
    converted_clips: BTreeMap<String, String>,
    behavior_ready: BTreeSet<fnv_catalog::RigFamilyKey>,
    atomic_records: BTreeMap<fnv_catalog::StableFormKey, LegacyAtomicRecordProjection>,
    race_data_rejections: BTreeMap<fnv_catalog::RigFamilyKey, CreatureRejectionReason>,
}

struct LegacyAtomicRecordProjection {
    display_name: String,
    level: u16,
    health: u16,
    action_points: u16,
}

impl LegacyBridgeAdapters {
    fn from_live_assets(native: &fnv_catalog::CreatureCorpusPlan, mesh_root: &Path) -> Self {
        let motion_evidence = native
            .rig_families
            .iter()
            .map(|family| {
                let source_skeleton = legacy_asset_path(mesh_root, &family.key.skeleton_path);
                let ordered_bone_names =
                    crate::phase::animations::load_ordered_source_skeleton_names(&source_skeleton)
                        .ok();
                let ordered_float_slot_names = Vec::new();
                let kf_evidence = family
                    .recursive_kf_paths
                    .iter()
                    .map(|source_kf| {
                        let source_path = legacy_asset_path(mesh_root, source_kf);
                        if !source_path.is_file() {
                            return fnv_motion::CreatureKfEvidence {
                                source_game: family.key.game,
                                source_kf: source_kf.clone(),
                                sequence: fnv_motion::KfParseEvidence::MissingAsset,
                                idle_claims: Vec::new(),
                            };
                        }
                        let bytes = match fs::read(&source_path) {
                            Ok(bytes) => bytes,
                            Err(error) => {
                                return fnv_motion::CreatureKfEvidence {
                                    source_game: family.key.game,
                                    source_kf: source_kf.clone(),
                                    sequence: fnv_motion::KfParseEvidence::ParseFailed(
                                        error.to_string(),
                                    ),
                                    idle_claims: Vec::new(),
                                };
                            }
                        };
                        let skeleton_contract = ordered_bone_names.as_deref().map(|names| {
                            fnv_motion::CreatureKfSkeletonContract {
                                skeleton_path: &family.key.skeleton_path,
                                ordered_bone_names: names,
                                ordered_float_slot_names: &ordered_float_slot_names,
                            }
                        });
                        fnv_motion::parse_creature_kf_evidence(
                            &bytes,
                            source_kf,
                            family.key.game,
                            None,
                            skeleton_contract,
                        )
                        .unwrap_or_else(|error| {
                            fnv_motion::CreatureKfEvidence {
                                source_game: family.key.game,
                                source_kf: source_kf.clone(),
                                sequence: fnv_motion::KfParseEvidence::ParseFailed(
                                    error.to_string(),
                                ),
                                idle_claims: Vec::new(),
                            }
                        })
                    })
                    .collect();
                fnv_motion::CreatureMotionFamilyEvidence {
                    rig: family.key.clone(),
                    referenced_kfs: family.recursive_kf_paths.clone(),
                    kf_evidence,
                    creature_traits: Vec::new(),
                }
            })
            .collect();
        Self {
            motion_evidence,
            ..Self::default()
        }
    }

    fn converted_clip(&self, rig: &fnv_catalog::RigFamilyKey, source_kf: &str) -> Option<&str> {
        self.converted_clips
            .get(&legacy_clip_key(rig, source_kf))
            .map(String::as_str)
    }
}

fn apply_legacy_race_data_adapter(
    native: &fnv_catalog::CreatureCorpusPlan,
    mesh_root: &Path,
    sources: &[fnv_catalog::LegacyRecordSource<'_>],
    records: &[Record],
    interner: &StringInterner,
    full_merged: bool,
    adapters: &mut LegacyBridgeAdapters,
) -> Result<(), PhaseError> {
    let policy = crate::source_rig::race_data::Fo4RaceDataScalePolicy {
        small_max_stature: 64.0,
        medium_max_stature: 128.0,
        large_max_stature: 256.0,
    };
    let rig_contexts = native
        .rig_families
        .iter()
        .map(|family| fnv_race_data::RigNifMeasurementContext {
            rig: family.key.clone(),
            up_axis: crate::source_rig::race_data::MeasurementAxis::Z,
            geometry_scale_to_fo4: Some(1.0),
        })
        .collect::<Vec<_>>();
    let selections = fnv_race_data::build_family_race_data_selections(native);
    let record_evidence = fnv_race_data::load_creature_record_race_data_evidence(
        native,
        &selections,
        records,
        interner,
    );
    let motion_catalog = fnv_motion::build_creature_motion_catalog(&adapters.motion_evidence)
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
    let graph_selections = legacy_movement_graph_selections(&motion_catalog);
    let base_settings =
        fnv_race_data::load_legacy_movement_base_settings(native, sources, interner);
    let movement = fnv_race_data::build_legacy_movement_mvp_contexts(
        native,
        &selections,
        &graph_selections,
        &base_settings.settings,
        &adapters.motion_evidence,
        records,
        interner,
    );
    let loaded_nifs = fnv_race_data::load_nif_race_data_evidence_with_controllers(
        native,
        mesh_root,
        &rig_contexts,
        &movement.controllers,
    );
    let inputs = fnv_race_data::LegacyRaceDataEvidenceInputs {
        selections: &selections,
        rigs: &loaded_nifs.rigs,
        bodies: &loaded_nifs.bodies,
        controllers: &loaded_nifs.controllers,
        creatures: &record_evidence.creatures,
    };
    let plan = if full_merged {
        fnv_race_data::build_full_merged_race_data_adapter(native, inputs, &policy)
    } else {
        fnv_race_data::build_race_data_adapter(native, inputs, &policy)
    }
    .map_err(|error| PhaseError::Internal(error.to_string()))?;
    if plan.accounting.rig_families != native.rig_families.len()
        || plan.families.len() != native.rig_families.len()
        || plan.accounting.record_dispositions != native.records.len()
    {
        return Err(PhaseError::Internal(
            "FNV/FO3 RACE.DATA adapter accounting drifted from the native corpus".to_string(),
        ));
    }
    for family in plan.families {
        match family.status {
            fnv_race_data::FamilyRaceDataStatus::Ready { derivation, .. } => {
                if let RaceDataMapping::Mapped { target } = derivation.mapping {
                    adapters.race_data.insert(family.rig, target);
                } else {
                    adapters.race_data_rejections.insert(
                        family.rig.clone(),
                        contract_rejection(
                            "source_owned_race_data_incomplete",
                            format!(
                                "ready adapter returned an incomplete mapping for {:?}",
                                family.rig
                            ),
                        ),
                    );
                }
            }
            fnv_race_data::FamilyRaceDataStatus::Missing { fields, issues } => {
                adapters.race_data_rejections.insert(
                    family.rig.clone(),
                    contract_rejection(
                        "source_owned_race_data_missing_evidence",
                        format!("family {:?} is missing {fields:?}: {issues:?}", family.rig),
                    ),
                );
            }
            fnv_race_data::FamilyRaceDataStatus::Invalid { fields, issues } => {
                adapters.race_data_rejections.insert(
                    family.rig.clone(),
                    contract_rejection(
                        "source_owned_race_data_invalid_evidence",
                        format!("family {:?} rejected {fields:?}: {issues:?}", family.rig),
                    ),
                );
            }
        }
    }
    Ok(())
}

fn legacy_movement_graph_selections(
    catalog: &fnv_motion::CreatureMotionCatalog,
) -> Vec<fnv_race_data::LegacyMovementGraphSelection> {
    let mut selections = Vec::new();
    for family in &catalog.families {
        if !matches!(family.readiness, fnv_motion::FamilyMotionReadiness::Ready) {
            continue;
        }
        let has_melee = motion_role(family, fnv_motion::MotionRole::MeleeAttack)
            .is_some_and(|role| !role.candidates.is_empty());
        let has_ranged = [
            fnv_motion::MotionRole::RangedAttack,
            fnv_motion::MotionRole::Fire,
            fnv_motion::MotionRole::Projectile,
        ]
        .into_iter()
        .any(|role| motion_role(family, role).is_some_and(|role| !role.candidates.is_empty()));
        let selection = if let (Some(locomotion), Some(turn)) = (
            ready_motion_candidate(family, fnv_motion::MotionRole::GroundLocomotion, |_| true),
            ready_motion_candidate(family, fnv_motion::MotionRole::Turn, |_| true),
        ) {
            if has_melee == has_ranged {
                None
            } else {
                Some(fnv_race_data::LegacyMovementGraphSelection {
                    rig: family.rig.clone(),
                    graph_template: if has_ranged {
                        CreatureGraphTemplate::GroundRangedProjectile
                    } else {
                        CreatureGraphTemplate::GroundMelee
                    },
                    locomotion_source_kf: Some(locomotion.source_kf.clone()),
                    turn_source_kf: Some(turn.source_kf.clone()),
                    stationary_cycle_source_kf: None,
                    uses_controller_speed: !matches!(
                        locomotion.root_motion,
                        fnv_motion::RootMotionEvidence::Planar { .. }
                    ),
                    aiming_capable: has_ranged,
                    aim_tolerance_degrees: None,
                    orientation_pitch_degrees: None,
                    orientation_roll_degrees: None,
                })
            }
        } else if let Some(stationary) =
            ready_motion_candidate(family, fnv_motion::MotionRole::StationaryOrTurret, |_| true)
        {
            Some(fnv_race_data::LegacyMovementGraphSelection {
                rig: family.rig.clone(),
                graph_template: CreatureGraphTemplate::StationaryTurret,
                locomotion_source_kf: None,
                turn_source_kf: None,
                stationary_cycle_source_kf: Some(stationary.source_kf.clone()),
                uses_controller_speed: false,
                aiming_capable: has_ranged,
                aim_tolerance_degrees: None,
                orientation_pitch_degrees: None,
                orientation_roll_degrees: None,
            })
        } else {
            None
        };
        if let Some(selection) = selection {
            selections.push(selection);
        }
    }
    selections.sort_by(|left, right| left.rig.cmp(&right.rig));
    selections
}

fn legacy_asset_path(mesh_root: &Path, relative: &str) -> PathBuf {
    relative
        .replace('\\', "/")
        .split('/')
        .filter(|component| !component.is_empty())
        .fold(mesh_root.to_path_buf(), |path, component| {
            path.join(component)
        })
}

fn legacy_clip_key(rig: &fnv_catalog::RigFamilyKey, source_kf: &str) -> String {
    format!(
        "{}|{}",
        legacy_family_id(rig),
        source_kf.replace('\\', "/").to_ascii_lowercase()
    )
}

fn build_legacy_motion_bridge(
    adapters: &LegacyBridgeAdapters,
) -> Result<(Vec<MotionSet>, NativeMotionBridgeLedger), PhaseError> {
    let catalog = fnv_motion::build_creature_motion_catalog(&adapters.motion_evidence)
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
    let mut motion_sets = Vec::new();
    let mut entries = Vec::with_capacity(catalog.families.len());
    for family in &catalog.families {
        let has_melee = motion_role(family, fnv_motion::MotionRole::MeleeAttack)
            .is_some_and(|role| !role.candidates.is_empty());
        let has_ranged = [
            fnv_motion::MotionRole::RangedAttack,
            fnv_motion::MotionRole::Fire,
            fnv_motion::MotionRole::Projectile,
        ]
        .into_iter()
        .any(|role| motion_role(family, role).is_some_and(|role| !role.candidates.is_empty()));
        let has_overlay = motion_role(family, fnv_motion::MotionRole::Overlay)
            .is_some_and(|role| !role.candidates.is_empty());
        let emitted = if matches!(family.readiness, fnv_motion::FamilyMotionReadiness::Ready)
            && has_melee
            && !has_ranged
            && !has_overlay
        {
            emit_ground_melee_motion_set(family, adapters)
        } else {
            None
        };
        let disposition = motion_disposition(
            family,
            emitted.is_some(),
            has_melee,
            has_ranged,
            has_overlay,
        );
        if let Some(motion_set) = emitted {
            motion_sets.push(motion_set);
        }
        let mut kfs = family
            .kf_accounting
            .iter()
            .map(|entry| NativeMotionKfEntry {
                source_kf: entry.source_kf.clone(),
                disposition: motion_kf_disposition(&entry.disposition),
            })
            .collect::<Vec<_>>();
        kfs.sort_by(|left, right| {
            left.source_kf
                .to_ascii_lowercase()
                .cmp(&right.source_kf.to_ascii_lowercase())
        });
        entries.push(NativeMotionFamilyEntry {
            family_id: legacy_family_id(&family.rig),
            disposition,
            motion_set_emitted: disposition == NativeMotionDisposition::ReadyGroundMelee,
            has_melee,
            has_ranged,
            has_overlay,
            kfs,
        });
    }
    motion_sets.sort_by(|left, right| left.id.cmp(&right.id));
    entries.sort_by(|left, right| left.family_id.cmp(&right.family_id));
    let ledger = NativeMotionBridgeLedger {
        catalog: "fnv_fo3_creature_motion_catalog".to_string(),
        family_count: entries.len(),
        terminal_family_count: entries.len(),
        referenced_kf_count: entries.iter().map(|entry| entry.kfs.len()).sum(),
        emitted_motion_set_count: entries
            .iter()
            .filter(|entry| entry.motion_set_emitted)
            .count(),
        entries_blake3: motion_entries_hash(&entries)?,
        entries,
    };
    validate_motion_bridge(&ledger)?;
    if ledger.family_count != catalog.accounting.families
        || ledger.referenced_kf_count != catalog.accounting.referenced_kfs
    {
        return Err(PhaseError::Internal(
            "FNV/FO3 motion bridge accounting drifted from the native catalog".to_string(),
        ));
    }
    Ok((motion_sets, ledger))
}

fn motion_role(
    family: &fnv_motion::CreatureFamilyMotionSet,
    role: fnv_motion::MotionRole,
) -> Option<&fnv_motion::MotionRoleCandidates> {
    family.roles.iter().find(|entry| entry.role == role)
}

fn emit_ground_melee_motion_set(
    family: &fnv_motion::CreatureFamilyMotionSet,
    adapters: &LegacyBridgeAdapters,
) -> Option<MotionSet> {
    if family.kf_accounting.iter().any(|entry| {
        adapters
            .converted_clip(&family.rig, &entry.source_kf)
            .is_none()
    }) {
        return None;
    }
    let idle = ready_motion_candidate(family, fnv_motion::MotionRole::Idle, |_| true)?;
    let forward = ready_motion_candidate(
        family,
        fnv_motion::MotionRole::GroundLocomotion,
        |candidate| motion_candidate_contains(candidate, "forward"),
    )?;
    let turn_left = ready_motion_candidate(family, fnv_motion::MotionRole::Turn, |candidate| {
        motion_candidate_contains(candidate, "left")
    })?;
    let turn_right = ready_motion_candidate(family, fnv_motion::MotionRole::Turn, |candidate| {
        motion_candidate_contains(candidate, "right")
    })?;
    let idle_clip = adapters.converted_clip(&family.rig, &idle.source_kf)?;
    let forward_clip = adapters.converted_clip(&family.rig, &forward.source_kf)?;
    let turn_left_clip = adapters.converted_clip(&family.rig, &turn_left.source_kf)?;
    let turn_right_clip = adapters.converted_clip(&family.rig, &turn_right.source_kf)?;
    let attacks = motion_role(family, fnv_motion::MotionRole::MeleeAttack)?;
    if !matches!(attacks.readiness, fnv_motion::RoleReadiness::Ready) {
        return None;
    }
    let mut emitted_attacks = Vec::new();
    for candidate in &attacks.candidates {
        if !candidate
            .events
            .iter()
            .any(|event| event.kind == fnv_motion::MotionEventKind::Hit)
        {
            continue;
        }
        let clip = adapters.converted_clip(&family.rig, &candidate.source_kf)?;
        let digest = blake3::hash(candidate.source_kf.to_ascii_lowercase().as_bytes())
            .to_hex()
            .to_string();
        emitted_attacks.push(MotionAttack {
            id: format!("attack_{}", &digest[..12]),
            event: format!("melee_{}", &digest[..12]),
            clip: clip.to_string(),
        });
    }
    if emitted_attacks.is_empty() {
        return None;
    }
    let attack_kinds = emitted_attacks
        .iter()
        .map(|attack| (attack.id.clone(), MotionAttackKind::MeleeUnarmed))
        .collect();
    Some(MotionSet {
        id: format!("{}-motion", legacy_family_id(&family.rig)),
        graph_template: CreatureGraphTemplate::GroundMelee,
        idle_clip: idle_clip.to_string(),
        locomotion_clips: BTreeMap::from([
            ("turn_left_90".to_string(), turn_left_clip.to_string()),
            ("turn_right_90".to_string(), turn_right_clip.to_string()),
            ("walk_forward".to_string(), forward_clip.to_string()),
        ]),
        attacks: emitted_attacks,
        attack_kinds,
        required_overlays: Vec::new(),
        overlays: Vec::new(),
        required_rigs: Vec::new(),
        rigs: Vec::new(),
    })
}

fn ready_motion_candidate<'a>(
    family: &'a fnv_motion::CreatureFamilyMotionSet,
    role: fnv_motion::MotionRole,
    predicate: impl Fn(&fnv_motion::MotionCandidate) -> bool,
) -> Option<&'a fnv_motion::MotionCandidate> {
    let role = motion_role(family, role)?;
    if !matches!(role.readiness, fnv_motion::RoleReadiness::Ready) {
        return None;
    }
    let mut candidates = role
        .candidates
        .iter()
        .filter(|candidate| predicate(candidate));
    let candidate = candidates.next()?;
    candidates.next().is_none().then_some(candidate)
}

fn motion_candidate_contains(candidate: &fnv_motion::MotionCandidate, token: &str) -> bool {
    candidate.sequence_name.to_ascii_lowercase().contains(token)
        || candidate.source_kf.to_ascii_lowercase().contains(token)
}

fn motion_disposition(
    family: &fnv_motion::CreatureFamilyMotionSet,
    emitted: bool,
    has_melee: bool,
    has_ranged: bool,
    has_overlay: bool,
) -> NativeMotionDisposition {
    if emitted {
        return NativeMotionDisposition::ReadyGroundMelee;
    }
    if has_overlay {
        return NativeMotionDisposition::UnsupportedOverlay;
    }
    match &family.readiness {
        fnv_motion::FamilyMotionReadiness::Incomplete { .. } => NativeMotionDisposition::Incomplete,
        fnv_motion::FamilyMotionReadiness::Ambiguous { .. } => NativeMotionDisposition::Ambiguous,
        fnv_motion::FamilyMotionReadiness::Unclassified { .. } => {
            NativeMotionDisposition::Unclassified
        }
        fnv_motion::FamilyMotionReadiness::EvidenceIncomplete { .. } => {
            NativeMotionDisposition::EvidenceIncomplete
        }
        fnv_motion::FamilyMotionReadiness::Ready if has_melee && has_ranged => {
            NativeMotionDisposition::ReadyMixedCreature
        }
        fnv_motion::FamilyMotionReadiness::Ready if has_ranged => {
            NativeMotionDisposition::ReadyRangedCreature
        }
        fnv_motion::FamilyMotionReadiness::Ready => NativeMotionDisposition::ConversionIncomplete,
    }
}

fn motion_kf_disposition(disposition: &fnv_motion::KfAccountingDisposition) -> String {
    match disposition {
        fnv_motion::KfAccountingDisposition::Classified(roles) => {
            format!("classified:{roles:?}")
        }
        fnv_motion::KfAccountingDisposition::Ambiguous(roles) => {
            format!("ambiguous:{roles:?}")
        }
        fnv_motion::KfAccountingDisposition::Unclassified => "unclassified".to_string(),
        fnv_motion::KfAccountingDisposition::MissingEvidence => "missing_evidence".to_string(),
        fnv_motion::KfAccountingDisposition::MissingAsset => "missing_asset".to_string(),
        fnv_motion::KfAccountingDisposition::ParseFailed(detail) => {
            format!("parse_failed:{detail}")
        }
        fnv_motion::KfAccountingDisposition::BindingUnverified(roles) => {
            format!("binding_unverified:{roles:?}")
        }
        fnv_motion::KfAccountingDisposition::BindingIncompatible(detail) => {
            format!("binding_incompatible:{detail}")
        }
    }
}

fn pending_motion_bridge(
    catalog: &str,
    family_ids: impl IntoIterator<Item = String>,
) -> Result<NativeMotionBridgeLedger, PhaseError> {
    let mut family_ids = family_ids.into_iter().collect::<Vec<_>>();
    family_ids.sort();
    family_ids.dedup();
    let entries = family_ids
        .into_iter()
        .map(|family_id| NativeMotionFamilyEntry {
            family_id,
            disposition: NativeMotionDisposition::AdapterUnavailable,
            motion_set_emitted: false,
            has_melee: false,
            has_ranged: false,
            has_overlay: false,
            kfs: Vec::new(),
        })
        .collect::<Vec<_>>();
    let ledger = NativeMotionBridgeLedger {
        catalog: catalog.to_string(),
        family_count: entries.len(),
        terminal_family_count: entries.len(),
        referenced_kf_count: 0,
        emitted_motion_set_count: 0,
        entries_blake3: motion_entries_hash(&entries)?,
        entries,
    };
    validate_motion_bridge(&ledger)?;
    Ok(ledger)
}

fn build_skyrim_motion_bridge(
    ctx: &PhaseCtx<'_>,
    native: &skyrim_catalog::CreatureCorpusPlan,
) -> Result<
    (
        Vec<MotionSet>,
        NativeMotionBridgeLedger,
        Vec<skyrim_race_data::DecodedCreatureFamilyEvidence>,
        skyrim_motion::SkyrimCreatureMotionCatalog,
    ),
    PhaseError,
> {
    let mesh_root = extracted_mesh_root(ctx.source_extracted_dir);
    let race_evidence = skyrim_motion::race_motion_evidence_from_creature_plan(native);
    let catalog = skyrim_motion::load_extracted_skyrim_creature_motion_catalog(
        &mesh_root.join("actors"),
        &mesh_root.join("animationdata"),
        &race_evidence,
    )
    .map_err(|error| {
        PhaseError::Internal(format!(
            "load exact Skyrim creature motion catalog from {}: {error}",
            mesh_root.display()
        ))
    })?;
    let mut motion_sets = Vec::new();
    let mut entries = Vec::with_capacity(native.families.len());
    for family in &native.families {
        let matches = catalog
            .families
            .iter()
            .filter(|motion_family| {
                let motion_path = canonical_motion_path(&motion_family.project_path);
                family.key.project_paths.iter().any(|project_path| {
                    let project_path = canonical_motion_path(project_path);
                    project_path == motion_path
                        || project_path.ends_with(&format!("/{motion_path}"))
                })
            })
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            entries.push(NativeMotionFamilyEntry {
                family_id: family.family_id.clone(),
                disposition: if matches.is_empty() {
                    NativeMotionDisposition::AdapterUnavailable
                } else {
                    NativeMotionDisposition::Ambiguous
                },
                motion_set_emitted: false,
                has_melee: false,
                has_ranged: false,
                has_overlay: false,
                kfs: Vec::new(),
            });
            continue;
        }
        let motion_family = matches[0];
        let clips = motion_family
            .clip_ids
            .iter()
            .filter_map(|clip_id| catalog.clip(clip_id))
            .collect::<Vec<_>>();
        let has_melee = clips.iter().any(|clip| {
            matches!(
                &clip.disposition,
                skyrim_motion::SkyrimClipDisposition::Role {
                    role: skyrim_motion::SkyrimCreatureMotionRole::MeleeAttack,
                    ..
                }
            )
        });
        let has_ranged = clips.iter().any(|clip| {
            matches!(
                &clip.disposition,
                skyrim_motion::SkyrimClipDisposition::Role {
                    role: skyrim_motion::SkyrimCreatureMotionRole::RangedAttack
                        | skyrim_motion::SkyrimCreatureMotionRole::ProjectileAttack
                        | skyrim_motion::SkyrimCreatureMotionRole::SpellAttack,
                    ..
                }
            )
        });
        let has_overlay = clips.iter().any(|clip| {
            matches!(
                &clip.disposition,
                skyrim_motion::SkyrimClipDisposition::SharedOverlay { .. }
            )
        });
        let has_ambiguous = clips.iter().any(|clip| {
            matches!(
                &clip.disposition,
                skyrim_motion::SkyrimClipDisposition::Ambiguous { .. }
            )
        });
        let has_unsupported = clips.iter().any(|clip| {
            matches!(
                &clip.disposition,
                skyrim_motion::SkyrimClipDisposition::Unsupported { .. }
            )
        });
        let emitted = if has_melee && !has_ranged && !has_overlay {
            catalog
                .motion_set(&motion_family.family_id)
                .and_then(|mut motion| {
                    let graph = catalog.capability_graph_manifest(&motion_family.family_id)?;
                    if graph.template != motion.graph_template {
                        return Err(
                            skyrim_motion::SkyrimCreatureMotionError::AdapterUnavailable {
                                family_id: motion_family.family_id.clone(),
                                target: "MotionSet",
                                detail: "motion and capability graph templates disagree"
                                    .to_string(),
                            },
                        );
                    }
                    motion.id = format!("{}-motion", family.family_id);
                    Ok(motion)
                })
                .ok()
        } else {
            None
        };
        let source_neutral_ready = emitted.is_some();
        let disposition = if has_ambiguous {
            NativeMotionDisposition::Ambiguous
        } else if has_unsupported {
            NativeMotionDisposition::Unclassified
        } else if has_overlay {
            NativeMotionDisposition::UnsupportedOverlay
        } else if has_melee && has_ranged {
            NativeMotionDisposition::ReadyMixedCreature
        } else if has_ranged {
            NativeMotionDisposition::ReadyRangedCreature
        } else if source_neutral_ready && has_melee {
            NativeMotionDisposition::ReadyGroundMelee
        } else {
            NativeMotionDisposition::ConversionIncomplete
        };
        if let Some(motion) = emitted {
            motion_sets.push(motion);
        }
        let mut kfs = clips
            .iter()
            .map(|clip| NativeMotionKfEntry {
                source_kf: clip.clip_path.clone(),
                disposition: format!("{:?}", clip.disposition),
            })
            .collect::<Vec<_>>();
        kfs.sort_by(|left, right| {
            left.source_kf
                .to_ascii_lowercase()
                .cmp(&right.source_kf.to_ascii_lowercase())
        });
        entries.push(NativeMotionFamilyEntry {
            family_id: family.family_id.clone(),
            disposition,
            motion_set_emitted: source_neutral_ready,
            has_melee,
            has_ranged,
            has_overlay,
            kfs,
        });
    }
    let mut race_data_evidence = Vec::with_capacity(catalog.families.len());
    for motion_family in &catalog.families {
        let (Ok(graph), Some(inventory)) = (
            catalog.capability_graph_manifest(&motion_family.family_id),
            catalog.inventory(&motion_family.family_id),
        ) else {
            continue;
        };
        let motion_path = canonical_motion_path(&motion_family.project_path);
        let matching_families = native.families.iter().filter(|family| {
            family.key.project_paths.iter().any(|project_path| {
                let project_path = canonical_motion_path(project_path);
                project_path == motion_path || project_path.ends_with(&format!("/{motion_path}"))
            })
        });
        let mut source_paths = skyrim_race_data::CreatureFamilySourcePaths {
            project_paths: vec![motion_family.project_path.clone()],
            character_paths: inventory.character_paths.clone(),
            skeleton_paths: Vec::new(),
            body_paths: Vec::new(),
        };
        for family in matching_families {
            source_paths
                .skeleton_paths
                .extend(family.key.skeleton_paths.iter().cloned());
            source_paths
                .body_paths
                .extend(family.key.body_models.iter().cloned());
        }
        if !source_paths.project_paths.is_empty() {
            race_data_evidence.push(load_skyrim_family_race_data_evidence(
                &mesh_root,
                &source_paths,
                inventory,
                graph.template,
            ));
        }
    }
    entries.sort_by(|left, right| left.family_id.cmp(&right.family_id));
    motion_sets.sort_by(|left, right| left.id.cmp(&right.id));
    let ledger = NativeMotionBridgeLedger {
        catalog: "skyrim_creature_motion_catalog".to_string(),
        family_count: entries.len(),
        terminal_family_count: entries.len(),
        referenced_kf_count: entries.iter().map(|entry| entry.kfs.len()).sum(),
        emitted_motion_set_count: motion_sets.len(),
        entries_blake3: motion_entries_hash(&entries)?,
        entries,
    };
    validate_motion_bridge(&ledger)?;
    Ok((motion_sets, ledger, race_data_evidence, catalog))
}

fn load_skyrim_family_race_data_evidence(
    mesh_root: &Path,
    source_paths: &skyrim_race_data::CreatureFamilySourcePaths,
    inventory: &skyrim_motion::SkyrimCreatureFamilyInventory,
    template: CreatureGraphTemplate,
) -> skyrim_race_data::DecodedCreatureFamilyEvidence {
    let movement_architecture = match template {
        CreatureGraphTemplate::GroundMelee
        | CreatureGraphTemplate::PassiveGround
        | CreatureGraphTemplate::GroundRangedProjectile
        | CreatureGraphTemplate::GroundMeleeRanged
        | CreatureGraphTemplate::RobotContinuousAttack => {
            crate::source_rig::race_data::MovementArchitecture::Grounded
        }
        CreatureGraphTemplate::GroundSwim => {
            crate::source_rig::race_data::MovementArchitecture::GroundedSwimming
        }
        CreatureGraphTemplate::GroundFly => {
            crate::source_rig::race_data::MovementArchitecture::GroundedFlying
        }
        CreatureGraphTemplate::Swim => crate::source_rig::race_data::MovementArchitecture::Swimming,
        CreatureGraphTemplate::Fly => crate::source_rig::race_data::MovementArchitecture::Flying,
        CreatureGraphTemplate::StationaryTurret => {
            crate::source_rig::race_data::MovementArchitecture::Stationary
        }
    };
    let loaded = skyrim_race_data::load_creature_family_evidence(
        mesh_root,
        source_paths,
        &skyrim_race_data::CreatureFamilyRuntimeSemantics {
            up_axis: crate::source_rig::race_data::MeasurementAxis::Z,
            controller_architecture: skyrim_controller_architecture(inventory),
            geometry_scale_to_fo4: None,
            movement_architecture: Some(movement_architecture),
            max_linear_speed: None,
            max_yaw_speed_degrees_per_second: None,
            pitch_limit_degrees: None,
            roll_limit_degrees: None,
            use_large_actor_pathing: None,
            use_subsegmented_damage: None,
            xp_value: None,
        },
    );
    let airborne = matches!(
        movement_architecture,
        crate::source_rig::race_data::MovementArchitecture::Flying
            | crate::source_rig::race_data::MovementArchitecture::Swimming
    );
    skyrim_race_data::apply_explicit_mvp_defaults(
        loaded.evidence,
        &skyrim_race_data::CreatureRaceDataMvpDefaults {
            policy_id: "skyrimse_fo4_creature_mvp_v1".to_string(),
            geometry_scale_to_fo4: 1.0,
            movement_architecture,
            max_linear_speed: (!matches!(
                movement_architecture,
                crate::source_rig::race_data::MovementArchitecture::Stationary
            ))
            .then_some(96.0),
            max_yaw_speed_degrees_per_second: 90.0,
            pitch_limit_degrees: if airborne { 30.0 } else { 0.0 },
            roll_limit_degrees: if airborne { 20.0 } else { 0.0 },
            use_large_actor_pathing: false,
            use_subsegmented_damage: false,
            xp_value: 0,
        },
    )
    .evidence
}

fn skyrim_controller_architecture(
    inventory: &skyrim_motion::SkyrimCreatureFamilyInventory,
) -> Option<crate::source_rig::race_data::ControllerArchitecture> {
    let architectures =
        inventory
            .controllers
            .iter()
            .filter(|controller| {
                controller.disposition
                    == skyrim_motion::SkyrimControllerEvidenceDisposition::Complete
            })
            .filter_map(|controller| match controller.architecture.as_ref() {
                Some(skyrim_motion::SkyrimControllerArchitectureEvidence::InlineRigidBodySetup)
                | Some(
                    skyrim_motion::SkyrimControllerArchitectureEvidence::CharacterRigidBodyCinfo,
                ) => Some(crate::source_rig::race_data::ControllerArchitecture::Standard),
                Some(skyrim_motion::SkyrimControllerArchitectureEvidence::CharacterProxyCinfo) => {
                    Some(crate::source_rig::race_data::ControllerArchitecture::Proxy)
                }
                Some(skyrim_motion::SkyrimControllerArchitectureEvidence::FixedCinfo) => {
                    Some(crate::source_rig::race_data::ControllerArchitecture::Fixed)
                }
                Some(skyrim_motion::SkyrimControllerArchitectureEvidence::CustomCinfo {
                    ..
                }) => None,
                None if controller.legacy_controller_recipe_evidence().is_some() => {
                    Some(crate::source_rig::race_data::ControllerArchitecture::Standard)
                }
                None => None,
            })
            .collect::<BTreeSet<_>>();
    (architectures.len() == 1)
        .then(|| architectures.into_iter().next())
        .flatten()
}

fn canonical_motion_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase()
}

fn discover_live_corpus(
    ctx: &mut PhaseCtx<'_>,
    debug_dir: &str,
) -> Result<LiveCorpusDiscovery, PhaseError> {
    if ctx.run.target != Game::Fo4 {
        return Err(PhaseError::BadParams(
            "all_creatures_v1 requires Fallout 4 as the target".to_string(),
        ));
    }
    match ctx.run.source {
        Game::SkyrimSe => discover_skyrim_corpus(ctx, debug_dir),
        Game::Fnv | Game::Fo3 => discover_legacy_live_corpus(ctx, debug_dir),
        source => Err(PhaseError::BadParams(format!(
            "all_creatures_v1 does not support {} -> fo4",
            source.as_str()
        ))),
    }
}

fn discover_skyrim_corpus(
    ctx: &mut PhaseCtx<'_>,
    debug_dir: &str,
) -> Result<LiveCorpusDiscovery, PhaseError> {
    let records = read_live_records(
        ctx,
        skyrim_dependencies::required_skyrim_creature_dependency_signatures(),
    )?;
    let native = skyrim_catalog::build_creature_corpus_plan(&records, &ctx.run.interner);
    let expected_candidates = native.summary.candidate_races + native.summary.curated_exclusions;
    if native.races.len() != native.summary.candidate_races
        || native.excluded_races.len() != native.summary.curated_exclusions
    {
        return Err(PhaseError::Internal(
            "Skyrim creature catalog race accounting drifted".to_string(),
        ));
    }
    let (motion_sets, motion_bridge, decoded_family_evidence, motion_catalog) =
        build_skyrim_motion_bridge(ctx, &native)?;
    let motion_by_family = motion_bridge
        .entries
        .iter()
        .map(|entry| (entry.family_id.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let race_data_policy = crate::source_rig::race_data::Fo4RaceDataScalePolicy {
        small_max_stature: 64.0,
        medium_max_stature: 128.0,
        large_max_stature: 256.0,
    };
    let race_data_plan = skyrim_race_data::build_creature_race_data_evidence(
        &native,
        &records,
        &decoded_family_evidence,
        &race_data_policy,
        &ctx.run.interner,
    );
    if race_data_plan.summary.race_count != native.races.len() {
        return Err(PhaseError::Internal(
            "Skyrim RACE.DATA adapter accounting drifted from the creature catalog".to_string(),
        ));
    }
    let race_data_by_race = race_data_plan
        .races
        .iter()
        .map(|entry| (entry.source_race, entry))
        .collect::<HashMap<_, _>>();

    let record_index = records
        .iter()
        .map(|record| (record.form_key, record))
        .collect::<HashMap<_, _>>();
    let race_keys = native
        .races
        .iter()
        .map(|race| race.source_race)
        .collect::<HashSet<_>>();
    let family_by_race = native
        .families
        .iter()
        .flat_map(|family| {
            family
                .races
                .iter()
                .map(move |race| (*race, family.family_id.clone()))
        })
        .collect::<HashMap<_, _>>();

    let mut retained_npcs = HashMap::<FormKey, Vec<FormKey>>::new();
    let mut dependent_entries = Vec::new();
    let mut seen_dependents = HashSet::new();
    for npc in &native.npcs {
        let identity = source_identity("skyrimse", npc.source_npc, &ctx.run.interner)?;
        let resolvable = npc.issues.is_empty()
            && npc.effective_races.len() == 1
            && race_keys.contains(&npc.effective_races[0])
            && npc.family_ids.len() == 1;
        if resolvable {
            retained_npcs
                .entry(npc.effective_races[0])
                .or_default()
                .push(npc.source_npc);
        }
        dependent_entries.push(NativeBridgeEntry {
            source_key: identity.stable_key(),
            subject: if resolvable {
                NativeBridgeSubject::RecordVariant
            } else {
                NativeBridgeSubject::RejectedDependent
            },
            disposition: if resolvable {
                "retained_record_variant".to_string()
            } else {
                "rejected_dependent".to_string()
            },
            reason_codes: npc.issues.iter().map(debug_reason_code).collect::<Vec<_>>(),
        });
        seen_dependents.insert(npc.source_npc);
        for template in &npc.template_records {
            dependent_entries.push(NativeBridgeEntry {
                source_key: stable_form_key_string(*template, &ctx.run.interner)?,
                subject: NativeBridgeSubject::TemplateDependency,
                disposition: if resolvable {
                    "retained_template_dependency".to_string()
                } else {
                    "rejected_template_dependency".to_string()
                },
                reason_codes: npc.issues.iter().map(debug_reason_code).collect::<Vec<_>>(),
            });
        }
    }
    for entry in &native.readiness_ledger {
        let skyrim_catalog::CreaturePlanSubject::Npc(form_key) = &entry.subject else {
            continue;
        };
        if seen_dependents.insert(*form_key) {
            dependent_entries.push(NativeBridgeEntry {
                source_key: stable_form_key_string(*form_key, &ctx.run.interner)?,
                subject: NativeBridgeSubject::RejectedDependent,
                disposition: "rejected_dependent".to_string(),
                reason_codes: entry.issues.iter().map(debug_reason_code).collect(),
            });
        }
    }

    let rig_families = native
        .families
        .iter()
        .map(|family| {
            let targets = family
                .races
                .iter()
                .filter_map(|race| race_data_by_race.get(race))
                .filter_map(|entry| entry.derivation.as_ref())
                .filter_map(|derivation| match &derivation.mapping {
                    RaceDataMapping::Mapped { target } => Some(target),
                    RaceDataMapping::Missing { .. } => None,
                })
                .collect::<Vec<_>>();
            let race_data = if !targets.is_empty()
                && targets.len() == family.races.len()
                && targets.windows(2).all(|pair| pair[0] == pair[1])
            {
                RaceDataMapping::Mapped {
                    target: targets[0].clone(),
                }
            } else {
                missing_race_data()
            };
            RigFamily {
                id: family.family_id.clone(),
                root_node: family
                    .key
                    .skeleton_paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "missing-root".to_string()),
                race_data,
            }
        })
        .collect::<Vec<_>>();
    let mut candidates = Vec::with_capacity(expected_candidates);
    let mut candidate_entries = Vec::with_capacity(expected_candidates);
    for race in &native.races {
        let identity = source_identity("skyrimse", race.source_race, &ctx.run.interner)?;
        let family_id = family_by_race
            .get(&race.source_race)
            .cloned()
            .unwrap_or_else(|| format!("missing-family-{:06x}", race.source_race.local));
        let body_nif = race.body_models.first().cloned().unwrap_or_default();
        let mut npc_keys = retained_npcs.remove(&race.source_race).unwrap_or_default();
        sort_form_keys(&mut npc_keys, &ctx.run.interner)?;
        let primary_record_identity = npc_keys
            .first()
            .copied()
            .map(|form_key| source_identity("skyrimse", form_key, &ctx.run.interner))
            .transpose()?
            .unwrap_or_else(|| identity.clone());
        let record_variants = npc_keys
            .iter()
            .enumerate()
            .map(|(index, form_key)| {
                let variant_identity = source_identity("skyrimse", *form_key, &ctx.run.interner)?;
                let record = record_index.get(form_key).copied();
                Ok(RecordVariant {
                    source_identity: variant_identity,
                    output_slug: record_slug(record, *form_key, &ctx.run.interner),
                    display_name: record_display_name(record, *form_key, &ctx.run.interner),
                    body_nif: body_nif.clone(),
                    level: 1,
                    health: 1,
                    action_points: 0,
                    primary: index == 0,
                    attack_ids: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, PhaseError>>()?;
        let mut preflight_rejections = race
            .issues
            .iter()
            .map(|issue| upstream_rejection("skyrim_creature_catalog", issue))
            .collect::<Vec<_>>();
        if let Some(motion) = motion_by_family.get(&family_id) {
            if !motion.motion_set_emitted {
                preflight_rejections.push(motion_contract_rejection(motion));
            }
        } else {
            preflight_rejections.push(contract_rejection(
                "creature_motion_adapter_unavailable",
                format!("no terminal Skyrim motion entry for {family_id}"),
            ));
        }
        match race_data_by_race.get(&race.source_race) {
            Some(result)
                if result.disposition
                    == skyrim_race_data::CreatureRaceDataDisposition::Complete
                    && matches!(
                        result
                            .derivation
                            .as_ref()
                            .map(|derivation| &derivation.mapping),
                        Some(RaceDataMapping::Mapped { .. })
                    ) => {}
            Some(result) => preflight_rejections.push(contract_rejection(
                match result.disposition {
                    skyrim_race_data::CreatureRaceDataDisposition::Complete => {
                        "source_owned_race_data_unmapped"
                    }
                    skyrim_race_data::CreatureRaceDataDisposition::Missing => {
                        "source_owned_race_data_missing_evidence"
                    }
                    skyrim_race_data::CreatureRaceDataDisposition::Invalid => {
                        "source_owned_race_data_invalid_evidence"
                    }
                },
                format!(
                    "Skyrim RACE.DATA evidence for {} is {:?}; missing={:?}; invalid={:?}",
                    identity.stable_key(),
                    result.disposition,
                    result.missing_fields,
                    result.invalid_fields
                ),
            )),
            None => preflight_rejections.push(contract_rejection(
                "source_owned_race_data_adapter_unavailable",
                format!(
                    "no Skyrim RACE.DATA evidence result for {}",
                    identity.stable_key()
                ),
            )),
        }
        for (code, detail) in [
            (
                "source_rig_conversion_unavailable",
                "no verified Skyrim source-rig conversion receipt exists for this family",
            ),
            (
                "source_body_conversion_unavailable",
                "no verified Skyrim source-body NIF conversion receipt exists for this family",
            ),
            (
                "source_rig_behavior_unavailable",
                "no verified packed Skyrim source-rig behavior receipt exists for this family",
            ),
            (
                "atomic_record_projection_unavailable",
                "no prepared Skyrim atomic record-family projection exists for this family",
            ),
        ] {
            preflight_rejections.push(contract_rejection(code, detail.to_string()));
        }
        candidates.push(CreatureCorpusCandidate {
            source_identity: identity.clone(),
            primary_record_identity,
            output_slug: source_slug(race.editor_id.as_deref(), race.source_race),
            rig_family: family_id.clone(),
            motion_set: format!("{family_id}-motion"),
            record_variants,
            preflight_rejections,
        });
        candidate_entries.push(NativeBridgeEntry {
            source_key: identity.stable_key(),
            subject: NativeBridgeSubject::RaceCandidate,
            disposition: if race.issues.is_empty() {
                "catalog_ready".to_string()
            } else {
                "catalog_rejected".to_string()
            },
            reason_codes: race.issues.iter().map(debug_reason_code).collect(),
        });
    }
    for form_key in &native.excluded_races {
        let identity = source_identity("skyrimse", *form_key, &ctx.run.interner)?;
        candidates.push(CreatureCorpusCandidate {
            source_identity: identity.clone(),
            primary_record_identity: identity.clone(),
            output_slug: source_slug(None, *form_key),
            rig_family: format!("curated-exclusion-{:06x}", form_key.local),
            motion_set: format!("curated-exclusion-{:06x}-motion", form_key.local),
            record_variants: Vec::new(),
            preflight_rejections: vec![CreatureRejectionReason::CuratedExclusion {
                policy: "skyrim_non_creature_race".to_string(),
            }],
        });
        candidate_entries.push(NativeBridgeEntry {
            source_key: identity.stable_key(),
            subject: NativeBridgeSubject::RaceCandidate,
            disposition: "curated_exclusion".to_string(),
            reason_codes: vec!["curated_non_creature_race".to_string()],
        });
    }
    if candidates.len() != expected_candidates {
        return Err(PhaseError::Internal(format!(
            "Skyrim bridge produced {} candidates for {expected_candidates} native race winners",
            candidates.len()
        )));
    }

    let corpus = CreatureCorpusPlan::build(rig_families, motion_sets, candidates)
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
    candidate_entries.extend(dependent_entries);
    let mut bridge = finalize_bridge(
        "skyrim_creature_catalog",
        expected_candidates,
        &corpus,
        candidate_entries,
    )?;
    bridge.motion = motion_bridge;
    validate_native_bridge(&bridge, &corpus)?;
    let dependency = skyrim_dependencies::build_skyrim_creature_dependency_ledger(
        &records,
        &native,
        &ctx.run.interner,
    )
    .map_err(|error| PhaseError::Internal(error.to_string()))?;
    let race_data_policy_receipt =
        skyrim_recipe::build_skyrim_creature_race_data_policy_receipt(race_data_policy)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
    let (jobs, preparation) = prepare_live_skyrim_jobs(
        ctx,
        debug_dir,
        &corpus,
        &native,
        &records,
        &motion_catalog,
        &decoded_family_evidence,
        &race_data_policy_receipt,
        &dependency,
    )?;
    Ok(LiveCorpusDiscovery {
        corpus,
        native_catalog: bridge,
        jobs,
        preparation,
    })
}

fn prepare_live_skyrim_jobs(
    ctx: &mut PhaseCtx<'_>,
    debug_dir: &str,
    corpus: &CreatureCorpusPlan,
    native_catalog: &skyrim_catalog::CreatureCorpusPlan,
    records: &[Record],
    motion: &skyrim_motion::SkyrimCreatureMotionCatalog,
    decoded_race_data: &[skyrim_race_data::DecodedCreatureFamilyEvidence],
    race_data_policy: &skyrim_recipe::SkyrimRaceDataPolicyReceipt,
    dependency: &skyrim_dependencies::SkyrimCreatureDependencyLedger,
) -> Result<(Vec<CreatureCorpusJob>, CreatureLivePreparationLedger), PhaseError> {
    let ancillary_requests = skyrim_live::enumerate_skyrim_creature_ancillary_reservation_requests(
        native_catalog,
        motion,
        &ctx.run.interner,
    )
    .map_err(|error| {
        PhaseError::Internal(format!(
            "enumerate live Skyrim creature ancillary reservations: {error}"
        ))
    })?;
    ctx.run
        .reserve_skyrim_creature_ancillary_records(ancillary_requests)
        .map_err(|error| {
            PhaseError::Internal(format!(
                "reserve live Skyrim creature ancillary records: {error}"
            ))
        })?;
    let actor_action_requests =
        skyrim_live::enumerate_skyrim_creature_actor_action_reservation_requests(
            skyrim_live::SkyrimCreatureLiveRecipeBuildInput {
                winning_records: records,
                motion,
                decoded_race_data,
                race_data_policy,
                dependency,
                source_data_root: ctx.source_extracted_dir,
            },
            "B21_",
            &ctx.run.interner,
        )
        .map_err(|error| {
            PhaseError::Internal(format!(
                "enumerate live Skyrim creature Actor Action reservations: {error}"
            ))
        })?;
    ctx.run
        .reserve_creature_actor_action_records(actor_action_requests)
        .map_err(|error| {
            PhaseError::Internal(format!(
                "reserve live Skyrim creature Actor Action records: {error}"
            ))
        })?;
    let mapper_state = ctx.run.mapper_state.as_ref().ok_or_else(|| {
        PhaseError::Internal(
            "live Skyrim creature preparation requires initialized mapper state".to_string(),
        )
    })?;
    let output_dir = ctx.mod_path.join(path_from_canonical(debug_dir));
    let work_root = output_dir.join(LIVE_PREPARED_WORK_DIR);
    reset_live_preparation_work_root(&work_root)?;
    let result = skyrim_live::build_live_skyrim_creature_mvp_families(
        skyrim_live::SkyrimCreatureLiveMvpPreparationBuildInput {
            winning_records: records,
            motion,
            decoded_race_data,
            race_data_policy,
            dependency,
            source_data_root: ctx.source_extracted_dir,
            private_staging_root: &work_root,
            reservations: ctx.run.skyrim_creature_record_reservations(),
            ancillary_reservations: ctx.run.skyrim_creature_ancillary_reservations(),
            actor_action_reservations: ctx.run.creature_actor_action_reservations(),
            mapper_state,
            editor_id_prefix: "B21_",
        },
        &ctx.run.interner,
    );
    let prepared = match result {
        Ok(prepared) => prepared,
        Err(error) => {
            cleanup_live_preparation_work_root(&work_root)?;
            return Err(PhaseError::Internal(format!(
                "build live Skyrim creature families: {error}"
            )));
        }
    };
    let preparation = skyrim_live_preparation_ledger(&prepared)?;
    if preparation.blocked_candidate_count != 0 || preparation.blocked_family_count != 0 {
        cleanup_live_preparation_work_root(&work_root)?;
        return Ok((Vec::new(), preparation));
    }

    let final_root = output_dir.join(LIVE_PREPARED_DIR);
    let bundles = skyrim_ready_family_bundles(&prepared)?;
    let jobs =
        match materialize_live_family_jobs(corpus, &bundles, ctx.mod_path, &work_root, &final_root)
        {
            Ok(jobs) => jobs,
            Err(error) => {
                cleanup_live_preparation_work_root(&work_root)?;
                return Err(error);
            }
        };
    if let Err(error) = promote_live_preparation_root(&work_root, &final_root) {
        cleanup_live_preparation_work_root(&work_root)?;
        return Err(error);
    }
    Ok((jobs, preparation))
}

fn skyrim_ready_family_bundles(
    prepared: &skyrim_live::SkyrimCreatureLivePreparationLedger,
) -> Result<Vec<LivePreparedFamilyBundle>, PhaseError> {
    prepared
        .families
        .iter()
        .filter_map(|family| {
            let skyrim_live::SkyrimCreatureLivePreparedFamilyDisposition::Ready {
                asset_recipe,
                candidate_recipes,
                closure,
                closure_staged_data_root,
                converted_artifacts,
                ..
            } = &family.disposition
            else {
                return None;
            };
            Some(Ok(LivePreparedFamilyBundle {
                family_id: family.family_id.clone(),
                member_source_keys: family.member_source_keys.clone(),
                asset_recipe: asset_recipe.clone(),
                candidate_recipes: candidate_recipes.clone(),
                closure: closure.clone(),
                closure_staged_data_root: closure_staged_data_root.clone(),
                converted_artifacts: converted_artifacts
                    .iter()
                    .map(|artifact| LivePreparedConvertedArtifact {
                        runtime_path: artifact.runtime_path.clone(),
                        source_path: artifact.source_path.clone(),
                    })
                    .collect(),
            }))
        })
        .collect()
}

fn skyrim_live_preparation_ledger(
    prepared: &skyrim_live::SkyrimCreatureLivePreparationLedger,
) -> Result<CreatureLivePreparationLedger, PhaseError> {
    let terminals = prepared
        .families
        .iter()
        .map(|family| {
            let (disposition, blockers, warnings) = match &family.disposition {
                skyrim_live::SkyrimCreatureLivePreparedFamilyDisposition::Ready {
                    warnings,
                    ..
                } => ("ready".to_string(), Vec::new(), warnings.clone()),
                skyrim_live::SkyrimCreatureLivePreparedFamilyDisposition::Blocked { blockers } => (
                    "blocked".to_string(),
                    blockers
                        .iter()
                        .map(|blocker| CreatureLivePreparationBlocker {
                            code: blocker.code.clone(),
                            detail: blocker.detail.clone(),
                        })
                        .collect(),
                    Vec::new(),
                ),
            };
            Ok(CreatureLiveFamilyTerminal {
                family_id: family.family_id.clone(),
                member_source_keys: family.member_source_keys.clone(),
                disposition,
                blockers,
                warnings: validate_degradation_receipts(warnings, "Skyrim", &family.family_id)?,
            })
        })
        .collect::<Result<Vec<_>, PhaseError>>()?;
    Ok(CreatureLivePreparationLedger {
        version: 1,
        source_game: "skyrimse".to_string(),
        candidate_count: prepared.accounting.candidates,
        family_count: prepared.accounting.families,
        ready_candidate_count: prepared.accounting.ready_candidates,
        accounted_candidate_count: 0,
        blocked_candidate_count: prepared.accounting.blocked_candidates,
        ready_family_count: prepared.accounting.ready_families,
        accounted_family_count: 0,
        blocked_family_count: prepared.accounting.blocked_families,
        terminals,
    })
}

fn blocked_live_preparation(
    source_game: &str,
    corpus: &CreatureCorpusPlan,
    code: &str,
    detail: &str,
) -> CreatureLivePreparationLedger {
    let mut members_by_family = BTreeMap::<String, Vec<String>>::new();
    for planned in &corpus.planned {
        members_by_family
            .entry(planned.rig_family.id.clone())
            .or_default()
            .push(planned.source_key.clone());
    }
    for rejected in &corpus.rejected {
        members_by_family
            .entry(format!("blocked:{}", rejected.source_key))
            .or_default()
            .push(rejected.source_key.clone());
    }
    let mut terminals = Vec::with_capacity(members_by_family.len());
    for (family_id, mut member_source_keys) in members_by_family {
        member_source_keys.sort_by_key(|value| value.to_ascii_lowercase());
        member_source_keys.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        terminals.push(CreatureLiveFamilyTerminal {
            family_id,
            member_source_keys,
            disposition: "blocked".to_string(),
            blockers: vec![CreatureLivePreparationBlocker {
                code: code.to_string(),
                detail: detail.to_string(),
            }],
            warnings: Vec::new(),
        });
    }
    let candidate_count = corpus.planned.len() + corpus.rejected.len();
    CreatureLivePreparationLedger {
        version: 1,
        source_game: source_game.to_string(),
        candidate_count,
        family_count: terminals.len(),
        ready_candidate_count: 0,
        accounted_candidate_count: 0,
        blocked_candidate_count: candidate_count,
        ready_family_count: 0,
        accounted_family_count: 0,
        blocked_family_count: terminals.len(),
        terminals,
    }
}

fn reset_live_preparation_work_root(path: &Path) -> Result<(), PhaseError> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|error| {
            PhaseError::Internal(format!(
                "remove stale live creature preparation root {}: {error}",
                path.display()
            ))
        })?;
    }
    fs::create_dir_all(path).map_err(|error| {
        PhaseError::Internal(format!(
            "create live creature preparation root {}: {error}",
            path.display()
        ))
    })
}

fn cleanup_live_preparation_work_root(path: &Path) -> Result<(), PhaseError> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_dir_all(path).map_err(|error| {
        PhaseError::Internal(format!(
            "roll back live creature preparation root {}: {error}",
            path.display()
        ))
    })
}

fn promote_live_preparation_root(work_root: &Path, final_root: &Path) -> Result<(), PhaseError> {
    let backup_root = final_root.with_extension("previous");
    if backup_root.exists() {
        return Err(PhaseError::Internal(format!(
            "live creature preparation backup already exists: {}",
            backup_root.display()
        )));
    }
    let had_previous = final_root.exists();
    if had_previous {
        fs::rename(final_root, &backup_root).map_err(|error| {
            PhaseError::Internal(format!(
                "stage previous live creature preparation {}: {error}",
                final_root.display()
            ))
        })?;
    }
    if let Err(error) = fs::rename(work_root, final_root) {
        if had_previous {
            let _ = fs::rename(&backup_root, final_root);
        }
        return Err(PhaseError::Internal(format!(
            "publish live creature preparation {}: {error}",
            final_root.display()
        )));
    }
    if had_previous {
        fs::remove_dir_all(&backup_root).map_err(|error| {
            PhaseError::Internal(format!(
                "remove replaced live creature preparation {}: {error}",
                backup_root.display()
            ))
        })?;
    }
    Ok(())
}

fn materialize_live_family_jobs(
    corpus: &CreatureCorpusPlan,
    prepared: &[LivePreparedFamilyBundle],
    mod_root: &Path,
    work_root: &Path,
    final_root: &Path,
) -> Result<Vec<CreatureCorpusJob>, PhaseError> {
    let output_slugs = corpus
        .planned
        .iter()
        .map(|candidate| {
            (
                candidate.source_key.to_ascii_lowercase(),
                candidate.output_slug.clone(),
            )
        })
        .chain(corpus.rejected.iter().map(|candidate| {
            (
                candidate.source_key.to_ascii_lowercase(),
                candidate.output_slug.clone(),
            )
        }))
        .collect::<BTreeMap<_, _>>();
    let mut jobs = Vec::with_capacity(prepared.len());
    for family in prepared {
        let asset_recipe = &family.asset_recipe;
        let candidate_recipes = &family.candidate_recipes;
        let closure = &family.closure;
        let closure_staged_data_root = &family.closure_staged_data_root;
        let converted_artifacts = &family.converted_artifacts;
        let family_dir_name = stable_family_dir(&family.family_id);
        let work_family_dir = work_root.join("recipes").join(&family_dir_name);
        fs::create_dir_all(&work_family_dir)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let final_family_dir = final_root.join("recipes").join(&family_dir_name);

        let asset_recipe_path = work_family_dir.join("asset_recipe.json");
        write_canonical_recipe(&asset_recipe_path, asset_recipe)?;
        let asset_recipe_final = final_family_dir.join("asset_recipe.json");
        let mut planned_candidates = Vec::with_capacity(candidate_recipes.len());
        let mut candidate_source_keys = Vec::with_capacity(candidate_recipes.len());
        for recipe in candidate_recipes {
            let source_key = recipe.projection.source_primary_identity.stable_key();
            let filename = format!(
                "candidate-{}.json",
                &blake3::hash(source_key.as_bytes()).to_hex().to_string()[..16]
            );
            write_canonical_recipe(&work_family_dir.join(&filename), recipe)?;
            planned_candidates.push(PlannedCandidateRecipe {
                source_key: source_key.clone(),
                recipe_source_root: ArtifactSourceRoot::Mod,
                recipe_path: mod_relative_path(mod_root, &final_family_dir.join(&filename))?,
                recipe_blake3: recipe
                    .stable_hash_blake3()
                    .map_err(|error| PhaseError::Internal(error.to_string()))?,
            });
            candidate_source_keys.push(source_key);
        }
        planned_candidates.sort_by(|left, right| {
            left.source_key
                .to_ascii_lowercase()
                .cmp(&right.source_key.to_ascii_lowercase())
        });
        candidate_source_keys.sort_by_key(|value| value.to_ascii_lowercase());
        candidate_source_keys.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        let mut member_source_keys = family.member_source_keys.clone();
        member_source_keys.sort_by_key(|value| value.to_ascii_lowercase());
        member_source_keys.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        if member_source_keys != candidate_source_keys {
            return Err(PhaseError::Internal(format!(
                "live Skyrim family {} candidate recipes do not exactly cover its member ledger",
                family.family_id
            )));
        }

        let closure_path = work_family_dir.join("creature_closure.json");
        let closure_json = closure
            .canonical_json()
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        fs::write(&closure_path, closure_json)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let closure_final = final_family_dir.join("creature_closure.json");

        let final_closure_root = relocate_live_path(
            closure_staged_data_root,
            work_root,
            final_root,
            "closure staged-data root",
        )?;
        let mut planned_artifacts = converted_artifacts
            .iter()
            .map(|artifact| {
                let final_source = relocate_live_path(
                    &artifact.source_path,
                    work_root,
                    final_root,
                    "converted artifact",
                )?;
                let bytes = fs::read(&artifact.source_path).map_err(|error| {
                    PhaseError::Internal(format!(
                        "read live converted artifact {}: {error}",
                        artifact.source_path.display()
                    ))
                })?;
                Ok(PlannedConvertedArtifact {
                    runtime_path: artifact.runtime_path.clone(),
                    source_root: ArtifactSourceRoot::Mod,
                    source_path: mod_relative_path(mod_root, &final_source)?,
                    source_blake3: blake3::hash(&bytes).to_hex().to_string(),
                })
            })
            .collect::<Result<Vec<_>, PhaseError>>()?;
        planned_artifacts.sort_by_key(|artifact| artifact.runtime_path.to_ascii_lowercase());
        let mut graph_paths = vec![
            format!(
                "Meshes/{}",
                asset_recipe.rig.paths.character.replace('\\', "/")
            ),
            format!(
                "Meshes/{}",
                asset_recipe.rig.paths.root_behavior.replace('\\', "/")
            ),
            format!(
                "Meshes/{}",
                asset_recipe.rig.paths.core_behavior.replace('\\', "/")
            ),
        ];
        graph_paths.sort_by_key(|path| path.to_ascii_lowercase());
        graph_paths.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        let source_key = asset_recipe.projection.source_primary_identity.stable_key();
        let output_slug = output_slugs
            .get(&source_key.to_ascii_lowercase())
            .cloned()
            .ok_or_else(|| {
                PhaseError::Internal(format!(
                    "live Skyrim asset recipe source {source_key} is absent from the corpus"
                ))
            })?;
        let publish_root = family_publish_root(asset_recipe);
        let executable_recipe = PlannedExecutableRecipe {
            source_key: source_key.clone(),
            family_id: family.family_id.clone(),
            publish_root: publish_root.clone(),
            recipe_source_root: ArtifactSourceRoot::Mod,
            recipe_path: mod_relative_path(mod_root, &asset_recipe_final)?,
            recipe_blake3: asset_recipe
                .stable_hash_blake3()
                .map_err(|error| PhaseError::Internal(error.to_string()))?,
            creature_closure_receipt_source_root: ArtifactSourceRoot::Mod,
            creature_closure_receipt_path: mod_relative_path(mod_root, &closure_final)?,
            creature_closure_receipt_blake3: closure.receipt_hash.clone(),
            creature_closure_staged_data_source_root: ArtifactSourceRoot::Mod,
            creature_closure_staged_data_root: mod_relative_path(mod_root, &final_closure_root)?,
            graph_paths: graph_paths.clone(),
            converted_artifacts: planned_artifacts,
        };
        let mut job = CreatureCorpusJob {
            job_id: stable_job_id(&source_key, &family.family_id, &output_slug),
            source_key,
            output_slug,
            family_id: family.family_id.clone(),
            adapter: CreatureAdapterProfile::EvidenceBoundRecipe,
            unsupported_capability: None,
            publish_root,
            graph_paths,
            artifacts: Vec::new(),
            executable_recipe: Some(executable_recipe),
            member_source_keys,
            candidate_recipes: planned_candidates,
            family_bundle_blake3: None,
            preflight_issues: Vec::new(),
        };
        job.family_bundle_blake3 = Some(stable_family_bundle_hash(&job));
        jobs.push(job);
    }
    sort_jobs_deterministically(&mut jobs);
    if jobs.len() != prepared.len() {
        return Err(PhaseError::Internal(format!(
            "materialized {} Skyrim family jobs for {} ready families",
            jobs.len(),
            prepared.len()
        )));
    }
    Ok(jobs)
}

fn write_canonical_recipe(
    path: &Path,
    recipe: &crate::source_rig::SourceRigExecutableRecipe,
) -> Result<(), PhaseError> {
    let json = recipe
        .canonical_json()
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
    fs::write(path, json).map_err(|error| PhaseError::Internal(error.to_string()))
}

fn relocate_live_path(
    path: &Path,
    work_root: &Path,
    final_root: &Path,
    label: &str,
) -> Result<PathBuf, PhaseError> {
    let relative = path.strip_prefix(work_root).map_err(|_| {
        PhaseError::Internal(format!(
            "live {label} {} is outside private staging root {}",
            path.display(),
            work_root.display()
        ))
    })?;
    Ok(final_root.join(relative))
}

fn mod_relative_path(mod_root: &Path, path: &Path) -> Result<String, PhaseError> {
    let relative = path.strip_prefix(mod_root).map_err(|_| {
        PhaseError::Internal(format!(
            "live creature path {} is outside mod root {}",
            path.display(),
            mod_root.display()
        ))
    })?;
    canonical_relative(&path_display(relative), "live creature generated path")
}

fn discover_legacy_live_corpus(
    ctx: &mut PhaseCtx<'_>,
    debug_dir: &str,
) -> Result<LiveCorpusDiscovery, PhaseError> {
    let loaded = load_live_legacy_plugin_records(ctx)?;
    let sources = loaded_legacy_record_sources(&loaded);
    let source_load_orders = loaded_legacy_source_load_orders(&loaded);
    let records = loaded
        .iter()
        .flat_map(|plugin| plugin.records.iter().cloned())
        .collect::<Vec<_>>();
    let source_data_root = legacy_namespaced_source_data_root(ctx.source_extracted_dir)?;
    let (corpus, native_catalog) =
        discover_legacy_corpus(ctx, &sources, &records, &source_data_root)?;
    let excluded_signatures = ctx
        .run
        .config
        .skip_record_signatures
        .iter()
        .map(|signature| signature.trim().to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    let dependency =
        fnv_dependencies::build_creature_dependency_ledger_with_load_orders_and_exclusions(
            &sources,
            fnv_dependencies::CreatureDependencyOptions {
                expected_creature_winners: (ctx.run.source == Game::Fnv)
                    .then_some(fnv_catalog::EXPECTED_FULL_MERGED_CREA_WINNERS),
            },
            &source_load_orders,
            &excluded_signatures,
            &ctx.run.interner,
        )
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
    let ancillary_npc_race_records =
        load_legacy_ancillary_npc_race_records(&dependency, &sources, &loaded, &ctx.run.interner)?;
    let ancillary_npc_race_sources =
        loaded_legacy_appearance_race_sources(&ancillary_npc_race_records);
    let target_data_root = ctx
        .target_data_dir
        .or(ctx.target_extracted_dir)
        .ok_or_else(|| {
            PhaseError::Internal(
                "live FNV/FO3 ancillary NPC appearance requires an immutable FO4 Data root"
                    .to_string(),
            )
        })?;
    let ancillary_npc_target_appearance_records =
        load_legacy_ancillary_npc_target_appearance_records(ctx, &ancillary_npc_race_records)?;
    let destinations = dependency
        .candidates
        .iter()
        .map(|candidate| {
            let game = match candidate.provenance.game {
                fnv_catalog::LegacyCreatureGame::Fnv => "fnv",
                fnv_catalog::LegacyCreatureGame::Fo3 => "fo3",
            };
            let output_slug = format!(
                "{}-{}-{:06x}",
                game,
                safe_name(&candidate.source.plugin),
                candidate.source.local
            );
            fnv_mvp::FnvFo3MvpCandidateDestination {
                source: candidate.source.clone(),
                publish_root: PathBuf::from("Meshes")
                    .join("Actors")
                    .join("B21LegacyCreatures")
                    .join(&output_slug),
                output_slug,
            }
        })
        .collect::<Vec<_>>();
    let actor_action_evidence = fnv_live_builder::build_live_fnv_fo3_creature_recipe_evidence(
        fnv_live_builder::FnvFo3LiveRecipeBuildInput {
            winning_records: &sources,
            source_data_root: &source_data_root,
            expected_creature_winners: (ctx.run.source == Game::Fnv)
                .then_some(fnv_catalog::EXPECTED_FULL_MERGED_CREA_WINNERS),
        },
        &ctx.run.interner,
    )
    .map_err(|error| {
        PhaseError::Internal(format!(
            "build live FNV/FO3 Actor Action reservation evidence: {error}"
        ))
    })?;
    let output_dir = ctx.mod_path.join(path_from_canonical(debug_dir));
    write_json(
        &output_dir.join(LIVE_RECIPE_DIAGNOSTICS_FILE),
        &FnvFo3LiveRecipeDiagnostics {
            motion_families: &actor_action_evidence.motion_families,
            motion_sets: &actor_action_evidence.motion_sets,
            movement_issues: &actor_action_evidence.movement_issues,
            graph_contracts: &actor_action_evidence.graph_contracts,
            recipe_ledger: &actor_action_evidence.recipe_ledger,
        },
    )?;
    let actor_action_requests = fnv_live::fnv_fo3_actor_action_reservation_requests(
        &actor_action_evidence,
        &dependency,
        "B21_",
    )
    .map_err(|error| {
        PhaseError::Internal(format!(
            "enumerate live FNV/FO3 creature Actor Action reservations: {error}"
        ))
    })?;
    ctx.run
        .reserve_creature_actor_action_records(actor_action_requests)
        .map_err(|error| {
            PhaseError::Internal(format!(
                "reserve live FNV/FO3 creature Actor Action records: {error}"
            ))
        })?;
    let mapper_state = ctx.run.mapper_state.as_ref().ok_or_else(|| {
        PhaseError::Internal(
            "live FNV/FO3 creature preparation requires initialized mapper state".to_string(),
        )
    })?;
    let work_root = output_dir.join(LIVE_PREPARED_WORK_DIR);
    reset_live_preparation_work_root(&work_root)?;
    let prepared = match fnv_live::build_live_fnv_fo3_mvp_candidate_preparations(
        fnv_live::FnvFo3LiveMvpPreparationBuildInput {
            winning_records: &sources,
            ancillary_npc_race_records: &ancillary_npc_race_sources,
            ancillary_npc_target_appearance_records: &ancillary_npc_target_appearance_records,
            dependency_ledger: &dependency,
            source_data_root: &source_data_root,
            target_data_root,
            private_staging_root: &work_root,
            mapper_state,
            ancillary_npc_reservations: ctx.run.creature_ancillary_npc_reservations(),
            actor_action_reservations: ctx.run.creature_actor_action_reservations(),
            destinations: &destinations,
            expected_creature_winners: (ctx.run.source == Game::Fnv)
                .then_some(fnv_catalog::EXPECTED_FULL_MERGED_CREA_WINNERS),
            editor_id_prefix: "B21_",
        },
        &ctx.run.interner,
    ) {
        Ok(prepared) => prepared,
        Err(error) => {
            cleanup_live_preparation_work_root(&work_root)?;
            return Err(PhaseError::Internal(format!(
                "build live FNV/FO3 creature candidates: {error}"
            )));
        }
    };
    let preparation = fnv_live_preparation_ledger(&prepared)?;
    if preparation.blocked_candidate_count != 0 {
        cleanup_live_preparation_work_root(&work_root)?;
        return Ok(LiveCorpusDiscovery {
            corpus,
            native_catalog,
            jobs: Vec::new(),
            preparation,
        });
    }
    let bundles = fnv_ready_family_bundles(&prepared)?;
    let final_root = output_dir.join(LIVE_PREPARED_DIR);
    let jobs = match materialize_live_family_jobs(
        &corpus,
        &bundles,
        ctx.mod_path,
        &work_root,
        &final_root,
    ) {
        Ok(jobs) => jobs,
        Err(error) => {
            cleanup_live_preparation_work_root(&work_root)?;
            return Err(error);
        }
    };
    if let Err(error) = promote_live_preparation_root(&work_root, &final_root) {
        cleanup_live_preparation_work_root(&work_root)?;
        return Err(error);
    }
    Ok(LiveCorpusDiscovery {
        corpus,
        native_catalog,
        jobs,
        preparation,
    })
}

fn publish_live_ancillary_npc_batch(
    ctx: &mut PhaseCtx<'_>,
    debug_dir: &str,
    prepared: crate::source_rig::PreparedCreatureAncillaryNpcBatch,
) -> Result<(), PhaseError> {
    let (projection_ledger, record_family_batch, staged_data_root) = prepared.into_parts();
    let target_plugin = crate::source_read::plugin_name_for_handle(ctx.run.target_handle_id)
        .map_err(|error| PhaseError::Internal(format!("read output plugin name: {error}")))?;
    if !projection_ledger
        .target_plugin
        .eq_ignore_ascii_case(&target_plugin)
    {
        return Err(PhaseError::Internal(format!(
            "ancillary NPC target plugin {:?} does not match output plugin {target_plugin:?}",
            projection_ledger.target_plugin
        )));
    }

    let mut registrations = Vec::new();
    let mut targets = BTreeSet::new();
    for artifact in projection_ledger
        .canonical_target_artifacts()
        .map_err(|error| PhaseError::Internal(error.to_string()))?
    {
        let relative =
            canonical_relative(&artifact.target_data_path, "ancillary NPC target artifact")?;
        if !targets.insert(relative.to_ascii_lowercase()) {
            return Err(PhaseError::Internal(format!(
                "duplicate ancillary NPC target artifact {relative}"
            )));
        }
        let staged = staged_data_root.join(path_from_runtime(&relative));
        let bytes = fs::read(&staged).map_err(|error| {
            PhaseError::Internal(format!(
                "read staged ancillary NPC artifact {}: {error}",
                staged.display()
            ))
        })?;
        let actual_hash = blake3::hash(&bytes).to_hex().to_string();
        if bytes.len() as u64 != artifact.target_byte_len || actual_hash != artifact.target_blake3 {
            return Err(PhaseError::Internal(format!(
                "staged ancillary NPC artifact {} changed after preparation",
                staged.display()
            )));
        }
        let destination = ctx
            .mod_path
            .join("data")
            .join(path_from_canonical(&relative));
        if destination.exists() {
            return Err(PhaseError::Internal(format!(
                "ancillary NPC target artifact already exists: {}",
                destination.display()
            )));
        }
        registrations.push((relative, staged, destination));
    }
    registrations.sort_by_key(|(relative, _, _)| relative.to_ascii_lowercase());

    let prepared_sink = if registrations.is_empty() {
        None
    } else if let Some(sink) = ctx.run.output_sink.as_deref() {
        let borrowed = registrations
            .iter()
            .map(|(relative, staged, _)| (relative.as_str(), staged.as_path()))
            .collect::<Vec<_>>();
        Some(
            sink.prepare_existing_files_batch(&borrowed)
                .map_err(|message| {
                    PhaseError::Internal(format!("prepare ancillary NPC asset sink: {message}"))
                })?,
        )
    } else {
        None
    };

    let mapper_state = ctx.run.mapper_state.as_mut().ok_or_else(|| {
        PhaseError::Internal(
            "ancillary NPC publication requires initialized mapper state".to_string(),
        )
    })?;
    let mut session = crate::session::open_session(ctx.run.target_handle_id, None)
        .map_err(|error| PhaseError::Internal(format!("open ancillary NPC session: {error}")))?;
    let prepared_records = crate::source_rig::prepare_creature_record_families_batch(
        &mut session,
        mapper_state,
        vec![record_family_batch],
        &ctx.run.schema_target,
        &ctx.run.interner,
    )
    .map_err(|error| PhaseError::Internal(format!("prepare ancillary NPC records: {error}")))?;
    let commit_ledger = crate::source_rig::CreatureAncillaryNpcCommitLedger::new(
        projection_ledger,
        prepared_records.receipt().clone(),
    )
    .map_err(|error| PhaseError::Internal(error.to_string()))?;
    let commit_ledger_json = commit_ledger
        .canonical_json()
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
    let commit_ledger_path = ctx
        .mod_path
        .join(path_from_canonical(debug_dir))
        .join(ANCILLARY_NPC_COMMIT_LEDGER_FILE);
    if commit_ledger_path.exists() {
        return Err(PhaseError::Internal(format!(
            "ancillary NPC commit ledger already exists: {}",
            commit_ledger_path.display()
        )));
    }
    let ledger_parent = commit_ledger_path
        .parent()
        .expect("ancillary NPC ledger path has a parent");
    fs::create_dir_all(ledger_parent).map_err(|error| {
        PhaseError::Internal(format!(
            "create ancillary NPC ledger directory {}: {error}",
            ledger_parent.display()
        ))
    })?;
    let mut staged_ledger = tempfile::NamedTempFile::new_in(ledger_parent).map_err(|error| {
        PhaseError::Internal(format!(
            "stage ancillary NPC commit ledger in {}: {error}",
            ledger_parent.display()
        ))
    })?;
    staged_ledger
        .write_all(commit_ledger_json.as_bytes())
        .and_then(|()| staged_ledger.flush())
        .map_err(|error| {
            PhaseError::Internal(format!("write staged ancillary NPC commit ledger: {error}"))
        })?;

    let mut promoted = Vec::new();
    for (_, staged, destination) in &registrations {
        if let Some(parent) = destination.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            let rollback = rollback_recipe_files(&promoted);
            return Err(PhaseError::Internal(append_rollback_errors(
                format!(
                    "create ancillary NPC output directory {}: {error}",
                    parent.display()
                ),
                rollback,
            )));
        }
        if let Err(error) = fs::rename(staged, destination) {
            let rollback = rollback_recipe_files(&promoted);
            return Err(PhaseError::Internal(append_rollback_errors(
                format!(
                    "promote ancillary NPC artifact {} to {}: {error}",
                    staged.display(),
                    destination.display()
                ),
                rollback,
            )));
        }
        promoted.push((destination.clone(), staged.clone()));
    }
    if let Some(prepared_sink) = prepared_sink
        && let Err(error) = prepared_sink.commit()
    {
        let rollback = rollback_recipe_files(&promoted);
        return Err(PhaseError::Internal(append_rollback_errors(
            format!("commit ancillary NPC asset sink: {error}"),
            rollback,
        )));
    }

    let committed_receipt = prepared_records.commit();
    debug_assert_eq!(&committed_receipt, &commit_ledger.record_receipt);
    staged_ledger
        .persist_noclobber(&commit_ledger_path)
        .map_err(|error| {
            PhaseError::Internal(format!(
                "ancillary NPC records committed but ledger persistence at {} failed: {}",
                commit_ledger_path.display(),
                error.error
            ))
        })?;
    Ok(())
}

fn legacy_namespaced_source_data_root(source_root: &Path) -> Result<PathBuf, PhaseError> {
    let has_both_namespaces = |root: &Path| {
        ["fnv", "falloutnv"]
            .into_iter()
            .any(|name| root.join(name).is_dir())
            && ["fo3", "fallout3"]
                .into_iter()
                .any(|name| root.join(name).is_dir())
    };
    if has_both_namespaces(source_root) {
        return Ok(source_root.to_path_buf());
    }
    if let Some(parent) = source_root.parent()
        && has_both_namespaces(parent)
    {
        return Ok(parent.to_path_buf());
    }
    Err(PhaseError::Internal(format!(
        "merged FNV/FO3 creature assets require distinct fnv/ and fo3/ namespaces under {}; no provenance-safe parent was found",
        source_root.display()
    )))
}

fn fnv_live_preparation_ledger(
    prepared: &fnv_mvp::FnvFo3MvpPreparationLedger,
) -> Result<CreatureLivePreparationLedger, PhaseError> {
    let mut terminals_by_family = BTreeMap::<String, CreatureLiveFamilyTerminal>::new();
    for candidate in &prepared.candidates {
        let source_key = legacy_candidate_source_key(candidate);
        let terminal = terminals_by_family
            .entry(candidate.family_id.clone())
            .or_insert_with(|| CreatureLiveFamilyTerminal {
                family_id: candidate.family_id.clone(),
                member_source_keys: Vec::new(),
                disposition: "ready".to_string(),
                blockers: Vec::new(),
                warnings: Vec::new(),
            });
        terminal.member_source_keys.push(source_key);
        terminal.warnings.extend(
            candidate
                .dependency_records
                .iter()
                .flat_map(|record| record.degradations.iter().cloned()),
        );
        match &candidate.disposition {
            fnv_mvp::FnvFo3MvpCandidateDisposition::Ready { .. } => {}
            fnv_mvp::FnvFo3MvpCandidateDisposition::Accounted { reason } => {
                terminal.disposition = "accounted".to_string();
                terminal.blockers.push(CreatureLivePreparationBlocker {
                    code: "not_applicable_to_asset_synthesis".to_string(),
                    detail: format!("{}: {reason}", candidate.source),
                });
            }
            fnv_mvp::FnvFo3MvpCandidateDisposition::Blocked { reasons } => {
                terminal.disposition = "blocked".to_string();
                terminal.blockers.extend(reasons.iter().map(|reason| {
                    CreatureLivePreparationBlocker {
                        code: fnv_live_blocker_code(reason),
                        detail: format!("{}: {reason:?}", candidate.source),
                    }
                }));
            }
        }
    }
    let mut terminals = terminals_by_family.into_values().collect::<Vec<_>>();
    for terminal in &mut terminals {
        terminal
            .member_source_keys
            .sort_by_key(|value| value.to_ascii_lowercase());
        terminal
            .member_source_keys
            .dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        terminal.blockers.sort();
        terminal.blockers.dedup();
        terminal.warnings = validate_degradation_receipts(
            std::mem::take(&mut terminal.warnings),
            "FNV/FO3",
            &terminal.family_id,
        )?;
    }
    let ready_family_count = terminals
        .iter()
        .filter(|terminal| terminal.disposition == "ready")
        .count();
    let accounted_family_count = terminals
        .iter()
        .filter(|terminal| terminal.disposition == "accounted")
        .count();
    let blocked_family_count = terminals
        .iter()
        .filter(|terminal| terminal.disposition == "blocked")
        .count();
    Ok(CreatureLivePreparationLedger {
        version: 2,
        source_game: "fnv_fo3".to_string(),
        candidate_count: prepared.candidate_count,
        family_count: terminals.len(),
        ready_candidate_count: prepared.ready_count,
        accounted_candidate_count: prepared.accounted_count,
        blocked_candidate_count: prepared.blocked_count,
        ready_family_count,
        accounted_family_count,
        blocked_family_count,
        terminals,
    })
}

fn validate_degradation_receipts(
    mut warnings: Vec<crate::source_rig::CreatureDegradationReceipt>,
    source_label: &str,
    family_id: &str,
) -> Result<Vec<crate::source_rig::CreatureDegradationReceipt>, PhaseError> {
    for warning in &mut warnings {
        warning.canonicalize();
        warning.validate().map_err(|error| {
            PhaseError::Internal(format!(
                "invalid {source_label} creature degradation receipt in family {family_id:?}: {error}"
            ))
        })?;
    }
    warnings.sort();
    warnings.dedup();
    Ok(warnings)
}

fn legacy_candidate_source_key(candidate: &fnv_mvp::FnvFo3MvpCandidatePreparation) -> String {
    SourceCreatureIdentity {
        namespace: match candidate.provenance.game {
            fnv_catalog::LegacyCreatureGame::Fnv => "fnv".to_string(),
            fnv_catalog::LegacyCreatureGame::Fo3 => "fo3".to_string(),
        },
        plugin: candidate.source.plugin.clone(),
        local_form_id: candidate.source.local,
    }
    .stable_key()
}

fn fnv_live_blocker_code(reason: &fnv_mvp::FnvFo3MvpCandidateBlocker) -> String {
    match reason {
        fnv_mvp::FnvFo3MvpCandidateBlocker::Dependency(blocker) => {
            legacy_dependency_blocker_code(blocker)
        }
        fnv_mvp::FnvFo3MvpCandidateBlocker::RecordDisposition { .. } => {
            "record_disposition".to_string()
        }
        fnv_mvp::FnvFo3MvpCandidateBlocker::FamilyDisposition { .. } => {
            "family_disposition".to_string()
        }
        fnv_mvp::FnvFo3MvpCandidateBlocker::MissingCandidatePreparation => {
            "missing_candidate_preparation".to_string()
        }
        fnv_mvp::FnvFo3MvpCandidateBlocker::FamilyPreparation { .. } => {
            "family_preparation".to_string()
        }
        fnv_mvp::FnvFo3MvpCandidateBlocker::MissingPrimaryNpcReservation => {
            "missing_primary_npc_reservation".to_string()
        }
        fnv_mvp::FnvFo3MvpCandidateBlocker::InvalidPrimaryNpcReservation { .. } => {
            "invalid_primary_npc_reservation".to_string()
        }
        fnv_mvp::FnvFo3MvpCandidateBlocker::RecordProjection { .. } => {
            "record_projection".to_string()
        }
        fnv_mvp::FnvFo3MvpCandidateBlocker::AssetPreparation { .. } => {
            "asset_preparation".to_string()
        }
    }
}

fn fnv_ready_family_bundles(
    prepared: &fnv_mvp::FnvFo3MvpPreparationLedger,
) -> Result<Vec<LivePreparedFamilyBundle>, PhaseError> {
    let mut candidates_by_family = BTreeMap::<
        String,
        Vec<(
            String,
            crate::source_rig::SourceRigExecutableRecipe,
            nif_core_native::creature_closure::CreatureClosureReceipt,
            PathBuf,
            Vec<fnv_mvp::FnvFo3MvpConvertedArtifact>,
        )>,
    >::new();
    for candidate in &prepared.candidates {
        let fnv_mvp::FnvFo3MvpCandidateDisposition::Ready {
            recipe,
            closure,
            closure_staged_data_root,
            converted_artifacts,
        } = &candidate.disposition
        else {
            continue;
        };
        candidates_by_family
            .entry(candidate.family_id.clone())
            .or_default()
            .push((
                legacy_candidate_source_key(candidate),
                recipe.clone(),
                closure.clone(),
                closure_staged_data_root.clone(),
                converted_artifacts.clone(),
            ));
    }
    let mut bundles = Vec::with_capacity(candidates_by_family.len());
    for (family_id, mut candidates) in candidates_by_family {
        candidates.sort_by(|left, right| {
            left.0
                .to_ascii_lowercase()
                .cmp(&right.0.to_ascii_lowercase())
        });
        let first = candidates.first().ok_or_else(|| {
            PhaseError::Internal(format!(
                "ready FNV/FO3 family {family_id} has no candidates"
            ))
        })?;
        if candidates
            .iter()
            .any(|candidate| candidate.2 != first.2 || candidate.3 != first.3)
        {
            return Err(PhaseError::Internal(format!(
                "ready FNV/FO3 family {family_id} candidates disagree on their shared asset closure"
            )));
        }
        let mut asset_recipe = first.1.clone();
        if let Some(runtime_model_closure) =
            merge_runtime_model_closures(candidates.iter().map(|candidate| &candidate.1))?
        {
            asset_recipe = asset_recipe
                .with_runtime_model_closure(runtime_model_closure)
                .map_err(|error| PhaseError::Internal(error.to_string()))?;
        }
        let converted_artifacts = merge_live_converted_artifacts(
            candidates.iter().flat_map(|candidate| candidate.4.iter()),
        )?;
        bundles.push(LivePreparedFamilyBundle {
            family_id,
            member_source_keys: candidates
                .iter()
                .map(|candidate| candidate.0.clone())
                .collect(),
            asset_recipe,
            candidate_recipes: candidates
                .iter()
                .map(|candidate| candidate.1.clone())
                .collect(),
            closure: first.2.clone(),
            closure_staged_data_root: first.3.clone(),
            converted_artifacts: converted_artifacts
                .iter()
                .map(|artifact| LivePreparedConvertedArtifact {
                    runtime_path: artifact.runtime_path.clone(),
                    source_path: artifact.source_path.clone(),
                })
                .collect(),
        });
    }
    Ok(bundles)
}

fn merge_runtime_model_closures<'a>(
    recipes: impl Iterator<Item = &'a crate::source_rig::SourceRigExecutableRecipe>,
) -> Result<Option<crate::source_rig::SourceRigRuntimeModelClosureReceipt>, PhaseError> {
    let mut rows = BTreeMap::new();
    for receipt in recipes.filter_map(|recipe| recipe.runtime_model_closure.as_ref()) {
        receipt
            .validate_structure()
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        for row in &receipt.rows {
            match rows.get(&row.key) {
                Some(existing) if existing != row => {
                    return Err(PhaseError::Internal(format!(
                        "runtime-model row {:?} has conflicting family receipts",
                        row.key
                    )));
                }
                Some(_) => {}
                None => {
                    rows.insert(row.key.clone(), row.clone());
                }
            }
        }
    }
    if rows.is_empty() {
        return Ok(None);
    }
    let receipt = crate::source_rig::SourceRigRuntimeModelClosureReceipt {
        version: crate::source_rig::SOURCE_RIG_RUNTIME_MODEL_CLOSURE_VERSION,
        rows: rows.into_values().collect(),
    };
    receipt
        .validate_structure()
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
    Ok(Some(receipt))
}

fn merge_live_converted_artifacts<'a>(
    artifacts: impl Iterator<Item = &'a fnv_mvp::FnvFo3MvpConvertedArtifact>,
) -> Result<Vec<fnv_mvp::FnvFo3MvpConvertedArtifact>, PhaseError> {
    let mut merged = BTreeMap::<String, (fnv_mvp::FnvFo3MvpConvertedArtifact, String)>::new();
    for artifact in artifacts {
        let bytes = fs::read(&artifact.source_path).map_err(|error| {
            PhaseError::Internal(format!(
                "read live converted artifact {}: {error}",
                artifact.source_path.display()
            ))
        })?;
        let hash = blake3::hash(&bytes).to_hex().to_string();
        let key = artifact.runtime_path.to_ascii_lowercase();
        match merged.get(&key) {
            Some((_, existing_hash)) if existing_hash != &hash => {
                return Err(PhaseError::Internal(format!(
                    "converted runtime path {:?} has conflicting family bytes",
                    artifact.runtime_path
                )));
            }
            Some(_) => {}
            None => {
                merged.insert(key, (artifact.clone(), hash));
            }
        }
    }
    Ok(merged.into_values().map(|(artifact, _)| artifact).collect())
}

fn discover_legacy_corpus(
    ctx: &PhaseCtx<'_>,
    sources: &[fnv_catalog::LegacyRecordSource<'_>],
    records: &[Record],
    mesh_root: &Path,
) -> Result<(CreatureCorpusPlan, NativeCatalogBridgeLedger), PhaseError> {
    let native = if ctx.run.source == Game::Fnv {
        fnv_catalog::build_full_merged_creature_corpus_plan(
            sources,
            Some(mesh_root),
            &ctx.run.interner,
        )
    } else {
        fnv_catalog::build_creature_corpus_plan(
            sources,
            fnv_catalog::CreatureCatalogOptions {
                mesh_root: Some(mesh_root),
                expected_creature_winners: None,
            },
            &ctx.run.interner,
        )
    }
    .map_err(|error| PhaseError::Internal(error.to_string()))?;

    if native.records.len() != native.accounting.winning_creatures
        || native.accounting.dispositions != native.accounting.winning_creatures
    {
        return Err(PhaseError::Internal(
            "FNV/FO3 creature catalog accounting drifted".to_string(),
        ));
    }
    let mut adapters = LegacyBridgeAdapters::from_live_assets(&native, mesh_root);
    apply_legacy_race_data_adapter(
        &native,
        mesh_root,
        sources,
        records,
        &ctx.run.interner,
        ctx.run.source == Game::Fnv,
        &mut adapters,
    )?;
    let (motion_sets, motion_bridge) = build_legacy_motion_bridge(&adapters)?;
    if motion_bridge.family_count != native.rig_families.len() {
        return Err(PhaseError::Internal(format!(
            "FNV/FO3 motion bridge produced {} families for {} rig families",
            motion_bridge.family_count,
            native.rig_families.len()
        )));
    }
    let motion_by_family = motion_bridge
        .entries
        .iter()
        .map(|entry| (entry.family_id.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let records_by_key = native
        .records
        .iter()
        .map(|record| (record.source.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let native_families = native
        .rig_families
        .iter()
        .map(|family| (family.key.clone(), family))
        .collect::<BTreeMap<_, _>>();
    let native_bodies = native
        .body_variants
        .iter()
        .map(|body| (body.key.clone(), body))
        .collect::<BTreeMap<_, _>>();
    let rig_families = native
        .rig_families
        .iter()
        .map(|family| RigFamily {
            id: legacy_family_id(&family.key),
            root_node: adapters
                .converted_rig_roots
                .get(&family.key)
                .cloned()
                .unwrap_or_default(),
            race_data: adapters
                .race_data
                .get(&family.key)
                .cloned()
                .map(|target| RaceDataMapping::Mapped { target })
                .unwrap_or_else(missing_race_data),
        })
        .collect::<Vec<_>>();

    let mut candidates = Vec::with_capacity(native.records.len());
    let mut entries = Vec::with_capacity(native.records.len());
    for record in &native.records {
        let namespace = match record.provenance.game {
            fnv_catalog::LegacyCreatureGame::Fnv => "fnv",
            fnv_catalog::LegacyCreatureGame::Fo3 => "fo3",
        };
        let identity = SourceCreatureIdentity {
            namespace: namespace.to_string(),
            plugin: record.source.plugin.clone(),
            local_form_id: record.source.local,
        };
        let (rig_key, body_key, mut reasons) = legacy_visual_contract(record, &records_by_key);
        let family_id = rig_key
            .as_ref()
            .map(legacy_family_id)
            .unwrap_or_else(|| format!("unsupported-{:06x}", record.source.local));
        let body_nif = body_key
            .as_ref()
            .and_then(|body| adapters.converted_body_nifs.get(body))
            .cloned()
            .unwrap_or_default();
        if let Some(rig) = rig_key.as_ref().and_then(|key| native_families.get(key))
            && !matches!(rig.readiness, fnv_catalog::AssetReadiness::Ready)
        {
            reasons.push(upstream_rejection(
                "fnv_fo3_creature_catalog",
                &rig.readiness,
            ));
        }
        if let Some(body) = body_key.as_ref().and_then(|key| native_bodies.get(key))
            && !matches!(body.readiness, fnv_catalog::BodyReadiness::Ready)
        {
            reasons.push(upstream_rejection(
                "fnv_fo3_creature_catalog",
                &body.readiness,
            ));
        }
        if let Some(rig_key) = &rig_key {
            if !adapters.converted_rig_roots.contains_key(rig_key) {
                reasons.push(contract_rejection(
                    "source_rig_conversion_unavailable",
                    format!("no converted source-owned rig closure for {family_id}"),
                ));
            }
            if !adapters.race_data.contains_key(rig_key) {
                reasons.push(
                    adapters
                        .race_data_rejections
                        .get(rig_key)
                        .cloned()
                        .unwrap_or_else(|| {
                            contract_rejection(
                                "source_owned_race_data_unavailable",
                                format!("no source-owned FO4 RACE.DATA derivation for {family_id}"),
                            )
                        }),
                );
            }
            if !adapters.behavior_ready.contains(rig_key) {
                reasons.push(contract_rejection(
                    "source_rig_behavior_unavailable",
                    format!("no packed source-rig behavior closure for {family_id}"),
                ));
            }
            if let Some(motion) = motion_by_family.get(&family_id)
                && !motion.motion_set_emitted
            {
                reasons.push(motion_contract_rejection(motion));
            }
        }
        if body_key
            .as_ref()
            .is_none_or(|body| !adapters.converted_body_nifs.contains_key(body))
        {
            reasons.push(contract_rejection(
                "source_body_conversion_unavailable",
                format!("no converted source-owned body NIF for {family_id}"),
            ));
        }
        let atomic_record = adapters.atomic_records.get(&record.source);
        if atomic_record.is_none() {
            reasons.push(contract_rejection(
                "atomic_record_projection_unavailable",
                format!("no atomic FO4 record projection for {}", record.source),
            ));
        }
        let output_slug = source_slug(
            record.editor_id.as_deref(),
            FormKey {
                local: record.source.local,
                plugin: ctx.run.interner.intern(&record.source.plugin),
            },
        );
        candidates.push(CreatureCorpusCandidate {
            source_identity: identity.clone(),
            primary_record_identity: identity.clone(),
            output_slug: output_slug.clone(),
            rig_family: family_id.clone(),
            motion_set: format!("{family_id}-motion"),
            record_variants: vec![RecordVariant {
                source_identity: identity.clone(),
                output_slug: "base".to_string(),
                display_name: atomic_record
                    .map(|projection| projection.display_name.clone())
                    .unwrap_or_default(),
                body_nif,
                level: atomic_record
                    .map(|projection| projection.level)
                    .unwrap_or_default(),
                health: atomic_record
                    .map(|projection| projection.health)
                    .unwrap_or_default(),
                action_points: atomic_record
                    .map(|projection| projection.action_points)
                    .unwrap_or_default(),
                primary: true,
                attack_ids: motion_sets
                    .iter()
                    .find(|motion| motion.id == format!("{family_id}-motion"))
                    .map(|motion| {
                        motion
                            .attacks
                            .iter()
                            .map(|attack| attack.id.clone())
                            .collect()
                    })
                    .unwrap_or_default(),
            }],
            preflight_rejections: reasons.clone(),
        });
        entries.push(NativeBridgeEntry {
            source_key: identity.stable_key(),
            subject: NativeBridgeSubject::CreatureCandidate,
            disposition: if reasons.is_empty() {
                "catalog_ready".to_string()
            } else {
                "catalog_rejected".to_string()
            },
            reason_codes: reasons.iter().map(source_rejection_code).collect(),
        });
    }
    let corpus = CreatureCorpusPlan::build(rig_families, motion_sets, candidates)
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
    let winner_count = native.accounting.winning_creatures;
    let mut bridge = finalize_bridge("fnv_fo3_creature_catalog", winner_count, &corpus, entries)?;
    bridge.motion = motion_bridge;
    validate_native_bridge(&bridge, &corpus)?;
    Ok((corpus, bridge))
}

fn read_live_records(ctx: &PhaseCtx<'_>, signatures: &[&str]) -> Result<Vec<Record>, PhaseError> {
    read_records_from_handle(
        ctx.run.source_handle_id,
        &ctx.run.schema_source,
        &ctx.run.interner,
        signatures,
    )
}

fn read_records_from_handle(
    handle_id: u64,
    schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
    signatures: &[&str],
) -> Result<Vec<Record>, PhaseError> {
    let mut keys = Vec::new();
    for signature in signatures {
        let signature = SigCode::from_str(signature).map_err(PhaseError::Internal)?;
        keys.extend(
            iter_form_keys_of_sig(handle_id, signature, interner)
                .map_err(|error| PhaseError::Internal(error.to_string()))?,
        );
    }
    sort_form_keys(&mut keys, interner)?;
    keys.dedup();
    let batch = snapshot_records_by_form_keys(handle_id, &keys, interner)
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
    batch
        .records
        .iter()
        .map(|snapshot| {
            decode_record_from_parsed(
                &snapshot.raw_record,
                &snapshot.form_key,
                schema,
                &batch.masters,
                &batch.plugin_name,
                batch.strings.as_ref(),
                batch.plugin_is_localized,
                interner,
            )
            .map_err(|error| PhaseError::Internal(error.to_string()))
        })
        .collect()
}

fn read_exact_records_from_handle(
    handle_id: u64,
    schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
    form_keys: &[FormKey],
) -> Result<Vec<Record>, PhaseError> {
    let batch = snapshot_records_by_form_keys(handle_id, form_keys, interner)
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
    if batch.records.len() != form_keys.len() {
        return Err(PhaseError::Internal(format!(
            "read {} exact records from handle {handle_id}, expected {}",
            batch.records.len(),
            form_keys.len()
        )));
    }
    batch
        .records
        .iter()
        .map(|snapshot| {
            decode_record_from_parsed(
                &snapshot.raw_record,
                &snapshot.form_key,
                schema,
                &batch.masters,
                &batch.plugin_name,
                batch.strings.as_ref(),
                batch.plugin_is_localized,
                interner,
            )
            .map_err(|error| PhaseError::Internal(error.to_string()))
        })
        .collect()
}

fn load_legacy_ancillary_npc_race_records(
    dependency: &fnv_dependencies::FnvFo3CreatureDependencyLedger,
    sources: &[fnv_catalog::LegacyRecordSource<'_>],
    loaded: &[LoadedLegacyPluginRecords],
    interner: &StringInterner,
) -> Result<Vec<LoadedLegacyAppearanceRaceRecord>, PhaseError> {
    let race_winners = legacy_winner_provenance_by_source(sources, "RACE", interner)?;
    let mut requested =
        BTreeMap::<fnv_catalog::StableFormKey, fnv_catalog::CreatureProvenance>::new();
    for npc in dependency
        .records
        .iter()
        .filter(|record| record.signature == "NPC_")
    {
        for reference in npc.references.iter().filter(|reference| {
            reference
                .source_locators
                .iter()
                .any(|value| value == "RNAM[0]")
        }) {
            let signature = match &reference.resolution {
                fnv_dependencies::DependencyReferenceResolution::Followed { signature }
                | fnv_dependencies::DependencyReferenceResolution::OutOfRuntimeScope {
                    signature,
                } => signature,
                resolution => {
                    return Err(PhaseError::Internal(format!(
                        "ancillary NPC {} RNAM target {} has non-record resolution {resolution:?}",
                        npc.source, reference.target
                    )));
                }
            };
            if signature != "RACE" {
                return Err(PhaseError::Internal(format!(
                    "ancillary NPC {} RNAM target {} resolves as {signature}, expected RACE",
                    npc.source, reference.target
                )));
            }
            let provenance = race_winners.get(&reference.target).ok_or_else(|| {
                PhaseError::Internal(format!(
                    "ancillary NPC {} RNAM target {} has no winning source provenance",
                    npc.source, reference.target
                ))
            })?;
            match requested.insert(reference.target.clone(), provenance.clone()) {
                Some(existing) if existing != *provenance => {
                    return Err(PhaseError::Internal(format!(
                        "ancillary RACE {} has conflicting winning provenance",
                        reference.target
                    )));
                }
                Some(_) | None => {}
            }
        }
    }

    let mut records = Vec::with_capacity(requested.len());
    for (source, provenance) in requested {
        let mut matching = loaded.iter().filter(|plugin| {
            plugin.provenance.game == provenance.game
                && plugin
                    .provenance
                    .source_plugin
                    .eq_ignore_ascii_case(&provenance.source_plugin)
        });
        let plugin = matching.next().ok_or_else(|| {
            PhaseError::Internal(format!(
                "ancillary RACE {source} winning plugin {} is not loaded",
                provenance.source_plugin
            ))
        })?;
        if matching.next().is_some() {
            return Err(PhaseError::Internal(format!(
                "ancillary RACE {source} winning plugin {} is loaded more than once",
                provenance.source_plugin
            )));
        }
        let schema_game = match provenance.game {
            fnv_catalog::LegacyCreatureGame::Fnv => "fnv",
            fnv_catalog::LegacyCreatureGame::Fo3 => "fo3",
        };
        let schema = crate::schema::AuthoringSchema::for_game(schema_game)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let form_key = FormKey {
            local: source.local,
            plugin: interner.intern(&source.plugin),
        };
        let mut decoded = read_exact_records_from_handle(
            plugin.handle_id,
            &schema,
            interner,
            std::slice::from_ref(&form_key),
        )?;
        let record = decoded.pop().ok_or_else(|| {
            PhaseError::Internal(format!("ancillary RACE {source} was not decoded"))
        })?;
        if record.sig.as_str() != "RACE" || record.form_key != form_key {
            return Err(PhaseError::Internal(format!(
                "ancillary RACE {source} decoded as {} {}",
                record.sig.as_str(),
                stable_form_key_string(record.form_key, interner)?
            )));
        }
        records.push(LoadedLegacyAppearanceRaceRecord { record, provenance });
    }
    records.sort_by(|left, right| {
        left.provenance
            .game
            .cmp(&right.provenance.game)
            .then_with(|| {
                let left_plugin = interner
                    .resolve(left.record.form_key.plugin)
                    .unwrap_or_default();
                let right_plugin = interner
                    .resolve(right.record.form_key.plugin)
                    .unwrap_or_default();
                left_plugin
                    .to_ascii_lowercase()
                    .cmp(&right_plugin.to_ascii_lowercase())
            })
            .then_with(|| left.record.form_key.local.cmp(&right.record.form_key.local))
    });
    Ok(records)
}

fn legacy_winner_provenance_by_source(
    sources: &[fnv_catalog::LegacyRecordSource<'_>],
    signature: &str,
    interner: &StringInterner,
) -> Result<BTreeMap<fnv_catalog::StableFormKey, fnv_catalog::CreatureProvenance>, PhaseError> {
    let mut winners =
        BTreeMap::<fnv_catalog::StableFormKey, fnv_catalog::CreatureProvenance>::new();
    for source in sources
        .iter()
        .filter(|source| source.record.sig.as_str() == signature)
    {
        let plugin = interner
            .resolve(source.record.form_key.plugin)
            .ok_or_else(|| PhaseError::Internal("unresolved legacy record plugin".to_string()))?;
        let key = fnv_catalog::StableFormKey {
            local: source.record.form_key.local,
            plugin: plugin.to_string(),
        };
        if let Some(existing) = winners.get(&key) {
            match source.provenance.precedence.cmp(&existing.precedence) {
                std::cmp::Ordering::Less => continue,
                std::cmp::Ordering::Equal => {
                    return Err(PhaseError::Internal(format!(
                        "legacy {signature} {key} has duplicate winner precedence {}",
                        source.provenance.precedence
                    )));
                }
                std::cmp::Ordering::Greater => {}
            }
        }
        winners.insert(key, source.provenance.clone());
    }
    Ok(winners)
}

fn read_legacy_dependency_records_from_handle(
    handle_id: u64,
    schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
) -> Result<Vec<Record>, PhaseError> {
    let runtime_signatures =
        fnv_dependencies::required_fnv_fo3_creature_runtime_dependency_signatures();
    let mut records = read_records_from_handle(handle_id, schema, interner, runtime_signatures)?;
    let mut signatures_by_form_key = records
        .iter()
        .map(|record| (record.form_key, record.sig))
        .collect::<HashMap<_, _>>();
    let runtime_signatures = runtime_signatures.iter().copied().collect::<HashSet<_>>();
    for signature in fnv_dependencies::required_fnv_fo3_creature_dependency_signatures() {
        if runtime_signatures.contains(signature) {
            continue;
        }
        let signature = SigCode::from_str(signature).map_err(PhaseError::Internal)?;
        for form_key in iter_form_keys_of_sig(handle_id, signature, interner)
            .map_err(|error| PhaseError::Internal(error.to_string()))?
        {
            match signatures_by_form_key.insert(form_key, signature) {
                Some(existing) if existing != signature => {
                    return Err(PhaseError::Internal(format!(
                        "source FormKey {} is indexed as both {} and {}",
                        stable_form_key_string(form_key, interner)?,
                        existing.as_str(),
                        signature.as_str()
                    )));
                }
                Some(_) => {}
                None => records.push(Record::new(signature, form_key)),
            }
        }
    }
    records.sort_by(|left, right| {
        let left_plugin = interner.resolve(left.form_key.plugin).unwrap_or_default();
        let right_plugin = interner.resolve(right.form_key.plugin).unwrap_or_default();
        left_plugin
            .to_ascii_lowercase()
            .cmp(&right_plugin.to_ascii_lowercase())
            .then_with(|| left.form_key.local.cmp(&right.form_key.local))
            .then_with(|| left.sig.cmp(&right.sig))
    });
    Ok(records)
}

fn legacy_visual_contract<'a>(
    record: &'a fnv_catalog::RecordVariant,
    records: &'a BTreeMap<fnv_catalog::StableFormKey, &fnv_catalog::RecordVariant>,
) -> (
    Option<fnv_catalog::RigFamilyKey>,
    Option<fnv_catalog::BodyVariantKey>,
    Vec<CreatureRejectionReason>,
) {
    match &record.disposition {
        fnv_catalog::CreatureDisposition::VisualOwner { rig, body } => {
            (Some(rig.clone()), Some(body.clone()), Vec::new())
        }
        fnv_catalog::CreatureDisposition::Proxy {
            terminal_visual_owners,
            readiness,
        } => {
            let mut reasons = Vec::new();
            if !matches!(readiness, fnv_catalog::ProxyReadiness::Ready) {
                reasons.push(upstream_rejection("fnv_fo3_creature_catalog", readiness));
            }
            let visual = terminal_visual_owners
                .first()
                .and_then(|owner| records.get(owner))
                .and_then(|owner| match &owner.disposition {
                    fnv_catalog::CreatureDisposition::VisualOwner { rig, body } => {
                        Some((rig.clone(), body.clone()))
                    }
                    _ => None,
                });
            match visual {
                Some((rig, body)) => (Some(rig), Some(body), reasons),
                None => {
                    reasons.push(CreatureRejectionReason::UpstreamCatalogRejected {
                        catalog: "fnv_fo3_creature_catalog".to_string(),
                        reason_code: "proxy_without_visual_owner".to_string(),
                        detail: format!("{:?}", record.disposition),
                    });
                    (None, None, reasons)
                }
            }
        }
        fnv_catalog::CreatureDisposition::Special { reason } => (
            None,
            None,
            vec![upstream_rejection("fnv_fo3_creature_catalog", reason)],
        ),
    }
}

fn finalize_bridge(
    catalog: &str,
    winner_count: usize,
    corpus: &CreatureCorpusPlan,
    mut entries: Vec<NativeBridgeEntry>,
) -> Result<NativeCatalogBridgeLedger, PhaseError> {
    for entry in &mut entries {
        entry.reason_codes.sort();
        entry.reason_codes.dedup();
    }
    entries.sort();
    entries.dedup();
    let candidate_count = entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.subject,
                NativeBridgeSubject::RaceCandidate | NativeBridgeSubject::CreatureCandidate
            )
        })
        .count();
    let dependent_count = entries.len() - candidate_count;
    let rejected_dependent_count = entries
        .iter()
        .filter(|entry| {
            matches!(entry.subject, NativeBridgeSubject::RejectedDependent)
                || entry.disposition.starts_with("rejected_")
        })
        .count();
    if candidate_count != winner_count || candidate_count != corpus.candidate_count {
        return Err(PhaseError::Internal(format!(
            "native bridge accounting mismatch: winners={winner_count}, bridge={candidate_count}, corpus={}",
            corpus.candidate_count
        )));
    }
    let entries_blake3 = bridge_entries_hash(&entries)?;
    let ledger = NativeCatalogBridgeLedger {
        catalog: catalog.to_string(),
        winner_count,
        candidate_count,
        dependent_count,
        rejected_dependent_count,
        entries_blake3,
        entries,
        motion: NativeMotionBridgeLedger::default(),
    };
    validate_native_bridge(&ledger, corpus)?;
    Ok(ledger)
}

#[cfg(test)]
fn fixture_bridge_ledger(
    corpus: &CreatureCorpusPlan,
) -> Result<NativeCatalogBridgeLedger, PhaseError> {
    let entries = corpus
        .planned
        .iter()
        .map(|entry| (&entry.source_key, "planned"))
        .chain(
            corpus
                .rejected
                .iter()
                .map(|entry| (&entry.source_key, "rejected")),
        )
        .map(|(source_key, disposition)| NativeBridgeEntry {
            source_key: source_key.clone(),
            subject: NativeBridgeSubject::CreatureCandidate,
            disposition: disposition.to_string(),
            reason_codes: Vec::new(),
        })
        .collect();
    finalize_bridge("injected_fixture", corpus.candidate_count, corpus, entries)
}

fn validate_native_bridge(
    bridge: &NativeCatalogBridgeLedger,
    corpus: &CreatureCorpusPlan,
) -> Result<(), PhaseError> {
    let candidates = bridge
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.subject,
                NativeBridgeSubject::RaceCandidate | NativeBridgeSubject::CreatureCandidate
            )
        })
        .map(|entry| entry.source_key.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let corpus_candidates = corpus
        .planned
        .iter()
        .map(|entry| entry.source_key.to_ascii_lowercase())
        .chain(
            corpus
                .rejected
                .iter()
                .map(|entry| entry.source_key.to_ascii_lowercase()),
        )
        .collect::<BTreeSet<_>>();
    if bridge.winner_count != bridge.candidate_count
        || bridge.candidate_count != corpus.candidate_count
        || candidates != corpus_candidates
    {
        return Err(PhaseError::BadParams(
            "native creature catalog bridge does not cover the source-rig corpus".to_string(),
        ));
    }
    let dependent_count = bridge.entries.len() - bridge.candidate_count;
    if dependent_count != bridge.dependent_count {
        return Err(PhaseError::BadParams(
            "native creature dependent accounting drifted".to_string(),
        ));
    }
    let expected_hash = bridge_entries_hash(&bridge.entries)?;
    if expected_hash != bridge.entries_blake3 {
        return Err(PhaseError::BadParams(
            "native creature catalog hash mismatch".to_string(),
        ));
    }
    validate_motion_bridge(&bridge.motion)?;
    Ok(())
}

fn bridge_entries_hash(entries: &[NativeBridgeEntry]) -> Result<String, PhaseError> {
    let bytes =
        serde_json::to_vec(entries).map_err(|error| PhaseError::Internal(error.to_string()))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn validate_motion_bridge(bridge: &NativeMotionBridgeLedger) -> Result<(), PhaseError> {
    if bridge.family_count != bridge.entries.len()
        || bridge.terminal_family_count != bridge.family_count
        || bridge.referenced_kf_count
            != bridge
                .entries
                .iter()
                .map(|entry| entry.kfs.len())
                .sum::<usize>()
        || bridge.emitted_motion_set_count
            != bridge
                .entries
                .iter()
                .filter(|entry| entry.motion_set_emitted)
                .count()
    {
        return Err(PhaseError::BadParams(
            "native creature motion accounting drifted".to_string(),
        ));
    }
    if bridge
        .entries
        .windows(2)
        .any(|pair| pair[0].family_id >= pair[1].family_id)
    {
        return Err(PhaseError::BadParams(
            "native creature motion families are not uniquely sorted".to_string(),
        ));
    }
    for entry in &bridge.entries {
        if entry.kfs.windows(2).any(|pair| {
            pair[0].source_kf.to_ascii_lowercase() >= pair[1].source_kf.to_ascii_lowercase()
        }) {
            return Err(PhaseError::BadParams(format!(
                "native creature motion KFs are not uniquely sorted for {}",
                entry.family_id
            )));
        }
    }
    let expected_hash = motion_entries_hash(&bridge.entries)?;
    if bridge.entries_blake3 != expected_hash {
        return Err(PhaseError::BadParams(
            "native creature motion hash mismatch".to_string(),
        ));
    }
    Ok(())
}

fn motion_entries_hash(entries: &[NativeMotionFamilyEntry]) -> Result<String, PhaseError> {
    let bytes =
        serde_json::to_vec(entries).map_err(|error| PhaseError::Internal(error.to_string()))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn source_identity(
    namespace: &str,
    form_key: FormKey,
    interner: &StringInterner,
) -> Result<SourceCreatureIdentity, PhaseError> {
    Ok(SourceCreatureIdentity {
        namespace: namespace.to_string(),
        plugin: interner
            .resolve(form_key.plugin)
            .ok_or_else(|| PhaseError::Internal("unresolved source plugin".to_string()))?
            .to_string(),
        local_form_id: form_key.local,
    })
}

fn stable_form_key_string(
    form_key: FormKey,
    interner: &StringInterner,
) -> Result<String, PhaseError> {
    Ok(format!(
        "{:06x}@{}",
        form_key.local,
        interner
            .resolve(form_key.plugin)
            .ok_or_else(|| PhaseError::Internal("unresolved source plugin".to_string()))?
            .to_ascii_lowercase()
    ))
}

fn sort_form_keys(
    form_keys: &mut Vec<FormKey>,
    interner: &StringInterner,
) -> Result<(), PhaseError> {
    let mut keyed = form_keys
        .drain(..)
        .map(|form_key| Ok((stable_form_key_string(form_key, interner)?, form_key)))
        .collect::<Result<Vec<_>, PhaseError>>()?;
    keyed.sort_by(|left, right| left.0.cmp(&right.0));
    form_keys.extend(keyed.into_iter().map(|(_, form_key)| form_key));
    Ok(())
}

fn missing_race_data() -> RaceDataMapping {
    RaceDataMapping::Missing {
        fields: vec![
            Fo4RaceDataField::Heights,
            Fo4RaceDataField::DefaultWeights,
            Fo4RaceDataField::MovementFlags,
            Fo4RaceDataField::LinearAcceleration,
            Fo4RaceDataField::AngularAcceleration,
            Fo4RaceDataField::Size,
            Fo4RaceDataField::BodyBipedObject,
            Fo4RaceDataField::InjuredHealthPercent,
        ],
    }
}

fn source_slug(editor_id: Option<&str>, form_key: FormKey) -> String {
    let base = editor_id
        .map(slug_component)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "creature".to_string());
    format!("{base}-{:06x}", form_key.local)
}

fn record_slug(record: Option<&Record>, form_key: FormKey, interner: &StringInterner) -> String {
    source_slug(
        record.and_then(|record| record.eid.and_then(|value| interner.resolve(value))),
        form_key,
    )
}

fn record_display_name(
    record: Option<&Record>,
    form_key: FormKey,
    interner: &StringInterner,
) -> String {
    record
        .and_then(|record| {
            record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "FULL")
                .and_then(|field| match field.value {
                    FieldValue::String(value) => interner.resolve(value).map(str::to_string),
                    _ => None,
                })
                .or_else(|| {
                    record
                        .eid
                        .and_then(|value| interner.resolve(value).map(str::to_string))
                })
        })
        .unwrap_or_else(|| format!("Creature {:06X}", form_key.local))
}

fn slug_component(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
        } else if !output.ends_with('-') && !output.is_empty() {
            output.push('-');
        }
    }
    output.trim_matches('-').to_string()
}

fn legacy_family_id(key: &fnv_catalog::RigFamilyKey) -> String {
    let game = match key.game {
        fnv_catalog::LegacyCreatureGame::Fnv => "fnv",
        fnv_catalog::LegacyCreatureGame::Fo3 => "fo3",
    };
    let digest = blake3::hash(format!("{game}|{}", key.skeleton_path).as_bytes())
        .to_hex()
        .to_string();
    format!("{game}-rig-{}", &digest[..12])
}

fn extracted_mesh_root(root: &Path) -> PathBuf {
    for name in ["Meshes", "meshes"] {
        let candidate = root.join(name);
        if candidate.is_dir() {
            return candidate;
        }
    }
    root.to_path_buf()
}

fn upstream_rejection(catalog: &str, issue: &impl std::fmt::Debug) -> CreatureRejectionReason {
    CreatureRejectionReason::UpstreamCatalogRejected {
        catalog: catalog.to_string(),
        reason_code: debug_reason_code(issue),
        detail: format!("{issue:?}"),
    }
}

fn contract_rejection(reason_code: &str, detail: String) -> CreatureRejectionReason {
    CreatureRejectionReason::UpstreamCatalogRejected {
        catalog: "source_rig_contract".to_string(),
        reason_code: reason_code.to_string(),
        detail,
    }
}

fn motion_contract_rejection(entry: &NativeMotionFamilyEntry) -> CreatureRejectionReason {
    let reason_code = match entry.disposition {
        NativeMotionDisposition::ReadyRangedCreature => "creature_ranged_behavior_unavailable",
        NativeMotionDisposition::ReadyMixedCreature => "creature_mixed_behavior_unavailable",
        NativeMotionDisposition::UnsupportedOverlay => {
            "creature_weapon_overlay_behavior_unavailable"
        }
        NativeMotionDisposition::Incomplete => "creature_motion_incomplete",
        NativeMotionDisposition::Ambiguous => "creature_motion_ambiguous",
        NativeMotionDisposition::Unclassified => "creature_motion_unclassified",
        NativeMotionDisposition::EvidenceIncomplete => "creature_motion_evidence_incomplete",
        NativeMotionDisposition::ConversionIncomplete => "creature_motion_conversion_incomplete",
        NativeMotionDisposition::AdapterUnavailable => "creature_motion_adapter_unavailable",
        NativeMotionDisposition::ReadyGroundMelee => "creature_motion_ready",
    };
    contract_rejection(
        reason_code,
        format!(
            "family {} has terminal creature-motion disposition {:?}",
            entry.family_id, entry.disposition
        ),
    )
}

fn debug_reason_code(issue: &impl std::fmt::Debug) -> String {
    let debug = format!("{issue:?}");
    let head = debug.split(['{', '(']).next().unwrap_or("unknown");
    slug_component(head).replace('-', "_")
}

fn source_rejection_code(reason: &CreatureRejectionReason) -> String {
    match reason {
        CreatureRejectionReason::UpstreamCatalogRejected { reason_code, .. } => reason_code.clone(),
        CreatureRejectionReason::CuratedExclusion { .. } => "curated_exclusion".to_string(),
        _ => debug_reason_code(reason),
    }
}

fn declarations_by_slug(
    declarations: Vec<JobDeclaration>,
) -> Result<BTreeMap<String, JobDeclaration>, PhaseError> {
    let mut by_slug = BTreeMap::new();
    for declaration in declarations {
        let key = declaration.output_slug.to_ascii_lowercase();
        if by_slug.insert(key.clone(), declaration).is_some() {
            return Err(PhaseError::BadParams(format!(
                "duplicate creature corpus job declaration for {key}"
            )));
        }
    }
    Ok(by_slug)
}

fn build_jobs(
    corpus: &CreatureCorpusPlan,
    declarations: &BTreeMap<String, JobDeclaration>,
    roots: &SourceRoots<'_>,
) -> Result<Vec<CreatureCorpusJob>, PhaseError> {
    let planned_slugs = corpus
        .planned
        .iter()
        .map(|planned| planned.output_slug.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let rejected_by_slug = corpus
        .rejected
        .iter()
        .map(|rejected| (rejected.output_slug.to_ascii_lowercase(), rejected))
        .collect::<BTreeMap<_, _>>();
    if let Some(extra) = declarations
        .keys()
        .find(|slug| !planned_slugs.contains(*slug) && !rejected_by_slug.contains_key(*slug))
    {
        return Err(PhaseError::BadParams(format!(
            "job declaration {extra} has no catalog creature"
        )));
    }

    let mut jobs = Vec::with_capacity(corpus.planned.len());
    for planned in &corpus.planned {
        let declaration = declarations
            .get(&planned.output_slug.to_ascii_lowercase())
            .ok_or_else(|| {
                PhaseError::BadParams(format!(
                    "planned creature {} has no converted artifact/record closure declaration",
                    planned.output_slug
                ))
            })?;
        let adapter = declaration.adapter.unwrap_or_else(|| {
            if declaration.executable_recipe.is_some() {
                CreatureAdapterProfile::EvidenceBoundRecipe
            } else {
                infer_adapter(&planned.source_identity.namespace, &planned.output_slug)
            }
        });
        if adapter == CreatureAdapterProfile::UnsupportedCapability {
            return Err(PhaseError::BadParams(format!(
                "planned creature {} has no executable source-rig adapter",
                planned.output_slug
            )));
        }
        let publish_root = canonical_relative(&declaration.publish_root, "publish_root")?;
        let unsupported_capability = declaration.unsupported_capability.clone().or_else(|| {
            (adapter == CreatureAdapterProfile::UnsupportedCapability)
                .then(|| "creature_adapter".to_string())
        });

        let mut artifacts = declaration
            .artifacts
            .iter()
            .map(|artifact| plan_artifact(artifact, &publish_root, roots))
            .collect::<Result<Vec<_>, _>>()?;
        artifacts.sort_by_key(artifact_key);

        let executable_recipe = declaration
            .executable_recipe
            .as_ref()
            .map(|declaration| plan_executable_recipe(declaration, roots))
            .transpose()?;
        if executable_recipe.is_some()
            && (!artifacts.is_empty()
                || adapter != CreatureAdapterProfile::EvidenceBoundRecipe
                || unsupported_capability.is_some())
        {
            return Err(PhaseError::BadParams(format!(
                "planned creature {} must use only its evidence-bound recipe closure",
                planned.output_slug
            )));
        }
        if executable_recipe
            .as_ref()
            .is_some_and(|recipe| recipe.publish_root != publish_root)
        {
            return Err(PhaseError::BadParams(format!(
                "planned creature {} publish root differs from its executable recipe",
                planned.output_slug
            )));
        }
        let mut graph_paths = executable_recipe
            .as_ref()
            .map(|recipe| recipe.graph_paths.clone())
            .unwrap_or_else(|| {
                artifacts
                    .iter()
                    .filter(|artifact| {
                        matches!(
                            artifact.kind,
                            ArtifactKind::CharacterHkx
                                | ArtifactKind::RootBehaviorHkx
                                | ArtifactKind::CoreBehaviorHkx
                        )
                    })
                    .map(|artifact| artifact.target_path.clone())
                    .collect()
            });
        graph_paths.sort_by_key(|path| path.to_ascii_lowercase());
        graph_paths.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

        let mut preflight_issues = preflight_artifacts(&artifacts, roots);
        if executable_recipe.is_none() {
            preflight_issues.extend(adapter_contract_issues(adapter, &artifacts));
        }
        if let Some(capability) = unsupported_capability.clone() {
            preflight_issues.push(PreflightIssue::UnsupportedCapability { capability });
        }
        preflight_issues.sort();
        preflight_issues.dedup();
        if (artifacts.is_empty() && executable_recipe.is_none()) || !preflight_issues.is_empty() {
            return Err(PhaseError::BadParams(format!(
                "planned creature {} does not have a complete converted closure: {:?}",
                planned.output_slug, preflight_issues
            )));
        }

        jobs.push(CreatureCorpusJob {
            job_id: stable_job_id(
                &planned.source_key,
                &planned.rig_family.id,
                &planned.output_slug,
            ),
            source_key: planned.source_key.clone(),
            output_slug: planned.output_slug.clone(),
            family_id: planned.rig_family.id.clone(),
            adapter,
            unsupported_capability,
            publish_root,
            graph_paths,
            artifacts,
            executable_recipe,
            member_source_keys: Vec::new(),
            candidate_recipes: Vec::new(),
            family_bundle_blake3: None,
            preflight_issues,
        });
    }
    for (slug, declaration) in declarations {
        if planned_slugs.contains(slug) {
            continue;
        }
        let rejected = rejected_by_slug
            .get(slug)
            .expect("declaration catalog membership checked above");
        if rejected
            .reasons
            .iter()
            .any(|reason| matches!(reason, CreatureRejectionReason::CuratedExclusion { .. }))
        {
            return Err(PhaseError::BadParams(format!(
                "curated creature exclusion {} cannot be promoted",
                rejected.output_slug
            )));
        }
        let recipe_declaration = declaration.executable_recipe.as_ref().ok_or_else(|| {
            PhaseError::BadParams(format!(
                "catalog-rejected creature {} can only be promoted by an evidence-bound executable recipe",
                rejected.output_slug
            ))
        })?;
        if declaration
            .adapter
            .is_some_and(|adapter| adapter != CreatureAdapterProfile::EvidenceBoundRecipe)
            || declaration.unsupported_capability.is_some()
            || !declaration.artifacts.is_empty()
        {
            return Err(PhaseError::BadParams(format!(
                "catalog-rejected creature {} must use only its evidence-bound recipe closure",
                rejected.output_slug
            )));
        }
        let executable_recipe = plan_executable_recipe(recipe_declaration, roots)?;
        if executable_recipe.source_key != rejected.source_key
            || executable_recipe.source_key != rejected.primary_record_identity.stable_key()
        {
            return Err(PhaseError::BadParams(format!(
                "executable recipe source does not match catalog creature {}",
                rejected.output_slug
            )));
        }
        let publish_root = canonical_relative(&declaration.publish_root, "publish_root")?;
        if executable_recipe.publish_root != publish_root {
            return Err(PhaseError::BadParams(format!(
                "catalog-rejected creature {} publish root differs from its executable recipe",
                rejected.output_slug
            )));
        }
        let graph_paths = executable_recipe.graph_paths.clone();
        jobs.push(CreatureCorpusJob {
            job_id: stable_job_id(
                &rejected.source_key,
                &executable_recipe.family_id,
                &rejected.output_slug,
            ),
            source_key: rejected.source_key.clone(),
            output_slug: rejected.output_slug.clone(),
            family_id: executable_recipe.family_id.clone(),
            adapter: CreatureAdapterProfile::EvidenceBoundRecipe,
            unsupported_capability: None,
            publish_root,
            graph_paths,
            artifacts: Vec::new(),
            executable_recipe: Some(executable_recipe),
            member_source_keys: Vec::new(),
            candidate_recipes: Vec::new(),
            family_bundle_blake3: None,
            preflight_issues: Vec::new(),
        });
    }
    sort_jobs_deterministically(&mut jobs);
    Ok(jobs)
}

fn sort_jobs_deterministically(jobs: &mut [CreatureCorpusJob]) {
    jobs.sort_by(|left, right| left.job_id.cmp(&right.job_id));
}

fn plan_artifact(
    declaration: &ArtifactDeclaration,
    publish_root: &str,
    roots: &SourceRoots<'_>,
) -> Result<PlannedArtifact, PhaseError> {
    let source_path = canonical_relative(&declaration.source_path, "artifact source_path")?;
    let target_path = canonical_relative(&declaration.target_path, "artifact target_path")?;
    let skeleton_name = declaration
        .skeleton_name
        .as_ref()
        .map(|name| name.trim().to_string());
    if declaration.source_root == ArtifactSourceRoot::SourceExtracted
        && declaration.kind == ArtifactKind::SkeletonHkx
        && has_extension(&source_path, "nif")
        && skeleton_name.as_deref().is_none_or(str::is_empty)
    {
        return Err(PhaseError::BadParams(format!(
            "source-owned skeleton {source_path} requires an explicit skeleton_name"
        )));
    }
    let mut seen_float_slots = BTreeSet::new();
    if declaration
        .float_slot_names
        .iter()
        .any(|slot| slot.trim().is_empty() || !seen_float_slots.insert(slot.to_ascii_lowercase()))
    {
        return Err(PhaseError::BadParams(format!(
            "source-owned skeleton {source_path} has empty or duplicate float slots"
        )));
    }
    if !declaration.kind.is_record() && !is_descendant(&target_path, publish_root) {
        return Err(PhaseError::BadParams(format!(
            "asset target {target_path} is outside family publish root {publish_root}"
        )));
    }
    validate_artifact_extension(declaration.kind, &target_path)?;
    let record_signature =
        canonical_record_signature(declaration.kind, declaration.record_signature.as_deref())?;
    let mut dependencies = declaration
        .dependencies
        .iter()
        .map(|path| canonical_relative(path, "artifact dependency"))
        .collect::<Result<Vec<_>, _>>()?;
    dependencies.sort_by_key(|value| value.to_ascii_lowercase());
    dependencies.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    let source_blake3 = roots
        .resolve(declaration.source_root, &source_path)
        .and_then(|path| fs::read(path).ok())
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string());
    Ok(PlannedArtifact {
        source_root: declaration.source_root,
        source_path,
        target_path,
        kind: declaration.kind,
        record_signature,
        dependencies,
        root: declaration.root,
        source_blake3,
        skeleton_name,
        float_slot_names: declaration.float_slot_names.clone(),
        sequence_index: declaration.sequence_index,
        event_map: declaration.event_map.clone(),
        target_sample_rate_hz: declaration.target_sample_rate_hz,
    })
}

fn plan_executable_recipe(
    declaration: &ExecutableRecipeDeclaration,
    roots: &SourceRoots<'_>,
) -> Result<PlannedExecutableRecipe, PhaseError> {
    use crate::source_rig::SourceRigExecutableRecipe;

    let recipe_path = canonical_relative(&declaration.recipe_path, "recipe_path")?;
    let closure_receipt_path = canonical_relative(
        &declaration.creature_closure_receipt_path,
        "creature_closure_receipt_path",
    )?;
    let closure_staged_data_root = canonical_relative(
        &declaration.creature_closure_staged_data_root,
        "creature_closure_staged_data_root",
    )?;
    let recipe_file = roots
        .resolve(declaration.recipe_source_root, &recipe_path)
        .ok_or_else(|| PhaseError::BadParams("recipe source root is unavailable".to_string()))?;
    let recipe_json = fs::read_to_string(&recipe_file).map_err(|error| {
        PhaseError::BadParams(format!(
            "read executable recipe {}: {error}",
            recipe_file.display()
        ))
    })?;
    let recipe = SourceRigExecutableRecipe::from_json(&recipe_json)
        .map_err(|error| PhaseError::BadParams(format!("invalid executable recipe: {error}")))?;
    let canonical_recipe = recipe
        .canonical_json()
        .map_err(|error| PhaseError::BadParams(error.to_string()))?;
    if recipe_json != canonical_recipe {
        return Err(PhaseError::BadParams(format!(
            "executable recipe is not canonical JSON: {}",
            recipe_file.display()
        )));
    }
    let recipe_blake3 = recipe
        .stable_hash_blake3()
        .map_err(|error| PhaseError::BadParams(error.to_string()))?;
    let publish_root = family_publish_root(&recipe);
    let mut graph_paths = vec![
        format!("Meshes/{}", recipe.rig.paths.character.replace('\\', "/")),
        format!(
            "Meshes/{}",
            recipe.rig.paths.root_behavior.replace('\\', "/")
        ),
        format!(
            "Meshes/{}",
            recipe.rig.paths.core_behavior.replace('\\', "/")
        ),
    ];
    graph_paths.sort_by_key(|path| path.to_ascii_lowercase());
    graph_paths.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

    let closure_file = roots
        .resolve(
            declaration.creature_closure_receipt_source_root,
            &closure_receipt_path,
        )
        .ok_or_else(|| {
            PhaseError::BadParams("creature closure receipt source root is unavailable".to_string())
        })?;
    let closure_json = fs::read_to_string(&closure_file).map_err(|error| {
        PhaseError::BadParams(format!(
            "read creature closure receipt {}: {error}",
            closure_file.display()
        ))
    })?;
    let closure = nif_core_native::creature_closure::CreatureClosureReceipt::parse_and_validate(
        &closure_json,
    )
    .map_err(|error| PhaseError::BadParams(format!("invalid creature closure receipt: {error}")))?;
    let bridge = recipe.bridge_receipt.as_ref().ok_or_else(|| {
        PhaseError::BadParams("executable recipe has no pair-evidence bridge receipt".to_string())
    })?;
    if bridge.creature_closure_receipt_blake3 != closure.receipt_hash {
        return Err(PhaseError::BadParams(
            "executable recipe and creature closure receipt hashes differ".to_string(),
        ));
    }
    let staged_data_root = roots
        .resolve(
            declaration.creature_closure_staged_data_source_root,
            &closure_staged_data_root,
        )
        .ok_or_else(|| {
            PhaseError::BadParams("creature closure staged-data root is unavailable".to_string())
        })?;
    if !staged_data_root.is_dir() {
        return Err(PhaseError::BadParams(format!(
            "creature closure staged-data root is not a directory: {}",
            staged_data_root.display()
        )));
    }

    let expected = bridge
        .artifact_receipts
        .iter()
        .map(|receipt| (receipt.runtime_path.to_ascii_lowercase(), receipt))
        .collect::<BTreeMap<_, _>>();
    let mut converted_artifacts = Vec::with_capacity(declaration.converted_artifacts.len());
    let mut seen = BTreeSet::new();
    for artifact in &declaration.converted_artifacts {
        canonical_relative(&artifact.runtime_path, "converted artifact runtime_path")?;
        let key = artifact
            .runtime_path
            .replace('/', "\\")
            .to_ascii_lowercase();
        if !seen.insert(key.clone()) {
            return Err(PhaseError::BadParams(format!(
                "duplicate converted artifact runtime path {}",
                artifact.runtime_path
            )));
        }
        let receipt = expected.get(&key).ok_or_else(|| {
            PhaseError::BadParams(format!(
                "converted artifact {} is not required by the executable recipe",
                artifact.runtime_path
            ))
        })?;
        let source_path = canonical_relative(&artifact.source_path, "converted artifact source")?;
        let source = roots
            .resolve(artifact.source_root, &source_path)
            .ok_or_else(|| {
                PhaseError::BadParams(format!("source root unavailable for {source_path}"))
            })?;
        let bytes = fs::read(&source).map_err(|error| {
            PhaseError::BadParams(format!(
                "read converted artifact {}: {error}",
                source.display()
            ))
        })?;
        let source_blake3 = blake3::hash(&bytes).to_hex().to_string();
        if receipt.byte_len != bytes.len() as u64 || receipt.blake3 != source_blake3 {
            return Err(PhaseError::BadParams(format!(
                "converted artifact {} differs from its recipe receipt",
                artifact.runtime_path
            )));
        }
        converted_artifacts.push(PlannedConvertedArtifact {
            runtime_path: receipt.runtime_path.clone(),
            source_root: artifact.source_root,
            source_path,
            source_blake3,
        });
    }
    if seen.len() != expected.len() {
        return Err(PhaseError::BadParams(
            "converted artifact declarations do not exactly close the executable recipe"
                .to_string(),
        ));
    }
    converted_artifacts.sort_by_key(|artifact| artifact.runtime_path.to_ascii_lowercase());

    Ok(PlannedExecutableRecipe {
        source_key: recipe.projection.source_primary_identity.stable_key(),
        family_id: recipe.batch_intent.family_id.clone(),
        publish_root,
        recipe_source_root: declaration.recipe_source_root,
        recipe_path,
        recipe_blake3,
        creature_closure_receipt_source_root: declaration.creature_closure_receipt_source_root,
        creature_closure_receipt_path: closure_receipt_path,
        creature_closure_receipt_blake3: closure.receipt_hash,
        creature_closure_staged_data_source_root: declaration
            .creature_closure_staged_data_source_root,
        creature_closure_staged_data_root: closure_staged_data_root,
        graph_paths,
        converted_artifacts,
    })
}

fn preflight_artifacts(
    artifacts: &[PlannedArtifact],
    roots: &SourceRoots<'_>,
) -> Vec<PreflightIssue> {
    let mut issues = Vec::new();
    for artifact in artifacts {
        let Some(path) = roots.resolve(artifact.source_root, &artifact.source_path) else {
            issues.push(PreflightIssue::MissingSourceRoot {
                root: artifact.source_root,
            });
            continue;
        };
        if !path.exists() {
            issues.push(PreflightIssue::MissingSourceArtifact {
                path: artifact.source_path.clone(),
            });
        } else if !path.is_file() {
            issues.push(PreflightIssue::SourceArtifactNotFile {
                path: artifact.source_path.clone(),
            });
        } else if let Err(error) = fs::read(&path) {
            issues.push(PreflightIssue::SourceArtifactRead {
                path: artifact.source_path.clone(),
                message: error.to_string(),
            });
        }
    }
    issues
}

fn adapter_contract_issues(
    adapter: CreatureAdapterProfile,
    artifacts: &[PlannedArtifact],
) -> Vec<PreflightIssue> {
    let mut issues = artifacts
        .iter()
        .filter(|artifact| {
            artifact.source_root != ArtifactSourceRoot::Mod
                && !is_source_rig_nif_conversion(artifact)
                && !is_source_rig_skeleton_conversion(artifact)
                && !is_fnv_kf_conversion(artifact)
                && !artifact.kind.is_record()
                && (artifact.kind.is_hkx()
                    || artifact.kind.is_nif()
                    || matches!(
                        artifact.kind,
                        ArtifactKind::Material | ArtifactKind::Texture | ArtifactKind::Asset
                    )
                    || has_extension(&artifact.source_path, "hkx")
                    || has_extension(&artifact.source_path, "nif")
                    || has_extension(&artifact.source_path, "kf")
                    || has_extension(&artifact.target_path, "hkx")
                    || has_extension(&artifact.target_path, "nif"))
        })
        .map(|artifact| PreflightIssue::UnconvertedSourceArtifact {
            path: artifact.source_path.clone(),
            kind: artifact.kind,
        })
        .collect::<Vec<_>>();
    if matches!(
        adapter,
        CreatureAdapterProfile::UnsupportedCapability | CreatureAdapterProfile::EvidenceBoundRecipe
    ) {
        return issues;
    }
    for kind in [
        ArtifactKind::ProjectHkx,
        ArtifactKind::CharacterHkx,
        ArtifactKind::RootBehaviorHkx,
        ArtifactKind::CoreBehaviorHkx,
        ArtifactKind::SkeletonHkx,
        ArtifactKind::IdleHkx,
        ArtifactKind::LocomotionHkx,
        ArtifactKind::MeleeHkx,
        ArtifactKind::SkeletonNif,
        ArtifactKind::VisualNif,
        ArtifactKind::Material,
        ArtifactKind::Texture,
    ] {
        if !artifacts.iter().any(|artifact| artifact.kind == kind) {
            issues.push(PreflightIssue::MissingAdapterArtifact { kind });
        }
    }
    for signature in ["RACE", "NPC_", "ARMO", "ARMA", "BPTD", "WEAP"] {
        if !artifacts.iter().any(|artifact| {
            artifact.kind == ArtifactKind::Record
                && artifact.record_signature.as_deref() == Some(signature)
        }) {
            issues.push(PreflightIssue::MissingRecordSignature {
                signature: signature.to_string(),
            });
        }
    }

    match adapter {
        CreatureAdapterProfile::EvidenceBoundRecipe => {}
        CreatureAdapterProfile::SkyrimWolf => {
            let (rig, _, _) = canonical_skyrim_wolf_runtime_contract();
            let expected = [
                rig.paths.project,
                rig.paths.character,
                rig.paths.root_behavior,
                rig.paths.core_behavior,
                rig.animation_skeleton.path,
                rig.visual_skeleton_nif,
            ]
            .into_iter()
            .chain(rig.clips.into_iter().map(|clip| clip.path))
            .map(|path| canonical_mesh_target(&path));
            for path in expected {
                if !artifacts
                    .iter()
                    .any(|artifact| artifact.target_path.eq_ignore_ascii_case(&path))
                {
                    issues.push(PreflightIssue::CanonicalAdapterPathMissing { path });
                }
            }
        }
        CreatureAdapterProfile::FnvGecko => {
            let evidence = canonical_gecko_asset_evidence();
            let clip_count = artifacts
                .iter()
                .filter(|artifact| {
                    matches!(
                        artifact.kind,
                        ArtifactKind::IdleHkx
                            | ArtifactKind::LocomotionHkx
                            | ArtifactKind::MeleeHkx
                    )
                })
                .count();
            if clip_count < evidence.clips.len() {
                issues.push(PreflightIssue::MissingAdapterArtifact {
                    kind: ArtifactKind::LocomotionHkx,
                });
            }
        }
        CreatureAdapterProfile::UnsupportedCapability => {}
    }
    issues
}

fn infer_adapter(namespace: &str, output_slug: &str) -> CreatureAdapterProfile {
    let namespace = namespace.to_ascii_lowercase();
    let slug = output_slug.to_ascii_lowercase();
    if namespace.contains("skyrim") && slug.contains("wolf") {
        CreatureAdapterProfile::SkyrimWolf
    } else if namespace.contains("fnv") && slug.contains("gecko") {
        CreatureAdapterProfile::FnvGecko
    } else {
        CreatureAdapterProfile::UnsupportedCapability
    }
}

fn effective_bundle_members(job: &CreatureCorpusJob) -> Vec<String> {
    if job.member_source_keys.is_empty() {
        vec![job.source_key.clone()]
    } else {
        job.member_source_keys.clone()
    }
}

fn effective_candidate_recipes(job: &CreatureCorpusJob) -> Vec<PlannedCandidateRecipe> {
    if job.candidate_recipes.is_empty() {
        job.executable_recipe
            .as_ref()
            .map(PlannedCandidateRecipe::from_asset_recipe)
            .into_iter()
            .collect()
    } else {
        job.candidate_recipes.clone()
    }
}

fn stable_family_bundle_hash(job: &CreatureCorpusJob) -> String {
    fn update(hasher: &mut blake3::Hasher, value: &str) {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }

    let mut hasher = blake3::Hasher::new();
    update(&mut hasher, &job.family_id);
    update(&mut hasher, &job.publish_root);
    let asset = job
        .executable_recipe
        .as_ref()
        .expect("family bundle has an asset recipe");
    update(&mut hasher, &asset.source_key);
    update(&mut hasher, &asset.recipe_path);
    update(&mut hasher, &asset.recipe_blake3);
    update(&mut hasher, &asset.creature_closure_receipt_blake3);
    update(&mut hasher, &asset.creature_closure_staged_data_root);
    for artifact in &asset.converted_artifacts {
        update(&mut hasher, &artifact.runtime_path);
        update(&mut hasher, &artifact.source_path);
        update(&mut hasher, &artifact.source_blake3);
    }
    for source in effective_bundle_members(job) {
        update(&mut hasher, &source);
    }
    for recipe in effective_candidate_recipes(job) {
        update(&mut hasher, &recipe.source_key);
        update(&mut hasher, &recipe.recipe_path);
        update(&mut hasher, &recipe.recipe_blake3);
    }
    hasher.finalize().to_hex().to_string()
}

fn validate_execution_plan(plan: &CreatureCorpusExecutionPlan) -> Result<(), PhaseError> {
    if plan.version != PLAN_VERSION {
        return Err(PhaseError::BadParams(format!(
            "unsupported creature corpus phase plan version {}",
            plan.version
        )));
    }
    plan.corpus
        .validate()
        .map_err(|error| PhaseError::BadParams(error.to_string()))?;
    let debug_dir = canonical_debug_dir(&plan.debug_dir)?;
    if debug_dir != plan.debug_dir {
        return Err(PhaseError::BadParams(
            "creature corpus debug_dir is not canonical".to_string(),
        ));
    }
    let executable_jobs = plan
        .jobs
        .iter()
        .filter(|job| job.executable_recipe.is_some())
        .count();
    if executable_jobs != 0 && executable_jobs != plan.jobs.len() {
        return Err(PhaseError::BadParams(
            "creature corpus plan cannot mix legacy and evidence-bound jobs".to_string(),
        ));
    }
    if executable_jobs != 0 {
        let mut families = BTreeSet::new();
        if plan
            .jobs
            .iter()
            .any(|job| !families.insert(job.family_id.to_ascii_lowercase()))
        {
            return Err(PhaseError::BadParams(
                "evidence-bound plan requires exactly one recipe job per family".to_string(),
            ));
        }
    }
    let family_bundle_plan = executable_jobs != 0
        && plan.jobs.iter().any(|job| {
            !job.member_source_keys.is_empty()
                || !job.candidate_recipes.is_empty()
                || job.family_bundle_blake3.is_some()
        });

    let planned_by_source = plan
        .corpus
        .planned
        .iter()
        .map(|planned| (planned.source_key.to_ascii_lowercase(), planned))
        .collect::<BTreeMap<_, _>>();
    let rejected_by_source = plan
        .corpus
        .rejected
        .iter()
        .map(|rejected| (rejected.source_key.to_ascii_lowercase(), rejected))
        .collect::<BTreeMap<_, _>>();
    let mut job_ids = BTreeSet::new();
    let mut job_sources = BTreeSet::new();
    let mut candidate_recipe_sources = BTreeSet::new();
    let mut member_sources = BTreeSet::new();
    let mut family_roots = BTreeMap::<String, String>::new();
    let mut family_targets = BTreeMap::<String, BTreeSet<String>>::new();
    for job in &plan.jobs {
        if !job_ids.insert(job.job_id.to_ascii_lowercase()) {
            return Err(PhaseError::BadParams(format!(
                "duplicate creature corpus job id {}",
                job.job_id
            )));
        }
        if !job_sources.insert(job.source_key.to_ascii_lowercase()) {
            return Err(PhaseError::BadParams(format!(
                "duplicate creature corpus job source {}",
                job.source_key
            )));
        }
        let source_key = job.source_key.to_ascii_lowercase();
        if let Some(planned) = planned_by_source.get(&source_key) {
            if planned.output_slug != job.output_slug
                || (!family_bundle_plan && planned.rig_family.id != job.family_id)
            {
                return Err(PhaseError::BadParams(format!(
                    "job {} identity does not match corpus plan",
                    job.job_id
                )));
            }
        } else if let Some(rejected) = rejected_by_source.get(&source_key) {
            if rejected
                .reasons
                .iter()
                .any(|reason| matches!(reason, CreatureRejectionReason::CuratedExclusion { .. }))
            {
                return Err(PhaseError::BadParams(format!(
                    "job {} attempts to promote a curated creature exclusion",
                    job.job_id
                )));
            }
            let executable = job.executable_recipe.as_ref().ok_or_else(|| {
                PhaseError::BadParams(format!(
                    "catalog-rejected source {} cannot be promoted without an evidence-bound recipe",
                    job.source_key
                ))
            })?;
            if rejected.output_slug != job.output_slug
                || executable.source_key != rejected.source_key
                || (!family_bundle_plan
                    && executable.source_key != rejected.primary_record_identity.stable_key())
                || executable.family_id != job.family_id
            {
                return Err(PhaseError::BadParams(format!(
                    "job {} evidence-bound identity does not match rejected catalog source",
                    job.job_id
                )));
            }
        } else {
            return Err(PhaseError::BadParams(format!(
                "job {} references unknown corpus source {}",
                job.job_id, job.source_key
            )));
        }
        validate_job(job)?;
        if family_bundle_plan {
            let local_members = effective_bundle_members(job)
                .into_iter()
                .map(|source| source.to_ascii_lowercase())
                .collect::<BTreeSet<_>>();
            for source in &local_members {
                let known = planned_by_source.contains_key(source)
                    || rejected_by_source.get(source).is_some_and(|rejected| {
                        !rejected.reasons.iter().any(|reason| {
                            matches!(reason, CreatureRejectionReason::CuratedExclusion { .. })
                        })
                    });
                if !known {
                    return Err(PhaseError::BadParams(format!(
                        "family {} has unknown or curated member source {}",
                        job.family_id, source
                    )));
                }
                member_sources.insert(source.clone());
            }
            for recipe in effective_candidate_recipes(job) {
                let source = recipe.source_key.to_ascii_lowercase();
                if !local_members.contains(&source) {
                    return Err(PhaseError::BadParams(format!(
                        "family {} owns candidate recipe {} outside its member set",
                        job.family_id, recipe.source_key
                    )));
                }
                if !candidate_recipe_sources.insert(source.clone()) {
                    return Err(PhaseError::BadParams(format!(
                        "candidate recipe {} is owned by more than one family bundle",
                        recipe.source_key
                    )));
                }
            }
        }
        if (job.artifacts.is_empty() && job.executable_recipe.is_none())
            || !job.preflight_issues.is_empty()
        {
            return Err(PhaseError::BadParams(format!(
                "planned job {} does not have a complete executable closure",
                job.job_id
            )));
        }
        let required_adapter_issues = adapter_contract_issues(job.adapter, &job.artifacts);
        if let Some(missing) = required_adapter_issues
            .iter()
            .find(|issue| !job.preflight_issues.contains(issue))
        {
            return Err(PhaseError::BadParams(format!(
                "job {} omits adapter preflight issue {missing:?}",
                job.job_id
            )));
        }
        match family_roots.get(&job.family_id) {
            Some(root) if root != &job.publish_root => {
                return Err(PhaseError::BadParams(format!(
                    "family {} has conflicting publish roots {} and {}",
                    job.family_id, root, job.publish_root
                )));
            }
            None => {
                family_roots.insert(job.family_id.clone(), job.publish_root.clone());
            }
            _ => {}
        }
        let targets = family_targets.entry(job.family_id.clone()).or_default();
        for artifact in &job.artifacts {
            if !targets.insert(artifact.target_path.to_ascii_lowercase()) {
                return Err(PhaseError::BadParams(format!(
                    "family {} has duplicate artifact target {}",
                    job.family_id, artifact.target_path
                )));
            }
        }
    }
    let covered_sources = if family_bundle_plan {
        &candidate_recipe_sources
    } else {
        &job_sources
    };
    if let Some(missing) = plan
        .corpus
        .planned
        .iter()
        .find(|planned| !covered_sources.contains(&planned.source_key.to_ascii_lowercase()))
    {
        return Err(PhaseError::BadParams(format!(
            "planned creature {} has no executable job",
            missing.output_slug
        )));
    }
    if plan
        .jobs
        .windows(2)
        .any(|pair| pair[0].job_id > pair[1].job_id)
    {
        return Err(PhaseError::BadParams(
            "creature corpus jobs are not in deterministic order".to_string(),
        ));
    }
    if family_bundle_plan {
        if let Some(unowned) = member_sources
            .iter()
            .find(|source| !candidate_recipe_sources.contains(*source))
        {
            return Err(PhaseError::BadParams(format!(
                "family membership {unowned} has no distinct candidate recipe owner"
            )));
        }
        if let Some(unattached) = candidate_recipe_sources
            .iter()
            .find(|source| !member_sources.contains(*source))
        {
            return Err(PhaseError::BadParams(format!(
                "candidate recipe {unattached} has no family membership"
            )));
        }
    }
    let family_roots = family_roots.into_iter().collect::<Vec<_>>();
    for (index, (left_family, left_root)) in family_roots.iter().enumerate() {
        for (right_family, right_root) in family_roots.iter().skip(index + 1) {
            if left_root.eq_ignore_ascii_case(right_root)
                || is_descendant(left_root, right_root)
                || is_descendant(right_root, left_root)
            {
                return Err(PhaseError::BadParams(format!(
                    "family publish roots overlap: {left_family}={left_root}, {right_family}={right_root}"
                )));
            }
        }
    }
    Ok(())
}

fn validate_job(job: &CreatureCorpusJob) -> Result<(), PhaseError> {
    if stable_job_id(&job.source_key, &job.family_id, &job.output_slug) != job.job_id {
        return Err(PhaseError::BadParams(format!(
            "job {} does not have its canonical stable id",
            job.job_id
        )));
    }
    if canonical_relative(&job.publish_root, "publish_root")? != job.publish_root {
        return Err(PhaseError::BadParams(format!(
            "job {} publish_root is not canonical",
            job.job_id
        )));
    }
    if job.adapter == CreatureAdapterProfile::UnsupportedCapability
        && job.unsupported_capability.is_none()
    {
        return Err(PhaseError::BadParams(format!(
            "unsupported job {} has no typed capability",
            job.job_id
        )));
    }
    if job.adapter == CreatureAdapterProfile::EvidenceBoundRecipe && job.executable_recipe.is_none()
    {
        return Err(PhaseError::BadParams(format!(
            "evidence-bound job {} has no executable recipe",
            job.job_id
        )));
    }
    let has_bundle_fields = !job.member_source_keys.is_empty()
        || !job.candidate_recipes.is_empty()
        || job.family_bundle_blake3.is_some();
    if has_bundle_fields {
        if job.executable_recipe.is_none()
            || job.member_source_keys.is_empty()
            || job.candidate_recipes.is_empty()
        {
            return Err(PhaseError::BadParams(format!(
                "family bundle {} requires one asset recipe, members, and candidate recipes",
                job.job_id
            )));
        }
        if job
            .member_source_keys
            .iter()
            .any(|source| source.trim().is_empty())
            || job
                .member_source_keys
                .windows(2)
                .any(|pair| pair[0].to_ascii_lowercase() >= pair[1].to_ascii_lowercase())
            || job.candidate_recipes.windows(2).any(|pair| {
                pair[0].source_key.to_ascii_lowercase() >= pair[1].source_key.to_ascii_lowercase()
            })
        {
            return Err(PhaseError::BadParams(format!(
                "family bundle {} members or candidate recipes are not strictly ordered",
                job.job_id
            )));
        }
        let member_sources = job
            .member_source_keys
            .iter()
            .map(|source| source.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        for recipe in &job.candidate_recipes {
            canonical_relative(&recipe.recipe_path, "candidate recipe_path")?;
            if recipe.source_key.trim().is_empty()
                || !member_sources.contains(&recipe.source_key.to_ascii_lowercase())
                || !is_blake3_hex(&recipe.recipe_blake3)
            {
                return Err(PhaseError::BadParams(format!(
                    "family bundle {} has an invalid candidate recipe commitment",
                    job.job_id
                )));
            }
        }
        if job
            .family_bundle_blake3
            .as_deref()
            .is_none_or(|hash| hash != stable_family_bundle_hash(job))
        {
            return Err(PhaseError::BadParams(format!(
                "family bundle {} has a missing or stale stable hash",
                job.job_id
            )));
        }
    }
    if let Some(executable) = &job.executable_recipe {
        if !job.artifacts.is_empty()
            || executable.source_key != job.source_key
            || executable.family_id != job.family_id
            || executable.publish_root != job.publish_root
        {
            return Err(PhaseError::BadParams(format!(
                "job {} mixes or mismatches its evidence-bound recipe identity",
                job.job_id
            )));
        }
        if job.graph_paths.is_empty()
            || job.graph_paths.iter().any(|path| {
                canonical_relative(path, "graph_path").is_err() || !has_extension(path, "hkx")
            })
            || job
                .graph_paths
                .windows(2)
                .any(|pair| pair[0].to_ascii_lowercase() >= pair[1].to_ascii_lowercase())
        {
            return Err(PhaseError::BadParams(format!(
                "job {} has invalid, empty, or noncanonical graph paths",
                job.job_id
            )));
        }
        if job.adapter != CreatureAdapterProfile::EvidenceBoundRecipe {
            return Err(PhaseError::BadParams(format!(
                "job {} declares an executable recipe under a legacy adapter",
                job.job_id
            )));
        }
        canonical_relative(&executable.recipe_path, "recipe_path")?;
        canonical_relative(
            &executable.creature_closure_receipt_path,
            "creature_closure_receipt_path",
        )?;
        canonical_relative(
            &executable.creature_closure_staged_data_root,
            "creature_closure_staged_data_root",
        )?;
        if !is_blake3_hex(&executable.recipe_blake3)
            || !is_blake3_hex(&executable.creature_closure_receipt_blake3)
            || executable.converted_artifacts.is_empty()
        {
            return Err(PhaseError::BadParams(format!(
                "job {} has an incomplete executable recipe commitment",
                job.job_id
            )));
        }
        if executable.graph_paths != job.graph_paths {
            return Err(PhaseError::BadParams(format!(
                "job {} graph paths differ from its executable recipe",
                job.job_id
            )));
        }
        let mut runtime_paths = BTreeSet::new();
        for artifact in &executable.converted_artifacts {
            canonical_relative(&artifact.runtime_path, "converted artifact runtime_path")?;
            canonical_relative(&artifact.source_path, "converted artifact source_path")?;
            if !is_blake3_hex(&artifact.source_blake3)
                || !runtime_paths.insert(artifact.runtime_path.to_ascii_lowercase())
            {
                return Err(PhaseError::BadParams(format!(
                    "job {} has an invalid converted artifact commitment",
                    job.job_id
                )));
            }
        }
    }

    let mut targets = BTreeSet::new();
    let mut roots = BTreeSet::new();
    for artifact in &job.artifacts {
        if canonical_relative(&artifact.source_path, "artifact source_path")?
            != artifact.source_path
            || canonical_relative(&artifact.target_path, "artifact target_path")?
                != artifact.target_path
        {
            return Err(PhaseError::BadParams(format!(
                "job {} has a non-canonical artifact path",
                job.job_id
            )));
        }
        if !artifact.kind.is_record() && !is_descendant(&artifact.target_path, &job.publish_root) {
            return Err(PhaseError::BadParams(format!(
                "job {} asset {} escapes publish root {}",
                job.job_id, artifact.target_path, job.publish_root
            )));
        }
        validate_artifact_extension(artifact.kind, &artifact.target_path)?;
        canonical_record_signature(artifact.kind, artifact.record_signature.as_deref())?;
        if is_source_rig_skeleton_conversion(artifact)
            && artifact
                .skeleton_name
                .as_deref()
                .is_none_or(|name| name.trim().is_empty())
        {
            return Err(PhaseError::BadParams(format!(
                "job {} source-owned skeleton {} has no skeleton_name",
                job.job_id, artifact.source_path
            )));
        }
        let mut float_slots = BTreeSet::new();
        if artifact
            .float_slot_names
            .iter()
            .any(|slot| slot.trim().is_empty() || !float_slots.insert(slot.to_ascii_lowercase()))
        {
            return Err(PhaseError::BadParams(format!(
                "job {} source-owned skeleton {} has invalid float slots",
                job.job_id, artifact.source_path
            )));
        }
        if is_fnv_kf_conversion(artifact) {
            if artifact.sequence_index.is_none() {
                return Err(PhaseError::BadParams(format!(
                    "job {} source KF {} has no selected sequence_index",
                    job.job_id, artifact.source_path
                )));
            }
            if artifact
                .target_sample_rate_hz
                .is_some_and(|rate| !rate.is_finite() || rate <= 0.0)
            {
                return Err(PhaseError::BadParams(format!(
                    "job {} source KF {} has an invalid target sample rate",
                    job.job_id, artifact.source_path
                )));
            }
            if artifact
                .event_map
                .iter()
                .any(|(source, target)| source.trim().is_empty() || target.trim().is_empty())
            {
                return Err(PhaseError::BadParams(format!(
                    "job {} source KF {} has an empty event mapping",
                    job.job_id, artifact.source_path
                )));
            }
            if artifact.kind == ArtifactKind::MeleeHkx
                && !artifact
                    .event_map
                    .values()
                    .any(|event| event.eq_ignore_ascii_case("HitFrame"))
            {
                return Err(PhaseError::BadParams(format!(
                    "job {} melee KF {} has no explicit HitFrame mapping",
                    job.job_id, artifact.source_path
                )));
            }
        }
        let key = artifact.target_path.to_ascii_lowercase();
        if !targets.insert(key.clone()) {
            return Err(PhaseError::BadParams(format!(
                "job {} has duplicate artifact target {}",
                job.job_id, artifact.target_path
            )));
        }
        if artifact.root {
            roots.insert(key);
        }
    }
    if !job.artifacts.is_empty() && roots.is_empty() {
        return Err(PhaseError::BadParams(format!(
            "job {} artifact closure has no root",
            job.job_id
        )));
    }
    for artifact in &job.artifacts {
        for dependency in &artifact.dependencies {
            if !targets.contains(&dependency.to_ascii_lowercase()) {
                return Err(PhaseError::BadParams(format!(
                    "job {} artifact {} references missing dependency {}",
                    job.job_id, artifact.target_path, dependency
                )));
            }
        }
    }
    let reachable = reachable_targets(&job.artifacts, &roots);
    if reachable != targets {
        let orphan = targets
            .difference(&reachable)
            .next()
            .cloned()
            .unwrap_or_default();
        return Err(PhaseError::BadParams(format!(
            "job {} artifact closure has unreachable target {}",
            job.job_id, orphan
        )));
    }
    Ok(())
}

fn reachable_targets(
    artifacts: &[PlannedArtifact],
    closure_roots: &BTreeSet<String>,
) -> BTreeSet<String> {
    let dependencies = artifacts
        .iter()
        .map(|artifact| {
            (
                artifact.target_path.to_ascii_lowercase(),
                artifact
                    .dependencies
                    .iter()
                    .map(|dependency| dependency.to_ascii_lowercase())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut reachable = BTreeSet::new();
    let mut pending = closure_roots.iter().cloned().collect::<Vec<_>>();
    while let Some(target) = pending.pop() {
        if !reachable.insert(target.clone()) {
            continue;
        }
        if let Some(children) = dependencies.get(&target) {
            pending.extend(children.iter().cloned());
        }
    }
    reachable
}

fn discovery_ledger(plan: &CreatureCorpusExecutionPlan) -> DiscoveryLedger {
    let mut capability_available = BTreeMap::new();
    let mut capability_missing = BTreeMap::new();
    for capabilities in plan
        .corpus
        .planned
        .iter()
        .map(|planned| &planned.capabilities)
        .chain(
            plan.corpus
                .rejected
                .iter()
                .map(|rejected| &rejected.capabilities),
        )
    {
        for capability in &capabilities.available {
            increment(&mut capability_available, capability_name(*capability));
        }
        for capability in &capabilities.missing {
            increment(&mut capability_missing, capability_name(*capability));
        }
    }
    let mut rejection_reason_counts = BTreeMap::new();
    for rejected in &plan.corpus.rejected {
        for reason in &rejected.reasons {
            let value = serde_json::to_value(reason).unwrap_or_default();
            let code = value
                .get("code")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
                .to_string();
            increment(&mut rejection_reason_counts, code);
        }
    }
    let mut preflight_issue_counts = BTreeMap::new();
    for job in &plan.jobs {
        for issue in &job.preflight_issues {
            let value = serde_json::to_value(issue).unwrap_or_default();
            let code = value
                .get("code")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
                .to_string();
            increment(&mut preflight_issue_counts, code);
        }
    }
    let jobs_preflight_failed = plan
        .jobs
        .iter()
        .filter(|job| !job.preflight_issues.is_empty())
        .count();
    DiscoveryLedger {
        version: PLAN_VERSION,
        candidate_count: plan.corpus.candidate_count,
        planned_count: plan.corpus.planned.len(),
        rejected_count: plan.corpus.rejected.len(),
        job_count: plan.jobs.len(),
        jobs_preflight_ready: plan.jobs.len() - jobs_preflight_failed,
        jobs_preflight_failed,
        native_catalog: plan.native_catalog.catalog.clone(),
        native_winner_count: plan.native_catalog.winner_count,
        native_dependent_count: plan.native_catalog.dependent_count,
        native_rejected_dependent_count: plan.native_catalog.rejected_dependent_count,
        native_entries_blake3: plan.native_catalog.entries_blake3.clone(),
        native_motion_family_count: plan.native_catalog.motion.family_count,
        native_motion_referenced_kf_count: plan.native_catalog.motion.referenced_kf_count,
        native_motion_emitted_set_count: plan.native_catalog.motion.emitted_motion_set_count,
        native_motion_entries_blake3: plan.native_catalog.motion.entries_blake3.clone(),
        capability_available,
        capability_missing,
        rejection_reason_counts,
        preflight_issue_counts,
    }
}

trait ArtifactStager {
    fn stage(&self, source: &Path, destination: &Path) -> Result<(), String>;
}

struct FilesystemStager;

impl ArtifactStager for FilesystemStager {
    fn stage(&self, source: &Path, destination: &Path) -> Result<(), String> {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::copy(source, destination)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

struct EvidenceBoundPreparedFamily {
    family_id: String,
    job_id: String,
    recipe: crate::source_rig::SourceRigExecutableRecipe,
    candidate_recipes: Vec<crate::source_rig::SourceRigExecutableRecipe>,
    execution: crate::source_rig::PreparedSourceRigExecution,
    data_files: Vec<(String, PathBuf)>,
}

fn parallel_prepare_ordered<T, R, E>(
    inputs: &[T],
    prepare: impl Fn(&T) -> Result<R, E> + Sync + Send,
) -> Vec<Result<R, E>>
where
    T: Sync,
    R: Send,
    E: Send,
{
    inputs.par_iter().map(prepare).collect()
}

fn execute_evidence_bound_batch(
    plan: &CreatureCorpusExecutionPlan,
    mode: ExecutionMode,
    mod_root: &Path,
    run: &mut crate::run::ConversionRun,
    roots: &SourceRoots<'_>,
) -> Result<BatchExecutionLedger, PhaseError> {
    if mode != ExecutionMode::Strict {
        return Err(PhaseError::BadParams(
            "evidence-bound creature recipes require strict execution".to_string(),
        ));
    }
    let staging_parent = mod_root
        .join(path_from_canonical(&plan.debug_dir))
        .join("recipe_staging")
        .join(stable_batch_id(plan));
    if staging_parent.exists() {
        fs::remove_dir_all(&staging_parent)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
    }
    fs::create_dir_all(&staging_parent).map_err(|error| PhaseError::Internal(error.to_string()))?;

    let results = parallel_prepare_ordered(&plan.jobs, |job| {
        prepare_evidence_bound_family(job, &staging_parent, roots, &run.interner)
            .map_err(|message| (job.family_id.clone(), message))
    });
    let mut prepared = Vec::with_capacity(results.len());
    for result in results {
        match result {
            Ok(family) => prepared.push(family),
            Err((family_id, message)) => {
                return Ok(failed_evidence_bound_ledger_with_cleanup(
                    plan,
                    &family_id,
                    message,
                    &staging_parent,
                ));
            }
        }
    }
    if prepared.is_empty() {
        fs::remove_dir_all(&staging_parent)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        return Ok(BatchExecutionLedger {
            version: PLAN_VERSION,
            mode,
            source_rejected_count: plan.corpus.rejected.len(),
            strict_aborted: false,
            candidate_disposition_counts: BTreeMap::new(),
            job_disposition_counts: BTreeMap::new(),
            family_disposition_counts: BTreeMap::new(),
            candidates: Vec::new(),
            jobs: Vec::new(),
            families: Vec::new(),
        });
    }

    let mut registrations = BTreeMap::new();
    for family in &prepared {
        for (relative, staged) in &family.data_files {
            let destination = mod_root.join("data").join(path_from_canonical(relative));
            if let Err(message) = register_prepared_data_file(
                &mut registrations,
                relative.clone(),
                staged.clone(),
                destination,
                run.config.overwrite_existing,
            ) {
                return Ok(failed_evidence_bound_ledger_with_cleanup(
                    plan,
                    &family.family_id,
                    message,
                    &staging_parent,
                ));
            }
        }
    }
    let registrations = registrations.into_values().collect::<Vec<_>>();

    let prepared_sink = if let Some(sink) = run.output_sink.as_deref() {
        let borrowed = registrations
            .iter()
            .map(|(relative, staged, _)| (relative.as_str(), staged.as_path()))
            .collect::<Vec<_>>();
        match sink.prepare_existing_files_batch(&borrowed) {
            Ok(prepared) => Some(prepared),
            Err(message) => {
                return Ok(failed_evidence_bound_ledger_with_cleanup(
                    plan,
                    &prepared[0].family_id,
                    format!("prepare creature asset sink: {message}"),
                    &staging_parent,
                ));
            }
        }
    } else {
        None
    };

    let target_plugin = crate::source_read::plugin_name_for_handle(run.target_handle_id)
        .map_err(|error| PhaseError::Internal(format!("read output plugin name: {error}")))?;
    let mapper_state = run.mapper_state.as_mut().ok_or_else(|| {
        PhaseError::Internal(
            "evidence-bound creature execution requires initialized mapper state".to_string(),
        )
    })?;
    let mut session = crate::session::open_session(run.target_handle_id, None)
        .map_err(|error| PhaseError::Internal(format!("open creature record session: {error}")))?;
    let mut recipes = prepared
        .iter()
        .flat_map(|family| family.candidate_recipes.iter().cloned())
        .collect::<Vec<_>>();
    recipes.sort_by_key(|recipe| {
        recipe
            .projection
            .source_primary_identity
            .stable_key()
            .to_ascii_lowercase()
    });
    if let Err(error) = validate_family_actor_action_ownership(&recipes) {
        return Ok(failed_evidence_bound_ledger_with_cleanup(
            plan,
            &prepared[0].family_id,
            error,
            &staging_parent,
        ));
    }
    let record_families = match recipes
        .iter()
        .map(|recipe| recipe.rebuild_record_family_batch(&run.interner))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
        .and_then(|families| {
            crate::source_rig::merge_creature_record_family_batches(families)
                .map_err(|error| error.to_string())
        }) {
        Ok(families) => families,
        Err(error) => {
            return Ok(failed_evidence_bound_ledger_with_cleanup(
                plan,
                &prepared[0].family_id,
                format!("merge creature candidate record batches: {error}"),
                &staging_parent,
            ));
        }
    };
    let prepared_records = match crate::source_rig::prepare_creature_record_families_batch(
        &mut session,
        mapper_state,
        record_families,
        &run.schema_target,
        &run.interner,
    ) {
        Ok(prepared) => prepared,
        Err(error) => {
            return Ok(failed_evidence_bound_ledger_with_cleanup(
                plan,
                &prepared[0].family_id,
                format!("prepare creature record batch: {error}"),
                &staging_parent,
            ));
        }
    };
    let prepared_records = match crate::source_rig::prepare_creature_record_commit_batch(
        prepared_records,
        target_plugin,
        run.target.as_str(),
        &recipes,
        &run.schema_target,
        &run.interner,
    ) {
        Ok(prepared) => prepared,
        Err(error) => {
            return Ok(failed_evidence_bound_ledger_with_cleanup(
                plan,
                &prepared[0].family_id,
                format!("bind creature record batch to recipes: {error}"),
                &staging_parent,
            ));
        }
    };

    let mut promoted = Vec::new();
    for (relative, staged, destination) in &registrations {
        if let Some(parent) = destination.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            rollback_replaced_recipe_files(&promoted);
            return Ok(failed_evidence_bound_ledger_with_cleanup(
                plan,
                &prepared[0].family_id,
                format!(
                    "create creature output directory {}: {error}",
                    parent.display()
                ),
                &staging_parent,
            ));
        }
        let backup = if destination.exists() {
            let backup = staging_parent
                .join("replaced")
                .join(path_from_canonical(relative));
            if let Some(parent) = backup.parent()
                && let Err(error) = fs::create_dir_all(parent)
            {
                rollback_replaced_recipe_files(&promoted);
                return Ok(failed_evidence_bound_ledger_with_cleanup(
                    plan,
                    &prepared[0].family_id,
                    format!(
                        "create creature asset backup directory {}: {error}",
                        parent.display()
                    ),
                    &staging_parent,
                ));
            }
            if let Err(error) = fs::rename(destination, &backup) {
                rollback_replaced_recipe_files(&promoted);
                return Ok(failed_evidence_bound_ledger_with_cleanup(
                    plan,
                    &prepared[0].family_id,
                    format!(
                        "back up existing creature asset {} to {}: {error}",
                        destination.display(),
                        backup.display()
                    ),
                    &staging_parent,
                ));
            }
            Some(backup)
        } else {
            None
        };
        if let Err(error) = fs::rename(staged, destination) {
            let mut rollback = rollback_replaced_recipe_files(&promoted);
            if let Some(backup) = backup.as_ref()
                && let Err(restore_error) = fs::rename(backup, destination)
            {
                rollback.push(format!(
                    "restore {} to {}: {restore_error}",
                    backup.display(),
                    destination.display()
                ));
            }
            return Ok(failed_evidence_bound_ledger_with_cleanup(
                plan,
                &prepared[0].family_id,
                append_rollback_errors(
                    format!(
                        "promote creature asset {} to {}: {error}",
                        staged.display(),
                        destination.display()
                    ),
                    rollback,
                ),
                &staging_parent,
            ));
        }
        promoted.push((destination.clone(), staged.clone(), backup));
    }
    if let Some(prepared_sink) = prepared_sink
        && let Err(error) = prepared_sink.commit()
    {
        let rollback = rollback_replaced_recipe_files(&promoted);
        return Ok(failed_evidence_bound_ledger_with_cleanup(
            plan,
            &prepared[0].family_id,
            append_rollback_errors(format!("commit creature asset sink: {error}"), rollback),
            &staging_parent,
        ));
    }

    let committed = prepared_records.commit();
    if let Err(error) = crate::source_rig::stage_creature_record_commit_ledger(
        mod_root,
        &committed,
        run.config.overwrite_existing,
    ) {
        return Err(PhaseError::Internal(format!(
            "creature target records committed but ledger persistence failed: {error}"
        )));
    }
    let receipt_dir = mod_root
        .join(path_from_canonical(&plan.debug_dir))
        .join("prepared_execution");
    for family in &prepared {
        write_json(
            &receipt_dir.join(format!("{}.json", stable_family_dir(&family.family_id))),
            &family.execution.receipt,
        )?;
    }
    let _ = fs::remove_dir_all(&staging_parent);

    let families = prepared
        .iter()
        .map(|family| FamilyExecutionLedger {
            family_id: family.family_id.clone(),
            staging_dir: path_display(&family.execution.staging_root),
            publish_root: family_publish_root(&family.recipe),
            disposition: TerminalDisposition::Published,
            job_ids: vec![family.job_id.clone()],
            assets_published: family.data_files.len(),
            records_deferred: 0,
            message: None,
        })
        .collect::<Vec<_>>();
    let jobs = prepared
        .iter()
        .map(|family| JobExecutionLedger {
            job_id: family.job_id.clone(),
            family_id: family.family_id.clone(),
            disposition: TerminalDisposition::Published,
            assets_published: family.data_files.len(),
            records_deferred: 0,
            message: None,
        })
        .collect();
    let candidates = candidate_execution_ledgers(plan, &families);
    Ok(BatchExecutionLedger {
        version: PLAN_VERSION,
        mode,
        source_rejected_count: plan.corpus.rejected.len(),
        strict_aborted: false,
        candidate_disposition_counts: BTreeMap::new(),
        job_disposition_counts: BTreeMap::new(),
        family_disposition_counts: BTreeMap::new(),
        candidates,
        jobs,
        families,
    })
}

fn validate_family_actor_action_ownership(
    recipes: &[crate::source_rig::SourceRigExecutableRecipe],
) -> Result<(), String> {
    let mut by_family =
        BTreeMap::<String, Vec<&crate::source_rig::SourceRigExecutableRecipe>>::new();
    for recipe in recipes {
        by_family
            .entry(recipe.batch_intent.family_id.to_ascii_lowercase())
            .or_default()
            .push(recipe);
    }
    let mut target_keys = BTreeSet::new();
    for family in by_family.values() {
        let owners = family
            .iter()
            .copied()
            .filter(|recipe| !recipe.actor_action_records.is_empty())
            .collect::<Vec<_>>();
        let [owner] = owners.as_slice() else {
            return Err(format!(
                "creature family {:?} must have exactly one Actor Action record owner, found {}",
                family[0].batch_intent.family_id,
                owners.len()
            ));
        };
        let owner_requirements = owner
            .actor_action_records
            .iter()
            .map(|record| record.requirement.clone())
            .collect::<BTreeSet<_>>();
        for recipe in family {
            let expected = crate::source_rig::required_actor_action_records(
                &recipe.graph,
                &recipe.rig.paths.root_behavior,
            )
            .into_iter()
            .collect::<BTreeSet<_>>();
            if expected != owner_requirements {
                return Err(format!(
                    "creature family {:?} candidate {} disagrees with its Actor Action owner",
                    recipe.batch_intent.family_id,
                    recipe.projection.source_primary_identity.stable_key()
                ));
            }
        }
        for record in &owner.actor_action_records {
            let key = (
                record.form_key.plugin.to_ascii_lowercase(),
                record.form_key.local,
            );
            if !target_keys.insert(key) {
                return Err(format!(
                    "generated Actor Action IDLE {:06X}@{} is owned by more than one creature family",
                    record.form_key.local, record.form_key.plugin
                ));
            }
        }
    }
    Ok(())
}

fn prepare_evidence_bound_family(
    job: &CreatureCorpusJob,
    staging_parent: &Path,
    roots: &SourceRoots<'_>,
    interner: &StringInterner,
) -> Result<EvidenceBoundPreparedFamily, String> {
    let planned = job
        .executable_recipe
        .as_ref()
        .ok_or_else(|| "job has no executable recipe".to_string())?;
    let recipe_path = roots
        .resolve(planned.recipe_source_root, &planned.recipe_path)
        .ok_or_else(|| "recipe source root is unavailable".to_string())?;
    let recipe_json = fs::read_to_string(&recipe_path)
        .map_err(|error| format!("read executable recipe {}: {error}", recipe_path.display()))?;
    let recipe = crate::source_rig::SourceRigExecutableRecipe::from_json(&recipe_json)
        .map_err(|error| format!("validate executable recipe: {error}"))?;
    let recipe_hash = recipe
        .stable_hash_blake3()
        .map_err(|error| error.to_string())?;
    if recipe_hash != planned.recipe_blake3 || recipe.batch_intent.family_id != job.family_id {
        return Err("executable recipe identity changed after discovery".to_string());
    }
    let closure_path = roots
        .resolve(
            planned.creature_closure_receipt_source_root,
            &planned.creature_closure_receipt_path,
        )
        .ok_or_else(|| "creature closure receipt source root is unavailable".to_string())?;
    let closure_json = fs::read_to_string(&closure_path).map_err(|error| {
        format!(
            "read creature closure receipt {}: {error}",
            closure_path.display()
        )
    })?;
    let closure = nif_core_native::creature_closure::CreatureClosureReceipt::parse_and_validate(
        &closure_json,
    )
    .map_err(|error| error.to_string())?;
    if closure.receipt_hash != planned.creature_closure_receipt_blake3 {
        return Err("creature closure receipt changed after discovery".to_string());
    }
    let closure_root = roots
        .resolve(
            planned.creature_closure_staged_data_source_root,
            &planned.creature_closure_staged_data_root,
        )
        .ok_or_else(|| "creature closure staged-data root is unavailable".to_string())?;
    let bridge = recipe
        .bridge_receipt
        .as_ref()
        .ok_or_else(|| "executable recipe has no bridge receipt".to_string())?;
    let receipts = bridge
        .artifact_receipts
        .iter()
        .map(|receipt| (receipt.runtime_path.to_ascii_lowercase(), receipt.clone()))
        .collect::<BTreeMap<_, _>>();
    let runtime_model_receipts = recipe
        .runtime_model_closure
        .as_ref()
        .map(|receipt| receipt.canonical_target_artifacts())
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default()
        .into_iter()
        .map(|receipt| (receipt.target_data_path.to_ascii_lowercase(), receipt))
        .collect::<BTreeMap<_, _>>();
    let mut artifacts = Vec::new();
    let mut runtime_model_artifacts = Vec::new();
    for artifact in &planned.converted_artifacts {
        let path = roots
            .resolve(artifact.source_root, &artifact.source_path)
            .ok_or_else(|| format!("source root unavailable for {}", artifact.source_path))?;
        let bytes = fs::read(&path)
            .map_err(|error| format!("read converted artifact {}: {error}", path.display()))?;
        if blake3::hash(&bytes).to_hex().to_string() != artifact.source_blake3 {
            return Err(format!(
                "converted artifact changed after discovery: {}",
                path.display()
            ));
        }
        let key = artifact.runtime_path.to_ascii_lowercase();
        match (receipts.get(&key), runtime_model_receipts.get(&key)) {
            (Some(receipt), None) => artifacts.push(crate::source_rig::SourceRigArtifactInput {
                path,
                receipt: receipt.clone(),
            }),
            (None, Some(receipt)) => runtime_model_artifacts.push(
                crate::source_rig::SourceRigRuntimeModelArtifactInput {
                    path,
                    receipt: receipt.clone(),
                },
            ),
            (Some(_), Some(_)) => {
                return Err(format!(
                    "converted artifact {} is assigned to both source-rig and runtime-model closures",
                    artifact.runtime_path
                ));
            }
            (None, None) => {
                return Err(format!(
                    "recipe no longer requires converted artifact {}",
                    artifact.runtime_path
                ));
            }
        }
    }
    let staging_root = staging_parent.join(stable_family_dir(&job.family_id));
    let execution = crate::source_rig::prepare_source_rig_execution_with_runtime_models(
        &recipe_path,
        &artifacts,
        &runtime_model_artifacts,
        &crate::source_rig::SourceRigCreatureClosureInput {
            receipt_json: closure_json,
            staged_data_root: closure_root,
        },
        &staging_root,
        interner,
    )
    .map_err(|error| error.to_string())?;
    let data_files = prepared_execution_data_files(&execution, &closure)?;
    let candidate_recipes = if job.candidate_recipes.is_empty() {
        vec![recipe.clone()]
    } else {
        job.candidate_recipes
            .iter()
            .map(|candidate| {
                if candidate.recipe_source_root == planned.recipe_source_root
                    && candidate.recipe_path == planned.recipe_path
                    && candidate.recipe_blake3 == planned.recipe_blake3
                    && candidate
                        .source_key
                        .eq_ignore_ascii_case(&planned.source_key)
                {
                    Ok(recipe.clone())
                } else {
                    load_candidate_recipe(candidate, roots)
                }
            })
            .collect::<Result<Vec<_>, String>>()?
    };
    if candidate_recipes
        .iter()
        .any(|candidate| candidate.batch_intent.family_id != job.family_id)
    {
        return Err("candidate recipe record family differs from its owning bundle".to_string());
    }
    Ok(EvidenceBoundPreparedFamily {
        family_id: job.family_id.clone(),
        job_id: job.job_id.clone(),
        recipe,
        candidate_recipes,
        execution,
        data_files,
    })
}

fn load_candidate_recipe(
    planned: &PlannedCandidateRecipe,
    roots: &SourceRoots<'_>,
) -> Result<crate::source_rig::SourceRigExecutableRecipe, String> {
    let path = roots
        .resolve(planned.recipe_source_root, &planned.recipe_path)
        .ok_or_else(|| "candidate recipe source root is unavailable".to_string())?;
    let json = fs::read_to_string(&path)
        .map_err(|error| format!("read candidate recipe {}: {error}", path.display()))?;
    let recipe = crate::source_rig::SourceRigExecutableRecipe::from_json(&json)
        .map_err(|error| format!("validate candidate recipe: {error}"))?;
    let hash = recipe
        .stable_hash_blake3()
        .map_err(|error| error.to_string())?;
    if hash != planned.recipe_blake3
        || !recipe
            .projection
            .source_primary_identity
            .stable_key()
            .eq_ignore_ascii_case(&planned.source_key)
    {
        return Err(format!(
            "candidate recipe {} identity changed after discovery",
            planned.source_key
        ));
    }
    Ok(recipe)
}

fn prepared_execution_data_files(
    execution: &crate::source_rig::PreparedSourceRigExecution,
    closure: &nif_core_native::creature_closure::CreatureClosureReceipt,
) -> Result<Vec<(String, PathBuf)>, String> {
    let mut files = BTreeMap::new();
    for artifact in &execution.receipt.source_artifacts {
        insert_prepared_data_file(
            &mut files,
            format!("Meshes/{}", artifact.runtime_path.replace('\\', "/")),
            execution
                .staging_root
                .join(path_from_runtime(&artifact.runtime_path)),
        )?;
    }
    for artifact in &execution.receipt.runtime_model_artifacts {
        insert_prepared_data_file(
            &mut files,
            artifact.target_data_path.replace('\\', "/"),
            execution
                .staging_root
                .join(path_from_runtime(&artifact.target_data_path)),
        )?;
    }
    for artifact in &execution.receipt.scaffold_artifacts {
        insert_prepared_data_file(
            &mut files,
            format!("Meshes/{}", artifact.runtime_path.replace('\\', "/")),
            execution
                .staging_root
                .join(path_from_runtime(&artifact.runtime_path)),
        )?;
    }
    for artifact in &closure.artifacts {
        let relative = canonical_relative(
            &artifact.target_data_relative_path,
            "creature closure target",
        )
        .map_err(|error| error.to_string())?;
        let runtime = relative
            .strip_prefix("meshes/")
            .or_else(|| relative.strip_prefix("Meshes/"))
            .unwrap_or(&relative)
            .to_string();
        insert_prepared_data_file(
            &mut files,
            relative,
            execution.staging_root.join(path_from_canonical(&runtime)),
        )?;
    }
    for (relative, path) in files.values() {
        let metadata = fs::metadata(path)
            .map_err(|error| format!("prepared asset {} is missing: {error}", path.display()))?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(format!("prepared asset is empty: {}", path.display()));
        }
        if relative.trim().is_empty() {
            return Err("prepared asset has an empty data path".to_string());
        }
    }
    Ok(files.into_values().collect())
}

fn insert_prepared_data_file(
    files: &mut BTreeMap<String, (String, PathBuf)>,
    relative: String,
    path: PathBuf,
) -> Result<(), String> {
    let relative =
        canonical_relative(&relative, "prepared data target").map_err(|error| error.to_string())?;
    let key = relative.to_ascii_lowercase();
    if files.insert(key, (relative.clone(), path)).is_some() {
        return Err(format!("duplicate prepared data target {relative}"));
    }
    Ok(())
}

fn register_prepared_data_file(
    registrations: &mut BTreeMap<String, (String, PathBuf, PathBuf)>,
    relative: String,
    staged: PathBuf,
    destination: PathBuf,
    overwrite_existing: bool,
) -> Result<(), String> {
    let key = relative.to_ascii_lowercase();
    if let Some((_, existing_staged, _)) = registrations.get(&key) {
        let existing_bytes = fs::read(existing_staged).map_err(|error| {
            format!(
                "read shared prepared data target {}: {error}",
                existing_staged.display()
            )
        })?;
        let staged_bytes = fs::read(&staged).map_err(|error| {
            format!(
                "read shared prepared data target {}: {error}",
                staged.display()
            )
        })?;
        if existing_bytes == staged_bytes {
            return Ok(());
        }
        return Err(format!("conflicting prepared data target {relative}"));
    }
    if destination.exists() && !overwrite_existing {
        return Err(format!(
            "prepared data target already exists: {}",
            destination.display()
        ));
    }
    registrations.insert(key, (relative, staged, destination));
    Ok(())
}

fn path_from_runtime(value: &str) -> PathBuf {
    value.replace('/', "\\").split('\\').collect()
}

fn family_publish_root(recipe: &crate::source_rig::SourceRigExecutableRecipe) -> String {
    let project = recipe.rig.paths.project.replace('\\', "/");
    let parent = project
        .rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("");
    format!("Meshes/{parent}")
}

fn rollback_recipe_files(promoted: &[(PathBuf, PathBuf)]) -> Vec<String> {
    let mut errors = Vec::new();
    for (destination, staged) in promoted.iter().rev() {
        if let Some(parent) = staged.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(error) = fs::rename(destination, staged) {
            errors.push(format!(
                "rollback {} to {}: {error}",
                destination.display(),
                staged.display()
            ));
        }
    }
    errors
}

fn rollback_replaced_recipe_files(promoted: &[(PathBuf, PathBuf, Option<PathBuf>)]) -> Vec<String> {
    let mut errors = Vec::new();
    for (destination, staged, backup) in promoted.iter().rev() {
        if let Some(parent) = staged.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(error) = fs::rename(destination, staged) {
            errors.push(format!(
                "rollback {} to {}: {error}",
                destination.display(),
                staged.display()
            ));
        }
        if let Some(backup) = backup
            && let Err(error) = fs::rename(backup, destination)
        {
            errors.push(format!(
                "restore {} to {}: {error}",
                backup.display(),
                destination.display()
            ));
        }
    }
    errors
}

fn candidate_execution_ledgers(
    plan: &CreatureCorpusExecutionPlan,
    families: &[FamilyExecutionLedger],
) -> Vec<CandidateExecutionLedger> {
    let dispositions = families
        .iter()
        .map(|family| (family.family_id.to_ascii_lowercase(), family))
        .collect::<BTreeMap<_, _>>();
    let mut candidates = Vec::new();
    for job in &plan.jobs {
        let family = dispositions
            .get(&job.family_id.to_ascii_lowercase())
            .expect("validated job family has an execution disposition");
        let source_keys = if job.executable_recipe.is_some() {
            effective_candidate_recipes(job)
                .into_iter()
                .map(|recipe| recipe.source_key)
                .collect::<Vec<_>>()
        } else {
            vec![job.source_key.clone()]
        };
        candidates.extend(
            source_keys
                .into_iter()
                .map(|source_key| CandidateExecutionLedger {
                    source_key,
                    owner_family_id: job.family_id.clone(),
                    disposition: family.disposition,
                    message: family.message.clone(),
                }),
        );
    }
    candidates.sort_by_key(|candidate| candidate.source_key.to_ascii_lowercase());
    candidates
}

fn failed_evidence_bound_ledger(
    plan: &CreatureCorpusExecutionPlan,
    failed_family: &str,
    message: String,
) -> BatchExecutionLedger {
    let families = plan
        .jobs
        .iter()
        .map(|job| {
            let failed = job.family_id == failed_family;
            FamilyExecutionLedger {
                family_id: job.family_id.clone(),
                staging_dir: String::new(),
                publish_root: job.publish_root.clone(),
                disposition: if failed {
                    TerminalDisposition::Failed
                } else {
                    TerminalDisposition::AbortedStrict
                },
                job_ids: vec![job.job_id.clone()],
                assets_published: 0,
                records_deferred: 0,
                message: Some(if failed {
                    message.clone()
                } else {
                    format!("strict creature batch aborted after {failed_family}: {message}")
                }),
            }
        })
        .collect::<Vec<_>>();
    let jobs = families
        .iter()
        .map(|family| JobExecutionLedger {
            job_id: family.job_ids[0].clone(),
            family_id: family.family_id.clone(),
            disposition: family.disposition,
            assets_published: 0,
            records_deferred: 0,
            message: family.message.clone(),
        })
        .collect();
    let candidates = candidate_execution_ledgers(plan, &families);
    BatchExecutionLedger {
        version: PLAN_VERSION,
        mode: ExecutionMode::Strict,
        source_rejected_count: plan.corpus.rejected.len(),
        strict_aborted: true,
        candidate_disposition_counts: BTreeMap::new(),
        job_disposition_counts: BTreeMap::new(),
        family_disposition_counts: BTreeMap::new(),
        candidates,
        jobs,
        families,
    }
}

fn failed_evidence_bound_ledger_with_cleanup(
    plan: &CreatureCorpusExecutionPlan,
    failed_family: &str,
    message: String,
    staging_parent: &Path,
) -> BatchExecutionLedger {
    let message = match fs::remove_dir_all(staging_parent) {
        Ok(()) => message,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => message,
        Err(error) => format!(
            "{message}; remove failed creature staging {}: {error}",
            staging_parent.display()
        ),
    };
    failed_evidence_bound_ledger(plan, failed_family, message)
}

fn execute_batch(
    plan: &CreatureCorpusExecutionPlan,
    mode: ExecutionMode,
    mod_root: &Path,
    roots: &SourceRoots<'_>,
    sink: Option<&crate::sinks::SinkSet>,
    stager: &dyn ArtifactStager,
) -> Result<BatchExecutionLedger, PhaseError> {
    let staging_root = mod_root
        .join(path_from_canonical(&plan.debug_dir))
        .join("staging")
        .join(stable_batch_id(plan));
    fs::create_dir_all(&staging_root).map_err(|error| PhaseError::Internal(error.to_string()))?;

    let mut grouped = BTreeMap::<String, Vec<&CreatureCorpusJob>>::new();
    for job in &plan.jobs {
        grouped.entry(job.family_id.clone()).or_default().push(job);
    }

    let mut staged = Vec::new();
    for (family_id, jobs) in grouped {
        let staging_dir = staging_root.join(stable_family_dir(&family_id));
        let result = stage_family(&family_id, &jobs, &staging_dir, roots, stager);
        staged.push((family_id, jobs, staging_dir, result));
    }

    let has_failure = staged.iter().any(|(_, _, _, result)| result.is_err());
    let mut strict_aborted = mode == ExecutionMode::Strict && has_failure;
    let mut strict_publish_failure = None;
    if mode == ExecutionMode::Strict && !has_failure {
        let ready = staged
            .iter()
            .filter_map(|(family_id, _, _, result)| {
                result
                    .as_ref()
                    .ok()
                    .map(|staged| (family_id.as_str(), staged))
            })
            .collect::<Vec<_>>();
        if let Err(failure) = publish_strict_batch(&ready, mod_root, sink) {
            strict_aborted = true;
            strict_publish_failure = Some(failure);
        }
    }
    let mut families = Vec::new();
    let mut jobs_ledger = Vec::new();

    for (family_id, jobs, staging_dir, result) in staged {
        let job_ids = jobs
            .iter()
            .map(|job| job.job_id.clone())
            .collect::<Vec<_>>();
        let publish_root = jobs
            .first()
            .map(|job| job.publish_root.clone())
            .unwrap_or_default();
        let records_deferred = jobs
            .iter()
            .flat_map(|job| &job.artifacts)
            .filter(|artifact| artifact.kind.is_record())
            .count();

        let (disposition, assets_published, message) = match result {
            Err(failure) => (failure.disposition, 0, Some(failure.message)),
            Ok(staged_family) if mode == ExecutionMode::Strict => {
                if let Some(failure) = &strict_publish_failure {
                    let disposition = if failure.family_id == family_id {
                        TerminalDisposition::Failed
                    } else {
                        TerminalDisposition::AbortedStrict
                    };
                    (disposition, 0, Some(failure.message.clone()))
                } else if strict_aborted {
                    (
                        TerminalDisposition::AbortedStrict,
                        0,
                        Some("another family failed in strict mode".to_string()),
                    )
                } else {
                    (
                        TerminalDisposition::Published,
                        staged_family.assets.len(),
                        None,
                    )
                }
            }
            Ok(staged_family) => match publish_family(&staged_family, mod_root, sink) {
                Ok(count) => (TerminalDisposition::Published, count, None),
                Err(message) => (TerminalDisposition::Failed, 0, Some(message)),
            },
        };

        for job in jobs {
            let job_assets = if disposition == TerminalDisposition::Published {
                job.artifacts
                    .iter()
                    .filter(|artifact| !artifact.kind.is_record())
                    .count()
            } else {
                0
            };
            let job_records = if disposition == TerminalDisposition::Published {
                job.artifacts
                    .iter()
                    .filter(|artifact| artifact.kind.is_record())
                    .count()
            } else {
                0
            };
            jobs_ledger.push(JobExecutionLedger {
                job_id: job.job_id.clone(),
                family_id: family_id.clone(),
                disposition,
                assets_published: job_assets,
                records_deferred: job_records,
                message: message.clone(),
            });
        }
        families.push(FamilyExecutionLedger {
            family_id,
            staging_dir: path_display(&staging_dir),
            publish_root,
            disposition,
            job_ids,
            assets_published,
            records_deferred: if disposition == TerminalDisposition::Published {
                records_deferred
            } else {
                0
            },
            message,
        });
    }

    let candidates = candidate_execution_ledgers(plan, &families);
    Ok(BatchExecutionLedger {
        version: PLAN_VERSION,
        mode,
        source_rejected_count: plan.corpus.rejected.len(),
        strict_aborted,
        candidate_disposition_counts: BTreeMap::new(),
        job_disposition_counts: BTreeMap::new(),
        family_disposition_counts: BTreeMap::new(),
        candidates,
        jobs: jobs_ledger,
        families,
    })
}

struct StagedFamily {
    publish_root: String,
    staged_publish_root: PathBuf,
    assets: Vec<String>,
}

#[derive(Debug)]
struct FamilyFailure {
    disposition: TerminalDisposition,
    message: String,
}

struct StrictPublishFailure {
    family_id: String,
    message: String,
}

fn stage_family(
    family_id: &str,
    jobs: &[&CreatureCorpusJob],
    staging_dir: &Path,
    roots: &SourceRoots<'_>,
    stager: &dyn ArtifactStager,
) -> Result<StagedFamily, FamilyFailure> {
    let publish_roots = jobs
        .iter()
        .map(|job| job.publish_root.as_str())
        .collect::<BTreeSet<_>>();
    if publish_roots.len() != 1 {
        return Err(FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!("family {family_id} has multiple publish roots"),
        });
    }
    if let Some(job) = jobs
        .iter()
        .find(|job| job.adapter == CreatureAdapterProfile::UnsupportedCapability)
    {
        return Err(FamilyFailure {
            disposition: TerminalDisposition::UnsupportedCapability,
            message: format!(
                "job {} requires unsupported capability {}",
                job.job_id,
                job.unsupported_capability
                    .as_deref()
                    .unwrap_or("creature_adapter")
            ),
        });
    }
    if let Some((job, issue)) = jobs
        .iter()
        .find_map(|job| job.preflight_issues.first().map(|issue| (*job, issue)))
    {
        return Err(FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!("job {} failed preflight: {issue:?}", job.job_id),
        });
    }

    let publish_root = (*publish_roots.iter().next().expect("one publish root")).to_string();
    let publish_tree = staging_dir.join("publish");
    let records_tree = staging_dir.join("records");
    let mut assets = Vec::new();

    let mut skeleton_receipt = None;
    for job in jobs {
        for artifact in job
            .artifacts
            .iter()
            .filter(|artifact| is_source_rig_skeleton_conversion(artifact))
        {
            if skeleton_receipt.is_some() {
                return Err(FamilyFailure {
                    disposition: TerminalDisposition::Failed,
                    message: format!(
                        "family {family_id} declares more than one source-owned animation skeleton"
                    ),
                });
            }
            let (source, _) = read_planned_source(artifact, roots)?;
            assets.push(artifact.target_path.clone());
            let destination = publish_tree.join(path_from_canonical(&artifact.target_path));
            skeleton_receipt = Some(stage_source_rig_skeleton(artifact, &source, &destination)?);
        }
    }

    for job in jobs {
        for artifact in &job.artifacts {
            if is_source_rig_skeleton_conversion(artifact) {
                continue;
            }
            let (source, bytes) = read_planned_source(artifact, roots)?;
            let destination = if artifact.kind.is_record() {
                records_tree.join(path_from_canonical(&artifact.target_path))
            } else {
                assets.push(artifact.target_path.clone());
                publish_tree.join(path_from_canonical(&artifact.target_path))
            };
            if is_source_rig_nif_conversion(artifact) {
                stage_source_rig_nif(job, artifact, &source, &destination)?;
            } else if is_fnv_kf_conversion(artifact) {
                let skeleton = skeleton_receipt.as_ref().ok_or_else(|| FamilyFailure {
                    disposition: TerminalDisposition::Failed,
                    message: format!(
                        "family {family_id} cannot convert KF {} without its staged source-owned skeleton receipt",
                        artifact.source_path
                    ),
                })?;
                stage_fnv_kf(job, artifact, &bytes, skeleton, &destination)?;
            } else {
                stager
                    .stage(&source, &destination)
                    .map_err(|message| FamilyFailure {
                        disposition: TerminalDisposition::Failed,
                        message: format!("stage {}: {message}", artifact.target_path),
                    })?;
            }
        }
    }
    audit_staged_family(jobs, &publish_tree, &records_tree).map_err(|message| FamilyFailure {
        disposition: TerminalDisposition::Failed,
        message,
    })?;
    if jobs
        .iter()
        .flat_map(|job| &job.artifacts)
        .any(|artifact| artifact.kind.is_record())
    {
        return Err(FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: "atomic creature record batch was staged but has no committed CreatureRecordBatchReceipt/record_commit_ledger.json"
                .to_string(),
        });
    }
    Ok(StagedFamily {
        staged_publish_root: publish_tree.join(path_from_canonical(&publish_root)),
        publish_root,
        assets,
    })
}

fn read_planned_source(
    artifact: &PlannedArtifact,
    roots: &SourceRoots<'_>,
) -> Result<(PathBuf, Vec<u8>), FamilyFailure> {
    let source = roots
        .resolve(artifact.source_root, &artifact.source_path)
        .ok_or_else(|| FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!("source root unavailable for {}", artifact.source_path),
        })?;
    let bytes = fs::read(&source).map_err(|error| FamilyFailure {
        disposition: TerminalDisposition::Failed,
        message: format!("read {}: {error}", source.display()),
    })?;
    if bytes.is_empty() {
        return Err(FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!("empty source artifact {}", source.display()),
        });
    }
    if let Some(expected) = &artifact.source_blake3 {
        let actual = blake3::hash(&bytes).to_hex().to_string();
        if &actual != expected {
            return Err(FamilyFailure {
                disposition: TerminalDisposition::Failed,
                message: format!(
                    "source artifact changed after preflight: {}",
                    source.display()
                ),
            });
        }
    }
    Ok((source, bytes))
}

fn is_source_rig_nif_conversion(artifact: &PlannedArtifact) -> bool {
    artifact.source_root == ArtifactSourceRoot::SourceExtracted
        && artifact.kind.is_nif()
        && has_extension(&artifact.source_path, "nif")
}

fn is_source_rig_skeleton_conversion(artifact: &PlannedArtifact) -> bool {
    artifact.source_root == ArtifactSourceRoot::SourceExtracted
        && artifact.kind == ArtifactKind::SkeletonHkx
        && has_extension(&artifact.source_path, "nif")
}

fn is_fnv_kf_conversion(artifact: &PlannedArtifact) -> bool {
    artifact.source_root == ArtifactSourceRoot::SourceExtracted
        && matches!(
            artifact.kind,
            ArtifactKind::IdleHkx | ArtifactKind::LocomotionHkx | ArtifactKind::MeleeHkx
        )
        && has_extension(&artifact.source_path, "kf")
}

fn stage_source_rig_nif(
    job: &CreatureCorpusJob,
    artifact: &PlannedArtifact,
    source: &Path,
    destination: &Path,
) -> Result<(), FamilyFailure> {
    let source_game = job
        .source_key
        .split('|')
        .next()
        .filter(|value| matches!(*value, "skyrimse" | "fnv" | "fo3"))
        .ok_or_else(|| FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!(
                "job {} has no supported source game in {}",
                job.job_id, job.source_key
            ),
        })?;
    let kind = match artifact.kind {
        ArtifactKind::SkeletonNif => nif_core_native::convert_file::SourceRigNifKind::Skeleton,
        ArtifactKind::VisualNif => nif_core_native::convert_file::SourceRigNifKind::Body,
        _ => {
            return Err(FamilyFailure {
                disposition: TerminalDisposition::Failed,
                message: format!(
                    "artifact {} is not a source-rig NIF conversion",
                    artifact.target_path
                ),
            });
        }
    };
    let receipt = nif_core_native::convert_file::stage_preserve_source_rig_nif(
        kind,
        source,
        destination,
        &artifact.target_path,
        source_game,
        None,
        &nif_core_native::convert_file::ConvertFileOptions::default(),
    )
    .map_err(|error| FamilyFailure {
        disposition: TerminalDisposition::Failed,
        message: format!("convert source-rig NIF {}: {error}", source.display()),
    })?;
    if receipt.output_provenance
        != nif_core_native::convert_file::SourceRigNifOutputProvenance::PreserveSourceRigConversion
        || receipt.output_len == 0
        || !receipt.staged_target_path.as_path().eq(destination)
    {
        return Err(FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!(
                "source-rig NIF receipt did not close {}",
                artifact.target_path
            ),
        });
    }
    Ok(())
}

fn stage_source_rig_skeleton(
    artifact: &PlannedArtifact,
    source: &Path,
    destination: &Path,
) -> Result<crate::phase::skeleton::SourceRigSkeletonArtifactReceipt, FamilyFailure> {
    let skeleton_name = artifact
        .skeleton_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!(
                "source-rig skeleton {} has no explicit skeleton_name",
                artifact.target_path
            ),
        })?;
    let output_path = artifact
        .target_path
        .strip_prefix("Meshes/")
        .or_else(|| artifact.target_path.strip_prefix("meshes/"))
        .ok_or_else(|| FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!(
                "source-rig skeleton target is not data-relative under Meshes/: {}",
                artifact.target_path
            ),
        })?;
    let receipt = crate::phase::skeleton::convert_source_owned_skeleton_artifact(
        source,
        output_path,
        skeleton_name,
        &artifact.float_slot_names,
    )
    .map_err(|error| FamilyFailure {
        disposition: TerminalDisposition::Failed,
        message: format!(
            "convert source-owned skeleton {}: {error}",
            source.display()
        ),
    })?;
    if !receipt
        .data_relative_path
        .eq_ignore_ascii_case(&artifact.target_path)
        || receipt.hkx_bytes.is_empty()
        || receipt.hkx_blake3 != blake3::hash(&receipt.hkx_bytes).to_hex().to_string()
    {
        return Err(FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!(
                "source-rig skeleton receipt did not close {}",
                artifact.target_path
            ),
        });
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!("create skeleton staging directory: {error}"),
        })?;
    }
    fs::write(destination, &receipt.hkx_bytes).map_err(|error| FamilyFailure {
        disposition: TerminalDisposition::Failed,
        message: format!("write staged skeleton {}: {error}", destination.display()),
    })?;
    Ok(receipt)
}

fn stage_fnv_kf(
    job: &CreatureCorpusJob,
    artifact: &PlannedArtifact,
    kf_bytes: &[u8],
    skeleton: &crate::phase::skeleton::SourceRigSkeletonArtifactReceipt,
    destination: &Path,
) -> Result<(), FamilyFailure> {
    let source_game = match job.source_key.split('|').next() {
        Some("fnv") => fnv_catalog::LegacyCreatureGame::Fnv,
        Some("fo3") => fnv_catalog::LegacyCreatureGame::Fo3,
        _ => {
            return Err(FamilyFailure {
                disposition: TerminalDisposition::Failed,
                message: format!(
                    "job {} is not an FNV/FO3 source for KF {}",
                    job.job_id, artifact.source_path
                ),
            });
        }
    };
    let selected_sequence_index = artifact.sequence_index.ok_or_else(|| FamilyFailure {
        disposition: TerminalDisposition::Failed,
        message: format!("KF {} has no selected sequence index", artifact.source_path),
    })?;
    let evidence = fnv_motion::parse_creature_kf_evidence(
        kf_bytes,
        &artifact.source_path,
        source_game,
        Some(selected_sequence_index),
        Some(fnv_motion::CreatureKfSkeletonContract {
            skeleton_path: &skeleton.runtime_path,
            ordered_bone_names: &skeleton.ordered_bone_names,
            ordered_float_slot_names: &skeleton.float_slot_names,
        }),
    )
    .map_err(|error| FamilyFailure {
        disposition: TerminalDisposition::Failed,
        message: format!("parse source KF {}: {error}", artifact.source_path),
    })?;
    let sequence = match &evidence.sequence {
        fnv_motion::KfParseEvidence::Parsed(sequence) => sequence,
        other => {
            return Err(FamilyFailure {
                disposition: TerminalDisposition::Failed,
                message: format!(
                    "source KF {} did not produce parsed evidence: {other:?}",
                    artifact.source_path
                ),
            });
        }
    };
    if sequence.binding.compatibility != fnv_motion::BindingCompatibility::Verified {
        return Err(FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!(
                "source KF {} binding is not verified: {:?}",
                artifact.source_path, sequence.binding.compatibility_detail
            ),
        });
    }
    let extracted_motion_policy = match &sequence.root_motion {
        fnv_motion::RootMotionEvidence::None
        | fnv_motion::RootMotionEvidence::Stationary { .. } => {
            crate::phase::animations::FnvExtractedMotionPolicy::RejectNonzero
        }
        fnv_motion::RootMotionEvidence::Planar { .. } => {
            crate::phase::animations::FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame
        }
        fnv_motion::RootMotionEvidence::Unknown => {
            return Err(FamilyFailure {
                disposition: TerminalDisposition::Failed,
                message: format!("source KF {} has unknown root motion", artifact.source_path),
            });
        }
        fnv_motion::RootMotionEvidence::Unsupported { detail, .. } => {
            return Err(FamilyFailure {
                disposition: TerminalDisposition::Failed,
                message: format!(
                    "source KF {} has unsupported root motion: {detail}",
                    artifact.source_path
                ),
            });
        }
    };
    let output_clip_path = artifact
        .target_path
        .strip_prefix("Meshes/")
        .or_else(|| artifact.target_path.strip_prefix("meshes/"))
        .ok_or_else(|| FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!(
                "KF target is not data-relative under Meshes/: {}",
                artifact.target_path
            ),
        })?
        .replace('\\', "/");
    let event_map = artifact
        .event_map
        .iter()
        .map(|(source, target)| (source.clone(), target.clone()))
        .collect::<HashMap<_, _>>();
    let staged = crate::phase::animations::stage_fnv_creature_kf(
        kf_bytes,
        crate::phase::animations::FnvKfStageRequest {
            source_kf: &artifact.source_path,
            output_clip_path: &output_clip_path,
            sequence_index: Some(selected_sequence_index),
            skeleton: crate::phase::animations::FnvKfSkeletonContract {
                skeleton_path: &skeleton.runtime_path,
                ordered_bone_names: &skeleton.ordered_bone_names,
                ordered_float_slot_names: &skeleton.float_slot_names,
            },
            original_skeleton_name: &skeleton.skeleton_name,
            event_map: &event_map,
            target_sample_rate_hz: artifact.target_sample_rate_hz,
            extracted_motion_policy,
        },
    )
    .map_err(|error| FamilyFailure {
        disposition: TerminalDisposition::Failed,
        message: format!("convert source KF {}: {error}", artifact.source_path),
    })?;
    if staged.hkx_bytes.is_empty()
        || staged.receipt.sequence_index != selected_sequence_index
        || !staged
            .receipt
            .output_clip_path
            .replace('\\', "/")
            .eq_ignore_ascii_case(&output_clip_path)
        || staged.receipt.runtime_skeleton_path != skeleton.runtime_path
        || staged.receipt.original_skeleton_name != skeleton.skeleton_name
    {
        return Err(FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!(
                "KF conversion receipt did not close {}",
                artifact.target_path
            ),
        });
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| FamilyFailure {
            disposition: TerminalDisposition::Failed,
            message: format!("create KF staging directory: {error}"),
        })?;
    }
    fs::write(destination, &staged.hkx_bytes).map_err(|error| FamilyFailure {
        disposition: TerminalDisposition::Failed,
        message: format!("write staged KF {}: {error}", destination.display()),
    })
}

fn audit_staged_family(
    jobs: &[&CreatureCorpusJob],
    publish_tree: &Path,
    records_tree: &Path,
) -> Result<(), String> {
    let mut expected_assets = BTreeSet::new();
    let mut expected_records = BTreeSet::new();
    for artifact in jobs.iter().flat_map(|job| &job.artifacts) {
        let destination = if artifact.kind.is_record() {
            expected_records.insert(artifact.target_path.to_ascii_lowercase());
            records_tree.join(path_from_canonical(&artifact.target_path))
        } else {
            expected_assets.insert(artifact.target_path.to_ascii_lowercase());
            publish_tree.join(path_from_canonical(&artifact.target_path))
        };
        let metadata = fs::metadata(&destination)
            .map_err(|error| format!("closure missing {}: {error}", destination.display()))?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(format!(
                "closure artifact is not a non-empty file: {}",
                destination.display()
            ));
        }
        if artifact.kind.is_hkx() && !has_extension(&artifact.target_path, "hkx") {
            return Err(format!(
                "HKX closure has wrong extension: {}",
                artifact.target_path
            ));
        }
        if artifact.kind.is_nif() && !has_extension(&artifact.target_path, "nif") {
            return Err(format!(
                "NIF closure has wrong extension: {}",
                artifact.target_path
            ));
        }
        if artifact.kind.is_record() && artifact.record_signature.is_none() {
            return Err(format!(
                "record closure has no signature: {}",
                artifact.target_path
            ));
        }
    }
    let actual_assets = recursive_relative_files(publish_tree)?;
    let actual_records = recursive_relative_files(records_tree)?;
    if actual_assets != expected_assets {
        return Err(format!(
            "asset closure mismatch: expected {expected_assets:?}, got {actual_assets:?}"
        ));
    }
    if actual_records != expected_records {
        return Err(format!(
            "record closure mismatch: expected {expected_records:?}, got {actual_records:?}"
        ));
    }
    Ok(())
}

fn publish_family(
    staged: &StagedFamily,
    mod_root: &Path,
    sink: Option<&crate::sinks::SinkSet>,
) -> Result<usize, String> {
    let destination = family_destination(staged, mod_root);
    if destination.exists() {
        return Err(format!(
            "family publish root already exists: {}",
            destination.display()
        ));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "family publish root has no parent".to_string())?;
    let prepared_sink = if let Some(sink) = sink {
        let registrations = staged_sink_registrations(staged)?;
        let borrowed = registrations
            .iter()
            .map(|(relative, path)| (relative.as_str(), path.as_path()))
            .collect::<Vec<_>>();
        Some(sink.prepare_existing_files_batch(&borrowed)?)
    } else {
        None
    };
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    fs::rename(&staged.staged_publish_root, &destination).map_err(|error| {
        format!(
            "promote {} to {}: {error}",
            staged.staged_publish_root.display(),
            destination.display()
        )
    })?;

    if let Some(prepared_sink) = prepared_sink {
        if let Err(error) = prepared_sink.commit() {
            let rollback = fs::rename(&destination, &staged.staged_publish_root);
            return Err(match rollback {
                Ok(()) => format!("commit prepared sink batch: {error}"),
                Err(rollback_error) => format!(
                    "commit prepared sink batch: {error}; rollback {} to {} failed: {rollback_error}",
                    destination.display(),
                    staged.staged_publish_root.display()
                ),
            });
        }
    }
    Ok(staged.assets.len())
}

fn publish_strict_batch(
    staged: &[(&str, &StagedFamily)],
    mod_root: &Path,
    sink: Option<&crate::sinks::SinkSet>,
) -> Result<(), StrictPublishFailure> {
    let first_family = staged
        .first()
        .map(|(family_id, _)| (*family_id).to_string())
        .unwrap_or_else(|| "empty-batch".to_string());
    let mut destinations = BTreeSet::new();
    for (family_id, family) in staged {
        let destination = family_destination(family, mod_root);
        let normalized = path_display(&destination).to_ascii_lowercase();
        if !destinations.insert(normalized) {
            return Err(StrictPublishFailure {
                family_id: (*family_id).to_string(),
                message: format!(
                    "strict batch contains duplicate publish root {}",
                    destination.display()
                ),
            });
        }
        if destination.exists() {
            return Err(StrictPublishFailure {
                family_id: (*family_id).to_string(),
                message: format!(
                    "family publish root already exists: {}",
                    destination.display()
                ),
            });
        }
    }

    let prepared_sink = if let Some(sink) = sink {
        let mut registrations = Vec::new();
        for (_, family) in staged {
            registrations.extend(staged_sink_registrations(family).map_err(|message| {
                StrictPublishFailure {
                    family_id: first_family.clone(),
                    message,
                }
            })?);
        }
        let borrowed = registrations
            .iter()
            .map(|(relative, path)| (relative.as_str(), path.as_path()))
            .collect::<Vec<_>>();
        Some(
            sink.prepare_existing_files_batch(&borrowed)
                .map_err(|message| StrictPublishFailure {
                    family_id: first_family.clone(),
                    message: format!("prepare sink batch: {message}"),
                })?,
        )
    } else {
        None
    };

    let mut promoted = Vec::<(&str, &StagedFamily, PathBuf)>::new();
    for (family_id, family) in staged {
        let destination = family_destination(family, mod_root);
        let result = destination
            .parent()
            .ok_or_else(|| "family publish root has no parent".to_string())
            .and_then(|parent| fs::create_dir_all(parent).map_err(|error| error.to_string()))
            .and_then(|_| {
                fs::rename(&family.staged_publish_root, &destination).map_err(|error| {
                    format!(
                        "promote {} to {}: {error}",
                        family.staged_publish_root.display(),
                        destination.display()
                    )
                })
            });
        if let Err(message) = result {
            let rollback_errors = rollback_promoted(&promoted);
            return Err(StrictPublishFailure {
                family_id: (*family_id).to_string(),
                message: append_rollback_errors(message, rollback_errors),
            });
        }
        promoted.push((family_id, family, destination));
    }

    if let Some(prepared_sink) = prepared_sink {
        if let Err(error) = prepared_sink.commit() {
            let rollback_errors = rollback_promoted(&promoted);
            return Err(StrictPublishFailure {
                family_id: first_family,
                message: append_rollback_errors(
                    format!("commit prepared sink batch: {error}"),
                    rollback_errors,
                ),
            });
        }
    }
    Ok(())
}

fn rollback_promoted(promoted: &[(&str, &StagedFamily, PathBuf)]) -> Vec<String> {
    let mut errors = Vec::new();
    for (_, family, destination) in promoted.iter().rev() {
        if let Err(error) = fs::rename(destination, &family.staged_publish_root) {
            errors.push(format!(
                "rollback {} to {}: {error}",
                destination.display(),
                family.staged_publish_root.display()
            ));
        }
    }
    errors
}

fn append_rollback_errors(message: String, rollback_errors: Vec<String>) -> String {
    if rollback_errors.is_empty() {
        message
    } else {
        format!("{message}; {}", rollback_errors.join("; "))
    }
}

fn family_destination(staged: &StagedFamily, mod_root: &Path) -> PathBuf {
    mod_root
        .join("data")
        .join(path_from_canonical(&staged.publish_root))
}

fn staged_sink_registrations(staged: &StagedFamily) -> Result<Vec<(String, PathBuf)>, String> {
    let publish_prefix = format!("{}/", staged.publish_root.trim_end_matches('/'));
    let mut registrations = Vec::with_capacity(staged.assets.len());
    for relative in &staged.assets {
        let suffix = relative.strip_prefix(&publish_prefix).ok_or_else(|| {
            format!(
                "asset {relative} is outside staged family root {}",
                staged.publish_root
            )
        })?;
        registrations.push((
            relative.clone(),
            staged.staged_publish_root.join(path_from_canonical(suffix)),
        ));
    }
    Ok(registrations)
}

fn recursive_relative_files(root: &Path) -> Result<BTreeSet<String>, String> {
    if !root.exists() {
        return Ok(BTreeSet::new());
    }
    let mut files = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|error| error.to_string())?;
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() {
                let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
                files.insert(path_display(relative).to_ascii_lowercase());
            } else {
                return Err(format!(
                    "closure contains non-file entry: {}",
                    path.display()
                ));
            }
        }
    }
    Ok(files)
}

fn normalize_execution_ledger(ledger: &mut BatchExecutionLedger) {
    ledger
        .candidates
        .sort_by_key(|candidate| candidate.source_key.to_ascii_lowercase());
    ledger
        .jobs
        .sort_by(|left, right| left.job_id.cmp(&right.job_id));
    ledger
        .families
        .sort_by(|left, right| left.family_id.cmp(&right.family_id));
    ledger.candidate_disposition_counts.clear();
    ledger.job_disposition_counts.clear();
    ledger.family_disposition_counts.clear();
    for candidate in &ledger.candidates {
        increment(
            &mut ledger.candidate_disposition_counts,
            disposition_name(candidate.disposition),
        );
    }
    for job in &ledger.jobs {
        increment(
            &mut ledger.job_disposition_counts,
            disposition_name(job.disposition),
        );
    }
    for family in &ledger.families {
        increment(
            &mut ledger.family_disposition_counts,
            disposition_name(family.disposition),
        );
    }
}

fn canonical_debug_dir(value: &str) -> Result<String, PhaseError> {
    let value = canonical_relative(value, "debug_dir")?;
    if value != "debug" && !value.starts_with("debug/") {
        return Err(PhaseError::BadParams(format!(
            "creature corpus output must be under debug/: {value}"
        )));
    }
    Ok(value)
}

fn canonical_debug_file(value: &str) -> Result<String, PhaseError> {
    let value = canonical_relative(value, "plan_path")?;
    if !value.starts_with("debug/") {
        return Err(PhaseError::BadParams(format!(
            "creature corpus plan must be under debug/: {value}"
        )));
    }
    Ok(value)
}

fn canonical_relative(value: &str, field: &str) -> Result<String, PhaseError> {
    if value.trim() != value || value.is_empty() || value.contains('\0') {
        return Err(PhaseError::BadParams(format!("invalid {field}: {value:?}")));
    }
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute()
        || path.components().any(|component| {
            !matches!(component, Component::Normal(_))
                || matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
        })
    {
        return Err(PhaseError::BadParams(format!(
            "{field} must be a safe relative path: {value:?}"
        )));
    }
    let components = normalized
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    if components.is_empty()
        || components
            .iter()
            .any(|component| *component == "." || *component == "..")
    {
        return Err(PhaseError::BadParams(format!(
            "{field} must be a canonical relative path: {value:?}"
        )));
    }
    Ok(components.join("/"))
}

fn canonical_record_signature(
    kind: ArtifactKind,
    signature: Option<&str>,
) -> Result<Option<String>, PhaseError> {
    if kind != ArtifactKind::Record {
        if signature.is_some() {
            return Err(PhaseError::BadParams(
                "only record artifacts may declare record_signature".to_string(),
            ));
        }
        return Ok(None);
    }
    let signature = signature
        .ok_or_else(|| PhaseError::BadParams("record artifact has no signature".to_string()))?
        .to_ascii_uppercase();
    if signature.len() != 4
        || !signature
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(PhaseError::BadParams(format!(
            "invalid record signature {signature:?}"
        )));
    }
    Ok(Some(signature))
}

fn validate_artifact_extension(kind: ArtifactKind, path: &str) -> Result<(), PhaseError> {
    if kind.is_hkx() && !has_extension(path, "hkx") {
        return Err(PhaseError::BadParams(format!(
            "{kind:?} artifact must end in .hkx: {path}"
        )));
    }
    if kind.is_nif() && !has_extension(path, "nif") {
        return Err(PhaseError::BadParams(format!(
            "{kind:?} artifact must end in .nif: {path}"
        )));
    }
    if kind == ArtifactKind::Texture && !has_extension(path, "dds") {
        return Err(PhaseError::BadParams(format!(
            "Texture artifact must end in .dds: {path}"
        )));
    }
    if kind == ArtifactKind::Material
        && !has_extension(path, "bgsm")
        && !has_extension(path, "bgem")
    {
        return Err(PhaseError::BadParams(format!(
            "Material artifact must end in .bgsm or .bgem: {path}"
        )));
    }
    Ok(())
}

fn stable_job_id(source_key: &str, family_id: &str, output_slug: &str) -> String {
    let digest = blake3::hash(format!("{source_key}|{family_id}|{output_slug}").as_bytes())
        .to_hex()
        .to_string();
    format!("{}-{}", safe_name(output_slug), &digest[..12])
}

fn stable_family_dir(family_id: &str) -> String {
    let digest = blake3::hash(family_id.as_bytes()).to_hex().to_string();
    format!("{}-{}", safe_name(family_id), &digest[..12])
}

fn stable_batch_id(plan: &CreatureCorpusExecutionPlan) -> String {
    let bytes = serde_json::to_vec(plan).unwrap_or_default();
    format!("batch-{}", &blake3::hash(&bytes).to_hex().to_string()[..16])
}

fn safe_name(value: &str) -> String {
    let value = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    value.trim_matches('-').to_string()
}

fn canonical_mesh_target(path: &str) -> String {
    format!("Meshes/{}", path.replace('\\', "/"))
}

fn artifact_key(artifact: &PlannedArtifact) -> String {
    artifact.target_path.to_ascii_lowercase()
}

fn is_descendant(path: &str, root: &str) -> bool {
    path.len() > root.len()
        && path
            .get(..root.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(root))
        && path.as_bytes().get(root.len()) == Some(&b'/')
}

fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

fn is_blake3_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn path_from_canonical(value: &str) -> PathBuf {
    value.split('/').collect()
}

fn path_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn capability_name(capability: CreatureCapability) -> String {
    serde_json::to_value(capability)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{capability:?}").to_ascii_lowercase())
}

fn disposition_name(disposition: TerminalDisposition) -> String {
    serde_json::to_value(disposition)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{disposition:?}").to_ascii_lowercase())
}

fn increment(counts: &mut BTreeMap<String, usize>, key: String) {
    *counts.entry(key).or_default() += 1;
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), PhaseError> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| PhaseError::Internal(error.to_string()))?;
    }
    fs::write(path, bytes).map_err(|error| PhaseError::Internal(error.to_string()))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, PhaseError> {
    let bytes = fs::read(path).map_err(|error| {
        PhaseError::BadParams(format!(
            "read creature corpus plan {}: {error}",
            path.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        PhaseError::BadParams(format!(
            "parse creature corpus plan {}: {error}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    const FNV_OFFICIAL_CREATURE_CENSUS_PLUGINS: &[&str] = &[
        "FalloutNV.esm",
        "DeadMoney.esm",
        "HonestHearts.esm",
        "OldWorldBlues.esm",
        "LonesomeRoad.esm",
        "GunRunnersArsenal.esm",
        "CaravanPack.esm",
        "ClassicPack.esm",
        "MercenaryPack.esm",
        "TribalPack.esm",
    ];
    const FO3_OFFICIAL_CREATURE_CENSUS_PLUGINS: &[&str] = &[
        "Fallout3.esm",
        "Anchorage.esm",
        "ThePitt.esm",
        "BrokenSteel.esm",
        "PointLookout.esm",
        "Zeta.esm",
    ];

    #[test]
    fn skyrim_transitive_race_dependency_is_not_a_primary_reservation() {
        let interner = StringInterner::new();
        let source = FormKey::parse("001234@Skyrim.esm", &interner).expect("source");
        let candidate_race =
            FormKey::parse("005678@Skyrim.esm", &interner).expect("candidate race");

        assert_eq!(
            skyrim_reservation_kind("RACE", source, candidate_race),
            None
        );
        assert_eq!(
            skyrim_reservation_kind("RACE", candidate_race, candidate_race),
            Some(SkyrimCreatureReservedRecordKind::Race)
        );
    }

    #[test]
    fn live_family_jobs_are_sorted_by_job_id() {
        let mut jobs = vec![
            fixture_job("family-a", "A", "Meshes/Actors/A", Vec::new()),
            fixture_job("family-z", "Z", "Meshes/Actors/Z", Vec::new()),
        ];
        jobs[0].job_id = "z-job".to_string();
        jobs[1].job_id = "a-job".to_string();

        sort_jobs_deterministically(&mut jobs);

        assert_eq!(jobs[0].job_id, "a-job");
        assert_eq!(jobs[1].job_id, "z-job");
    }

    #[test]
    fn shared_prepared_targets_coalesce_only_when_bytes_match() {
        let temp = tempfile::tempdir().expect("tempdir");
        let first = temp.path().join("first.nif");
        let identical = temp.path().join("identical.nif");
        let conflicting = temp.path().join("conflicting.nif");
        fs::write(&first, b"shared debris").expect("first");
        fs::write(&identical, b"shared debris").expect("identical");
        fs::write(&conflicting, b"different debris").expect("conflicting");
        let destination = temp.path().join("data/Meshes/Shared/Debris.nif");
        let mut registrations = BTreeMap::new();

        register_prepared_data_file(
            &mut registrations,
            "Meshes/Shared/Debris.nif".to_string(),
            first.clone(),
            destination.clone(),
            false,
        )
        .expect("first registration");
        register_prepared_data_file(
            &mut registrations,
            "meshes/shared/debris.NIF".to_string(),
            identical,
            destination.clone(),
            false,
        )
        .expect("identical shared registration");

        assert_eq!(registrations.len(), 1);
        assert_eq!(
            registrations.values().next().expect("registration").1,
            first
        );
        assert_eq!(
            register_prepared_data_file(
                &mut registrations,
                "Meshes/Shared/Debris.nif".to_string(),
                conflicting,
                destination,
                false,
            )
            .expect_err("conflicting shared registration"),
            "conflicting prepared data target Meshes/Shared/Debris.nif"
        );
    }

    #[test]
    fn existing_prepared_target_requires_explicit_overwrite() {
        let temp = tempfile::tempdir().expect("tempdir");
        let staged = temp.path().join("staged.hkx");
        let destination = temp.path().join("data/Meshes/Actors/Test/idle.hkx");
        fs::create_dir_all(destination.parent().unwrap()).expect("destination parent");
        fs::write(&staged, b"new animation").expect("staged");
        fs::write(&destination, b"old animation").expect("destination");

        let mut registrations = BTreeMap::new();
        let error = register_prepared_data_file(
            &mut registrations,
            "Meshes/Actors/Test/idle.hkx".to_string(),
            staged.clone(),
            destination.clone(),
            false,
        )
        .expect_err("overwrite must be explicit");
        assert!(error.contains("prepared data target already exists"));

        register_prepared_data_file(
            &mut registrations,
            "Meshes/Actors/Test/idle.hkx".to_string(),
            staged,
            destination,
            true,
        )
        .expect("explicit overwrite registration");
        assert_eq!(registrations.len(), 1);
    }

    #[test]
    fn dependency_owned_only_by_blocked_candidates_has_no_ready_owners() {
        let owners_by_record = BTreeMap::<String, BTreeSet<String>>::new();

        assert!(ready_owner_ids(&owners_by_record, &"00002e@skyrim.esm".to_string()).is_none());
    }

    #[test]
    fn ancillary_race_provenance_uses_the_source_winner() {
        let interner = StringInterner::new();
        let form_key = FormKey {
            local: 0x19,
            plugin: interner.intern("FalloutNV.esm"),
        };
        let first = Record::new(SigCode::from_str("RACE").unwrap(), form_key);
        let second = first.clone();
        let sources = [
            fnv_catalog::LegacyRecordSource {
                record: &first,
                provenance: fnv_catalog::CreatureProvenance {
                    game: fnv_catalog::LegacyCreatureGame::Fnv,
                    source_plugin: "FalloutNV.esm".to_string(),
                    precedence: 0,
                },
            },
            fnv_catalog::LegacyRecordSource {
                record: &second,
                provenance: fnv_catalog::CreatureProvenance {
                    game: fnv_catalog::LegacyCreatureGame::Fnv,
                    source_plugin: "WinningPatch.esm".to_string(),
                    precedence: 1,
                },
            },
        ];

        let winners = legacy_winner_provenance_by_source(&sources, "RACE", &interner).unwrap();

        assert_eq!(
            winners
                .get(&fnv_catalog::StableFormKey {
                    local: 0x19,
                    plugin: "FalloutNV.esm".to_string(),
                })
                .unwrap()
                .source_plugin,
            "WinningPatch.esm"
        );
    }

    #[test]
    fn legacy_record_loader_does_not_read_fo4_target_masters() {
        use crate::run::{
            OwnedPluginHandle, RunConfig, RunError, RunParams, create_run, drop_run, with_run,
        };

        let source = OwnedPluginHandle::new("FalloutNV.esm", "fnv");
        let target = OwnedPluginHandle::new("Converted.esm", "fo4");
        let target_master = OwnedPluginHandle::new("Fallout4.esm", "fo4");
        let source_id = source.id();
        let run_id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_id,
            target_handle_id: target.id(),
            master_handle_ids: vec![target_master.id()],
            config: RunConfig {
                output_plugin_name: "Converted.esm".to_string(),
                ..Default::default()
            },
        })
        .expect("create legacy conversion run");
        let temp = tempfile::tempdir().expect("temporary phase root");
        let params = serde_json::json!({});
        let cancel = AtomicBool::new(false);

        with_run(run_id, |run| {
            let ctx = PhaseCtx {
                run,
                mod_path: temp.path(),
                source_extracted_dir: temp.path(),
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            let loaded =
                load_live_legacy_plugin_records(&ctx).expect("load only the FNV conversion source");
            assert_eq!(loaded.len(), 1);
            assert_eq!(loaded[0].handle_id, source_id);
            Ok::<_, RunError>(())
        })
        .expect("inspect legacy source handles");

        drop_run(run_id).expect("drop legacy conversion run");
    }

    struct InjectFailure {
        fail_target: String,
    }

    #[derive(Default, Serialize)]
    struct LegacyPayloadRange {
        count: usize,
        min: Option<f64>,
        max: Option<f64>,
    }

    #[derive(Default, Serialize)]
    struct LegacyRecordShapeCensus {
        record_count: usize,
        field_signature_sets: BTreeMap<String, usize>,
        payload_shapes: BTreeMap<String, usize>,
        numeric_ranges: BTreeMap<String, LegacyPayloadRange>,
        dependency_locator_edges: Vec<String>,
    }

    #[derive(Serialize)]
    struct LegacyWeaponDnamCensus {
        length: usize,
        animation_type: Option<u32>,
        flags_1: Option<u8>,
        flags_2: Option<u32>,
        projectile_raw: Option<u32>,
    }

    #[derive(Serialize)]
    struct LegacyWeaponDetailCensus {
        game: String,
        source: String,
        etyp_values: Vec<String>,
        data_lengths: Vec<usize>,
        dnam: Vec<LegacyWeaponDnamCensus>,
        sound_fields: BTreeMap<String, Vec<String>>,
        model_fields: BTreeMap<String, Vec<String>>,
        optional_field_presence: BTreeMap<String, bool>,
        candidate_incidence: usize,
        candidate_sources: Vec<String>,
    }

    #[derive(Serialize)]
    struct LegacyRuntimeRecordDetailCensus {
        game: String,
        signature: String,
        source: String,
        field_values: BTreeMap<String, Vec<String>>,
        dependency_locator_edges: Vec<String>,
        candidate_incidence: usize,
        candidate_sources: Vec<String>,
    }

    #[derive(Default, Serialize)]
    struct LegacyBptdDecodedCensus {
        record_count: usize,
        candidate_owners_by_record: BTreeMap<String, Vec<String>>,
        part_count_distribution: BTreeMap<usize, usize>,
        field_value_counts: BTreeMap<String, BTreeMap<String, usize>>,
        field_numeric_ranges: BTreeMap<String, LegacyPayloadRange>,
        dependency_edges_by_signature: BTreeMap<String, Vec<String>>,
        nam1_content_counts: BTreeMap<String, usize>,
        nam4_content_counts: BTreeMap<String, usize>,
        nam5_rows: Vec<String>,
        raga_records: Vec<String>,
    }

    #[derive(Default, Serialize)]
    struct LegacyLvliDecodedCensus {
        record_count: usize,
        candidate_owners_by_record: BTreeMap<String, Vec<String>>,
        inbound_runtime_edges_by_source_signature: BTreeMap<String, Vec<String>>,
        entry_count: usize,
        lvlo_byte_value_counts: BTreeMap<String, BTreeMap<u8, usize>>,
        lvlo_target_signature_counts: BTreeMap<String, usize>,
        lvlo_resolution_counts: BTreeMap<String, usize>,
        coed_rows: Vec<String>,
    }

    #[derive(Default, Serialize)]
    struct LegacyIpdsSlotCensus {
        record_count: usize,
        record_sources: Vec<String>,
        inbound_runtime_edges_by_source_signature: BTreeMap<String, Vec<String>>,
        slot_reference_incidence: BTreeMap<String, usize>,
        slot_unique_targets: BTreeMap<String, usize>,
        slot_unique_candidate_owners: BTreeMap<String, usize>,
        slot_null_record_count: BTreeMap<String, usize>,
        exact_edges: Vec<String>,
    }

    #[derive(Serialize)]
    struct LegacyClosureDispositionCensus {
        unique_records: usize,
        unique_candidate_owners: usize,
    }

    fn legacy_value_shape(value: &FieldValue, interner: &StringInterner) -> String {
        match value {
            FieldValue::None => "none".to_string(),
            FieldValue::Bool(_) => "bool".to_string(),
            FieldValue::Int(_) => "int".to_string(),
            FieldValue::Uint(_) => "uint".to_string(),
            FieldValue::Float(_) => "float".to_string(),
            FieldValue::String(_) => "string".to_string(),
            FieldValue::Bytes(bytes) => format!("bytes[{}]", bytes.len()),
            FieldValue::FormKey(_) => "form_key".to_string(),
            FieldValue::List(values) => {
                let shapes = values
                    .iter()
                    .map(|value| legacy_value_shape(value, interner))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join("|");
                format!("list[{}]<{shapes}>", values.len())
            }
            FieldValue::Struct(fields) => {
                let fields = fields
                    .iter()
                    .map(|(name, value)| {
                        format!(
                            "{}:{}",
                            interner.resolve(*name).unwrap_or("<unresolved>"),
                            legacy_value_shape(value, interner)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("struct{{{fields}}}")
            }
        }
    }

    fn collect_legacy_numeric_ranges(
        value: &FieldValue,
        path: &str,
        interner: &StringInterner,
        ranges: &mut BTreeMap<String, LegacyPayloadRange>,
    ) {
        let numeric = match value {
            FieldValue::Int(value) => Some(("int", *value as f64)),
            FieldValue::Uint(value) => Some(("uint", *value as f64)),
            FieldValue::Float(value) if value.is_finite() => Some(("float", *value as f64)),
            _ => None,
        };
        if let Some((kind, value)) = numeric {
            let range = ranges.entry(format!("{path}:{kind}")).or_default();
            range.count += 1;
            range.min = Some(range.min.map_or(value, |current| current.min(value)));
            range.max = Some(range.max.map_or(value, |current| current.max(value)));
            return;
        }
        match value {
            FieldValue::List(values) => {
                for value in values {
                    collect_legacy_numeric_ranges(value, &format!("{path}[]"), interner, ranges);
                }
            }
            FieldValue::Struct(fields) => {
                for (name, value) in fields {
                    collect_legacy_numeric_ranges(
                        value,
                        &format!(
                            "{path}.{}",
                            interner.resolve(*name).unwrap_or("<unresolved>")
                        ),
                        interner,
                        ranges,
                    );
                }
            }
            _ => {}
        }
    }

    fn legacy_census_field_value(value: &FieldValue, interner: &StringInterner) -> String {
        match value {
            FieldValue::None => "none".to_string(),
            FieldValue::Bool(value) => format!("bool:{value}"),
            FieldValue::Int(value) => format!("int:{value}"),
            FieldValue::Uint(value) => format!("uint:{value}"),
            FieldValue::Float(value) => format!("float_bits:{:08x}", value.to_bits()),
            FieldValue::String(value) => format!(
                "string:{}",
                interner.resolve(*value).unwrap_or("<unresolved>")
            ),
            FieldValue::Bytes(bytes) => format!(
                "bytes:{}",
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            ),
            FieldValue::FormKey(form_key) => format!(
                "form:{:06X}@{}",
                form_key.local,
                interner.resolve(form_key.plugin).unwrap_or("<unresolved>")
            ),
            FieldValue::List(values) => format!(
                "list:[{}]",
                values
                    .iter()
                    .map(|value| legacy_census_field_value(value, interner))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            FieldValue::Struct(fields) => format!(
                "struct:{{{}}}",
                fields
                    .iter()
                    .map(|(name, value)| format!(
                        "{}={}",
                        interner.resolve(*name).unwrap_or("<unresolved>"),
                        legacy_census_field_value(value, interner)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }

    fn legacy_weapon_dnam_census(value: &FieldValue) -> Option<LegacyWeaponDnamCensus> {
        let FieldValue::Bytes(bytes) = value else {
            return None;
        };
        let read_u32 = |offset: usize| {
            bytes
                .get(offset..offset + 4)
                .and_then(|bytes| bytes.try_into().ok())
                .map(u32::from_le_bytes)
        };
        Some(LegacyWeaponDnamCensus {
            length: bytes.len(),
            animation_type: read_u32(0),
            flags_1: bytes.get(12).copied(),
            flags_2: read_u32(56),
            projectile_raw: read_u32(36),
        })
    }

    fn add_exact_numeric_value(
        census: &mut LegacyBptdDecodedCensus,
        field: &str,
        exact: String,
        numeric: Option<f64>,
    ) {
        *census
            .field_value_counts
            .entry(field.to_string())
            .or_default()
            .entry(exact)
            .or_default() += 1;
        let Some(numeric) = numeric.filter(|value| value.is_finite()) else {
            return;
        };
        let range = census
            .field_numeric_ranges
            .entry(field.to_string())
            .or_default();
        range.count += 1;
        range.min = Some(range.min.map_or(numeric, |current| current.min(numeric)));
        range.max = Some(range.max.map_or(numeric, |current| current.max(numeric)));
    }

    fn decode_legacy_bpnd_row(
        bytes: &[u8],
        census: &mut LegacyBptdDecodedCensus,
    ) -> Result<(), String> {
        if bytes.len() != 84 {
            return Err(format!("expected 84-byte BPND row, got {}", bytes.len()));
        }
        let u16_at = |offset| u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        let u32_at = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let i32_at = |offset| i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let float_at = |offset| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        for (field, offset) in [
            ("damage_mult", 0usize),
            ("tracking_max_angle", 20),
            ("explodable_debris_scale", 24),
            ("severable_debris_scale", 40),
            ("gore_position_x", 44),
            ("gore_position_y", 48),
            ("gore_position_z", 52),
            ("gore_rotation_x", 56),
            ("gore_rotation_y", 60),
            ("gore_rotation_z", 64),
            ("limb_replacement_scale", 80),
        ] {
            let value = float_at(offset);
            add_exact_numeric_value(
                census,
                field,
                format!("float_bits:{:08x}", value.to_bits()),
                Some(value as f64),
            );
        }
        for (field, offset) in [
            ("flags", 4usize),
            ("health_percent", 6),
            ("to_hit_chance", 8),
            ("explodable_explosion_chance", 9),
            ("severable_decal_count", 76),
            ("explodable_decal_count", 77),
            ("unknown_u8_26", 78),
            ("unknown_u8_27", 79),
        ] {
            add_exact_numeric_value(
                census,
                field,
                format!("uint8:{}", bytes[offset]),
                Some(bytes[offset] as f64),
            );
        }
        for (field, offset) in [("part_type", 5usize), ("actor_value", 7)] {
            let value = bytes[offset] as i8;
            add_exact_numeric_value(census, field, format!("int8:{value}"), Some(value as f64));
        }
        add_exact_numeric_value(
            census,
            "explodable_debris_count",
            format!("uint16:{}", u16_at(10)),
            Some(u16_at(10) as f64),
        );
        add_exact_numeric_value(
            census,
            "severable_debris_count",
            format!("int32:{}", i32_at(28)),
            Some(i32_at(28) as f64),
        );
        for (field, offset) in [
            ("explodable_debris", 12usize),
            ("explodable_explosion", 16),
            ("severable_debris", 32),
            ("severable_explosion", 36),
            ("severable_impact_dataset", 68),
            ("explodable_impact_dataset", 72),
        ] {
            let value = u32_at(offset);
            add_exact_numeric_value(
                census,
                field,
                format!("raw_formid:{value:08x}"),
                Some(value as f64),
            );
        }
        Ok(())
    }

    fn census_text_content(value: &FieldValue, interner: &StringInterner) -> &'static str {
        match value {
            FieldValue::String(value) => match interner.resolve(*value) {
                Some("") | None => "empty",
                Some(_) => "nonempty",
            },
            FieldValue::Bytes(bytes) if bytes.is_empty() || bytes.iter().all(|byte| *byte == 0) => {
                "empty"
            }
            FieldValue::Bytes(_) => "nonempty",
            _ => "unexpected_shape",
        }
    }

    fn legacy_census_form_key_for_raw(
        raw_form_id: u32,
        source_plugin: &str,
        source_master_names: &[String],
        interner: &StringInterner,
    ) -> Option<FormKey> {
        if raw_form_id == 0 {
            return None;
        }
        let load_order_index = (raw_form_id >> 24) as usize;
        let plugin = if load_order_index < source_master_names.len() {
            source_master_names.get(load_order_index)?.as_str()
        } else if load_order_index == source_master_names.len() {
            source_plugin
        } else {
            return None;
        };
        Some(FormKey {
            local: raw_form_id & 0x00ff_ffff,
            plugin: interner.intern(plugin),
        })
    }

    impl ArtifactStager for InjectFailure {
        fn stage(&self, source: &Path, destination: &Path) -> Result<(), String> {
            if path_display(destination).ends_with(&self.fail_target) {
                return Err("injected family failure".to_string());
            }
            FilesystemStager.stage(source, destination)
        }
    }

    #[test]
    fn evidence_bound_preparation_uses_workers_and_preserves_input_order() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .expect("rayon pool");
        let worker_indices = Arc::new(Mutex::new(BTreeSet::new()));
        let inputs = (0..512usize).collect::<Vec<_>>();
        let results = pool.install(|| {
            parallel_prepare_ordered(&inputs, |value| {
                worker_indices
                    .lock()
                    .expect("worker index lock")
                    .insert(rayon::current_thread_index().expect("rayon worker"));
                for _ in 0..256 {
                    std::hint::spin_loop();
                }
                Ok::<_, ()>(*value)
            })
        });

        assert_eq!(
            results.into_iter().collect::<Result<Vec<_>, _>>().unwrap(),
            inputs
        );
        assert!(worker_indices.lock().unwrap().len() > 1);
    }

    #[test]
    fn all_ready_evidence_preparation_is_parallel_deterministic_and_commits_exact_records() {
        let temp = tempfile::tempdir().expect("tempdir");
        let fixture_root = temp.path().join("live-builder-output");
        let (recipe, artifacts, closure) =
            crate::source_rig::tests::evidence_bound_execution_fixture(&fixture_root);
        let relative = |path: &Path| {
            path.strip_prefix(temp.path())
                .expect("fixture path below mod root")
                .to_string_lossy()
                .replace('\\', "/")
        };
        let recipe_path = fixture_root.join("recipe.json");
        fs::write(&recipe_path, recipe.canonical_json().unwrap()).expect("recipe");
        let closure_path = fixture_root.join("closure.json");
        fs::write(&closure_path, &closure.receipt_json).expect("closure receipt");
        let closure_receipt =
            nif_core_native::creature_closure::CreatureClosureReceipt::parse_and_validate(
                &closure.receipt_json,
            )
            .expect("closure receipt");
        let source_key = recipe.projection.source_primary_identity.stable_key();
        let family_id = recipe.batch_intent.family_id.clone();
        let publish_root = family_publish_root(&recipe);
        let planned_recipe = PlannedExecutableRecipe {
            source_key: source_key.clone(),
            family_id: family_id.clone(),
            publish_root: publish_root.clone(),
            recipe_source_root: ArtifactSourceRoot::Mod,
            recipe_path: relative(&recipe_path),
            recipe_blake3: recipe.stable_hash_blake3().unwrap(),
            creature_closure_receipt_source_root: ArtifactSourceRoot::Mod,
            creature_closure_receipt_path: relative(&closure_path),
            creature_closure_receipt_blake3: closure_receipt.receipt_hash,
            creature_closure_staged_data_source_root: ArtifactSourceRoot::Mod,
            creature_closure_staged_data_root: relative(&closure.staged_data_root),
            graph_paths: vec![
                recipe.rig.paths.project.clone(),
                recipe.rig.paths.character.clone(),
                recipe.rig.paths.root_behavior.clone(),
                recipe.rig.paths.core_behavior.clone(),
            ],
            converted_artifacts: artifacts
                .iter()
                .map(|artifact| PlannedConvertedArtifact {
                    runtime_path: artifact.receipt.runtime_path.clone(),
                    source_root: ArtifactSourceRoot::Mod,
                    source_path: relative(&artifact.path),
                    source_blake3: artifact.receipt.blake3.clone(),
                })
                .collect(),
        };
        let job = CreatureCorpusJob {
            job_id: stable_job_id(&source_key, &family_id, "all-ready"),
            source_key,
            output_slug: "all-ready".to_string(),
            family_id,
            adapter: CreatureAdapterProfile::EvidenceBoundRecipe,
            unsupported_capability: None,
            publish_root,
            graph_paths: planned_recipe.graph_paths.clone(),
            artifacts: Vec::new(),
            executable_recipe: Some(planned_recipe),
            member_source_keys: Vec::new(),
            candidate_recipes: Vec::new(),
            family_bundle_blake3: None,
            preflight_issues: Vec::new(),
        };
        let roots = SourceRoots {
            mod_root: temp.path(),
            source_extracted: temp.path(),
            target_extracted: None,
            target_data: None,
        };
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .expect("rayon pool");
        let worker_indices = Arc::new(Mutex::new(BTreeSet::new()));
        let inputs = (0..4usize)
            .map(|index| (index, temp.path().join(format!("worker-{index}"))))
            .collect::<Vec<_>>();
        let interner = StringInterner::new();
        let prepared = pool.install(|| {
            parallel_prepare_ordered(&inputs, |(index, staging)| {
                worker_indices
                    .lock()
                    .expect("worker lock")
                    .insert(rayon::current_thread_index().expect("rayon worker"));
                std::thread::sleep(std::time::Duration::from_millis(25));
                prepare_evidence_bound_family(&job, staging, &roots, &interner)
                    .map(|prepared| (*index, prepared))
            })
        });
        let prepared = prepared
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .expect("all families ready");
        assert_eq!(
            prepared.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert!(worker_indices.lock().unwrap().len() > 1);
        let first_receipt = &prepared[0].1.execution.receipt;
        assert!(prepared.iter().all(
            |(_, family)| family.execution.receipt.recipe_blake3 == first_receipt.recipe_blake3
        ));
        let packed_scaffold_paths = first_receipt
            .scaffold_artifacts
            .iter()
            .map(|artifact| artifact.runtime_path.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            packed_scaffold_paths,
            [
                recipe.rig.paths.project.to_ascii_lowercase(),
                recipe.rig.paths.character.to_ascii_lowercase(),
                recipe.rig.paths.root_behavior.to_ascii_lowercase(),
                recipe.rig.paths.core_behavior.to_ascii_lowercase(),
            ]
            .into_iter()
            .collect()
        );

        let recipe = prepared[0].1.candidate_recipes[0].clone();
        validate_family_actor_action_ownership(std::slice::from_ref(&recipe))
            .expect("single complete Actor Action owner");
        let family = recipe
            .rebuild_record_family_batch(&interner)
            .expect("record family");
        let emitted_signatures = family.closures[0]
            .closure
            .records
            .iter()
            .map(|record| record.sig.as_str().to_string())
            .collect::<Vec<_>>();
        assert_eq!(recipe.actor_action_records.len(), 5);
        assert_eq!(emitted_signatures.len(), 11);
        assert_eq!(
            emitted_signatures
                .iter()
                .filter(|signature| signature.as_str() == "WEAP")
                .count(),
            1
        );
        assert_eq!(
            emitted_signatures
                .iter()
                .filter(|signature| signature.as_str() == "IDLE")
                .count(),
            recipe.actor_action_records.len()
        );
        assert!(
            !emitted_signatures
                .iter()
                .any(|signature| signature == "SPEL")
        );
        let race = family.closures[0]
            .closure
            .records
            .iter()
            .find(|record| record.sig.as_str() == "RACE")
            .expect("generated RACE");
        let race_strings = |signature: &str| {
            race.fields
                .iter()
                .filter(|field| field.sig.as_str() == signature)
                .filter_map(|field| match field.value {
                    FieldValue::String(value) => interner.resolve(value),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert!(
            race_strings("MODL")
                .iter()
                .any(|path| path.eq_ignore_ascii_case(&recipe.rig.paths.project))
        );
        let subgraphs =
            crate::fixups::havok::anim_text_data_emit::subgraphs_from_race_record(race, &interner);
        assert_eq!(subgraphs.len(), 1);
        assert_eq!(subgraphs[0].core_behavior, recipe.rig.paths.core_behavior);
        assert!(
            family.closures[0]
                .required_target_records
                .iter()
                .any(|dependency| dependency.signature == "AACT")
        );
        let emitted_keys = family.closures[0]
            .closure
            .records
            .iter()
            .map(|record| record.form_key.local)
            .collect::<BTreeSet<_>>();
        let npc = family.closures[0]
            .closure
            .records
            .iter()
            .find(|record| record.sig.as_str() == "NPC_")
            .expect("generated NPC");
        assert!(npc.fields.iter().any(|field| {
            field.sig.as_str() == "WNAM"
                && matches!(field.value, FieldValue::FormKey(form_key) if emitted_keys.contains(&form_key.local))
        }));
        assert!(
            !emitted_signatures
                .iter()
                .any(|signature| signature == "PROJ")
        );

        let target_plugin = recipe.projection.base.target_plugin.clone();
        let target_handle = esp_authoring_core::plugin_runtime::plugin_handle_new_native(
            &target_plugin,
            Some("fo4"),
        )
        .expect("target plugin");
        esp_authoring_core::plugin_runtime::plugin_handle_add_master_native(
            target_handle,
            "Fallout4.esm",
            None,
        )
        .expect("Fallout4 master");
        let requested_unrelated = format!("00F000:{target_plugin}");
        let unrelated =
            esp_authoring_core::plugin_runtime::plugin_handle_replace_authoring_record_value(
                target_handle,
                &serde_json::json!({
                    "signature": "NPC_",
                    "form_id": requested_unrelated,
                    "eid": "UnrelatedSameSignature",
                    "fields": [{ "EDID": "UnrelatedSameSignature" }]
                }),
            )
            .expect("unrelated NPC");
        let mut mapper = crate::formkey_mapper::MapperState::new(
            std::iter::empty(),
            crate::formkey_mapper::MapperOptions {
                output_plugin_name: target_plugin.clone(),
                ..Default::default()
            },
        );
        let schema = crate::schema::AuthoringSchema::for_game("fo4").expect("FO4 schema");
        let mut session = crate::session::open_session(target_handle, None).expect("session");
        let prepared_records = crate::source_rig::prepare_creature_record_families_batch(
            &mut session,
            &mut mapper,
            vec![family],
            &schema,
            &interner,
        )
        .expect("prepare exact record family");
        let prepared_commit = crate::source_rig::prepare_creature_record_commit_batch(
            prepared_records,
            target_plugin,
            "fo4",
            std::slice::from_ref(&recipe),
            &schema,
            &interner,
        )
        .expect("bind recipe to record batch");
        let committed = prepared_commit.commit();
        assert_eq!(committed.receipt().mapping_count, 1);
        assert_eq!(committed.receipt().record_count, emitted_signatures.len());
        assert!(
            session
                .record_exists_in_handle(target_handle, &unrelated)
                .expect("read unrelated NPC")
        );
        drop(session);

        let anim_text_input = temp.path().join("animtext-input");
        for (relative, staged) in &prepared[0].1.data_files {
            let normalized = relative.replace('\\', "/");
            let Some(runtime) = normalized.strip_prefix("Meshes/") else {
                continue;
            };
            let destination = anim_text_input.join(path_from_canonical(runtime));
            fs::create_dir_all(destination.parent().expect("AnimTextData input parent")).unwrap();
            fs::copy(staged, destination).expect("copy packed live-builder artifact");
        }
        let anim_text_output = temp.path().join("animtext-output");
        let written =
            crate::fixups::havok::anim_text_data_emit::generate_anim_text_data_for_handle(
                target_handle,
                &anim_text_input,
                &anim_text_output,
                None,
                Some("B21_"),
            )
            .expect("generated AnimTextData discovery");
        assert!(written > 0);
        assert!(
            anim_text_output
                .join("AnimTextData")
                .join("AnimationFileData")
                .is_dir()
        );
    }

    #[test]
    fn evidence_bound_failure_removes_private_staging() {
        let temp = tempfile::tempdir().expect("tempdir");
        let staging_parent = temp.path().join("recipe_staging").join("batch");
        fs::create_dir_all(&staging_parent).expect("staging directory");
        fs::write(staging_parent.join("partial.hkx"), b"partial").expect("partial artifact");
        let corpus = CreatureCorpusPlan {
            version: 1,
            candidate_count: 0,
            planned: Vec::new(),
            rejected: Vec::new(),
        };
        let plan = CreatureCorpusExecutionPlan {
            version: PLAN_VERSION,
            debug_dir: DEFAULT_DEBUG_DIR.to_string(),
            native_catalog: fixture_bridge_ledger(&corpus).expect("fixture bridge"),
            corpus,
            jobs: vec![fixture_job(
                "failed-family",
                "Failed",
                "Meshes/Actors/Failed",
                Vec::new(),
            )],
        };

        let ledger = failed_evidence_bound_ledger_with_cleanup(
            &plan,
            "failed-family",
            "injected failure".to_string(),
            &staging_parent,
        );

        assert!(ledger.strict_aborted);
        assert_eq!(ledger.families[0].disposition, TerminalDisposition::Failed);
        assert!(!staging_parent.exists());
    }

    #[test]
    #[ignore = "requires explicit official FNV and FO3 Data directories"]
    fn live_official_fnv_fo3_dependency_census() {
        #[derive(Clone)]
        struct LoadedRecords {
            records: Vec<Record>,
            provenance: fnv_catalog::CreatureProvenance,
            source_master_names: Vec<String>,
        }

        let fnv_data = PathBuf::from(
            std::env::var("BACUP_CENSUS_FNV_DATA_DIR")
                .expect("BACUP_CENSUS_FNV_DATA_DIR must name the official FNV Data directory"),
        );
        let fo3_data = PathBuf::from(
            std::env::var("BACUP_CENSUS_FO3_DATA_DIR")
                .expect("BACUP_CENSUS_FO3_DATA_DIR must name the official FO3 Data directory"),
        );
        let interner = StringInterner::new();
        let mut loaded = Vec::new();
        for (game, data_dir, plugins) in [
            (
                fnv_catalog::LegacyCreatureGame::Fnv,
                fnv_data,
                FNV_OFFICIAL_CREATURE_CENSUS_PLUGINS,
            ),
            (
                fnv_catalog::LegacyCreatureGame::Fo3,
                fo3_data,
                FO3_OFFICIAL_CREATURE_CENSUS_PLUGINS,
            ),
        ] {
            let schema = crate::schema::AuthoringSchema::for_game(match game {
                fnv_catalog::LegacyCreatureGame::Fnv => "fnv",
                fnv_catalog::LegacyCreatureGame::Fo3 => "fo3",
            })
            .expect("source schema");
            for plugin in plugins {
                let path = data_dir.join(plugin);
                assert!(
                    path.is_file(),
                    "official source is missing: {}",
                    path.display()
                );
                let handle = crate::merge_sources::load_no_py(
                    path.to_str().expect("Unicode plugin path"),
                    Some(match game {
                        fnv_catalog::LegacyCreatureGame::Fnv => "fnv",
                        fnv_catalog::LegacyCreatureGame::Fo3 => "fo3",
                    }),
                )
                .unwrap_or_else(|error| panic!("load {}: {error}", path.display()));
                let mut records =
                    read_legacy_dependency_records_from_handle(handle, &schema, &interner)
                        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
                let detailed_impact_records =
                    read_records_from_handle(handle, &schema, &interner, &["IPCT", "TXST"])
                        .unwrap_or_else(|error| {
                            panic!("read impact records {}: {error}", path.display())
                        });
                let detailed_impact_keys = detailed_impact_records
                    .iter()
                    .map(|record| record.form_key)
                    .collect::<HashSet<_>>();
                records.retain(|record| !detailed_impact_keys.contains(&record.form_key));
                records.extend(detailed_impact_records);
                records.sort_by(|left, right| {
                    let left_plugin = interner.resolve(left.form_key.plugin).unwrap_or_default();
                    let right_plugin = interner.resolve(right.form_key.plugin).unwrap_or_default();
                    left_plugin
                        .to_ascii_lowercase()
                        .cmp(&right_plugin.to_ascii_lowercase())
                        .then_with(|| left.form_key.local.cmp(&right.form_key.local))
                        .then_with(|| left.sig.cmp(&right.sig))
                });
                let source_master_names =
                    esp_authoring_core::plugin_runtime::plugin_handle_master_names_no_py(handle)
                        .unwrap_or_else(|error| panic!("read masters {}: {error}", path.display()));
                assert!(
                    esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle),
                    "close {}",
                    path.display()
                );
                loaded.push(LoadedRecords {
                    records,
                    provenance: fnv_catalog::CreatureProvenance {
                        game,
                        source_plugin: (*plugin).to_string(),
                        precedence: loaded.len() as u32,
                    },
                    source_master_names,
                });
            }
        }
        let sources = loaded
            .iter()
            .flat_map(|input| {
                input
                    .records
                    .iter()
                    .map(|record| fnv_catalog::LegacyRecordSource {
                        record,
                        provenance: input.provenance.clone(),
                    })
            })
            .collect::<Vec<_>>();
        let source_load_orders = loaded
            .iter()
            .map(|input| fnv_dependencies::LegacyPluginLoadOrder {
                game: input.provenance.game,
                source_plugin: input.provenance.source_plugin.clone(),
                source_master_names: input.source_master_names.clone(),
            })
            .collect::<Vec<_>>();
        let source_master_names_by_plugin = source_load_orders
            .iter()
            .map(|load_order| {
                (
                    load_order.source_plugin.to_ascii_lowercase(),
                    load_order.source_master_names.as_slice(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut winners = BTreeMap::<(String, u32), &fnv_catalog::LegacyRecordSource<'_>>::new();
        for source in &sources {
            let plugin = interner
                .resolve(source.record.form_key.plugin)
                .expect("resolved source plugin")
                .to_ascii_lowercase();
            let key = (plugin, source.record.form_key.local);
            if winners.get(&key).is_none_or(|existing| {
                source.provenance.precedence > existing.provenance.precedence
            }) {
                winners.insert(key, source);
            }
        }
        let creature_winners = winners
            .values()
            .filter(|source| source.record.sig.as_str() == "CREA")
            .copied()
            .collect::<Vec<_>>();
        let mut creature_winners_by_plugin = BTreeMap::new();
        let mut creature_winner_flag_counts = BTreeMap::new();
        for source in &creature_winners {
            *creature_winners_by_plugin
                .entry(source.provenance.source_plugin.clone())
                .or_insert(0usize) += 1;
            for (name, flag) in [
                ("deleted", crate::record::RecordFlags::DELETED),
                (
                    "initially_disabled",
                    crate::record::RecordFlags::INITIALLY_DISABLED,
                ),
                ("ignored", crate::record::RecordFlags::IGNORED),
            ] {
                if source.record.flags.contains(flag) {
                    *creature_winner_flag_counts
                        .entry(name.to_string())
                        .or_insert(0usize) += 1;
                }
            }
        }
        let ledger = fnv_dependencies::build_creature_dependency_ledger_with_load_orders(
            &sources,
            fnv_dependencies::CreatureDependencyOptions {
                expected_creature_winners: Some(fnv_catalog::EXPECTED_FULL_SOURCE_CREA_WINNERS),
            },
            &source_load_orders,
            &interner,
        )
        .expect("full official dependency census");
        let mut blocker_counts = BTreeMap::new();
        for candidate in &ledger.candidates {
            for blocker in &candidate.blockers {
                *blocker_counts
                    .entry(legacy_dependency_blocker_code(blocker))
                    .or_insert(0usize) += 1;
            }
        }
        let mut candidate_owners_by_record = BTreeMap::<(String, u32), BTreeSet<String>>::new();
        for candidate in &ledger.candidates {
            for dependency in &candidate.closure_form_keys {
                candidate_owners_by_record
                    .entry((dependency.plugin.to_ascii_lowercase(), dependency.local))
                    .or_default()
                    .insert(candidate.source.to_string());
            }
        }
        let mut closure_disposition_sets =
            BTreeMap::<String, (BTreeSet<String>, BTreeSet<String>)>::new();
        for receipt in &ledger.records {
            let keys = match &receipt.lowering {
                fnv_dependencies::PairRecordLoweringDisposition::Ready { mechanism } => {
                    vec![format!("{}:ready:{mechanism}", receipt.signature)]
                }
                fnv_dependencies::PairRecordLoweringDisposition::Consumed { mechanism } => {
                    vec![format!("{}:consumed:{mechanism}", receipt.signature)]
                }
                fnv_dependencies::PairRecordLoweringDisposition::Blocked { reason_codes } => {
                    reason_codes
                        .iter()
                        .map(|reason| format!("{}:blocked:{reason}", receipt.signature))
                        .collect()
                }
            };
            let owners = candidate_owners_by_record
                .get(&(
                    receipt.source.plugin.to_ascii_lowercase(),
                    receipt.source.local,
                ))
                .cloned()
                .unwrap_or_default();
            for key in keys {
                let (records, candidate_owners) = closure_disposition_sets.entry(key).or_default();
                records.insert(receipt.source.to_string());
                candidate_owners.extend(owners.iter().cloned());
            }
        }
        let closure_dispositions = closure_disposition_sets
            .into_iter()
            .map(|(key, (records, candidate_owners))| {
                (
                    key,
                    LegacyClosureDispositionCensus {
                        unique_records: records.len(),
                        unique_candidate_owners: candidate_owners.len(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let weapon_keys = ledger
            .records
            .iter()
            .filter(|record| record.signature == "WEAP")
            .map(|record| {
                (
                    record.source.plugin.to_ascii_lowercase(),
                    record.source.local,
                )
            })
            .collect::<BTreeSet<_>>();
        let mut weapon_candidate_sources = BTreeMap::<(String, u32), BTreeSet<String>>::new();
        for candidate in &ledger.candidates {
            for dependency in &candidate.closure_form_keys {
                let key = (dependency.plugin.to_ascii_lowercase(), dependency.local);
                if weapon_keys.contains(&key) {
                    weapon_candidate_sources
                        .entry(key)
                        .or_default()
                        .insert(candidate.source.to_string());
                }
            }
        }
        let mut record_shapes = BTreeMap::<String, LegacyRecordShapeCensus>::new();
        let mut weapon_details = BTreeMap::<String, LegacyWeaponDetailCensus>::new();
        let mut runtime_record_details = BTreeMap::<String, LegacyRuntimeRecordDetailCensus>::new();
        let mut bptd_decoded = LegacyBptdDecodedCensus::default();
        let mut lvli_decoded = LegacyLvliDecodedCensus::default();
        let mut ipds_slots = LegacyIpdsSlotCensus::default();
        let mut ipds_slot_targets = BTreeMap::<String, BTreeSet<String>>::new();
        let mut ipds_slot_candidate_owners = BTreeMap::<String, BTreeSet<String>>::new();
        for receipt in ledger.records.iter().filter(|receipt| {
            matches!(
                receipt.signature.as_str(),
                "WEAP"
                    | "AMMO"
                    | "DEBR"
                    | "EXPL"
                    | "FACT"
                    | "GLOB"
                    | "HAIR"
                    | "IMOD"
                    | "KEYM"
                    | "LIGH"
                    | "MISC"
                    | "NOTE"
                    | "NPC_"
                    | "SOUN"
                    | "STAT"
                    | "CSTY"
                    | "BPTD"
                    | "LVLI"
                    | "FLST"
                    | "IPCT"
                    | "IPDS"
                    | "TXST"
                    | "IDLE"
                    | "PACK"
            )
        }) {
            let winner = winners
                .get(&(
                    receipt.source.plugin.to_ascii_lowercase(),
                    receipt.source.local,
                ))
                .expect("dependency receipt has a winning source record");
            let game = match receipt.provenance.game {
                fnv_catalog::LegacyCreatureGame::Fnv => "fnv",
                fnv_catalog::LegacyCreatureGame::Fo3 => "fo3",
            };
            let group = record_shapes
                .entry(format!("{game}:{}", receipt.signature))
                .or_default();
            group.record_count += 1;
            let field_set = winner
                .record
                .fields
                .iter()
                .map(|field| field.sig.as_str().to_string())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(",");
            *group.field_signature_sets.entry(field_set).or_default() += 1;
            for field in &winner.record.fields {
                let path = format!("{}.{}", receipt.signature, field.sig.as_str());
                *group
                    .payload_shapes
                    .entry(format!(
                        "{path}:{}",
                        legacy_value_shape(&field.value, &interner)
                    ))
                    .or_default() += 1;
                collect_legacy_numeric_ranges(
                    &field.value,
                    &path,
                    &interner,
                    &mut group.numeric_ranges,
                );
            }
            group
                .dependency_locator_edges
                .extend(receipt.references.iter().map(|reference| {
                    serde_json::to_string(&serde_json::json!({
                        "source": &receipt.source,
                        "target": &reference.target,
                        "source_locators": &reference.source_locators,
                        "resolution": &reference.resolution,
                    }))
                    .expect("reference edge json")
                }));

            let candidate_sources = candidate_owners_by_record
                .get(&(
                    receipt.source.plugin.to_ascii_lowercase(),
                    receipt.source.local,
                ))
                .map(|sources| sources.iter().cloned().collect::<Vec<_>>())
                .unwrap_or_default();

            if receipt.signature == "BPTD" {
                bptd_decoded.record_count += 1;
                bptd_decoded
                    .candidate_owners_by_record
                    .insert(receipt.source.to_string(), candidate_sources.clone());
                let part_count = winner
                    .record
                    .fields
                    .iter()
                    .filter(|field| field.sig.as_str() == "BPND")
                    .count();
                *bptd_decoded
                    .part_count_distribution
                    .entry(part_count)
                    .or_default() += 1;
                let mut nam5_index = 0usize;
                let mut bpnd_index = 0usize;
                let source_masters = source_master_names_by_plugin
                    .get(&receipt.provenance.source_plugin.to_ascii_lowercase())
                    .copied()
                    .expect("BPTD source master list");
                for field in &winner.record.fields {
                    match (field.sig.as_str(), &field.value) {
                        ("BPND", FieldValue::Bytes(bytes)) => {
                            decode_legacy_bpnd_row(bytes, &mut bptd_decoded).unwrap_or_else(
                                |error| panic!("decode BPND {}: {error}", receipt.source),
                            );
                            for (field_name, offset) in [
                                ("explodable_debris", 12usize),
                                ("explodable_explosion", 16),
                                ("severable_debris", 32),
                                ("severable_explosion", 36),
                                ("severable_impact_dataset", 68),
                                ("explodable_impact_dataset", 72),
                            ] {
                                let raw = u32::from_le_bytes(
                                    bytes[offset..offset + 4].try_into().unwrap(),
                                );
                                if raw == 0 {
                                    continue;
                                }
                                let target = legacy_census_form_key_for_raw(
                                    raw,
                                    &receipt.provenance.source_plugin,
                                    source_masters,
                                    &interner,
                                );
                                let signature = target
                                    .and_then(|key| {
                                        winners
                                            .get(&(
                                                interner
                                                    .resolve(key.plugin)
                                                    .unwrap_or_default()
                                                    .to_ascii_lowercase(),
                                                key.local,
                                            ))
                                            .map(|source| source.record.sig.as_str())
                                    })
                                    .unwrap_or("missing");
                                bptd_decoded
                                    .dependency_edges_by_signature
                                    .entry(signature.to_string())
                                    .or_default()
                                    .push(
                                        serde_json::to_string(&serde_json::json!({
                                            "source": &receipt.source,
                                            "source_locator": format!(
                                                "BPND[{bpnd_index}].{field_name}@{offset}"
                                            ),
                                            "raw_formid": format!("{raw:08x}"),
                                            "target": target.map(|key| stable_form_key_string(key, &interner).expect("BPND target key")),
                                            "target_signature": signature,
                                        }))
                                        .expect("BPTD embedded dependency edge json"),
                                    );
                            }
                            bpnd_index += 1;
                        }
                        ("NAM1", value) => {
                            *bptd_decoded
                                .nam1_content_counts
                                .entry(census_text_content(value, &interner).to_string())
                                .or_default() += 1;
                        }
                        ("NAM4", value) => {
                            *bptd_decoded
                                .nam4_content_counts
                                .entry(census_text_content(value, &interner).to_string())
                                .or_default() += 1;
                        }
                        ("NAM5", FieldValue::Bytes(bytes)) => {
                            bptd_decoded.nam5_rows.push(
                                serde_json::to_string(&serde_json::json!({
                                    "source": &receipt.source,
                                    "index": nam5_index,
                                    "length": bytes.len(),
                                    "blake3": blake3::hash(bytes).to_hex().to_string(),
                                    "all_zero": bytes.iter().all(|byte| *byte == 0),
                                }))
                                .expect("BPTD NAM5 row json"),
                            );
                            nam5_index += 1;
                        }
                        ("RAGA", _) => {
                            bptd_decoded.raga_records.push(receipt.source.to_string());
                        }
                        _ => {}
                    }
                }
                for reference in &receipt.references {
                    let signature = match &reference.resolution {
                        fnv_dependencies::DependencyReferenceResolution::Followed { signature }
                        | fnv_dependencies::DependencyReferenceResolution::OutOfRuntimeScope {
                            signature,
                        }
                        | fnv_dependencies::DependencyReferenceResolution::TargetIntrinsic {
                            signature,
                            ..
                        } => signature.as_str(),
                        fnv_dependencies::DependencyReferenceResolution::MissingSourceRecord => {
                            "missing"
                        }
                    };
                    if !reference
                        .source_locators
                        .iter()
                        .any(|locator| locator.starts_with("BPND["))
                    {
                        continue;
                    }
                    bptd_decoded
                        .dependency_edges_by_signature
                        .entry(signature.to_string())
                        .or_default()
                        .push(
                            serde_json::to_string(&serde_json::json!({
                                "source": &receipt.source,
                                "target": &reference.target,
                                "source_locators": &reference.source_locators,
                                "resolution": &reference.resolution,
                            }))
                            .expect("BPTD dependency edge json"),
                        );
                }
            }

            if receipt.signature == "LVLI" {
                lvli_decoded.record_count += 1;
                lvli_decoded
                    .candidate_owners_by_record
                    .insert(receipt.source.to_string(), candidate_sources.clone());
                let mut lvlo_index = 0usize;
                let source_masters = source_master_names_by_plugin
                    .get(&receipt.provenance.source_plugin.to_ascii_lowercase())
                    .copied()
                    .expect("LVLI source master list");
                for field in &winner.record.fields {
                    match (field.sig.as_str(), &field.value) {
                        ("LVLO", FieldValue::Bytes(bytes)) if bytes.len() == 12 => {
                            lvli_decoded.entry_count += 1;
                            for (name, offset) in [
                                ("byte2_unknown_u8_1", 2usize),
                                ("byte3_unknown_u8_2", 3),
                                ("byte10_source_unknown_target_chance_none", 10),
                                ("byte11_unknown_u8_6", 11),
                            ] {
                                *lvli_decoded
                                    .lvlo_byte_value_counts
                                    .entry(name.to_string())
                                    .or_default()
                                    .entry(bytes[offset])
                                    .or_default() += 1;
                            }
                            lvlo_index += 1;
                        }
                        ("COED", FieldValue::Bytes(bytes)) if bytes.len() == 12 => {
                            let owner_raw = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
                            let union_raw = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
                            let condition_bits =
                                u32::from_le_bytes(bytes[8..12].try_into().unwrap());
                            let owner_key = legacy_census_form_key_for_raw(
                                owner_raw,
                                &receipt.provenance.source_plugin,
                                source_masters,
                                &interner,
                            );
                            let owner_signature = owner_key
                                .and_then(|key| {
                                    winners
                                        .get(&(
                                            interner
                                                .resolve(key.plugin)
                                                .unwrap_or_default()
                                                .to_ascii_lowercase(),
                                            key.local,
                                        ))
                                        .map(|source| source.record.sig.as_str().to_string())
                                })
                                .unwrap_or_else(|| {
                                    if owner_raw == 0 {
                                        "null".to_string()
                                    } else {
                                        "missing".to_string()
                                    }
                                });
                            let union = if owner_signature == "FACT" {
                                format!("required_rank:{}", union_raw as i32)
                            } else if union_raw == 0 {
                                "global:null".to_string()
                            } else {
                                let global = legacy_census_form_key_for_raw(
                                    union_raw,
                                    &receipt.provenance.source_plugin,
                                    source_masters,
                                    &interner,
                                );
                                let global_signature = global
                                    .and_then(|key| {
                                        winners
                                            .get(&(
                                                interner
                                                    .resolve(key.plugin)
                                                    .unwrap_or_default()
                                                    .to_ascii_lowercase(),
                                                key.local,
                                            ))
                                            .map(|source| source.record.sig.as_str())
                                    })
                                    .unwrap_or("missing");
                                format!("global:{union_raw:08x}:{global_signature}")
                            };
                            lvli_decoded.coed_rows.push(
                                serde_json::to_string(&serde_json::json!({
                                    "source": &receipt.source,
                                    "preceding_lvlo_index": lvlo_index.checked_sub(1),
                                    "owner_raw": format!("{owner_raw:08x}"),
                                    "owner_signature": owner_signature,
                                    "union_raw": format!("{union_raw:08x}"),
                                    "union_interpretation": union,
                                    "condition_bits": format!("{condition_bits:08x}"),
                                    "condition": f32::from_bits(condition_bits),
                                }))
                                .expect("LVLI COED row json"),
                            );
                        }
                        _ => {}
                    }
                }
                for reference in &receipt.references {
                    let resolution = match &reference.resolution {
                        fnv_dependencies::DependencyReferenceResolution::Followed { signature } => {
                            *lvli_decoded
                                .lvlo_target_signature_counts
                                .entry(signature.clone())
                                .or_default() += reference.source_locators.len();
                            format!("followed:{signature}")
                        }
                        fnv_dependencies::DependencyReferenceResolution::OutOfRuntimeScope {
                            signature,
                        } => {
                            *lvli_decoded
                                .lvlo_target_signature_counts
                                .entry(signature.clone())
                                .or_default() += reference.source_locators.len();
                            format!("out_of_runtime_scope:{signature}")
                        }
                        fnv_dependencies::DependencyReferenceResolution::TargetIntrinsic {
                            signature,
                            ..
                        } => format!("target_intrinsic:{signature}"),
                        fnv_dependencies::DependencyReferenceResolution::MissingSourceRecord => {
                            "missing_source_record".to_string()
                        }
                    };
                    let lvlo_locator_count = reference
                        .source_locators
                        .iter()
                        .filter(|locator| locator.starts_with("LVLO["))
                        .count();
                    *lvli_decoded
                        .lvlo_resolution_counts
                        .entry(resolution)
                        .or_default() += lvlo_locator_count;
                }
            }

            if receipt.signature == "IPDS" {
                ipds_slots.record_count += 1;
                ipds_slots.record_sources.push(format!(
                    "{}|{}",
                    receipt.source.plugin, receipt.source.local
                ));
                const SLOT_NAMES: [&str; 12] = [
                    "stone",
                    "dirt",
                    "grass",
                    "glass",
                    "metal",
                    "wood",
                    "organic",
                    "cloth",
                    "water",
                    "hollow_metal",
                    "organic_bug",
                    "organic_glow",
                ];
                let mut nonnull_slots = BTreeSet::new();
                for reference in &receipt.references {
                    for locator in reference
                        .source_locators
                        .iter()
                        .filter(|locator| locator.starts_with("DATA[0]."))
                    {
                        let Some(slot) = locator
                            .strip_prefix("DATA[0].")
                            .and_then(|value| value.split('@').next())
                        else {
                            continue;
                        };
                        nonnull_slots.insert(slot.to_string());
                        *ipds_slots
                            .slot_reference_incidence
                            .entry(slot.to_string())
                            .or_default() += 1;
                        ipds_slot_targets
                            .entry(slot.to_string())
                            .or_default()
                            .insert(reference.target.to_string());
                        ipds_slot_candidate_owners
                            .entry(slot.to_string())
                            .or_default()
                            .extend(candidate_sources.iter().cloned());
                        ipds_slots.exact_edges.push(
                            serde_json::to_string(&serde_json::json!({
                                "source": &receipt.source,
                                "slot": slot,
                                "target": &reference.target,
                                "resolution": &reference.resolution,
                                "candidate_incidence": candidate_sources.len(),
                            }))
                            .expect("IPDS slot edge json"),
                        );
                    }
                }
                for slot in SLOT_NAMES {
                    if !nonnull_slots.contains(slot) {
                        *ipds_slots
                            .slot_null_record_count
                            .entry(slot.to_string())
                            .or_default() += 1;
                    }
                }
            }
            let mut field_values = BTreeMap::<String, Vec<String>>::new();
            for field in &winner.record.fields {
                field_values
                    .entry(field.sig.as_str().to_string())
                    .or_default()
                    .push(legacy_census_field_value(&field.value, &interner));
            }
            runtime_record_details.insert(
                format!("{game}:{}", receipt.source),
                LegacyRuntimeRecordDetailCensus {
                    game: game.to_string(),
                    signature: receipt.signature.clone(),
                    source: receipt.source.to_string(),
                    field_values,
                    dependency_locator_edges: receipt
                        .references
                        .iter()
                        .map(|reference| {
                            serde_json::to_string(&serde_json::json!({
                                "target": &reference.target,
                                "source_locators": &reference.source_locators,
                                "resolution": &reference.resolution,
                            }))
                            .expect("runtime record reference edge json")
                        })
                        .collect(),
                    candidate_incidence: candidate_sources.len(),
                    candidate_sources,
                },
            );

            if receipt.signature == "WEAP" {
                let sound_signature = |signature: &str| {
                    matches!(
                        signature,
                        "SNAM" | "XNAM" | "NAM7" | "TNAM" | "NAM6" | "UNAM" | "NAM9" | "NAM8"
                    ) || signature.starts_with("WMS")
                };
                let model_signature = |signature: &str| {
                    matches!(signature, "MODL" | "MOD2" | "MOD3" | "MOD4" | "WNAM")
                        || signature.starts_with("MWD")
                        || signature.starts_with("WNM")
                        || signature.starts_with("WMI")
                };
                let fields = &winner.record.fields;
                let values_for = |signature: &str| {
                    fields
                        .iter()
                        .filter(|field| field.sig.as_str() == signature)
                        .map(|field| legacy_census_field_value(&field.value, &interner))
                        .collect::<Vec<_>>()
                };
                let mut sound_fields = BTreeMap::<String, Vec<String>>::new();
                let mut model_fields = BTreeMap::<String, Vec<String>>::new();
                for field in fields {
                    let signature = field.sig.as_str();
                    if sound_signature(signature) {
                        sound_fields
                            .entry(signature.to_string())
                            .or_default()
                            .push(legacy_census_field_value(&field.value, &interner));
                    }
                    if model_signature(signature) {
                        model_fields
                            .entry(signature.to_string())
                            .or_default()
                            .push(legacy_census_field_value(&field.value, &interner));
                    }
                }
                let optional_field_presence = ["CRDT", "EITM", "SCRI", "BIPL", "REPL"]
                    .into_iter()
                    .map(|signature| {
                        (
                            signature.to_string(),
                            fields.iter().any(|field| field.sig.as_str() == signature),
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                let candidate_sources = weapon_candidate_sources
                    .get(&(
                        receipt.source.plugin.to_ascii_lowercase(),
                        receipt.source.local,
                    ))
                    .map(|sources| sources.iter().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                weapon_details.insert(
                    format!("{game}:{}", receipt.source),
                    LegacyWeaponDetailCensus {
                        game: game.to_string(),
                        source: receipt.source.to_string(),
                        etyp_values: values_for("ETYP"),
                        data_lengths: fields
                            .iter()
                            .filter(|field| field.sig.as_str() == "DATA")
                            .filter_map(|field| match &field.value {
                                FieldValue::Bytes(bytes) => Some(bytes.len()),
                                _ => None,
                            })
                            .collect(),
                        dnam: fields
                            .iter()
                            .filter(|field| field.sig.as_str() == "DNAM")
                            .filter_map(|field| legacy_weapon_dnam_census(&field.value))
                            .collect(),
                        sound_fields,
                        model_fields,
                        optional_field_presence,
                        candidate_incidence: candidate_sources.len(),
                        candidate_sources,
                    },
                );
            }
        }

        ipds_slots.slot_unique_targets = ipds_slot_targets
            .into_iter()
            .map(|(slot, targets)| (slot, targets.len()))
            .collect();
        for receipt in &ledger.records {
            for reference in &receipt.references {
                let followed_signature = match &reference.resolution {
                    fnv_dependencies::DependencyReferenceResolution::Followed { signature } => {
                        signature.as_str()
                    }
                    _ => continue,
                };
                let edge = serde_json::to_string(&serde_json::json!({
                    "source": &receipt.source,
                    "target": &reference.target,
                    "source_locators": &reference.source_locators,
                }))
                .expect("runtime inbound edge json");
                if followed_signature == "LVLI" {
                    lvli_decoded
                        .inbound_runtime_edges_by_source_signature
                        .entry(receipt.signature.clone())
                        .or_default()
                        .push(edge.clone());
                }
                if followed_signature == "IPDS" {
                    ipds_slots
                        .inbound_runtime_edges_by_source_signature
                        .entry(receipt.signature.clone())
                        .or_default()
                        .push(edge);
                }
            }
        }
        for edges in lvli_decoded
            .inbound_runtime_edges_by_source_signature
            .values_mut()
        {
            edges.sort();
            edges.dedup();
        }
        ipds_slots.record_sources.sort();
        ipds_slots.record_sources.dedup();
        for edges in ipds_slots
            .inbound_runtime_edges_by_source_signature
            .values_mut()
        {
            edges.sort();
            edges.dedup();
        }
        ipds_slots.slot_unique_candidate_owners = ipds_slot_candidate_owners
            .into_iter()
            .map(|(slot, owners)| (slot, owners.len()))
            .collect();
        ipds_slots.exact_edges.sort();
        ipds_slots.exact_edges.dedup();
        for edges in bptd_decoded.dependency_edges_by_signature.values_mut() {
            edges.sort();
            edges.dedup();
        }
        bptd_decoded.nam5_rows.sort();
        bptd_decoded.raga_records.sort();
        bptd_decoded.raga_records.dedup();
        lvli_decoded.coed_rows.sort();
        for group in record_shapes.values_mut() {
            group.dependency_locator_edges.sort();
            group.dependency_locator_edges.dedup();
        }
        assert_eq!(
            weapon_details.len(),
            weapon_keys.len(),
            "every closure WEAP needs one exact detail census row"
        );
        assert!(
            weapon_details
                .values()
                .all(
                    |weapon| weapon.candidate_incidence == weapon.candidate_sources.len()
                        && weapon.candidate_incidence > 0
                ),
            "every closure WEAP must name every owning creature candidate"
        );
        assert_eq!(bptd_decoded.record_count, 95, "official BPTD closure drift");
        assert_eq!(
            lvli_decoded.record_count, 286,
            "official LVLI closure drift after explicit PACK condition omission"
        );
        assert_eq!(
            ipds_slots.record_count, 91,
            "official IPDS closure drift; sources={:?}; inbound={:?}",
            ipds_slots.record_sources, ipds_slots.inbound_runtime_edges_by_source_signature
        );
        assert!(
            ipds_slots
                .record_sources
                .iter()
                .any(|source| source == "FalloutNV.esm|1461223"),
            "official IPDS closure must retain BloodSpoutRedDataSet 164BE7:FalloutNV.esm"
        );
        let weapon_ipds_edges = ipds_slots
            .inbound_runtime_edges_by_source_signature
            .get("WEAP")
            .expect("official IPDS closure must retain inbound WEAP evidence");
        for source_local in [1_400_422_u32, 1_400_429_u32] {
            assert!(
                weapon_ipds_edges.iter().any(|edge| {
                    edge.contains(&format!("\"local\":{source_local}"))
                        && edge.contains("\"local\":1461223")
                        && edge.contains("INAM[0]")
                }),
                "official IPDS closure must retain WEAP {source_local:06X} INAM[0] -> 164BE7:FalloutNV.esm"
            );
        }
        let json = ledger
            .canonical_json()
            .expect("canonical dependency ledger");
        if let Ok(output) = std::env::var("BACUP_CREATURE_CENSUS_OUTPUT") {
            fs::write(&output, &json)
                .unwrap_or_else(|error| panic!("write census ledger {output}: {error}"));
            let shape_output =
                Path::new(&output).with_file_name("fnv_fo3_creature_record_shape_census.json");
            fs::write(
                &shape_output,
                serde_json::to_vec_pretty(&record_shapes).expect("shape census json"),
            )
            .unwrap_or_else(|error| {
                panic!("write shape census {}: {error}", shape_output.display())
            });
            let detail_output =
                Path::new(&output).with_file_name("fnv_fo3_creature_weap_detail_census.json");
            fs::write(
                &detail_output,
                serde_json::to_vec_pretty(&weapon_details).expect("WEAP detail census json"),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "write WEAP detail census {}: {error}",
                    detail_output.display()
                )
            });
            let summary_output =
                Path::new(&output).with_file_name("fnv_fo3_creature_closure_summary.json");
            fs::write(
                &summary_output,
                serde_json::to_vec_pretty(&closure_dispositions).expect("closure summary json"),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "write closure summary {}: {error}",
                    summary_output.display()
                )
            });
            let runtime_detail_output = Path::new(&output)
                .with_file_name("fnv_fo3_creature_runtime_record_detail_census.json");
            fs::write(
                &runtime_detail_output,
                serde_json::to_vec_pretty(&runtime_record_details)
                    .expect("runtime record detail census json"),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "write runtime record detail census {}: {error}",
                    runtime_detail_output.display()
                )
            });
            for (file_name, payload) in [
                (
                    "fnv_fo3_creature_bptd_decoded_census.json",
                    serde_json::to_vec_pretty(&bptd_decoded).expect("BPTD decoded census json"),
                ),
                (
                    "fnv_fo3_creature_lvli_decoded_census.json",
                    serde_json::to_vec_pretty(&lvli_decoded).expect("LVLI decoded census json"),
                ),
                (
                    "fnv_fo3_creature_ipds_slot_census.json",
                    serde_json::to_vec_pretty(&ipds_slots).expect("IPDS slot census json"),
                ),
            ] {
                let path = Path::new(&output).with_file_name(file_name);
                fs::write(&path, payload).unwrap_or_else(|error| {
                    panic!("write decoded census {}: {error}", path.display())
                });
            }
        }
        println!(
            "FNV_FO3_CREATURE_CENSUS winners={} ready={} blocked={} records={} blockers={}",
            ledger.winning_creatures,
            ledger.ready_candidates,
            ledger.blocked_candidates,
            ledger.records.len(),
            serde_json::to_string(&blocker_counts).unwrap(),
        );
        println!(
            "FNV_FO3_CREATURE_CENSUS_BY_PLUGIN {} flags={}",
            serde_json::to_string(&creature_winners_by_plugin).unwrap(),
            serde_json::to_string(&creature_winner_flag_counts).unwrap(),
        );
    }

    #[test]
    fn real_fnv_kf_stages_against_the_converted_source_owned_skeleton() {
        let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../extracted/fnv/meshes/creatures/nvgecko");
        let skeleton_source = fixture_root.join("skeleton.nif");
        let kf_source = fixture_root.join("mtidle.kf");
        if !skeleton_source.is_file() || !kf_source.is_file() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let skeleton_artifact = PlannedArtifact {
            source_root: ArtifactSourceRoot::SourceExtracted,
            source_path: "meshes/creatures/nvgecko/skeleton.nif".to_string(),
            target_path: "Meshes/Actors/B21_FNVGecko/CharacterAssets/Skeleton.hkx".to_string(),
            kind: ArtifactKind::SkeletonHkx,
            record_signature: None,
            dependencies: Vec::new(),
            root: false,
            source_blake3: None,
            skeleton_name: Some("NVGecko".to_string()),
            float_slot_names: Vec::new(),
            sequence_index: None,
            event_map: BTreeMap::new(),
            target_sample_rate_hz: None,
        };
        let skeleton_destination = temp.path().join("Skeleton.hkx");
        let skeleton =
            stage_source_rig_skeleton(&skeleton_artifact, &skeleton_source, &skeleton_destination)
                .expect("stage source-owned skeleton");
        let kf_bytes = fs::read(&kf_source).expect("read KF fixture");
        let clip_artifact = PlannedArtifact {
            source_root: ArtifactSourceRoot::SourceExtracted,
            source_path: "meshes/creatures/nvgecko/mtidle.kf".to_string(),
            target_path: "Meshes/Actors/B21_FNVGecko/Animations/Idle.hkx".to_string(),
            kind: ArtifactKind::IdleHkx,
            record_signature: None,
            dependencies: Vec::new(),
            root: false,
            source_blake3: Some(blake3::hash(&kf_bytes).to_hex().to_string()),
            skeleton_name: None,
            float_slot_names: Vec::new(),
            sequence_index: Some(0),
            event_map: BTreeMap::new(),
            target_sample_rate_hz: None,
        };
        let job = CreatureCorpusJob {
            job_id: "fixture".to_string(),
            source_key: "fnv|falloutnv.esm|10cd73".to_string(),
            output_slug: "fnv-gecko".to_string(),
            family_id: "fnv-gecko".to_string(),
            adapter: CreatureAdapterProfile::FnvGecko,
            unsupported_capability: None,
            publish_root: "Meshes/Actors/B21_FNVGecko".to_string(),
            graph_paths: Vec::new(),
            artifacts: Vec::new(),
            executable_recipe: None,
            member_source_keys: Vec::new(),
            candidate_recipes: Vec::new(),
            family_bundle_blake3: None,
            preflight_issues: Vec::new(),
        };
        let clip_destination = temp.path().join("Idle.hkx");
        stage_fnv_kf(
            &job,
            &clip_artifact,
            &kf_bytes,
            &skeleton,
            &clip_destination,
        )
        .expect("stage verified KF");
        let hkx_bytes = fs::read(clip_destination).expect("read staged HKX");
        assert!(!hkx_bytes.is_empty());
        assert_ne!(blake3::hash(&hkx_bytes), blake3::hash(&kf_bytes));
    }

    #[test]
    fn best_effort_publishes_good_family_and_leaves_failed_family_uncommitted() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (plan, roots) = two_family_fixture(temp.path());
        let mut ledger = execute_batch(
            &plan,
            ExecutionMode::BestEffort,
            temp.path(),
            &roots,
            None,
            &InjectFailure {
                fail_target: "bad.hkx".to_string(),
            },
        )
        .expect("batch");
        normalize_execution_ledger(&mut ledger);

        assert!(
            temp.path()
                .join("data/Meshes/Actors/Good/good.hkx")
                .is_file()
        );
        assert!(!temp.path().join("data/Meshes/Actors/Bad").exists());
        assert_eq!(ledger.family_disposition_counts.get("published"), Some(&1));
        assert_eq!(ledger.family_disposition_counts.get("failed"), Some(&1));
    }

    #[test]
    fn strict_failure_aborts_every_family_without_publishing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (plan, roots) = two_family_fixture(temp.path());
        let mut ledger = execute_batch(
            &plan,
            ExecutionMode::Strict,
            temp.path(),
            &roots,
            None,
            &InjectFailure {
                fail_target: "bad.hkx".to_string(),
            },
        )
        .expect("batch");
        normalize_execution_ledger(&mut ledger);

        assert!(ledger.strict_aborted);
        assert!(!temp.path().join("data/Meshes/Actors/Good").exists());
        assert!(!temp.path().join("data/Meshes/Actors/Bad").exists());
        assert_eq!(
            ledger.family_disposition_counts.get("aborted_strict"),
            Some(&1)
        );
        assert_eq!(ledger.family_disposition_counts.get("failed"), Some(&1));
    }

    #[test]
    fn strict_publish_failure_rolls_back_every_promoted_family() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (mut plan, roots) = two_family_fixture(temp.path());
        plan.jobs[1].publish_root = "Textures/Bad".to_string();
        plan.jobs[1].artifacts[0].target_path = "Textures/Bad/bad.hkx".to_string();
        fs::create_dir_all(temp.path().join("data")).expect("data root");
        fs::write(temp.path().join("data/Textures"), b"parent collision").expect("blocking parent");

        let mut ledger = execute_batch(
            &plan,
            ExecutionMode::Strict,
            temp.path(),
            &roots,
            None,
            &FilesystemStager,
        )
        .expect("batch");
        normalize_execution_ledger(&mut ledger);

        assert!(ledger.strict_aborted);
        assert!(!temp.path().join("data/Meshes/Actors/Good").exists());
        assert_eq!(ledger.family_disposition_counts.get("failed"), Some(&1));
        assert_eq!(
            ledger.family_disposition_counts.get("aborted_strict"),
            Some(&1)
        );
    }

    #[test]
    fn attached_ba2_sink_commits_the_strict_family_batch_atomically() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (plan, roots) = two_family_fixture(temp.path());
        let sink = crate::sinks::SinkSet {
            ba2: Some(
                crate::sinks::Ba2ShardWriter::new(temp.path().join("spill")).expect("BA2 sink"),
            ),
            loose: crate::sinks::LooseSink {
                enabled: true,
                mod_root: temp.path().to_path_buf(),
            },
            terrain: crate::sinks::TerrainSidecarSink::default(),
        };

        let mut ledger = execute_batch(
            &plan,
            ExecutionMode::Strict,
            temp.path(),
            &roots,
            Some(&sink),
            &FilesystemStager,
        )
        .expect("batch");
        normalize_execution_ledger(&mut ledger);

        assert!(!ledger.strict_aborted);
        assert_eq!(sink.ba2.as_ref().expect("BA2").entry_count(), 2);
        assert!(temp.path().join("data/Meshes/Actors/Good").exists());
        assert!(temp.path().join("data/Meshes/Actors/Bad").exists());
    }

    #[test]
    fn strict_sink_commit_failure_rolls_back_assets_and_leaves_ba2_empty() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (plan, roots) = two_family_fixture(temp.path());
        let sink = crate::sinks::SinkSet {
            ba2: Some(
                crate::sinks::Ba2ShardWriter::new(temp.path().join("spill")).expect("BA2 sink"),
            ),
            loose: crate::sinks::LooseSink {
                enabled: true,
                mod_root: temp.path().join("wrong-loose-root"),
            },
            terrain: crate::sinks::TerrainSidecarSink::default(),
        };

        let mut ledger = execute_batch(
            &plan,
            ExecutionMode::Strict,
            temp.path(),
            &roots,
            Some(&sink),
            &FilesystemStager,
        )
        .expect("batch");
        normalize_execution_ledger(&mut ledger);

        assert!(ledger.strict_aborted);
        assert_eq!(sink.ba2.as_ref().expect("BA2").entry_count(), 0);
        assert!(!temp.path().join("data/Meshes/Actors/Good").exists());
        assert!(!temp.path().join("data/Meshes/Actors/Bad").exists());
    }

    #[test]
    fn missing_prepared_record_batch_publishes_zero_family_assets() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (mut plan, roots) = two_family_fixture(temp.path());
        fs::write(temp.path().join("race.record"), b"prepared record fixture")
            .expect("record source");
        plan.jobs.truncate(1);
        plan.jobs[0].artifacts.push(PlannedArtifact {
            source_root: ArtifactSourceRoot::Mod,
            source_path: "race.record".to_string(),
            target_path: "Records/Good/RACE.record".to_string(),
            kind: ArtifactKind::Record,
            record_signature: Some("RACE".to_string()),
            dependencies: Vec::new(),
            root: false,
            source_blake3: None,
            skeleton_name: None,
            float_slot_names: Vec::new(),
            sequence_index: None,
            event_map: BTreeMap::new(),
            target_sample_rate_hz: None,
        });

        let mut ledger = execute_batch(
            &plan,
            ExecutionMode::Strict,
            temp.path(),
            &roots,
            None,
            &FilesystemStager,
        )
        .expect("batch");
        normalize_execution_ledger(&mut ledger);

        assert!(ledger.strict_aborted);
        assert_eq!(ledger.family_disposition_counts.get("failed"), Some(&1));
        assert!(!temp.path().join("data/Meshes/Actors/Good").exists());
    }

    #[test]
    fn unsupported_family_is_terminal_and_counted() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (mut plan, roots) = two_family_fixture(temp.path());
        plan.jobs[1].adapter = CreatureAdapterProfile::UnsupportedCapability;
        plan.jobs[1].unsupported_capability = Some("flight_behavior".to_string());
        plan.jobs[1].preflight_issues = vec![PreflightIssue::UnsupportedCapability {
            capability: "flight_behavior".to_string(),
        }];
        let mut ledger = execute_batch(
            &plan,
            ExecutionMode::BestEffort,
            temp.path(),
            &roots,
            None,
            &FilesystemStager,
        )
        .expect("batch");
        normalize_execution_ledger(&mut ledger);

        assert_eq!(
            ledger
                .family_disposition_counts
                .get("unsupported_capability"),
            Some(&1)
        );
        assert!(!temp.path().join("data/Meshes/Actors/Bad").exists());
    }

    #[test]
    fn source_extracted_havok_is_rejected_before_publish() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("source.hkx"), b"skyrim bytes").expect("source");
        let mut artifact =
            fixture_artifact("source.hkx", "Meshes/Actors/Unsafe/Animations/Idle.hkx");
        artifact.source_root = ArtifactSourceRoot::SourceExtracted;
        artifact.root = true;
        let mut job = fixture_job(
            "family-unsafe",
            "Unsafe",
            "Meshes/Actors/Unsafe",
            vec![artifact],
        );
        job.preflight_issues = adapter_contract_issues(job.adapter, &job.artifacts);
        assert!(
            job.preflight_issues
                .iter()
                .any(|issue| matches!(issue, PreflightIssue::UnconvertedSourceArtifact { .. }))
        );
        let mut nif = fixture_artifact(
            "source.nif",
            "Meshes/Actors/Unsafe/CharacterAssets/Body.nif",
        );
        nif.source_root = ArtifactSourceRoot::SourceExtracted;
        nif.kind = ArtifactKind::VisualNif;
        assert!(
            adapter_contract_issues(job.adapter, &[nif])
                .iter()
                .all(|issue| {
                    !matches!(issue, PreflightIssue::UnconvertedSourceArtifact { .. })
                })
        );
        let corpus = CreatureCorpusPlan {
            version: 1,
            candidate_count: 0,
            planned: Vec::new(),
            rejected: Vec::new(),
        };
        let plan = CreatureCorpusExecutionPlan {
            version: PLAN_VERSION,
            debug_dir: DEFAULT_DEBUG_DIR.to_string(),
            native_catalog: fixture_bridge_ledger(&corpus).expect("bridge"),
            corpus,
            jobs: vec![job],
        };
        let roots = SourceRoots {
            mod_root: temp.path(),
            source_extracted: temp.path(),
            target_extracted: None,
            target_data: None,
        };
        let mut ledger = execute_batch(
            &plan,
            ExecutionMode::BestEffort,
            temp.path(),
            &roots,
            None,
            &FilesystemStager,
        )
        .expect("batch");
        normalize_execution_ledger(&mut ledger);
        assert_eq!(ledger.family_disposition_counts.get("failed"), Some(&1));
        assert!(!temp.path().join("data/Meshes/Actors/Unsafe").exists());
    }

    #[test]
    fn native_bridge_hash_and_candidate_accounting_are_pinned() {
        let identity = SourceCreatureIdentity {
            namespace: "skyrimse".to_string(),
            plugin: "Skyrim.esm".to_string(),
            local_form_id: 0x1320a,
        };
        let corpus = CreatureCorpusPlan::build(
            Vec::new(),
            Vec::new(),
            vec![CreatureCorpusCandidate {
                source_identity: identity.clone(),
                primary_record_identity: identity.clone(),
                output_slug: "wolf-race-01320a".to_string(),
                rig_family: "wolf-family".to_string(),
                motion_set: "wolf-motion".to_string(),
                record_variants: Vec::new(),
                preflight_rejections: vec![CreatureRejectionReason::UpstreamCatalogRejected {
                    catalog: "synthetic_skyrim_catalog".to_string(),
                    reason_code: "missing_source_owned_race_data".to_string(),
                    detail: "synthetic bridge fixture".to_string(),
                }],
            }],
        )
        .expect("corpus");
        let bridge = finalize_bridge(
            "synthetic_skyrim_catalog",
            1,
            &corpus,
            vec![NativeBridgeEntry {
                source_key: identity.stable_key(),
                subject: NativeBridgeSubject::RaceCandidate,
                disposition: "catalog_rejected".to_string(),
                reason_codes: vec!["missing_source_owned_race_data".to_string()],
            }],
        )
        .expect("bridge");
        validate_native_bridge(&bridge, &corpus).expect("valid bridge");
        assert_eq!(bridge.winner_count, 1);
        assert_eq!(corpus.rejected.len(), 1);
        assert_eq!(
            bridge.entries_blake3,
            bridge_entries_hash(&bridge.entries).unwrap()
        );

        let mut tampered = bridge;
        tampered.entries_blake3 = "00".repeat(32);
        assert!(validate_native_bridge(&tampered, &corpus).is_err());
    }

    #[test]
    fn live_preparation_terminal_accounting_retains_every_blocked_candidate() {
        let candidates = [0x100u32, 0x101]
            .into_iter()
            .map(|local| {
                let identity = SourceCreatureIdentity {
                    namespace: "fnv".to_string(),
                    plugin: "FalloutNV.esm".to_string(),
                    local_form_id: local,
                };
                CreatureCorpusCandidate {
                    source_identity: identity.clone(),
                    primary_record_identity: identity,
                    output_slug: format!("blocked-{local:06x}"),
                    rig_family: "blocked-family".to_string(),
                    motion_set: "missing-motion".to_string(),
                    record_variants: Vec::new(),
                    preflight_rejections: vec![contract_rejection(
                        "fixture_blocker",
                        "fixture blocker".to_string(),
                    )],
                }
            })
            .collect();
        let corpus = CreatureCorpusPlan::build(Vec::new(), Vec::new(), candidates).expect("corpus");
        let ledger = blocked_live_preparation(
            "fnv",
            &corpus,
            "live_fixture_blocked",
            "fixture live preparation is blocked",
        );

        assert_eq!(ledger.candidate_count, 2);
        assert_eq!(ledger.ready_candidate_count, 0);
        assert_eq!(ledger.blocked_candidate_count, 2);
        assert_eq!(
            ledger
                .terminals
                .iter()
                .map(|terminal| terminal.member_source_keys.len())
                .sum::<usize>(),
            2
        );
        assert!(ledger.terminals.iter().all(|terminal| {
            terminal.disposition == "blocked" && terminal.blockers[0].code == "live_fixture_blocked"
        }));
    }

    #[test]
    fn live_preparation_promotion_is_atomic_and_failed_publish_keeps_previous_output() {
        let temp = tempfile::tempdir().expect("tempdir");
        let final_root = temp.path().join("live_prepared");
        let work_root = temp.path().join("live_prepared.work");
        fs::create_dir_all(&final_root).expect("final root");
        fs::write(final_root.join("old.txt"), b"old").expect("old output");
        fs::create_dir_all(&work_root).expect("work root");
        fs::write(work_root.join("new.txt"), b"new").expect("new output");

        promote_live_preparation_root(&work_root, &final_root).expect("atomic publish");
        assert!(!work_root.exists());
        assert!(!final_root.join("old.txt").exists());
        assert_eq!(fs::read(final_root.join("new.txt")).unwrap(), b"new");
        assert!(!final_root.with_extension("previous").exists());

        fs::create_dir_all(&work_root).expect("second work root");
        fs::write(work_root.join("failed.txt"), b"failed").expect("failed output");
        fs::create_dir_all(final_root.with_extension("previous")).expect("stale backup");
        assert!(promote_live_preparation_root(&work_root, &final_root).is_err());
        cleanup_live_preparation_work_root(&work_root).expect("rollback work root");
        assert!(!work_root.exists());
        assert_eq!(fs::read(final_root.join("new.txt")).unwrap(), b"new");
        assert!(!final_root.join("failed.txt").exists());
    }

    #[test]
    fn evidence_recipe_promotes_only_its_exact_rejected_catalog_identity() {
        let identity = SourceCreatureIdentity {
            namespace: "fnv".to_string(),
            plugin: "FalloutNV.esm".to_string(),
            local_form_id: 0x11_2233,
        };
        let corpus = CreatureCorpusPlan::build(
            Vec::new(),
            Vec::new(),
            vec![CreatureCorpusCandidate {
                source_identity: identity.clone(),
                primary_record_identity: identity.clone(),
                output_slug: "promoted-gecko".to_string(),
                rig_family: "catalog-gecko".to_string(),
                motion_set: "catalog-gecko-motion".to_string(),
                record_variants: Vec::new(),
                preflight_rejections: vec![CreatureRejectionReason::UpstreamCatalogRejected {
                    catalog: "synthetic_fnv_catalog".to_string(),
                    reason_code: "source_rig_conversion_unavailable".to_string(),
                    detail: "prepared recipe supplies the verified conversion".to_string(),
                }],
            }],
        )
        .expect("rejected corpus");
        let source_key = identity.stable_key();
        let family_id = "family-promoted-gecko".to_string();
        let graph_paths = vec![
            "Meshes/Actors/PromotedGecko/Character.hkx".to_string(),
            "Meshes/Actors/PromotedGecko/Core.hkx".to_string(),
            "Meshes/Actors/PromotedGecko/Root.hkx".to_string(),
        ];
        let executable_recipe = PlannedExecutableRecipe {
            source_key: source_key.clone(),
            family_id: family_id.clone(),
            publish_root: "Meshes/Actors/PromotedGecko".to_string(),
            recipe_source_root: ArtifactSourceRoot::Mod,
            recipe_path: "debug/creature_corpus/prepared/gecko/recipe.json".to_string(),
            recipe_blake3: "11".repeat(32),
            creature_closure_receipt_source_root: ArtifactSourceRoot::Mod,
            creature_closure_receipt_path: "debug/creature_corpus/prepared/gecko/nif_receipt.json"
                .to_string(),
            creature_closure_receipt_blake3: "22".repeat(32),
            creature_closure_staged_data_source_root: ArtifactSourceRoot::Mod,
            creature_closure_staged_data_root: "debug/creature_corpus/prepared/gecko/nif_data"
                .to_string(),
            graph_paths: graph_paths.clone(),
            converted_artifacts: vec![PlannedConvertedArtifact {
                runtime_path: "Actors/PromotedGecko/Skeleton.hkx".to_string(),
                source_root: ArtifactSourceRoot::Mod,
                source_path: "debug/creature_corpus/prepared/gecko/Skeleton.hkx".to_string(),
                source_blake3: "33".repeat(32),
            }],
        };
        let job = CreatureCorpusJob {
            job_id: stable_job_id(&source_key, &family_id, "promoted-gecko"),
            source_key,
            output_slug: "promoted-gecko".to_string(),
            family_id,
            adapter: CreatureAdapterProfile::EvidenceBoundRecipe,
            unsupported_capability: None,
            publish_root: "Meshes/Actors/PromotedGecko".to_string(),
            graph_paths,
            artifacts: Vec::new(),
            executable_recipe: Some(executable_recipe),
            member_source_keys: Vec::new(),
            candidate_recipes: Vec::new(),
            family_bundle_blake3: None,
            preflight_issues: Vec::new(),
        };
        let mut plan = CreatureCorpusExecutionPlan {
            version: PLAN_VERSION,
            debug_dir: DEFAULT_DEBUG_DIR.to_string(),
            native_catalog: fixture_bridge_ledger(&corpus).expect("bridge"),
            corpus,
            jobs: vec![job],
        };

        validate_execution_plan(&plan).expect("verified rejected source may be promoted");
        let mut bundle = plan.clone();
        bundle.corpus.rejected[0].primary_record_identity = SourceCreatureIdentity {
            namespace: "fnv".to_string(),
            plugin: "FalloutNV.esm".to_string(),
            local_form_id: 0x11_2244,
        };
        let source_key = bundle.jobs[0].source_key.clone();
        bundle.jobs[0].member_source_keys = vec![source_key.clone()];
        bundle.jobs[0].candidate_recipes = vec![PlannedCandidateRecipe {
            source_key,
            recipe_source_root: ArtifactSourceRoot::Mod,
            recipe_path: "debug/creature_corpus/prepared/gecko/candidate.json".to_string(),
            recipe_blake3: "44".repeat(32),
        }];
        bundle.jobs[0].family_bundle_blake3 = Some(stable_family_bundle_hash(&bundle.jobs[0]));
        validate_execution_plan(&bundle)
            .expect("family bundle ownership follows the candidate source identity");
        let mut curated = plan.clone();
        curated.corpus.rejected[0].reasons = vec![CreatureRejectionReason::CuratedExclusion {
            policy: "synthetic-curated-exclusion".to_string(),
        }];
        assert!(validate_execution_plan(&curated).is_err());
        plan.jobs[0]
            .executable_recipe
            .as_mut()
            .expect("recipe")
            .source_key = "fnv|FalloutNV.esm|00ffffff".to_string();
        assert!(validate_execution_plan(&plan).is_err());
    }

    #[test]
    fn family_bundle_has_one_asset_closure_and_distinct_candidate_recipes() {
        let family = RigFamily {
            id: "catalog-family".to_string(),
            root_node: "Root".to_string(),
            race_data: RaceDataMapping::Mapped {
                target: grounded_race_data(),
            },
        };
        let motion = MotionSet {
            id: "catalog-motion".to_string(),
            graph_template: CreatureGraphTemplate::PassiveGround,
            idle_clip: "Idle".to_string(),
            locomotion_clips: BTreeMap::new(),
            attacks: Vec::new(),
            attack_kinds: BTreeMap::new(),
            required_overlays: Vec::new(),
            overlays: Vec::new(),
            required_rigs: Vec::new(),
            rigs: Vec::new(),
        };
        let planned = [0x100u32, 0x101]
            .into_iter()
            .map(|local| {
                let identity = SourceCreatureIdentity {
                    namespace: "fnv".to_string(),
                    plugin: "FalloutNV.esm".to_string(),
                    local_form_id: local,
                };
                let output_slug = format!("creature-{local:06x}");
                crate::source_rig::PlannedCreature {
                    source_identity: identity.clone(),
                    primary_record_identity: identity.clone(),
                    source_key: identity.stable_key(),
                    output_slug: output_slug.clone(),
                    readiness: crate::source_rig::CreatureReadiness::Ready,
                    capabilities: crate::source_rig::CapabilityLedger {
                        required: Vec::new(),
                        available: Vec::new(),
                        missing: Vec::new(),
                    },
                    rig_family: family.clone(),
                    motion_set: motion.clone(),
                    record_variants: vec![RecordVariant {
                        source_identity: identity,
                        output_slug: "base".to_string(),
                        display_name: format!("Creature {local:06X}"),
                        body_nif: "Meshes/Actors/Test/Body.nif".to_string(),
                        level: 1,
                        health: 10,
                        action_points: 10,
                        primary: true,
                        attack_ids: Vec::new(),
                    }],
                }
            })
            .collect::<Vec<_>>();
        let corpus = CreatureCorpusPlan {
            version: 1,
            candidate_count: planned.len(),
            planned,
            rejected: Vec::new(),
        };
        corpus.validate().expect("fixture corpus");
        let members = corpus
            .planned
            .iter()
            .map(|candidate| candidate.source_key.clone())
            .collect::<Vec<_>>();
        let family_id = "motion-project-family".to_string();
        let publish_root = "Meshes/Actors/TestFamily".to_string();
        let graph_paths = vec![
            "Meshes/Actors/TestFamily/Character.hkx".to_string(),
            "Meshes/Actors/TestFamily/Core.hkx".to_string(),
            "Meshes/Actors/TestFamily/Root.hkx".to_string(),
        ];
        let asset_recipe = PlannedExecutableRecipe {
            source_key: members[0].clone(),
            family_id: family_id.clone(),
            publish_root: publish_root.clone(),
            recipe_source_root: ArtifactSourceRoot::Mod,
            recipe_path: "debug/creature_corpus/live/family/asset_recipe.json".to_string(),
            recipe_blake3: "11".repeat(32),
            creature_closure_receipt_source_root: ArtifactSourceRoot::Mod,
            creature_closure_receipt_path: "debug/creature_corpus/live/family/closure.json"
                .to_string(),
            creature_closure_receipt_blake3: "22".repeat(32),
            creature_closure_staged_data_source_root: ArtifactSourceRoot::Mod,
            creature_closure_staged_data_root: "debug/creature_corpus/live/family/closure_data"
                .to_string(),
            graph_paths: graph_paths.clone(),
            converted_artifacts: vec![PlannedConvertedArtifact {
                runtime_path: "Actors/TestFamily/Skeleton.hkx".to_string(),
                source_root: ArtifactSourceRoot::Mod,
                source_path: "debug/creature_corpus/live/family/Skeleton.hkx".to_string(),
                source_blake3: "33".repeat(32),
            }],
        };
        let candidate_recipes = members
            .iter()
            .enumerate()
            .map(|(index, source_key)| PlannedCandidateRecipe {
                source_key: source_key.clone(),
                recipe_source_root: ArtifactSourceRoot::Mod,
                recipe_path: format!("debug/creature_corpus/live/family/candidate-{index}.json"),
                recipe_blake3: format!("{:02x}", 0x44 + index).repeat(32),
            })
            .collect::<Vec<_>>();
        let mut job = CreatureCorpusJob {
            job_id: stable_job_id(&members[0], &family_id, &corpus.planned[0].output_slug),
            source_key: members[0].clone(),
            output_slug: corpus.planned[0].output_slug.clone(),
            family_id,
            adapter: CreatureAdapterProfile::EvidenceBoundRecipe,
            unsupported_capability: None,
            publish_root,
            graph_paths,
            artifacts: Vec::new(),
            executable_recipe: Some(asset_recipe),
            member_source_keys: members,
            candidate_recipes,
            family_bundle_blake3: None,
            preflight_issues: Vec::new(),
        };
        job.family_bundle_blake3 = Some(stable_family_bundle_hash(&job));
        let plan = CreatureCorpusExecutionPlan {
            version: PLAN_VERSION,
            debug_dir: DEFAULT_DEBUG_DIR.to_string(),
            native_catalog: fixture_bridge_ledger(&corpus).expect("bridge"),
            corpus,
            jobs: vec![job],
        };

        validate_execution_plan(&plan).expect("valid family bundle");
        assert_eq!(plan.jobs.len(), 1);
        assert_eq!(effective_candidate_recipes(&plan.jobs[0]).len(), 2);

        let mut missing_candidate = plan.clone();
        missing_candidate.jobs[0].candidate_recipes.pop();
        missing_candidate.jobs[0].member_source_keys.pop();
        missing_candidate.jobs[0].family_bundle_blake3 =
            Some(stable_family_bundle_hash(&missing_candidate.jobs[0]));
        assert!(validate_execution_plan(&missing_candidate).is_err());

        let mut stale = plan;
        stale.jobs[0].candidate_recipes[0].recipe_blake3 = "aa".repeat(32);
        assert!(validate_execution_plan(&stale).is_err());
    }

    #[test]
    fn optional_prepared_skyrim_atronach_flame_family_executes() {
        let Some(mod_root) = std::env::var_os("SKYRIM_CREATURE_MOD_ROOT").map(PathBuf::from) else {
            return;
        };
        let plan_json = fs::read_to_string(mod_root.join(DEFAULT_DEBUG_DIR).join(PLAN_FILE))
            .expect("prepared Skyrim corpus plan");
        let plan: CreatureCorpusExecutionPlan =
            serde_json::from_str(&plan_json).expect("valid prepared Skyrim corpus plan");
        let job = plan
            .jobs
            .iter()
            .find(|job| job.family_id == "atronach_flame")
            .expect("Flame Atronach family job");
        let temp = tempfile::tempdir().expect("private staging root");
        let roots = SourceRoots {
            mod_root: &mod_root,
            source_extracted: &mod_root,
            target_extracted: None,
            target_data: None,
        };

        prepare_evidence_bound_family(job, temp.path(), &roots, &StringInterner::new())
            .expect("prepared Flame Atronach family executes");
    }

    #[test]
    fn recursive_closure_rejects_an_orphan() {
        let mut artifacts = vec![fixture_artifact("a.hkx", "Meshes/Actors/A/a.hkx")];
        artifacts.push(fixture_artifact("b.hkx", "Meshes/Actors/A/b.hkx"));
        artifacts[0].root = true;
        let job = fixture_job("family-a", "A", "Meshes/Actors/A", artifacts);
        let error = validate_job(&job).expect_err("orphan must fail");
        assert!(error.to_string().contains("unreachable target"));
    }

    #[test]
    fn supported_ground_melee_family_becomes_planned_with_complete_evidence() {
        let evidence = ground_melee_motion_evidence("creatures/test/skeleton.nif");
        let rig = evidence.rig.clone();
        let mut adapters = LegacyBridgeAdapters {
            motion_evidence: vec![evidence.clone()],
            ..LegacyBridgeAdapters::default()
        };
        add_converted_clips(&mut adapters, &evidence);
        adapters
            .converted_rig_roots
            .insert(rig.clone(), "Bip01".to_string());
        adapters.race_data.insert(rig.clone(), grounded_race_data());
        adapters.behavior_ready.insert(rig.clone());

        let (motion_sets, ledger) = build_legacy_motion_bridge(&adapters).expect("motion bridge");
        assert_eq!(ledger.family_count, 1);
        assert_eq!(ledger.terminal_family_count, 1);
        assert_eq!(ledger.emitted_motion_set_count, 1);
        assert_eq!(
            ledger.entries[0].disposition,
            NativeMotionDisposition::ReadyGroundMelee
        );
        let motion = motion_sets.first().expect("ground melee motion").clone();
        let identity = SourceCreatureIdentity {
            namespace: "fnv".to_string(),
            plugin: "FalloutNV.esm".to_string(),
            local_form_id: 0x100,
        };
        let corpus = CreatureCorpusPlan::build(
            vec![RigFamily {
                id: legacy_family_id(&rig),
                root_node: "Bip01".to_string(),
                race_data: RaceDataMapping::Mapped {
                    target: grounded_race_data(),
                },
            }],
            vec![motion.clone()],
            vec![CreatureCorpusCandidate {
                source_identity: identity.clone(),
                primary_record_identity: identity.clone(),
                output_slug: "test-creature-000100".to_string(),
                rig_family: legacy_family_id(&rig),
                motion_set: motion.id.clone(),
                record_variants: vec![RecordVariant {
                    source_identity: identity,
                    output_slug: "base".to_string(),
                    display_name: "Test Creature".to_string(),
                    body_nif: "Meshes/Actors/Test/CharacterAssets/Body.nif".to_string(),
                    level: 1,
                    health: 50,
                    action_points: 25,
                    primary: true,
                    attack_ids: motion
                        .attacks
                        .iter()
                        .map(|attack| attack.id.clone())
                        .collect(),
                }],
                preflight_rejections: Vec::new(),
            }],
        )
        .expect("source-rig corpus");
        assert_eq!(corpus.candidate_count, 1);
        assert_eq!(corpus.planned.len(), 1);
        assert!(corpus.rejected.is_empty());
        assert_eq!(
            corpus.candidate_count,
            corpus.planned.len() + corpus.rejected.len()
        );

        let root = tempfile::tempdir().expect("tempdir");
        let roots = SourceRoots {
            mod_root: root.path(),
            source_extracted: root.path(),
            target_extracted: None,
            target_data: None,
        };
        assert!(build_jobs(&corpus, &BTreeMap::new(), &roots).is_err());
    }

    #[test]
    fn ranged_motion_stays_a_typed_creature_capability() {
        let evidence = ranged_motion_evidence("creatures/ranged/skeleton.nif");
        let mut adapters = LegacyBridgeAdapters {
            motion_evidence: vec![evidence.clone()],
            ..LegacyBridgeAdapters::default()
        };
        add_converted_clips(&mut adapters, &evidence);
        let (motion_sets, ledger) = build_legacy_motion_bridge(&adapters).expect("motion bridge");
        assert!(motion_sets.is_empty());
        assert_eq!(
            ledger.entries[0].disposition,
            NativeMotionDisposition::ReadyRangedCreature
        );
        assert!(ledger.entries[0].has_ranged);
        assert!(!ledger.entries[0].motion_set_emitted);
        let rejection = motion_contract_rejection(&ledger.entries[0]);
        assert!(matches!(
            rejection,
            CreatureRejectionReason::UpstreamCatalogRejected {
                ref catalog,
                ref reason_code,
                ..
            } if catalog == "source_rig_contract"
                && reason_code == "creature_ranged_behavior_unavailable"
                && !reason_code.contains("weapon")
        ));
    }

    #[test]
    fn missing_ambiguous_and_overlay_motion_remain_typed_and_accounted() {
        let missing_rig = test_motion_rig("creatures/missing/skeleton.nif");
        let missing = fnv_motion::CreatureMotionFamilyEvidence {
            rig: missing_rig,
            referenced_kfs: vec!["creatures/missing/idle.kf".to_string()],
            kf_evidence: Vec::new(),
            creature_traits: Vec::new(),
        };
        let mut ambiguous = ground_melee_motion_evidence("creatures/ambiguous/skeleton.nif");
        let second_idle = parsed_motion(
            &ambiguous.rig,
            "creatures/ambiguous/combatidle.kf",
            "CombatIdle",
            fnv_motion::SequenceCycle::Loop,
            fnv_motion::RootMotionEvidence::Stationary {
                accum_root: Some("Bip01".to_string()),
            },
            &["start", "end"],
            &["Bip01"],
        );
        ambiguous.referenced_kfs.push(second_idle.source_kf.clone());
        ambiguous.kf_evidence.push(second_idle);
        let mut overlay = ground_melee_motion_evidence("creatures/overlay/skeleton.nif");
        let attack = overlay
            .kf_evidence
            .iter_mut()
            .find(|entry| entry.source_kf.contains("attack"))
            .expect("attack evidence");
        let fnv_motion::KfParseEvidence::Parsed(sequence) = &mut attack.sequence else {
            panic!("parsed attack");
        };
        sequence.binding.target_names.push("##Weapon".to_string());

        let mut adapters = LegacyBridgeAdapters {
            motion_evidence: vec![missing, ambiguous.clone(), overlay.clone()],
            ..LegacyBridgeAdapters::default()
        };
        add_converted_clips(&mut adapters, &ambiguous);
        add_converted_clips(&mut adapters, &overlay);
        let (_, ledger) = build_legacy_motion_bridge(&adapters).expect("motion bridge");
        assert_eq!(ledger.family_count, 3);
        assert_eq!(ledger.terminal_family_count, 3);
        assert_eq!(
            ledger.referenced_kf_count,
            ledger
                .entries
                .iter()
                .map(|entry| entry.kfs.len())
                .sum::<usize>()
        );
        let dispositions = ledger
            .entries
            .iter()
            .map(|entry| entry.disposition)
            .collect::<BTreeSet<_>>();
        assert!(dispositions.contains(&NativeMotionDisposition::EvidenceIncomplete));
        assert!(dispositions.contains(&NativeMotionDisposition::Ambiguous));
        assert!(dispositions.contains(&NativeMotionDisposition::UnsupportedOverlay));
        assert!(
            ledger
                .entries
                .iter()
                .flat_map(|entry| &entry.kfs)
                .any(|entry| {
                    entry.source_kf == "creatures/missing/idle.kf"
                        && entry.disposition == "missing_evidence"
                })
        );
        assert_eq!(
            ledger.entries_blake3,
            motion_entries_hash(&ledger.entries).expect("motion hash")
        );
    }

    fn ground_melee_motion_evidence(skeleton: &str) -> fnv_motion::CreatureMotionFamilyEvidence {
        let rig = test_motion_rig(skeleton);
        let specs = [
            (
                "idle.kf",
                "Idle",
                fnv_motion::SequenceCycle::Loop,
                fnv_motion::RootMotionEvidence::Stationary {
                    accum_root: Some("Bip01".to_string()),
                },
                vec!["start", "end"],
            ),
            (
                "forward.kf",
                "Forward",
                fnv_motion::SequenceCycle::Loop,
                fnv_motion::RootMotionEvidence::Planar {
                    accum_root: "Bip01".to_string(),
                    distance: 64.0,
                    yaw_radians: 0.0,
                },
                vec!["start", "m:L", "m:R", "end"],
            ),
            (
                "turnleft.kf",
                "TurnLeft",
                fnv_motion::SequenceCycle::Loop,
                fnv_motion::RootMotionEvidence::Planar {
                    accum_root: "Bip01".to_string(),
                    distance: 0.0,
                    yaw_radians: -1.570_796,
                },
                vec!["start", "end"],
            ),
            (
                "turnright.kf",
                "TurnRight",
                fnv_motion::SequenceCycle::Loop,
                fnv_motion::RootMotionEvidence::Planar {
                    accum_root: "Bip01".to_string(),
                    distance: 0.0,
                    yaw_radians: 1.570_796,
                },
                vec!["start", "end"],
            ),
            (
                "attack.kf",
                "AttackRight",
                fnv_motion::SequenceCycle::Clamp,
                fnv_motion::RootMotionEvidence::Stationary {
                    accum_root: Some("Bip01".to_string()),
                },
                vec!["start", "Hit", "end"],
            ),
            (
                "death.kf",
                "Death",
                fnv_motion::SequenceCycle::Clamp,
                fnv_motion::RootMotionEvidence::Stationary {
                    accum_root: Some("Bip01".to_string()),
                },
                vec!["start", "end"],
            ),
        ];
        let mut evidence = Vec::new();
        for (filename, name, cycle, root_motion, events) in specs {
            let source_kf = format!(
                "{}/{}",
                skeleton
                    .trim_end_matches("skeleton.nif")
                    .trim_end_matches('/'),
                filename
            );
            evidence.push(parsed_motion(
                &rig,
                &source_kf,
                name,
                cycle,
                root_motion,
                &events,
                &["Bip01"],
            ));
        }
        fnv_motion::CreatureMotionFamilyEvidence {
            rig,
            referenced_kfs: evidence
                .iter()
                .map(|entry| entry.source_kf.clone())
                .collect(),
            kf_evidence: evidence,
            creature_traits: vec![fnv_motion::CreatureTraitEvidence {
                motion_trait: fnv_motion::CreatureMotionTrait::Walks,
                source_creature: fnv_catalog::StableFormKey {
                    local: 0x100,
                    plugin: "FalloutNV.esm".to_string(),
                },
                field: "Walks".to_string(),
                value: "true".to_string(),
            }],
        }
    }

    fn ranged_motion_evidence(skeleton: &str) -> fnv_motion::CreatureMotionFamilyEvidence {
        let mut evidence = ground_melee_motion_evidence(skeleton);
        evidence
            .referenced_kfs
            .retain(|path| !path.ends_with("attack.kf"));
        evidence
            .kf_evidence
            .retain(|entry| !entry.source_kf.ends_with("attack.kf"));
        let source_kf = format!(
            "{}/spitattack.kf",
            skeleton
                .trim_end_matches("skeleton.nif")
                .trim_end_matches('/')
        );
        let ranged = parsed_motion(
            &evidence.rig,
            &source_kf,
            "SpitAttack",
            fnv_motion::SequenceCycle::Clamp,
            fnv_motion::RootMotionEvidence::Stationary {
                accum_root: Some("Bip01".to_string()),
            },
            &["start", "Release", "end"],
            &["Bip01"],
        );
        evidence.referenced_kfs.push(source_kf);
        evidence.kf_evidence.push(ranged);
        evidence
    }

    fn test_motion_rig(skeleton: &str) -> fnv_catalog::RigFamilyKey {
        fnv_catalog::RigFamilyKey {
            game: fnv_catalog::LegacyCreatureGame::Fnv,
            skeleton_path: skeleton.to_string(),
        }
    }

    fn parsed_motion(
        rig: &fnv_catalog::RigFamilyKey,
        source_kf: &str,
        name: &str,
        cycle: fnv_motion::SequenceCycle,
        root_motion: fnv_motion::RootMotionEvidence,
        events: &[&str],
        targets: &[&str],
    ) -> fnv_motion::CreatureKfEvidence {
        fnv_motion::CreatureKfEvidence {
            source_game: rig.game,
            source_kf: source_kf.to_string(),
            sequence: fnv_motion::KfParseEvidence::Parsed(
                fnv_motion::NiControllerSequenceEvidence {
                    sequence_index: 0,
                    name: name.to_string(),
                    cycle,
                    start_time: 0.0,
                    stop_time: 1.0,
                    frequency: 1.0,
                    text_keys: events
                        .iter()
                        .enumerate()
                        .map(|(index, text)| fnv_motion::TextKeyEvidence {
                            time: index as f64 / events.len() as f64,
                            text: (*text).to_string(),
                        })
                        .collect(),
                    binding: fnv_motion::BindingEvidence {
                        source_skeleton_path: rig.skeleton_path.clone(),
                        transform_track_count: 1,
                        float_track_count: 0,
                        controller_types: vec!["NiTransformController".to_string()],
                        interpolator_types: vec!["NiTransformInterpolator".to_string()],
                        target_names: targets.iter().map(|target| (*target).to_string()).collect(),
                        required_float_slots: Vec::new(),
                        compatibility: fnv_motion::BindingCompatibility::Verified,
                        compatibility_detail: None,
                    },
                    root_motion,
                },
            ),
            idle_claims: Vec::new(),
        }
    }

    fn add_converted_clips(
        adapters: &mut LegacyBridgeAdapters,
        evidence: &fnv_motion::CreatureMotionFamilyEvidence,
    ) {
        for source_kf in &evidence.referenced_kfs {
            let stem = Path::new(source_kf)
                .file_stem()
                .and_then(|value| value.to_str())
                .expect("KF stem");
            adapters.converted_clips.insert(
                legacy_clip_key(&evidence.rig, source_kf),
                format!("Meshes/Actors/Test/Animations/{stem}.hkx"),
            );
        }
    }

    fn grounded_race_data() -> Fo4RaceDataTarget {
        Fo4RaceDataTarget {
            male_height: 1.0,
            female_height: 1.0,
            male_default_weight: [0.33, 0.34, 0.33],
            female_default_weight: [0.33, 0.34, 0.33],
            flags: vec![crate::source_rig::Fo4RaceFlag::Walks],
            acceleration_rate: 100.0,
            deceleration_rate: 100.0,
            size: crate::source_rig::Fo4RaceSize::Medium,
            injured_health_percent: 0.2,
            body_biped_object: 0,
            aim_angle_tolerance: 30.0,
            flight_radius: 0.0,
            angular_acceleration_rate: 90.0,
            angular_tolerance: 5.0,
            flags_2: Vec::new(),
            xp_value: 10,
            orientation_limit_pitch: 45.0,
            orientation_limit_roll: 20.0,
        }
    }

    fn two_family_fixture(root: &Path) -> (CreatureCorpusExecutionPlan, SourceRoots<'_>) {
        fs::write(root.join("good.hkx"), b"good").expect("good source");
        fs::write(root.join("bad.hkx"), b"bad").expect("bad source");
        let mut good = fixture_artifact("good.hkx", "Meshes/Actors/Good/good.hkx");
        good.root = true;
        let mut bad = fixture_artifact("bad.hkx", "Meshes/Actors/Bad/bad.hkx");
        bad.root = true;
        let jobs = vec![
            fixture_job("family-good", "Good", "Meshes/Actors/Good", vec![good]),
            fixture_job("family-bad", "Bad", "Meshes/Actors/Bad", vec![bad]),
        ];
        let corpus = CreatureCorpusPlan {
            version: 1,
            candidate_count: 0,
            planned: Vec::new(),
            rejected: Vec::new(),
        };
        let native_catalog = fixture_bridge_ledger(&corpus).expect("fixture bridge");
        (
            CreatureCorpusExecutionPlan {
                version: PLAN_VERSION,
                debug_dir: DEFAULT_DEBUG_DIR.to_string(),
                native_catalog,
                corpus,
                jobs,
            },
            SourceRoots {
                mod_root: root,
                source_extracted: root,
                target_extracted: Some(root),
                target_data: Some(root),
            },
        )
    }

    fn fixture_artifact(source: &str, target: &str) -> PlannedArtifact {
        PlannedArtifact {
            source_root: ArtifactSourceRoot::Mod,
            source_path: source.to_string(),
            target_path: target.to_string(),
            kind: ArtifactKind::Asset,
            record_signature: None,
            dependencies: Vec::new(),
            root: false,
            source_blake3: None,
            skeleton_name: None,
            float_slot_names: Vec::new(),
            sequence_index: None,
            event_map: BTreeMap::new(),
            target_sample_rate_hz: None,
        }
    }

    fn fixture_job(
        family: &str,
        slug: &str,
        publish_root: &str,
        artifacts: Vec<PlannedArtifact>,
    ) -> CreatureCorpusJob {
        let source_key = format!("test|test.esm|{}", slug.to_ascii_lowercase());
        CreatureCorpusJob {
            job_id: stable_job_id(&source_key, family, slug),
            source_key,
            output_slug: slug.to_string(),
            family_id: family.to_string(),
            adapter: CreatureAdapterProfile::SkyrimWolf,
            unsupported_capability: None,
            publish_root: publish_root.to_string(),
            graph_paths: artifacts
                .iter()
                .filter(|artifact| {
                    matches!(
                        artifact.kind,
                        ArtifactKind::CharacterHkx
                            | ArtifactKind::RootBehaviorHkx
                            | ArtifactKind::CoreBehaviorHkx
                    )
                })
                .map(|artifact| artifact.target_path.clone())
                .collect(),
            artifacts,
            executable_recipe: None,
            member_source_keys: Vec::new(),
            candidate_recipes: Vec::new(),
            family_bundle_blake3: None,
            preflight_issues: Vec::new(),
        }
    }
}
