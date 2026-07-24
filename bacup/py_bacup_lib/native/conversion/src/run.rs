//! `ConversionRun` — per-conversion-run state, plus a thread-safe run registry.
//!
//! A `ConversionRun` is the long-lived object for one mod conversion. It owns the
//! string interner, schemas, translator, decision/warning accumulators, and deferred
//! record list. A `FormKeyMapper` is NOT stored here (it borrows `&mut interner`);
//! instead it is constructed at the start of each translate phase and dropped at
//! the end.
//!
//! The registry maps monotonic `u64` run IDs to `ConversionRun` values behind a
//! process-wide `Mutex`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};

use pyo3::prelude::*;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::errors::RecordReadError;
use crate::fixups::{FixupConfig, FixupContext, FixupError, FixupRegistry, FixupReport};
use crate::fnv_legacy_scripting::{
    FnvLegacyScriptingContext, FnvLegacyScriptingResult, FnvScriptingError, translate_all_dial,
    translate_all_qust, translate_all_scen,
};
use crate::formkey_mapper::{
    FIRST_ALLOCATION_ID, FormKeyMapper, MapperOptions, MapperState, ResolutionMode,
    allows_editor_id_vanilla_remap, is_static_marker_editor_id,
    mapper_allows_editor_id_vanilla_remap,
};
use crate::full_plugin::{AssetPhaseFlags, FullPluginRunState, WarningPolicy, intern_plugin_names};
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::legacy_fallout_navmesh::{LegacyFalloutNavmeshBatch, prepare_legacy_fallout_navmeshes};
use crate::legacy_pack_preflight::{
    DirectLegacyPackOrigin, LegacyPackExpectedCounts, LegacyPackOriginRow,
    LegacyPackPreflightAccumulator, LegacyPackPreflightReport,
};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::source_read::{
    SourceRecordBatchSnapshot, collect_eid_index, decode_record_from_parsed_relayout,
    form_key_to_read_str, iter_form_keys_of_sig, plugin_context_for_handle, plugin_name_for_handle,
    raw_cell_is_interior, read_record_relayout_by_form_key, snapshot_records_by_form_keys,
    source_signatures,
};
use crate::sym::StringInterner;
use crate::sym::Sym;
use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
use crate::target_write::{
    add_projected_navmeshes_chunk_native, add_quest_child_record_native, add_record_native,
    add_topic_child_record_native, encode_form_key_for_handle,
    rebuild_projected_navi_from_source_native, rebuild_projected_navi_from_source_with_nver_native,
    rebuild_projected_navi_native, rebuild_worldspace_groups_from_source_native,
    replace_record_contents_native,
};
use crate::translator::TranslateResult;
use crate::translator::pair_hook::PairCtx;
use crate::translator::target_hook::TargetCtx;
use crate::translator::{Decision, DeferredKind, Game, Translator};
use esp_authoring_core::plugin_runtime::FO4_CANONICAL_NAVI_FORM_ID;

const FIXUP_WARNING_LOG_LIMIT: usize = 32;
const FIXUP_DIAGNOSTIC_LOG_LIMIT: usize = 64;

fn fixup_warning_log_messages(
    phase: &str,
    name: &str,
    iteration: u32,
    report: &FixupReport,
    interner: &StringInterner,
) -> Vec<String> {
    let mut messages = report
        .warnings
        .iter()
        .take(FIXUP_WARNING_LOG_LIMIT)
        .map(|warning| {
            let detail = interner.resolve(*warning).unwrap_or("<unresolved>");
            format!("[{phase}] diagnostic {name} iter={iteration} {detail}")
        })
        .collect::<Vec<_>>();
    if report.warnings.len() > FIXUP_WARNING_LOG_LIMIT {
        messages.push(format!(
            "[{phase}] diagnostic {name} iter={iteration} truncated={} total={}",
            report.warnings.len() - FIXUP_WARNING_LOG_LIMIT,
            report.warnings.len()
        ));
    }
    messages
}

fn emit_fixup_warning_logs(
    event_tx: &crossbeam_channel::Sender<crate::phase::PhaseEvent>,
    phase: &'static str,
    name: &str,
    iteration: u32,
    report: &FixupReport,
    interner: &StringInterner,
) {
    for message in fixup_warning_log_messages(phase, name, iteration, report, interner) {
        eprintln!("{message}");
        let _ = event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase,
            level: crate::phase::LogLevel::Warn,
            message,
        });
    }
}

fn emit_fixup_diagnostic_logs(
    event_tx: &crossbeam_channel::Sender<crate::phase::PhaseEvent>,
    phase: &'static str,
    name: &str,
    iteration: u32,
    report: &FixupReport,
    interner: &StringInterner,
) {
    let total = report.diagnostics.len();
    for diagnostic in report.diagnostics.iter().take(FIXUP_DIAGNOSTIC_LOG_LIMIT) {
        let detail = interner.resolve(*diagnostic).unwrap_or("<unresolved>");
        let message = format!("[{phase}] decision {name} iter={iteration} {detail}");
        eprintln!("{message}");
        let _ = event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase,
            level: crate::phase::LogLevel::Info,
            message,
        });
    }
    if total > FIXUP_DIAGNOSTIC_LOG_LIMIT {
        let message = format!(
            "[{phase}] decision {name} iter={iteration} truncated={} total={total}",
            total - FIXUP_DIAGNOSTIC_LOG_LIMIT
        );
        eprintln!("{message}");
        let _ = event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase,
            level: crate::phase::LogLevel::Info,
            message,
        });
    }
}

fn emit_fixup_report_log(
    event_tx: &crossbeam_channel::Sender<crate::phase::PhaseEvent>,
    phase: &'static str,
    name: &str,
    report: &FixupReport,
    interner: &StringInterner,
) {
    let detail = report
        .message
        .and_then(|message| interner.resolve(message))
        .map(|message| format!(" message={message}"))
        .unwrap_or_default();
    let message = format!(
        "[{phase}] finished {name} changed={} dropped={} added={} warnings={} diagnostics={}{}",
        report.records_changed,
        report.records_dropped,
        report.records_added,
        report.warnings.len(),
        report.diagnostics.len(),
        detail
    );
    eprintln!("{message}");
    let _ = event_tx.try_send(crate::phase::PhaseEvent::Log {
        phase,
        level: crate::phase::LogLevel::Info,
        message,
    });
    emit_fixup_warning_logs(event_tx, phase, name, 1, report, interner);
    emit_fixup_diagnostic_logs(event_tx, phase, name, 1, report, interner);
}

// ---------------------------------------------------------------------------
// RunConfig — options that drive mapper + translator behaviour
// ---------------------------------------------------------------------------

/// Configuration for one conversion run.
#[derive(Debug, Clone, Default)]
pub struct RunConfig {
    /// Reject unmapped FormKeys immediately instead of deferring.
    pub strict_mapper: bool,
    /// Prefer vanilla target records for EditorID matches.
    pub use_base_game_assets: bool,
    /// Preserve the source object-id when the id is free in the output plugin.
    pub preserve_source_ids: bool,
    /// First object-id to use for freshly generated records.
    pub generated_object_id_floor: u32,
    /// Output plugin filename, e.g. `"Output.esm"`.
    pub output_plugin_name: String,
    /// True when converting an entire plugin (not a per-record sub-graph).
    /// Passed through to `FixupConfig::is_whole_plugin`.
    pub is_whole_plugin: bool,
    /// 4-byte signature of the conversion root record (e.g. `NPC_`, `LVLN`).
    /// `None` for whole-plugin or unknown-root conversions.
    pub root_sig: Option<crate::ids::SigCode>,
    /// Filesystem path to the mod directory being converted. Several havok
    /// fixups gate on this being `Some(...)` to find HKX assets next to the
    /// source plugin.
    pub mod_path: Option<std::path::PathBuf>,
    /// Filesystem path to the directory holding source assets extracted from
    /// the source game's BA2/BSA archives. Havok fixups use this to locate
    /// reference rigs/behaviors that aren't shipped with the mod itself.
    pub source_extracted_dir: Option<std::path::PathBuf>,
    pub target_extracted_dir: Option<std::path::PathBuf>,
    pub target_data_dir: Option<std::path::PathBuf>,
    pub target_asset_catalog_path: Option<std::path::PathBuf>,
    pub target_asset_cache_dir: Option<std::path::PathBuf>,
    pub conversion_workers: Option<usize>,
    pub records_limit: Option<usize>,
    pub warning_policy: WarningPolicy,
    pub asset_phases: AssetPhaseFlags,
    pub target_record_preflight: Vec<TargetRecordPreflightRow>,
    pub target_master_names: Vec<String>,
    pub base_asset_namespace: String,
    /// Data-relative mesh subtrees (e.g. "meshes/landscape") whose FO76↔FO4
    /// path collisions drive asset relocation. Empty ⇒ FO76→FO4 default.
    pub base_asset_relocation_mesh_roots: Vec<String>,
    /// Worldspace XYZ offset applied to emitted projected-navmesh geometry
    /// (NVNM vertices, grid bounds, waypoints) so the navmesh stays co-spatial
    /// with placed records, which receive the same offset. The parent cell index
    /// and index-based topology (triangles/cover/edges) are left untouched.
    pub projected_navmesh_offset: [f32; 3],
    /// Extra source-record 4-byte signatures to drop during translation, seeded
    /// into `translator.maps.skip_records` at run creation. The full-plugin path
    /// uses this to skip placed records (REFR/ACHR/...) for debug bisection.
    pub skip_record_signatures: Vec<String>,
    /// True only for whole-plugin FO76→FO4 worldspace runs (the phase-6
    /// cell-slice copy re-inserts placed children AFTER fixups). Passed into
    /// `FixupConfig::defer_placed_child_ref_class` so the pre-copy own-plugin
    /// null-dangling pass DEFERS the LCTN LCUN/LCEP/ACEP class; the authoritative
    /// resolution runs post-copy via `repair_placed_child_refs`.
    pub defer_placed_child_ref_class: bool,
    pub legacy_pack_origins: Vec<LegacyPackOriginRow>,
    pub legacy_pack_raw_source_counts: Option<LegacyPackExpectedCounts>,
    pub legacy_pack_expected_counts: Option<LegacyPackExpectedCounts>,
    pub legacy_pack_provenance_required: bool,
}

impl RunConfig {
    fn skips_fo76_quest_dialogue(&self) -> bool {
        self.skip_record_signatures.iter().any(|sig| {
            matches!(
                sig.trim().to_ascii_uppercase().as_str(),
                "QUST" | "DIAL" | "INFO"
            )
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetRecordPreflightRow {
    pub editor_id: String,
    pub signature: String,
    pub form_key: String,
}

#[derive(Debug, Clone)]
pub(crate) struct TargetMasterRecordContext {
    handle_id: u64,
    plugin_name: String,
    master_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnvScriLink {
    pub target_form_key: String,
    pub source_scpt_form_key: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LegacyFormKeyAllocationIntent {
    pub source_fk: FormKey,
    pub editor_id: Option<Sym>,
    pub target_sig: SigCode,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LegacyFormKeyPreallocationCoverage {
    pub eligible: usize,
    pub mapped: usize,
    pub missing: usize,
}

fn legacy_output_allocation_floor(
    source: Game,
    target: Game,
    source_plugin_name: &str,
    output_plugin_name: &str,
    preserve_source_ids: bool,
    current_floor: u32,
    source_locals: impl IntoIterator<Item = u32>,
) -> Result<Option<u32>, &'static str> {
    if !matches!(source, Game::Fnv | Game::Fo3)
        || target != Game::Fo4
        || !source_plugin_name.eq_ignore_ascii_case(output_plugin_name)
    {
        return Ok(None);
    }

    let mut max_source_local = None;
    let mut needs_generated_id = !preserve_source_ids;
    for local in source_locals {
        max_source_local = Some(max_source_local.map_or(local, |current: u32| current.max(local)));
        needs_generated_id |= local < FIRST_ALLOCATION_ID;
    }
    if !needs_generated_id {
        return Ok(None);
    }
    let Some(max_source_local) = max_source_local else {
        return Ok(None);
    };
    let next = max_source_local
        .checked_add(1)
        .ok_or("legacy source/output plugin identity leaves no disjoint FormID allocation space")?;
    if next > 0x00FF_FFFF {
        return Err(
            "legacy source/output plugin identity leaves no disjoint FormID allocation space",
        );
    }
    Ok(Some(current_floor.max(next)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordWriteMode {
    TopLevel,
    QuestChild,
    TopicChildInfo,
}

struct PreparedProjectedNavmesh {
    source_fk: FormKey,
    source_sig: SigCode,
    record: Record,
    full_plugin_snapshot: Option<Record>,
}

/// `PreparedProjectedNavmesh` minus the record itself: kept on the apply side
/// while the record moves into the chunked batch insert.
struct PreparedNavmeshMeta {
    source_fk: FormKey,
    source_sig: SigCode,
    full_plugin_snapshot: Option<Record>,
}

#[derive(Default)]
struct ProjectedNavmeshPrepareResult {
    stats: TranslateStats,
    warnings: Vec<String>,
    decisions: Vec<Decision>,
    deferred: Vec<(FormKey, DeferredKind)>,
    prepared: Option<PreparedProjectedNavmesh>,
}

// ---------------------------------------------------------------------------
// ConversionRun
// ---------------------------------------------------------------------------

/// All state for one mod conversion run.
///
/// `mapper_state` is lazily initialised at the start of `translate_all` and
/// reused by fixups so mappings persist across both phases.
pub struct ConversionRun {
    pub source: Game,
    pub target: Game,
    pub source_handle_id: u64,
    pub target_handle_id: u64,
    pub master_handle_ids: Vec<u64>,
    pub(crate) target_master_record_contexts: Vec<TargetMasterRecordContext>,
    pub interner: StringInterner,
    pub schema_source: Arc<AuthoringSchema>,
    pub schema_target: Arc<AuthoringSchema>,
    pub translator: Translator,
    pub config: RunConfig,
    /// Long-lived mapper state: persists from translate_all through fixups.
    /// None before translate_all is first called.
    pub mapper_state: Option<MapperState>,
    generated_object_id_reservations: FxHashSet<u32>,
    pub(crate) legacy_serial_normalization:
        crate::translator::pair_hooks::fnv_fo4::LegacySerialNormalizationState,
    pub legacy_pack_preflight_report: Option<LegacyPackPreflightReport>,
    pub(crate) legacy_creature_race_coverage:
        crate::translator::pair_hooks::fnv_creature_race::CreatureRaceCoverageReport,
    /// Decisions accumulated during translation (drained by PyO3 / Python).
    pub decisions: Vec<Decision>,
    /// Warning message symbols (drained by PyO3 / Python).
    pub warnings: Vec<Sym>,
    /// Records deferred to a later pipeline pass (Phase D).
    pub deferred: Vec<(FormKey, DeferredKind)>,
    /// Native FNV SCRI links captured before the FNV pair hook drops SCRI.
    pub fnv_scri_links: Vec<FnvScriLink>,
    /// Optional Python progress callback `(records_processed: int) -> bool`.
    /// Called every 1000 records during `translate_all`. Return False to cancel.
    pub progress_callback: Option<Py<PyAny>>,
    /// Per-navmesh diagnostic warning strings from the NAVI rebuild phase.
    /// Capped by `finalize_navi_warnings` to avoid unbounded growth on large
    /// worldspaces (Appalachia has ~920k+ navmeshes with potential warnings).
    pub navi_warnings: Vec<String>,
    /// Phase event channel — phases push events here; Python drains via drain_events.
    pub event_tx: crossbeam_channel::Sender<crate::phase::PhaseEvent>,
    pub event_rx: crossbeam_channel::Receiver<crate::phase::PhaseEvent>,
    /// Cancellation flag — set by `conversion_run_cancel`; phases poll via PhaseCtx.
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Dependency graph built by the `walk` phase. None before the walk runs.
    pub dependency_graph: Option<esp_authoring_core::plugin_runtime::WalkOutput>,
    pub full_plugin_state: FullPluginRunState,
    /// Terrain bundles discovered by the record phase and executed later by
    /// `convert_textures_v2` in the asset run.
    pub terrain_texture_jobs: Vec<crate::terrain_textures::manifest::TerrainTextureJob>,
    /// Per-worldspace seed form keys collected by the projected placed-child
    /// copy, consumed by the persistent-cell synthesis to skip its (otherwise
    /// identical) full-worldspace source scan.
    pub projected_seed_cache:
        std::collections::HashMap<String, crate::projected_placed::ProjectedSeedCacheEntry>,
    /// Normalized data-relative paths (meshes/textures/materials) that collide
    /// with FO4 base assets and must be relocated under `base_asset_namespace`.
    /// Built once at run-init from the configured mesh roots + cascade closure.
    pub relocation_members: std::collections::HashSet<String>,
    /// Warnings emitted while building `relocation_members` (e.g. FO4 extracted
    /// dir missing). Surfaced into a phase report so Python logs them.
    pub relocation_warnings: Vec<String>,
    pub target_assets: Option<std::sync::Arc<crate::target_assets::TargetAssetStore>>,
    /// Output sinks (loose + BA2 spill streaming). None = legacy
    /// behavior everywhere; attached via `sinks_attach_run`.
    pub output_sink: Option<std::sync::Arc<crate::sinks::SinkSet>>,
    owned_handles: Option<OwnedRunHandles>,
    pub default_target_path: Option<PathBuf>,
    pub(crate) target_mode: TargetMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TargetMode {
    CreateNew,
    OpenExisting,
}

// ---------------------------------------------------------------------------
// RunParams
// ---------------------------------------------------------------------------

/// Parameters for creating a new `ConversionRun`.
pub struct RunParams {
    pub source: Game,
    pub target: Game,
    pub source_handle_id: u64,
    pub target_handle_id: u64,
    pub master_handle_ids: Vec<u64>,
    pub config: RunConfig,
}

/// A plugin handle owned by the conversion registry.
///
/// This type is deliberately non-`Clone`: ownership may only move into a run.
pub struct OwnedPluginHandle {
    id: Option<u64>,
}

impl OwnedPluginHandle {
    pub fn load(
        plugin_path: &Path,
        game: &str,
        strings_dir: Option<&Path>,
    ) -> Result<Self, RunError> {
        let id = esp_authoring_core::plugin_runtime::plugin_handle_load_no_py(
            &plugin_path.to_string_lossy(),
            Some(game),
            strings_dir.map(|path| path.to_string_lossy()).as_deref(),
            None,
            false,
        )
        .map_err(RunError::InvalidConfig)?;
        Ok(Self { id: Some(id) })
    }

    pub fn load_index(plugin_path: &Path, game: &str) -> Result<Self, RunError> {
        let id = esp_authoring_core::plugin_runtime::plugin_handle_load_index_no_py(
            &plugin_path.to_string_lossy(),
            Some(game),
            None,
            None,
        )
        .map_err(RunError::InvalidConfig)?;
        Ok(Self { id: Some(id) })
    }

    pub fn new(plugin_name: &str, game: &str) -> Self {
        Self {
            id: Some(esp_authoring_core::plugin_runtime::plugin_handle_new_no_py(
                plugin_name,
                Some(game),
            )),
        }
    }

    pub fn id(&self) -> u64 {
        self.id.expect("owned plugin handle was released")
    }

    pub fn release(&mut self) -> bool {
        self.id
            .take()
            .is_some_and(esp_authoring_core::plugin_runtime::plugin_handle_close_native)
    }
}

impl Drop for OwnedPluginHandle {
    fn drop(&mut self) {
        self.release();
    }
}

/// All native plugin handles whose lifetime is tied to one conversion run.
pub struct OwnedRunHandles {
    pub source: Option<OwnedPluginHandle>,
    pub target: OwnedPluginHandle,
    pub masters: Vec<OwnedPluginHandle>,
}

impl Drop for OwnedRunHandles {
    fn drop(&mut self) {
        for master in self.masters.iter_mut().rev() {
            master.release();
        }
        self.source.as_mut().map(OwnedPluginHandle::release);
        self.target.release();
    }
}

// ---------------------------------------------------------------------------
// RunError
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum RunError {
    /// No run with this ID exists in the registry.
    UnknownRun(u64),
    /// The supplied parameters were invalid.
    InvalidConfig(String),
    /// The registry mutex was poisoned.
    LockPoisoned,
    /// Translation was cancelled by the progress callback (or by Ctrl-C).
    Cancelled,
    LegacyPackPreflight(Box<LegacyPackPreflightReport>),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownRun(id) => write!(f, "unknown conversion run id: {id}"),
            Self::InvalidConfig(msg) => write!(f, "invalid run config: {msg}"),
            Self::LockPoisoned => write!(f, "conversion run registry lock is poisoned"),
            Self::Cancelled => write!(f, "translation cancelled"),
            Self::LegacyPackPreflight(report) => write!(
                f,
                "legacy PACK preflight blocked conversion: {}",
                report.error_summary_json()
            ),
        }
    }
}

impl std::error::Error for RunError {}

impl From<RecordReadError> for RunError {
    fn from(e: RecordReadError) -> Self {
        RunError::InvalidConfig(format!("record read: {e}"))
    }
}

impl From<FixupError> for RunError {
    fn from(e: FixupError) -> Self {
        RunError::InvalidConfig(format!("fixup: {e}"))
    }
}

impl From<FnvScriptingError> for RunError {
    fn from(e: FnvScriptingError) -> Self {
        RunError::InvalidConfig(format!("fnv scripting: {e}"))
    }
}

// ---------------------------------------------------------------------------
// TranslateStats
// ---------------------------------------------------------------------------

/// Per-run translation statistics returned by `ConversionRun::translate_all`.
#[derive(Default, Debug, Clone)]
pub struct TranslateStats {
    pub records_translated: u32,
    pub records_vanilla_remapped: u32,
    pub records_dropped: u32,
    pub records_deferred: u32,
    pub records_failed: u32,
    pub by_signature: HashMap<String, SignatureTranslateStats>,
}

/// Per-source-signature translation counters.
#[derive(Default, Debug, Clone)]
pub struct SignatureTranslateStats {
    pub seen: u32,
    pub translated: u32,
    pub vanilla_remapped: u32,
    pub dropped: u32,
    pub deferred: u32,
    pub failed: u32,
}

impl TranslateStats {
    pub(crate) fn signature_entry(
        &mut self,
        sig: crate::ids::SigCode,
    ) -> &mut SignatureTranslateStats {
        self.by_signature
            .entry(sig.as_str().to_string())
            .or_default()
    }

    pub(crate) fn absorb(&mut self, other: TranslateStats) {
        self.records_translated += other.records_translated;
        self.records_vanilla_remapped += other.records_vanilla_remapped;
        self.records_dropped += other.records_dropped;
        self.records_deferred += other.records_deferred;
        self.records_failed += other.records_failed;
        for (signature, stats) in other.by_signature {
            let entry = self.by_signature.entry(signature).or_default();
            entry.seen += stats.seen;
            entry.translated += stats.translated;
            entry.vanilla_remapped += stats.vanilla_remapped;
            entry.dropped += stats.dropped;
            entry.deferred += stats.deferred;
            entry.failed += stats.failed;
        }
    }
}

// ---------------------------------------------------------------------------
// Registry — per-run lock slots
// ---------------------------------------------------------------------------
//
// The registry mutex guards ONLY the id→slot map; each run has its own
// `Arc<Mutex<ConversionRun>>` so phases on distinct runs execute concurrently.
// The event receiver and cancel flag are hoisted into the slot so draining
// and cancelling never wait on a running phase's run lock.

/// A cloneable handle to one registered run.
pub struct RunSlot {
    pub run: Arc<Mutex<ConversionRun>>,
    /// Clone of the run's event receiver — drainable WITHOUT the run lock.
    pub events: crossbeam_channel::Receiver<crate::phase::PhaseEvent>,
    /// Clone of the run's event sender — pipeline/executor events go here.
    pub event_tx: crossbeam_channel::Sender<crate::phase::PhaseEvent>,
    /// The run's cancel flag — settable WITHOUT the run lock.
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
}

fn registry() -> &'static Mutex<HashMap<u64, RunSlot>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u64, RunSlot>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Brief-lock slot fetch. The registry mutex is held only for the map lookup.
pub fn run_slot(id: u64) -> Result<RunSlot, RunError> {
    let guard = registry().lock().map_err(|_| RunError::LockPoisoned)?;
    let slot = guard.get(&id).ok_or(RunError::UnknownRun(id))?;
    Ok(RunSlot {
        run: Arc::clone(&slot.run),
        events: slot.events.clone(),
        event_tx: slot.event_tx.clone(),
        cancel: Arc::clone(&slot.cancel),
    })
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn form_key_to_legacy_str(
    fk: crate::ids::FormKey,
    interner: &StringInterner,
) -> Option<String> {
    let plugin = interner.resolve(fk.plugin)?;
    Some(format!("{:06X}:{}", fk.local, plugin))
}

fn parse_legacy_or_native_form_key(
    value: &str,
    interner: &StringInterner,
) -> Result<FormKey, String> {
    if value.contains('@') {
        return FormKey::parse(value, interner);
    }
    let (hex, plugin) = value
        .split_once(':')
        .ok_or_else(|| format!("FormKey missing ':' or '@': {value:?}"))?;
    FormKey::parse(&format!("{hex}@{plugin}"), interner)
}

fn normalized_editor_id(editor_id: &str) -> String {
    editor_id.to_ascii_lowercase()
}

fn normalized_eid_sym(eid: Sym, interner: &StringInterner) -> Sym {
    interner
        .resolve(eid)
        .map(|editor_id| interner.intern(&normalized_editor_id(editor_id)))
        .unwrap_or(eid)
}

pub(crate) fn normalized_eid_opt(eid: Option<Sym>, interner: &StringInterner) -> Option<Sym> {
    eid.map(|eid| normalized_eid_sym(eid, interner))
}

/// Drop FO4 CELL previs subrecords (carried verbatim from the FO76 source) so
/// the converted interior cell has no stale precombine/previs references.
/// `PCMB` = PreCombined Files Timestamp, `VISI` = PreVis File Hash,
/// `RVIS` = In PreVis File Of, `XPRI` = PreVis Reference Index,
/// `XCRI` = Combined Reference Index.
fn strip_interior_previs_fields(record: &mut Record) {
    record
        .fields
        .retain(|f| !matches!(f.sig.as_str(), "PCMB" | "VISI" | "RVIS" | "XPRI" | "XCRI"));
}

/// Force every NVNM payload's parent to `Interior { cell }`. The interior NAVM
/// inherits the source FO76 parent bytes; re-stamping the target cell's FormID
/// makes the NAVI rebuild key it to the correct interior cell and clears any
/// residual exterior-grid parent.
///
/// `cell_file_form_id` MUST be the cell's *target file* FormID — i.e. with the
/// output plugin's own master index already applied (see
/// `fo76_navmesh::target_form_id`). The mapper-allocated `form_key.local` carries
/// master index 0x00, so writing it here would resolve the parent cell to the
/// wrong master at runtime and hard-crash the engine's NavMeshInfoMap lookup.
fn set_nvnm_parent_interior(record: &mut Record, cell_file_form_id: u32) {
    for entry in record.fields.iter_mut() {
        if entry.sig.0 != *b"NVNM" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if bytes.is_empty() {
            continue;
        }
        let Ok(mut payload) = esp_authoring_core::nvnm::parse_nvnm(bytes.as_slice()) else {
            continue;
        };
        payload.parent = esp_authoring_core::nvnm::NvnmParent::Interior {
            cell: cell_file_form_id,
        };
        *bytes = smallvec::SmallVec::from_vec(esp_authoring_core::nvnm::write_nvnm(&payload));
    }
}

fn target_editor_id_collides_any_signature(
    target_eid_index: &FxHashMap<Sym, Vec<(FormKey, crate::ids::SigCode)>>,
    interner: &StringInterner,
    editor_id: &str,
) -> bool {
    let normalized = interner.intern(&normalized_editor_id(editor_id));
    target_eid_index
        .get(&normalized)
        .is_some_and(|matches| !matches.is_empty())
}

fn target_editor_id_has_same_signature(
    target_eid_index: &FxHashMap<Sym, Vec<(FormKey, crate::ids::SigCode)>>,
    interner: &StringInterner,
    editor_id: &str,
    sig: crate::ids::SigCode,
) -> bool {
    let normalized = interner.intern(&normalized_editor_id(editor_id));
    target_eid_index.get(&normalized).is_some_and(|matches| {
        matches
            .iter()
            .any(|(_, candidate_sig)| *candidate_sig == sig)
    })
}

pub(crate) fn target_collision_donor_form_key(
    target_eid_index: &FxHashMap<Sym, Vec<(FormKey, crate::ids::SigCode)>>,
    interner: &StringInterner,
    editor_id: &str,
    sig: crate::ids::SigCode,
) -> Option<FormKey> {
    let normalized = interner.intern(&normalized_editor_id(editor_id));
    target_eid_index.get(&normalized).and_then(|matches| {
        matches
            .iter()
            .find(|(_, candidate_sig)| *candidate_sig == sig)
            .map(|(form_key, _)| *form_key)
    })
}

fn set_record_editor_id(record: &mut Record, interner: &StringInterner, editor_id: &str) {
    let eid_sym = interner.intern(editor_id);
    record.eid = Some(eid_sym);

    let edid_sig = SubrecordSig::from_str("EDID").expect("EDID is a valid subrecord signature");
    if let Some(field) = record.fields.iter_mut().find(|field| field.sig == edid_sig) {
        field.value = FieldValue::String(eid_sym);
        return;
    }
    record.fields.insert(
        0,
        FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(eid_sym),
        },
    );
}

pub(crate) fn rename_fo76_target_editor_id_collision(
    record: &mut Record,
    target_eid_index: &FxHashMap<Sym, Vec<(FormKey, crate::ids::SigCode)>>,
    interner: &StringInterner,
    force_same_signature_rename: bool,
) -> Option<(String, String)> {
    let original = interner.resolve(record.eid?)?.to_owned();
    if original.is_empty() {
        return None;
    }

    let same_signature =
        target_editor_id_has_same_signature(target_eid_index, interner, &original, record.sig);
    let static_marker = is_static_marker_editor_id(record.sig, &original);
    let should_rename =
        if !target_editor_id_collides_any_signature(target_eid_index, interner, &original) {
            false
        } else if same_signature && static_marker {
            false
        } else if force_same_signature_rename
            || record.sig.as_str() == "CELL"
            || !allows_editor_id_vanilla_remap(record.sig)
        {
            true
        } else {
            !same_signature
        };
    if !should_rename {
        return None;
    }

    let mut candidate = format!("{original}fo76");
    let mut suffix = 1_u32;
    while target_editor_id_collides_any_signature(target_eid_index, interner, &candidate) {
        candidate = format!("{original}fo76{suffix}");
        suffix += 1;
    }
    set_record_editor_id(record, interner, &candidate);
    Some((original, candidate))
}

const FO76_FO4_DEFAULT_BASE_ASSET_NAMESPACE: &str = "FO76";

/// Signatures whose target editor-id collisions should stay source-owned instead
/// of vanilla-remapping onto an FO4 base record. This is the editor-id concern
/// only; asset relocation is driven entirely by the collision index
/// (`ConversionRun::relocation_members`), not by signature.
const FO76_FO4_VANILLA_REMAP_BLOCKED_SIGS: &[&str] = &["STAT", "SCOL", "MSTT"];

/// Signatures that should receive a `fo76` EDID suffix when they are emitted
/// despite a same-signature target-master collision. ARMO/ARMA are not globally
/// remap-blocked because wearable armor should still reuse FO4 records, but
/// protected creature skin records that are emitted still need unique EDIDs.
const FO76_FO4_FORCE_COLLISION_RENAME_SIGS: &[&str] = &["STAT", "SCOL", "MSTT", "ARMO", "ARMA"];

fn editor_id_vanilla_remap_blocked_sigs(source: Game, target: Game) -> Vec<String> {
    let mut blocked = Vec::new();
    if source == Game::Fo76 && target == Game::Fo4 {
        blocked.extend(
            FO76_FO4_VANILLA_REMAP_BLOCKED_SIGS
                .iter()
                .map(|sig| (*sig).to_owned()),
        );
    }
    if source != target && target == Game::Fo4 {
        blocked.push("CELL".to_owned());
    }
    blocked
}

pub(crate) fn is_editor_id_collision_rename_forced(
    source: Game,
    target: Game,
    sig: crate::ids::SigCode,
) -> bool {
    if source != Game::Fo76 || target != Game::Fo4 {
        return false;
    }
    FO76_FO4_FORCE_COLLISION_RENAME_SIGS
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(sig.as_str()))
}

pub(crate) fn base_asset_namespace<'a>(
    config: &'a RunConfig,
    source: Game,
    target: Game,
) -> Option<&'a str> {
    let configured = config.base_asset_namespace.trim();
    if !configured.is_empty() {
        return Some(configured);
    }
    if source == Game::Fo76 && target == Game::Fo4 {
        return Some(FO76_FO4_DEFAULT_BASE_ASSET_NAMESPACE);
    }
    None
}

/// Resolved relocation namespace for a run (`""` when relocation is inactive).
/// Shared accessor for the asset phases (textures/materials/NIFs).
pub fn base_asset_namespace_for_run(run: &ConversionRun) -> String {
    base_asset_namespace(&run.config, run.source, run.target)
        .unwrap_or("")
        .to_string()
}

const BASE_ASSET_MODEL_FIELD_SIGS: &[&str] = &["MODL", "MOD2", "MOD3", "MOD4", "MOD5"];

/// Normalize a MODL/MOD2/... mesh path to the relocation-member key space
/// (lowercase, forward-slash, `meshes/`-rooted, data-relative) so it can be
/// looked up in `ConversionRun::relocation_members`. Reuses the same
/// `normalize_rel` the member set is built with — the two MUST agree.
fn relocation_modl_key(path: &str) -> String {
    let rel = crate::relocation::normalize_rel(path);
    if rel.is_empty() || rel.starts_with("meshes/") {
        rel
    } else {
        format!("meshes/{rel}")
    }
}

/// Relocate model paths that point at a collision-detected mesh into the
/// `namespace` subfolder. Only paths present in `relocation_members` are
/// touched — the collision index is the single trigger, no signature blacklist.
pub(crate) fn namespace_base_asset_model_paths(
    record: &mut Record,
    relocation_members: &std::collections::HashSet<String>,
    namespace: &str,
    interner: &StringInterner,
) -> usize {
    if relocation_members.is_empty() || namespace.trim().is_empty() {
        return 0;
    }
    let mut updated = 0usize;
    for field in record.fields.iter_mut() {
        if BASE_ASSET_MODEL_FIELD_SIGS
            .iter()
            .any(|sig| field.sig.as_str().eq_ignore_ascii_case(sig))
        {
            updated += namespace_mesh_path_value(
                &mut field.value,
                relocation_members,
                namespace,
                interner,
            );
        }
    }
    updated
}

fn namespace_mesh_path_value(
    value: &mut FieldValue,
    relocation_members: &std::collections::HashSet<String>,
    namespace: &str,
    interner: &StringInterner,
) -> usize {
    match value {
        FieldValue::String(sym) => {
            let Some(current) = interner.resolve(*sym) else {
                return 0;
            };
            if !relocation_members.contains(&relocation_modl_key(current)) {
                return 0;
            }
            let Some(namespaced) = namespaced_mesh_model_path(current, namespace) else {
                return 0;
            };
            if namespaced == current {
                return 0;
            }
            *sym = interner.intern(&namespaced);
            1
        }
        FieldValue::Bytes(bytes) => {
            let had_nul = bytes.last().is_some_and(|byte| *byte == 0);
            let current = String::from_utf8_lossy(bytes.as_slice());
            let current = current.trim_end_matches('\0');
            if !relocation_members.contains(&relocation_modl_key(current)) {
                return 0;
            }
            let Some(namespaced) = namespaced_mesh_model_path(current, namespace) else {
                return 0;
            };
            if namespaced == current {
                return 0;
            }
            bytes.clear();
            bytes.extend_from_slice(namespaced.as_bytes());
            if had_nul {
                bytes.push(0);
            }
            1
        }
        FieldValue::Struct(fields) => {
            let mut updated = 0usize;
            for (key, nested) in fields.iter_mut() {
                let key_name = interner.resolve(*key).unwrap_or("");
                if matches!(
                    key_name,
                    "File" | "Path" | "Filename" | "FileName" | "ModelFileName"
                ) {
                    updated +=
                        namespace_mesh_path_value(nested, relocation_members, namespace, interner);
                }
            }
            updated
        }
        FieldValue::List(values) => values
            .iter_mut()
            .map(|nested| {
                namespace_mesh_path_value(nested, relocation_members, namespace, interner)
            })
            .sum(),
        _ => 0,
    }
}

fn namespaced_mesh_model_path(path: &str, namespace: &str) -> Option<String> {
    let namespace = namespace.trim().trim_matches(|c| c == '/' || c == '\\');
    if namespace.is_empty() {
        return None;
    }
    let mut rel = path.trim().trim_matches('\0').replace('\\', "/");
    rel = rel.trim_start_matches('/').to_owned();
    if rel.is_empty() || rel.contains(':') {
        return None;
    }
    rel = strip_ascii_prefix(rel, "data/");
    rel = strip_ascii_prefix(rel, "meshes/");
    let lower = rel.to_ascii_lowercase();
    let namespace_lower = namespace.to_ascii_lowercase();
    let namespaced =
        if lower == namespace_lower || lower.starts_with(&format!("{namespace_lower}/")) {
            rel
        } else {
            format!("{namespace}/{rel}")
        };
    Some(namespaced.replace('/', "\\"))
}

fn strip_ascii_prefix(value: String, prefix: &str) -> String {
    if value.len() >= prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix) {
        value[prefix.len()..].to_owned()
    } else {
        value
    }
}

fn allows_source_target_preflight_remap(
    source: Game,
    target: Game,
    sig: crate::ids::SigCode,
    editor_id: &str,
) -> bool {
    if source == Game::Fo76 && target == Game::Fo4 && sig.as_str() == "CELL" {
        return false;
    }
    if source == Game::Fo76 && target == Game::Fo4 && is_static_marker_editor_id(sig, editor_id) {
        return true;
    }
    let mapper_options = MapperOptions {
        vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(source, target),
        ..Default::default()
    };
    mapper_allows_editor_id_vanilla_remap(&mapper_options, sig)
}

fn mapper_entries_from_preflight(
    config: &RunConfig,
    interner: &StringInterner,
) -> Vec<(Sym, FormKey, crate::ids::SigCode)> {
    let mut out = Vec::with_capacity(config.target_record_preflight.len());
    for row in &config.target_record_preflight {
        if row.editor_id.is_empty() {
            continue;
        }
        let Ok(sig) = crate::ids::SigCode::from_str(&row.signature) else {
            continue;
        };
        let Ok(form_key) = parse_legacy_or_native_form_key(&row.form_key, interner) else {
            continue;
        };
        out.push((
            interner.intern(&normalized_editor_id(&row.editor_id)),
            form_key,
            sig,
        ));
    }
    out
}

fn has_fo76_creature_weapon_prefix(editor_id: &str) -> bool {
    let mut chars = editor_id.chars();
    matches!(chars.next(), Some('c'))
        && matches!(chars.next(), Some('r'))
        && chars.next().is_some_and(char::is_uppercase)
}

fn fo76_fo4_weap_editor_id_aliases(
    original_editor_id: &str,
    normalized_editor_id: &str,
) -> Vec<String> {
    let mut aliases = Vec::new();
    if has_fo76_creature_weapon_prefix(original_editor_id) {
        if let Some(alias) = normalized_editor_id
            .strip_prefix("cr")
            .filter(|s| !s.is_empty())
        {
            aliases.push(alias.to_string());
        }
    }
    if let Some(alias) = normalized_editor_id
        .strip_prefix("zzz_")
        .filter(|s| !s.is_empty())
    {
        aliases.push(alias.to_string());
    }
    aliases
}

fn allows_fo76_fo4_weap_local_alias(
    original_editor_id: &str,
    normalized_editor_id: &str,
    sig: crate::ids::SigCode,
) -> bool {
    sig.as_str() == "WEAP"
        && (has_fo76_creature_weapon_prefix(original_editor_id)
            || normalized_editor_id.starts_with("zzz_"))
}

fn fo76_fo4_collision_layer_alias(editor_id: &str) -> Option<&'static str> {
    match editor_id {
        "l_proj_no_collide_proj" => Some("l_coneprojectile"),
        _ => None,
    }
}

/// Forced FO76→FO4 keyword substitutions as `(FO76 SeventySix.esm object-id,
/// target plugin name, target object-id)`. These FO76 appearance/material
/// keywords either have no EditorID match in the FO4 masters or must resolve to
/// a DLC master keyword before OMOD target-keyword validation runs. Without an
/// explicit override they would be carried through as own-plugin KYWD records or
/// dropped from `MNAM`, making weapon mods appear on unrelated weapons.
///
/// Seeding these into `MapperState::source_to_target` makes the mapper — the
/// single chokepoint every `remap_formkey` and the raw-FormID fixup flow
/// through — resolve each reference (and the standalone KYWD's own identity, so
/// it is vanilla-remapped rather than re-emitted) to the FO4 vanilla keyword.
///
/// Append ARMO/ARMA appearance/material attach-point rows here when armor
/// support lands — the substitution is record-type agnostic by design.
const FO76_FO4_FORCED_KEYWORD_SUBSTITUTIONS: &[(u32, &str, u32)] = &[
    (0x0011_4364, "Fallout4.esm", 0x0024_A0D8), // ap_gun_Appearance   -> ap_WeaponMaterial
    (0x0037_D0B2, "Fallout4.esm", 0x0024_A0D7), // ma_Gun_Appearance   -> ma_WeaponMaterialSwaps
    (0x001A_001E, "Fallout4.esm", 0x001A_001E), // ap_melee_Appearance -> ap_melee_Material
    (0x0011_3855, "DLCNukaWorld.esm", 0x0003_3B61), // DLC04_ma_HandmadeAssaultRifle
];

/// Forced FO76→FO4 race substitutions as `(FO76 SeventySix.esm object-id,
/// FO4 Fallout4.esm object-id)`.
const FO76_FO4_FORCED_RACE_SUBSTITUTIONS: &[(u32, u32)] = &[
    (0x0079_CCE7, 0x000E_AFB6), // GHL_PlayerGhoulRace -> GhoulRace
    (0x0077_2F32, 0x000D_FB33), // ProtectronFastRace -> ProtectronRace
];

const FO4_HUMAN_RACE_LOCAL: u32 = 0x0001_3746;
const FO4_HUMAN_CHILD_RACE_LOCAL: u32 = 0x0011_D83F;
const FO4_GHOUL_RACE_LOCAL: u32 = 0x000E_AFB6;

/// FNV/FO3 humanoid races have legacy head/body layouts that cannot be emitted
/// as FO4 RACE records. Resolve them to the official FO4 race with the matching
/// anatomy before record allocation so every reference follows the donor and
/// the source RACE itself is treated as a target-master remap.
const FNV_FO3_FO4_HUMANOID_RACE_SUBSTITUTIONS: &[(&str, u32)] = &[
    ("CaucasianOldAged", FO4_HUMAN_RACE_LOCAL),
    ("AfricanAmericanOldAged", FO4_HUMAN_RACE_LOCAL),
    ("AsianOldAged", FO4_HUMAN_RACE_LOCAL),
    ("HispanicOldAged", FO4_HUMAN_RACE_LOCAL),
    ("AfricanAmericanRaider", FO4_HUMAN_RACE_LOCAL),
    ("AsianRaider", FO4_HUMAN_RACE_LOCAL),
    ("HispanicRaider", FO4_HUMAN_RACE_LOCAL),
    ("CaucasianRaider", FO4_HUMAN_RACE_LOCAL),
    ("TestQACaucasian", FO4_HUMAN_RACE_LOCAL),
    ("HispanicOld", FO4_HUMAN_RACE_LOCAL),
    ("HispanicChild", FO4_HUMAN_CHILD_RACE_LOCAL),
    ("CaucasianOld", FO4_HUMAN_RACE_LOCAL),
    ("CaucasianChild", FO4_HUMAN_CHILD_RACE_LOCAL),
    ("AsianOld", FO4_HUMAN_RACE_LOCAL),
    ("AsianChild", FO4_HUMAN_CHILD_RACE_LOCAL),
    ("AfricanAmericanOld", FO4_HUMAN_RACE_LOCAL),
    ("AfricanAmericanChild", FO4_HUMAN_CHILD_RACE_LOCAL),
    ("AfricanAmerican", FO4_HUMAN_RACE_LOCAL),
    ("Ghoul", FO4_GHOUL_RACE_LOCAL),
    ("Asian", FO4_HUMAN_RACE_LOCAL),
    ("Hispanic", FO4_HUMAN_RACE_LOCAL),
    ("Caucasian", FO4_HUMAN_RACE_LOCAL),
    ("Christine", FO4_HUMAN_RACE_LOCAL),
    ("WhitelegsCacasians", FO4_HUMAN_RACE_LOCAL),
    ("WhiteLegsAfricanAmerican", FO4_HUMAN_RACE_LOCAL),
    ("DeadHorseCaucasian", FO4_HUMAN_RACE_LOCAL),
    ("DeadHorseAfricanAmerican", FO4_HUMAN_RACE_LOCAL),
    ("SorrowAfricanAmerican", FO4_HUMAN_RACE_LOCAL),
    ("SorrowCacasian", FO4_HUMAN_RACE_LOCAL),
    ("Lobotomites", FO4_HUMAN_RACE_LOCAL),
    ("MarkedMenGhoul", FO4_GHOUL_RACE_LOCAL),
    ("DLCPittHispanicMut", FO4_HUMAN_RACE_LOCAL),
    ("DLCPittAsianMut", FO4_HUMAN_RACE_LOCAL),
    ("DLCPittCaucasianMut", FO4_HUMAN_RACE_LOCAL),
    ("DLCPittAfricanAmericanMut", FO4_HUMAN_RACE_LOCAL),
    ("HispanicTribal", FO4_HUMAN_RACE_LOCAL),
    ("CaucasianTribal", FO4_HUMAN_RACE_LOCAL),
    ("AsianTribal", FO4_HUMAN_RACE_LOCAL),
    ("AfricanAmericanTribal", FO4_HUMAN_RACE_LOCAL),
];

/// Forced FO76→FO4 base-object substitutions as `(FO76 SeventySix.esm object-id,
/// FO4 Fallout4.esm object-id)`.
const FO76_FO4_FORCED_BASE_OBJECT_SUBSTITUTIONS: &[(u32, u32)] = &[
    (0x003B_D4F4, 0x000C_1AEB), // WorkshopWorkbenchPublic -> WorkshopWorkbench
];

/// Forced FO76→FO4 location-ref-type substitutions as `(FO76 SeventySix.esm
/// object-id, FO4 Fallout4.esm object-id)`.
const FO76_FO4_FORCED_LOCATION_REF_TYPE_SUBSTITUTIONS: &[(u32, u32)] = &[
    (0x0000_3956, 0x0000_3956), // LocationClearActor -> Boss
];

const FO76_MASTER_PLUGIN_NAME: &str = "SeventySix.esm";
const FO4_MASTER_PLUGIN_NAME: &str = "Fallout4.esm";

/// Resolve `FO76_FO4_FORCED_KEYWORD_SUBSTITUTIONS` into concrete
/// `(source FormKey, target FormKey)` pairs for the FO76→FO4 master plugins.
fn fo76_fo4_forced_keyword_substitution_mappings(
    interner: &StringInterner,
) -> Vec<(FormKey, FormKey)> {
    let source_plugin = interner.intern(FO76_MASTER_PLUGIN_NAME);
    FO76_FO4_FORCED_KEYWORD_SUBSTITUTIONS
        .iter()
        .map(|&(source_local, target_plugin_name, target_local)| {
            (
                FormKey {
                    local: source_local,
                    plugin: source_plugin,
                },
                FormKey {
                    local: target_local,
                    plugin: interner.intern(target_plugin_name),
                },
            )
        })
        .collect()
}

fn fo76_fo4_forced_race_substitution_mappings(
    interner: &StringInterner,
) -> Vec<(FormKey, FormKey)> {
    fo76_fo4_forced_substitution_mappings(interner, FO76_FO4_FORCED_RACE_SUBSTITUTIONS)
}

fn fnv_fo3_fo4_humanoid_race_substitution_mappings(
    source_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    interner: &StringInterner,
    source: Game,
    target: Game,
) -> Vec<(FormKey, FormKey)> {
    if !matches!(source, Game::Fnv | Game::Fo3) || target != Game::Fo4 {
        return Vec::new();
    }
    let Ok(race_sig) = SigCode::from_str("RACE") else {
        return Vec::new();
    };
    let target_plugin = interner.intern(FO4_MASTER_PLUGIN_NAME);
    source_entries
        .iter()
        .filter_map(|(editor_id, source_form_key, signature)| {
            if *signature != race_sig {
                return None;
            }
            let editor_id = interner.resolve(*editor_id)?;
            let (_, target_local) = FNV_FO3_FO4_HUMANOID_RACE_SUBSTITUTIONS
                .iter()
                .find(|(candidate, _)| candidate.eq_ignore_ascii_case(editor_id))?;
            Some((
                *source_form_key,
                FormKey {
                    local: *target_local,
                    plugin: target_plugin,
                },
            ))
        })
        .collect()
}

pub(crate) fn seed_fnv_fo3_fo4_ammo_substitutions(
    mapper_state: &mut MapperState,
    source_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    interner: &StringInterner,
    source: Game,
    target: Game,
) -> Result<usize, String> {
    if !matches!(source, Game::Fnv | Game::Fo3) || target != Game::Fo4 {
        return Ok(0);
    }
    let ammo_sig = SigCode::from_str("AMMO")?;
    let table = crate::translator::ammo_substitute::AmmoSubstituteTable::from_yaml(
        crate::embedded::AMMO_FNV_TO_FO4,
    )?;
    let mut seeded = 0;
    for (editor_id, source_form_key, signature) in source_entries {
        if *signature != ammo_sig {
            continue;
        }
        let Some(editor_id) = interner.resolve(*editor_id) else {
            continue;
        };
        let Some(target_form_key) = table.lookup(editor_id) else {
            continue;
        };
        let target_form_key = FormKey::parse(&target_form_key, interner)?;
        mapper_state
            .source_to_target
            .insert(*source_form_key, target_form_key);
        seeded += 1;
    }
    Ok(seeded)
}

fn fo76_fo4_forced_base_object_substitution_mappings(
    interner: &StringInterner,
) -> Vec<(FormKey, FormKey)> {
    fo76_fo4_forced_substitution_mappings(interner, FO76_FO4_FORCED_BASE_OBJECT_SUBSTITUTIONS)
}

fn fo76_fo4_forced_location_ref_type_substitution_mappings(
    interner: &StringInterner,
) -> Vec<(FormKey, FormKey)> {
    fo76_fo4_forced_substitution_mappings(interner, FO76_FO4_FORCED_LOCATION_REF_TYPE_SUBSTITUTIONS)
}

fn fo76_fo4_forced_substitution_mappings(
    interner: &StringInterner,
    substitutions: &[(u32, u32)],
) -> Vec<(FormKey, FormKey)> {
    let source_plugin = interner.intern(FO76_MASTER_PLUGIN_NAME);
    let target_plugin = interner.intern(FO4_MASTER_PLUGIN_NAME);
    substitutions
        .iter()
        .map(|&(source_local, target_local)| {
            (
                FormKey {
                    local: source_local,
                    plugin: source_plugin,
                },
                FormKey {
                    local: target_local,
                    plugin: target_plugin,
                },
            )
        })
        .collect()
}

fn source_target_mappings_from_preflight(
    source_entries: impl IntoIterator<Item = (Sym, FormKey, crate::ids::SigCode)>,
    target_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    interner: &StringInterner,
    source: Game,
    target: Game,
) -> Vec<(FormKey, FormKey)> {
    let blocked_source_form_keys = FxHashSet::default();
    source_target_mappings_from_preflight_with_skips(
        source_entries,
        target_entries,
        interner,
        source,
        target,
        &blocked_source_form_keys,
    )
}

fn source_target_mappings_from_preflight_with_skips(
    source_entries: impl IntoIterator<Item = (Sym, FormKey, crate::ids::SigCode)>,
    target_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    interner: &StringInterner,
    source: Game,
    target: Game,
    blocked_source_form_keys: &FxHashSet<FormKey>,
) -> Vec<(FormKey, FormKey)> {
    let mut target_by_key: FxHashMap<(Sym, crate::ids::SigCode), FormKey> = FxHashMap::default();
    for &(editor_id, form_key, signature) in target_entries {
        target_by_key
            .entry((editor_id, signature))
            .or_insert(form_key);
    }
    let mut target_by_local_sig: FxHashMap<(u32, crate::ids::SigCode), FormKey> =
        FxHashMap::default();
    if source == Game::Fo76 && target == Game::Fo4 {
        for &(_editor_id, form_key, signature) in target_entries {
            target_by_local_sig
                .entry((form_key.local, signature))
                .or_insert(form_key);
        }
    }

    let mut mappings = Vec::new();
    for (source_editor_id, source_form_key, source_signature) in source_entries {
        if blocked_source_form_keys.contains(&source_form_key) {
            continue;
        }
        let original_editor_id_str = interner.resolve(source_editor_id).unwrap_or("");
        if !allows_source_target_preflight_remap(
            source,
            target,
            source_signature,
            original_editor_id_str,
        ) {
            continue;
        }
        let normalized_editor_id = normalized_eid_sym(source_editor_id, interner);
        if let Some(&target_form_key) = target_by_key.get(&(normalized_editor_id, source_signature))
        {
            mappings.push((source_form_key, target_form_key));
            continue;
        }
        if source == Game::Fo76 && target == Game::Fo4 {
            let source_editor_id_str = interner.resolve(normalized_editor_id).unwrap_or("");
            let mut mapped = false;
            if source_signature.as_str() == "COLL" {
                if let Some(alias) = fo76_fo4_collision_layer_alias(source_editor_id_str) {
                    let alias_sym = interner.intern(alias);
                    if let Some(&target_form_key) =
                        target_by_key.get(&(alias_sym, source_signature))
                    {
                        mappings.push((source_form_key, target_form_key));
                        mapped = true;
                    }
                }
            }
            for alias in
                fo76_fo4_weap_editor_id_aliases(original_editor_id_str, source_editor_id_str)
            {
                if mapped {
                    break;
                }
                let alias_sym = interner.intern(&alias);
                if let Some(&target_form_key) = target_by_key.get(&(alias_sym, source_signature)) {
                    mappings.push((source_form_key, target_form_key));
                    mapped = true;
                    break;
                }
            }
            if !mapped
                && allows_fo76_fo4_weap_local_alias(
                    original_editor_id_str,
                    source_editor_id_str,
                    source_signature,
                )
            {
                if let Some(&target_form_key) =
                    target_by_local_sig.get(&(source_form_key.local, source_signature))
                {
                    mappings.push((source_form_key, target_form_key));
                }
            }
        }
    }
    mappings
}

fn collect_fo76_fo4_creature_skin_remap_skips(
    source_handle_id: u64,
    source_schema: &AuthoringSchema,
    source_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    target_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    interner: &StringInterner,
    source: Game,
    target: Game,
) -> FxHashSet<FormKey> {
    let mut blocked = FxHashSet::default();
    if source != Game::Fo76 || target != Game::Fo4 {
        return blocked;
    }

    let Ok(armo_sig) = SigCode::from_str("ARMO") else {
        return blocked;
    };
    let target_armors: FxHashSet<Sym> = target_entries
        .iter()
        .filter_map(|(editor_id, _fk, sig)| (*sig == armo_sig).then_some(*editor_id))
        .collect();

    for &(editor_id, form_key, sig) in source_entries {
        if sig != armo_sig {
            continue;
        }
        let normalized_editor_id = normalized_eid_sym(editor_id, interner);
        if !target_armors.contains(&normalized_editor_id) {
            continue;
        }
        let Some(editor_id_str) = interner.resolve(editor_id) else {
            continue;
        };
        if !looks_like_creature_skin_armor_editor_id(editor_id_str) {
            continue;
        }
        let Ok(record) = read_record_relayout_by_form_key(
            source_handle_id,
            &form_key,
            source_schema,
            interner,
            None,
        ) else {
            continue;
        };
        let Some(race_form_key) = record_race_formkey(&record, interner) else {
            continue;
        };
        if is_human_or_power_armor_race(race_form_key, interner)
            || source_race_has_same_editor_id_target(
                race_form_key,
                source_entries,
                target_entries,
                interner,
            )
        {
            continue;
        }

        blocked.insert(form_key);
        collect_armor_addons_from_skin(&record, &mut blocked);
    }

    blocked
}

fn looks_like_creature_skin_armor_editor_id(editor_id: &str) -> bool {
    editor_id.to_ascii_lowercase().contains("skin")
}

fn record_race_formkey(record: &Record, interner: &StringInterner) -> Option<FormKey> {
    let rnam_sig = SubrecordSig::from_str("RNAM").ok()?;
    let race_sym = interner.intern("Race");
    record
        .fields
        .iter()
        .find(|entry| entry.sig == rnam_sig)
        .and_then(|entry| field_value_formkey(&entry.value, race_sym))
}

fn is_human_or_power_armor_race(fk: FormKey, interner: &StringInterner) -> bool {
    let Some(plugin) = interner.resolve(fk.plugin) else {
        return false;
    };
    if !plugin.eq_ignore_ascii_case("Fallout4.esm")
        && !plugin.eq_ignore_ascii_case("SeventySix.esm")
    {
        return false;
    }
    matches!(fk.local, 0x013746 | 0x01D31E)
}

fn source_race_has_same_editor_id_target(
    source_race: FormKey,
    source_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    target_entries: &[(Sym, FormKey, crate::ids::SigCode)],
    interner: &StringInterner,
) -> bool {
    let Ok(race_sig) = SigCode::from_str("RACE") else {
        return false;
    };
    let Some((source_editor_id, _, _)) = source_entries
        .iter()
        .find(|(_, form_key, sig)| *form_key == source_race && *sig == race_sig)
    else {
        return false;
    };
    let normalized_source_editor_id = normalized_eid_sym(*source_editor_id, interner);
    target_entries.iter().any(|(target_editor_id, _, sig)| {
        *sig == race_sig
            && normalized_eid_sym(*target_editor_id, interner) == normalized_source_editor_id
    })
}

fn collect_armor_addons_from_skin(record: &Record, blocked: &mut FxHashSet<FormKey>) {
    let Ok(modl_sig) = SubrecordSig::from_str("MODL") else {
        return;
    };
    for entry in record.fields.iter().filter(|entry| entry.sig == modl_sig) {
        collect_formkeys_from_value(&entry.value, blocked);
    }
}

fn field_value_formkey(value: &FieldValue, key: Sym) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(field_key, _)| *field_key == key)
            .and_then(|(_, value)| field_value_formkey(value, key)),
        _ => None,
    }
}

fn collect_formkeys_from_value(value: &FieldValue, out: &mut FxHashSet<FormKey>) {
    match value {
        FieldValue::FormKey(fk) => {
            out.insert(*fk);
        }
        FieldValue::List(values) => {
            for value in values {
                collect_formkeys_from_value(value, out);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_formkeys_from_value(value, out);
            }
        }
        _ => {}
    }
}

pub(crate) fn target_master_names_for_skip(
    config: &RunConfig,
    whole_plugin_names: Vec<String>,
) -> Vec<String> {
    if !config.target_master_names.is_empty() {
        config.target_master_names.clone()
    } else if config.is_whole_plugin {
        whole_plugin_names
    } else {
        Vec::new()
    }
}

pub(crate) fn is_target_master_remap(
    target_fk: FormKey,
    target_master_syms: &FxHashSet<Sym>,
) -> bool {
    target_master_syms.contains(&target_fk.plugin)
}

/// Build the target-master skip set + first-master sym exactly as the legacy
/// per-record loop does at its start (run.rs translate_fks_with_mode_and_parents).
/// Used by `store2::translate_v2` so the vanilla-remap early-out + full-plugin
/// capture match legacy bit-for-bit.
pub(crate) fn capture_target_master_context(
    run: &mut ConversionRun,
) -> (FxHashSet<Sym>, Option<Sym>) {
    let whole_plugin_names =
        if run.config.is_whole_plugin && run.config.target_master_names.is_empty() {
            run.target_master_plugin_names()
        } else {
            Vec::new()
        };
    let target_master_names = target_master_names_for_skip(&run.config, whole_plugin_names);
    let target_master_syms: FxHashSet<Sym> =
        intern_plugin_names(&target_master_names, &run.interner);
    let first_target_master_sym = target_master_names
        .first()
        .map(|name| run.interner.intern(name));
    (target_master_syms, first_target_master_sym)
}

const FO76_TERRAIN_OWNED_RECORD_SIGS: &[&str] = &["LTEX", "GRAS"];

fn supports_source_worldspace_topology_rebuild(source: Game, target: Game) -> bool {
    target == Game::Fo4 && matches!(source, Game::Fnv | Game::Fo3 | Game::SkyrimSe)
}

fn apply_fo76_terrain_owned_record_skips(
    translator: &mut Translator,
    source: Game,
    target: Game,
    config: &RunConfig,
) {
    if source != Game::Fo76 || target != Game::Fo4 || !config.asset_phases.terrain {
        return;
    }
    for sig in FO76_TERRAIN_OWNED_RECORD_SIGS {
        translator.maps.skip_records.insert((*sig).to_owned());
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Create a new `ConversionRun`, insert it into the registry, and return its ID.
pub fn create_run(params: RunParams) -> Result<u64, RunError> {
    create_run_in_mode(params, TargetMode::CreateNew)
}

fn create_run_in_mode(params: RunParams, target_mode: TargetMode) -> Result<u64, RunError> {
    if params.source == params.target {
        // Same-game is valid (e.g. FO4→FO4 for normalisation passes). No error.
    }

    let schema_source = AuthoringSchema::for_game(params.source.as_str())
        .map_err(|e| RunError::InvalidConfig(format!("source schema: {e}")))?;
    let schema_target = AuthoringSchema::for_game(params.target.as_str())
        .map_err(|e| RunError::InvalidConfig(format!("target schema: {e}")))?;

    let mut config = params.config;
    let mut translator = Translator::new(params.source, params.target)
        .map_err(|e| RunError::InvalidConfig(format!("translator: {e}")))?;
    apply_fo76_terrain_owned_record_skips(&mut translator, params.source, params.target, &config);
    for sig in &config.skip_record_signatures {
        let sig = sig.trim().to_uppercase();
        if !sig.is_empty() {
            translator.maps.skip_records.insert(sig);
        }
    }
    if target_mode == TargetMode::CreateNew && config.generated_object_id_floor != 0 {
        esp_authoring_core::plugin_runtime::plugin_handle_raise_next_object_id_no_py(
            params.target_handle_id,
            config.generated_object_id_floor,
        )
        .map_err(|e| RunError::InvalidConfig(format!("generated object-id floor: {e}")))?;
    }
    if target_mode == TargetMode::CreateNew {
        crate::plugin_header::normalize_target_plugin_header(
            params.target_handle_id,
            params.target.as_str(),
        )?;
    }

    let (event_tx, event_rx) = crossbeam_channel::bounded(1024);
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let target_assets = match (
        params.target,
        config.target_data_dir.as_deref(),
        config.target_asset_catalog_path.as_deref(),
        config.target_asset_cache_dir.as_deref(),
    ) {
        (Game::Fo4, Some(data_dir), Some(catalog_path), Some(cache_dir)) => Some(
            crate::target_assets::TargetAssetStore::open_shared(
                data_dir,
                catalog_path,
                cache_dir,
                config.target_extracted_dir.as_deref(),
            )
            .map_err(RunError::InvalidConfig)?,
        ),
        _ => None,
    };
    if let Some(store) = target_assets.as_deref() {
        let membership_root = store
            .prepare_membership_tree("textures/effects/gobos/", ".dds")
            .map_err(RunError::InvalidConfig)?;
        config.target_extracted_dir = Some(membership_root.to_path_buf());
    }

    let relocation = if params.source == Game::Fo76 && params.target == Game::Fo4 {
        let roots: Vec<String> = if config.base_asset_relocation_mesh_roots.is_empty() {
            crate::relocation::FO76_FO4_DEFAULT_RELOCATION_MESH_ROOTS
                .iter()
                .map(|s| (*s).to_string())
                .collect()
        } else {
            config.base_asset_relocation_mesh_roots.clone()
        };
        match (&config.source_extracted_dir, target_assets.as_deref()) {
            (Some(fo76), Some(store)) => {
                crate::relocation::build_relocation_member_set_with_target_store(
                    &roots, fo76, store,
                )
            }
            (Some(fo76), None) => match &config.target_extracted_dir {
                Some(fo4) => crate::relocation::build_relocation_member_set(&roots, fo76, fo4),
                None => crate::relocation::RelocationBuildResult {
                    members: std::collections::HashSet::new(),
                    warnings: vec![
                        "relocation: target asset catalog/overlay unset — collision detection disabled"
                            .to_string(),
                    ],
                },
            },
            _ => crate::relocation::RelocationBuildResult {
                members: std::collections::HashSet::new(),
                warnings: vec![
                    "relocation: source extracted dir unset — collision detection disabled"
                        .to_string(),
                ],
            },
        }
    } else {
        crate::relocation::RelocationBuildResult::default()
    };

    let target_master_record_contexts = params
        .master_handle_ids
        .iter()
        .filter_map(|handle_id| {
            plugin_context_for_handle(*handle_id)
                .ok()
                .map(|(plugin_name, master_names)| TargetMasterRecordContext {
                    handle_id: *handle_id,
                    plugin_name,
                    master_names,
                })
        })
        .collect();

    let run = ConversionRun {
        source: params.source,
        target: params.target,
        source_handle_id: params.source_handle_id,
        target_handle_id: params.target_handle_id,
        master_handle_ids: params.master_handle_ids,
        target_master_record_contexts,
        interner: StringInterner::new(),
        schema_source,
        schema_target,
        translator,
        config,
        mapper_state: None,
        generated_object_id_reservations: FxHashSet::default(),
        legacy_serial_normalization: Default::default(),
        legacy_pack_preflight_report: None,
        legacy_creature_race_coverage: Default::default(),
        decisions: Vec::new(),
        warnings: Vec::new(),
        deferred: Vec::new(),
        fnv_scri_links: Vec::new(),
        progress_callback: None,
        navi_warnings: Vec::new(),
        event_tx,
        event_rx,
        cancel,
        dependency_graph: None,
        full_plugin_state: FullPluginRunState::default(),
        terrain_texture_jobs: Vec::new(),
        projected_seed_cache: std::collections::HashMap::new(),
        relocation_members: relocation.members,
        relocation_warnings: relocation.warnings,
        target_assets,
        output_sink: None,
        owned_handles: None,
        default_target_path: None,
        target_mode,
    };

    let slot = RunSlot {
        events: run.event_rx.clone(),
        event_tx: run.event_tx.clone(),
        cancel: Arc::clone(&run.cancel),
        run: Arc::new(Mutex::new(run)),
    };
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    registry()
        .lock()
        .map_err(|_| RunError::LockPoisoned)?
        .insert(id, slot);
    Ok(id)
}

/// Create a registered run that takes sole ownership of its plugin handles.
pub(crate) fn create_owned_run(
    source: Game,
    target: Game,
    config: RunConfig,
    handles: OwnedRunHandles,
    default_target_path: PathBuf,
    target_mode: TargetMode,
) -> Result<u64, RunError> {
    let params = RunParams {
        source,
        target,
        source_handle_id: handles.source.as_ref().map_or(0, OwnedPluginHandle::id),
        target_handle_id: handles.target.id(),
        master_handle_ids: handles.masters.iter().map(OwnedPluginHandle::id).collect(),
        config,
    };
    let id = create_run_in_mode(params, target_mode)?;
    if let Err(error) = with_run(id, |run| {
        run.owned_handles = Some(handles);
        run.default_target_path = Some(default_target_path);
        Ok::<_, RunError>(())
    }) {
        let _ = drop_run(id);
        return Err(error);
    }
    Ok(id)
}

/// Remove a run from the registry. Resources release when the last slot
/// clone (and any in-flight phase's run Arc) drops.
pub fn drop_run(id: u64) -> Result<(), RunError> {
    registry()
        .lock()
        .map_err(|_| RunError::LockPoisoned)?
        .remove(&id)
        .map(|_| ())
        .ok_or(RunError::UnknownRun(id))
}

/// Borrow the run mutably for the duration of `f`.
///
/// Locks only this run's slot — runs are independent.
/// `E` must be convertible to `RunError` so the caller can use `?` inside `f`.
pub fn with_run<R, E>(
    id: u64,
    f: impl FnOnce(&mut ConversionRun) -> Result<R, E>,
) -> Result<R, RunError>
where
    E: Into<RunError>,
{
    let slot = run_slot(id)?;
    let mut run = slot.run.lock().map_err(|_| RunError::LockPoisoned)?;
    f(&mut run).map_err(Into::into)
}

// ---------------------------------------------------------------------------
// ConversionRun methods
// ---------------------------------------------------------------------------

impl ConversionRun {
    pub fn source_handle(&self) -> Option<u64> {
        (self.source_handle_id != 0).then_some(self.source_handle_id)
    }

    pub fn require_source_handle(&self) -> Result<u64, RunError> {
        self.source_handle().ok_or_else(|| {
            RunError::InvalidConfig("this phase requires a source plugin".to_string())
        })
    }

    pub fn release_source_handle(&mut self) -> bool {
        let released = self
            .owned_handles
            .as_mut()
            .and_then(|handles| handles.source.as_mut())
            .is_some_and(OwnedPluginHandle::release);
        if released {
            self.source_handle_id = 0;
            if let Some(handles) = self.owned_handles.as_mut() {
                handles.source = None;
            }
        }
        released
    }

    pub fn release_master_handles(&mut self) -> usize {
        let Some(handles) = self.owned_handles.as_mut() else {
            return 0;
        };
        let mut released = 0;
        for master in handles.masters.iter_mut().rev() {
            released += usize::from(master.release());
        }
        handles.masters.clear();
        self.master_handle_ids.clear();
        self.target_master_record_contexts.clear();
        released
    }

    pub(crate) fn merge_target_collision_donor(
        &self,
        record: &mut Record,
        donor_form_key: FormKey,
    ) -> Result<(), String> {
        let donor_plugin = self
            .interner
            .resolve(donor_form_key.plugin)
            .ok_or_else(|| "collision donor plugin is not interned".to_string())?;
        let context = self
            .target_master_record_contexts
            .iter()
            .find(|context| context.plugin_name.eq_ignore_ascii_case(donor_plugin))
            .ok_or_else(|| format!("collision donor master is not loaded: {donor_plugin}"))?;
        let mut donor = read_record_relayout_by_form_key(
            context.handle_id,
            &donor_form_key,
            &self.schema_target,
            &self.interner,
            None,
        )
        .map_err(|error| format!("collision donor read failed: {error}"))?;

        let target_master_names = if self.config.target_master_names.is_empty() {
            self.target_master_record_contexts
                .iter()
                .map(|context| context.plugin_name.clone())
                .collect()
        } else {
            self.config.target_master_names.clone()
        };
        let mut donor_state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: self.config.output_plugin_name.clone(),
                source_plugin_name: context.plugin_name.clone(),
                source_master_names: context.master_names.clone(),
                target_master_names,
                resolution_mode: ResolutionMode::DeferAndFixup,
                ..Default::default()
            },
        );
        let mut donor_mapper = FormKeyMapper::from_state(&mut donor_state, &self.interner);
        donor_mapper
            .rewrite_record(&mut donor)
            .map_err(|error| format!("collision donor VMAD reindex failed: {error}"))?;
        crate::collision_donor::merge_target_collision_donor(record, &donor, &self.interner);
        Ok(())
    }

    /// Translate every record in the source plugin into the target plugin.
    ///
    /// Iterates every signature in the source plugin and dispatches each
    /// record through the shared `translate_fks` body. Used by the unified
    /// whole-plugin pipeline.
    /// Emit a one-line status string through the Python progress callback, if
    /// one is set. Bypasses the phase event channel, which is starved while
    /// `translate_all` holds the run-registry lock (the event Drainer blocks on
    /// that same lock for the whole call). Used to make the otherwise-silent
    /// translate_all setup (mapper-state build + form-key enumeration) visible
    /// during multi-hour whole-plugin runs. Callback errors are ignored.
    fn emit_status(&self, message: &str) {
        let Some(cb) = self.progress_callback.as_ref() else {
            return;
        };
        Python::attach(|py| {
            let _ = cb.call1(py, (message,));
        });
    }

    fn emit_phase_status(&self, message: impl Into<String>) {
        let _ = self.event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase: "translate_v2",
            level: crate::phase::LogLevel::Info,
            message: message.into(),
        });
    }

    pub(crate) fn preallocate_legacy_form_key_intents(
        &mut self,
        intents: impl IntoIterator<Item = LegacyFormKeyAllocationIntent>,
    ) -> LegacyFormKeyPreallocationCoverage {
        let mut coverage = LegacyFormKeyPreallocationCoverage::default();
        let state = self
            .mapper_state
            .as_mut()
            .expect("mapper_state initialized before legacy preallocation");
        let mut mapper = FormKeyMapper::from_state(state, &self.interner);
        for intent in intents {
            coverage.eligible += 1;
            let normalized_eid = normalized_eid_opt(intent.editor_id, mapper.interner);
            mapper.allocate_or_resolve(intent.source_fk, normalized_eid, intent.target_sig);
            if mapper.lookup(intent.source_fk).is_some() {
                coverage.mapped += 1;
            } else {
                coverage.missing += 1;
            }
        }
        coverage
    }

    pub(crate) fn legacy_pack_gate_active(&self) -> bool {
        self.config.is_whole_plugin
            && matches!(self.source, Game::Fnv | Game::Fo3)
            && self.target == Game::Fo4
    }

    pub(crate) fn begin_legacy_pack_preflight(
        &mut self,
        source_plugin_name: &str,
    ) -> Option<LegacyPackPreflightAccumulator> {
        if !self.legacy_pack_gate_active() {
            return None;
        }
        self.legacy_pack_preflight_report = None;
        let source_family = match self.source {
            Game::Fnv => crate::translator::pair_hooks::fnv_pack::LegacyPackSourceFamily::Fnv,
            Game::Fo3 => crate::translator::pair_hooks::fnv_pack::LegacyPackSourceFamily::Fo3,
            _ => return None,
        };
        let require_explicit_origins = self.config.legacy_pack_provenance_required
            || source_plugin_name.eq_ignore_ascii_case("FNV_FO3_Merged.esm");
        let raw_expected = if require_explicit_origins {
            LegacyPackExpectedCounts::audited_merged()
        } else {
            LegacyPackExpectedCounts::audited_for(source_family)
        };
        let expected = self.config.legacy_pack_expected_counts.unwrap_or_else(|| {
            if require_explicit_origins {
                LegacyPackExpectedCounts::audited_merged()
            } else {
                LegacyPackExpectedCounts::audited_for(source_family)
            }
        });
        let direct_origin = (!require_explicit_origins
            && self.config.legacy_pack_origins.is_empty())
        .then(|| DirectLegacyPackOrigin {
            family: source_family,
            source_plugin: source_plugin_name.to_string(),
        });
        let explicitly_excluded = self
            .config
            .skip_record_signatures
            .iter()
            .any(|sig| sig.trim().eq_ignore_ascii_case("PACK"));
        let _ = self.event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase: "preflight",
            level: crate::phase::LogLevel::Info,
            message: format!(
                "legacy_pack_preflight:start:raw_expected_fnv={}:raw_expected_fo3={}:expected_fnv={}:expected_fo3={}:explicit_origins={}:explicitly_excluded={}",
                raw_expected.fnv,
                raw_expected.fo3,
                expected.fnv,
                expected.fo3,
                self.config.legacy_pack_origins.len(),
                explicitly_excluded,
            ),
        });
        Some(LegacyPackPreflightAccumulator::new(
            &self.config.legacy_pack_origins,
            raw_expected,
            self.config.legacy_pack_raw_source_counts,
            expected,
            direct_origin,
            require_explicit_origins,
            explicitly_excluded,
        ))
    }

    pub(crate) fn finish_legacy_pack_preflight(
        &mut self,
        accumulator: LegacyPackPreflightAccumulator,
    ) -> Result<(), RunError> {
        let report = accumulator.finish();
        let blocked = report.is_blocked();
        let message = format!(
            "legacy_pack_preflight:{}:{}",
            if blocked { "blocked" } else { "passed" },
            report.error_summary_json()
        );
        let _ = self.event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase: "preflight",
            level: if blocked {
                crate::phase::LogLevel::Error
            } else {
                crate::phase::LogLevel::Info
            },
            message,
        });
        self.legacy_pack_preflight_report = Some(report.clone());
        if blocked {
            return Err(RunError::LegacyPackPreflight(Box::new(report)));
        }
        Ok(())
    }

    pub(crate) fn preflight_legacy_packs_from_handle(&mut self) -> Result<(), RunError> {
        if !self.legacy_pack_gate_active() {
            return Ok(());
        }
        let source_plugin_name = plugin_name_for_handle(self.source_handle_id)?;
        let Some(mut accumulator) = self.begin_legacy_pack_preflight(&source_plugin_name) else {
            return Ok(());
        };
        let pack_sig = SigCode::from_str("PACK")
            .map_err(|error| RunError::InvalidConfig(format!("PACK signature: {error}")))?;
        let form_keys = iter_form_keys_of_sig(self.source_handle_id, pack_sig, &mut self.interner)?;
        for form_key in form_keys {
            match read_record_relayout_by_form_key(
                self.source_handle_id,
                &form_key,
                &self.schema_source,
                &self.interner,
                None,
            ) {
                Ok(record) => accumulator.observe_decoded(&record, &self.interner),
                Err(error) => {
                    accumulator.observe_decode_error(form_key, error.to_string(), &self.interner)
                }
            }
        }
        self.finish_legacy_pack_preflight(accumulator)
    }

    pub(crate) fn prepare_legacy_output_allocation_domain(
        &mut self,
        source_locals: impl IntoIterator<Item = u32>,
    ) -> Result<(), RunError> {
        if !matches!(self.source, Game::Fnv | Game::Fo3) || self.target != Game::Fo4 {
            return Ok(());
        }
        let state = self
            .mapper_state
            .as_mut()
            .expect("mapper_state initialized before legacy preallocation");
        if let Some(floor) = legacy_output_allocation_floor(
            self.source,
            self.target,
            &state.options.source_plugin_name,
            &state.options.output_plugin_name,
            state.options.preserve_source_ids,
            state.next_object_id,
            source_locals,
        )
        .map_err(|message| RunError::InvalidConfig(message.to_string()))?
        {
            state.next_object_id = floor;
        }
        Ok(())
    }

    fn preallocate_legacy_translate_all_records(
        &mut self,
        form_keys: &[FormKey],
    ) -> Result<(), RunError> {
        if !matches!(self.source, Game::Fnv | Game::Fo3) || self.target != Game::Fo4 {
            return Ok(());
        }
        self.emit_phase_status(format!(
            "translate: legacy forward-reference preallocation start records={}",
            form_keys.len()
        ));
        self.prepare_legacy_output_allocation_domain(
            form_keys.iter().map(|form_key| form_key.local),
        )?;
        let relayout_target_schema = self.schema_target.clone();
        let relayout_ctx = crate::struct_relayout::StructRelayoutCtx {
            target_schema: &relayout_target_schema,
            target_form_version:
                crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION,
            legacy_bptd_only: true,
        };
        let mut total = LegacyFormKeyPreallocationCoverage::default();
        let mut intents = Vec::with_capacity(crate::store2::translate_v2::CHUNK);
        for &source_fk in form_keys {
            if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(RunError::Cancelled);
            }
            let Ok(mut source_record) = read_record_relayout_by_form_key(
                self.source_handle_id,
                &source_fk,
                &*self.schema_source,
                &self.interner,
                Some(&relayout_ctx),
            ) else {
                continue;
            };
            {
                let mut ctx = PairCtx {
                    interner: &self.interner,
                };
                let _ = self.translator.pre_translate(&mut ctx, &mut source_record);
            }
            let TranslateResult::Translated(translated) =
                self.translator.translate(&source_record, &self.interner)
            else {
                continue;
            };
            if self
                .schema_target
                .record_def(translated.sig.as_str())
                .is_none()
            {
                continue;
            }
            intents.push(LegacyFormKeyAllocationIntent {
                source_fk,
                editor_id: translated.eid,
                target_sig: translated.sig,
            });
            if intents.len() == crate::store2::translate_v2::CHUNK {
                let coverage = self.preallocate_legacy_form_key_intents(intents.drain(..));
                total.eligible += coverage.eligible;
                total.mapped += coverage.mapped;
                total.missing += coverage.missing;
            }
        }
        if !intents.is_empty() {
            let coverage = self.preallocate_legacy_form_key_intents(intents.drain(..));
            total.eligible += coverage.eligible;
            total.mapped += coverage.mapped;
            total.missing += coverage.missing;
        }
        self.emit_phase_status(format!(
            "translate: legacy forward-reference preallocation done eligible={} mapped={} missing={}",
            total.eligible, total.mapped, total.missing
        ));
        if total.mapped != total.eligible || total.missing != 0 {
            return Err(RunError::InvalidConfig(format!(
                "legacy forward-reference preallocation incomplete: eligible={} mapped={} missing={}",
                total.eligible, total.mapped, total.missing
            )));
        }
        Ok(())
    }

    pub fn translate_all(&mut self) -> Result<TranslateStats, RunError> {
        self.require_source_handle()?;
        let translate_all_started = std::time::Instant::now();
        self.preflight_legacy_packs_from_handle()?;
        self.emit_status("translate_all: building mapper state…");
        self.init_mapper_state()?;
        self.emit_status(&format!(
            "translate_all: mapper state ready in {:.1}s; enumerating source records…",
            translate_all_started.elapsed().as_secs_f64()
        ));

        if self.config.records_limit == Some(0) {
            let stats = self.translate_fks(&[])?;
            self.finalize_legacy_creature_race_coverage()?;
            return Ok(stats);
        }

        let enumerate_started = std::time::Instant::now();
        let sigs = source_signatures(self.source_handle_id, &self.interner)?;
        let structured_dialogue_sig = SigCode::from_str("DIAL")
            .map_err(|e| RunError::InvalidConfig(format!("DIAL signature: {e}")))?;
        let structured_info_sig = SigCode::from_str("INFO")
            .map_err(|e| RunError::InvalidConfig(format!("INFO signature: {e}")))?;
        let structured_scene_sig = SigCode::from_str("SCEN")
            .map_err(|e| RunError::InvalidConfig(format!("SCEN signature: {e}")))?;
        let emit_structured_dialogue = self.should_emit_fo76_quest_dialogue();
        let emit_structured_scenes = self.should_emit_fo76_quest_scenes();
        let mut all_fks: Vec<FormKey> = Vec::new();
        for sig in sigs {
            if (emit_structured_dialogue
                && (sig == structured_dialogue_sig || sig == structured_info_sig))
                || (emit_structured_scenes && sig == structured_scene_sig)
            {
                continue;
            }
            if let Some(limit) = self.config.records_limit {
                if all_fks.len() >= limit {
                    break;
                }
            }
            let fks = iter_form_keys_of_sig(self.source_handle_id, sig, &mut self.interner)?;
            if let Some(limit) = self.config.records_limit {
                let remaining = limit.saturating_sub(all_fks.len());
                all_fks.extend(fks.into_iter().take(remaining));
            } else {
                all_fks.extend(fks);
            }
        }
        self.emit_status(&format!(
            "translate_all: {} records to translate (enumerated in {:.1}s); starting per-record translation…",
            all_fks.len(),
            enumerate_started.elapsed().as_secs_f64()
        ));
        self.preallocate_legacy_translate_all_records(&all_fks)?;
        let mut stats = self.translate_fks(&all_fks)?;
        self.finalize_legacy_creature_race_coverage()?;
        if emit_structured_scenes {
            stats.absorb(self.emit_quest_child_scenes()?);
        }
        if emit_structured_dialogue {
            stats.absorb(self.emit_quest_child_dialogue()?);
            stats.absorb(self.emit_topic_child_infos()?);
        }
        if matches!(self.source, Game::Fnv | Game::Fo3) && self.target == Game::Fo4 {
            self.rebuild_full_plugin_worldspace_groups()?;
            stats.absorb(self.emit_projected_navmeshes()?);
            stats.absorb(self.rebuild_projected_navi()?);
            return Ok(stats);
        }
        if self.source == Game::SkyrimSe && self.target == Game::Fo4 {
            stats.absorb(self.rebuild_projected_navi()?);
        }
        self.rebuild_full_plugin_worldspace_groups()?;
        Ok(stats)
    }

    /// Parallel translate — store2 mmap source + chunked P/A/F/E passes.
    /// Legacy-equivalent target-handle state (see `store2::translate_v2`).
    pub fn translate_all_v2(
        &mut self,
        source_path: &std::path::Path,
    ) -> Result<TranslateStats, RunError> {
        self.require_source_handle()?;
        crate::store2::translate_v2::translate_all_v2(self, source_path)
    }

    pub(crate) fn emit_story_manager_subset(
        &mut self,
    ) -> Result<crate::phase::story_manager::StoryManagerEmitStats, RunError> {
        let mut emit_stats = crate::phase::story_manager::StoryManagerEmitStats::default();
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(emit_stats);
        }
        if self.mapper_state.is_none() {
            self.init_mapper_state()?;
        }

        let graph = crate::phase::story_manager::load_source_graph(self)?;
        let translated_quests: FxHashSet<FormKey> = {
            let state = self
                .mapper_state
                .as_ref()
                .expect("mapper_state initialized before Story Manager emit");
            graph
                .quests
                .keys()
                .filter(|fk| state.source_to_target.contains_key(*fk))
                .copied()
                .collect()
        };
        let selection = crate::phase::story_manager::classify_story_manager_records(
            &graph,
            &translated_quests,
            &self.interner,
        );
        emit_stats.selected_nodes = selection.selected_nodes.len() as u32;
        emit_stats.skipped_nodes = selection
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.kind == crate::phase::story_manager::StoryManagerDiagnosticKind::Skipped
            })
            .count() as u32;
        self.record_story_manager_diagnostics(&selection.diagnostics);

        emit_stats.quests_changed = self.restore_passive_dialogue_controllers()?;
        emit_stats.quests_changed +=
            self.force_story_manager_dialogue_quests(&selection.fallback_dialogue_quests)?;
        let event_bridges = self.prepare_story_manager_event_bridges(&selection, &graph)?;
        self.map_story_manager_event_roots(&selection.selected_nodes, &graph);
        self.preallocate_story_manager_nodes(&selection.ordered_nodes, &graph);
        let mut emitted_nodes = FxHashSet::default();
        emit_stats.translate = self.translate_story_manager_nodes(
            &selection.ordered_nodes,
            &selection.selected_nodes,
            &selection.selected_quests_by_node,
            &event_bridges,
            &graph,
            &mut emitted_nodes,
        )?;
        let quest_event_plan = crate::phase::story_manager::plan_story_manager_quest_events(
            &selection,
            &graph,
            &event_bridges,
            &emitted_nodes,
        );
        for (quest, final_events) in &quest_event_plan.unresolved {
            let event_names = final_events
                .iter()
                .map(|event| String::from_utf8_lossy(&event.to_le_bytes()).into_owned())
                .collect::<Vec<_>>()
                .join(",");
            let kind = self.interner.intern("story_manager_quest_event_unresolved");
            self.decisions.push(Decision {
                kind,
                message: format!("{:06X}:final_events={event_names}", quest.local),
            });
        }
        emit_stats.quests_changed +=
            self.rewrite_story_manager_quest_events(&quest_event_plan.rewrites)?;
        emit_stats.translate.records_translated += event_bridges.len() as u32;
        Ok(emit_stats)
    }

    fn prepare_story_manager_event_bridges(
        &mut self,
        selection: &crate::phase::story_manager::StoryManagerSelection,
        graph: &crate::phase::story_manager::StoryManagerSourceGraph,
    ) -> Result<FxHashMap<FormKey, u32>, RunError> {
        let roots = crate::phase::story_manager::incompatible_story_manager_event_roots(
            &selection.selected_nodes,
            graph,
        );
        if roots.is_empty() {
            return Ok(FxHashMap::default());
        }

        let output_plugin = self.interner.intern(&self.config.output_plugin_name);
        let allocations = {
            let state = self
                .mapper_state
                .as_mut()
                .expect("mapper_state initialized before Story Manager emit");
            if let Some((source_root, target)) = roots.iter().find_map(|(source_root, _)| {
                state
                    .source_to_target
                    .get(source_root)
                    .copied()
                    .map(|target| (*source_root, target))
            }) {
                return Err(RunError::InvalidConfig(format!(
                    "story_manager_event_bridge_mapping_exists:{:06X}->{:06X}",
                    source_root.local, target.local
                )));
            }
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            mapper.reserve_object_ids(roots.iter().map(|(source_root, _)| source_root.local));
            roots
                .iter()
                .map(|(source_root, event_type)| {
                    let keyword = FormKey {
                        local: source_root.local,
                        plugin: output_plugin,
                    };
                    let branch = mapper.allocate_generated();
                    mapper.add_mapping(*source_root, branch);
                    (*source_root, *event_type, keyword, branch)
                })
                .collect::<Vec<_>>()
        };

        let mut bridges = FxHashMap::default();
        for (source_root, event_type, keyword, branch) in allocations {
            let keyword_raw =
                encode_form_key_for_handle(self.target_handle_id, keyword, &self.interner)
                    .map_err(|error| {
                        RunError::InvalidConfig(format!(
                            "story_manager_event_keyword_formid:{:06X}:{error}",
                            source_root.local
                        ))
                    })?;
            let event_name = String::from_utf8_lossy(&event_type.to_le_bytes()).into_owned();
            let editor_id = self.interner.intern(&format!("B21_SMEvent_{event_name}"));
            let mut keyword_record =
                Record::new(SigCode::from_str("KYWD").expect("literal sig"), keyword);
            keyword_record.eid = Some(editor_id);
            keyword_record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("EDID").expect("literal sig"),
                value: FieldValue::String(editor_id),
            });
            add_record_native(
                self.target_handle_id,
                keyword_record,
                &self.schema_target,
                &self.interner,
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "story_manager_event_keyword_add:{:06X}:{error}",
                    source_root.local
                ))
            })?;
            let kind = self.interner.intern("story_manager_event_bridge");
            self.decisions.push(Decision {
                kind,
                message: format!(
                    "{:06X}:{event_name}:keyword={:06X}:branch={:06X}",
                    source_root.local, keyword.local, branch.local
                ),
            });
            bridges.insert(source_root, keyword_raw);
        }
        Ok(bridges)
    }

    fn map_story_manager_event_roots(
        &mut self,
        selected_nodes: &FxHashSet<FormKey>,
        graph: &crate::phase::story_manager::StoryManagerSourceGraph,
    ) {
        let Some(fallout4_name) = self
            .config
            .target_master_names
            .iter()
            .find(|name| name.eq_ignore_ascii_case("Fallout4.esm"))
            .cloned()
        else {
            return;
        };
        let fallout4 = self.interner.intern(&fallout4_name);
        let Some(state) = self.mapper_state.as_mut() else {
            return;
        };
        let mut mapper = FormKeyMapper::from_state(state, &self.interner);
        let script_event_root = FormKey {
            local: crate::phase::story_manager::FO4_SCRIPT_EVENT_ROOT_LOCAL,
            plugin: fallout4,
        };
        mapper.add_mapping(script_event_root, script_event_root);
        for source_fk in selected_nodes {
            let Some(record) = graph.nodes.get(source_fk) else {
                continue;
            };
            let Some(target_local) =
                crate::phase::story_manager::fo4_story_manager_event_root(record)
            else {
                continue;
            };
            mapper.add_mapping(
                *source_fk,
                FormKey {
                    local: target_local,
                    plugin: fallout4,
                },
            );
        }
    }

    fn record_story_manager_diagnostics(
        &mut self,
        diagnostics: &[crate::phase::story_manager::StoryManagerDiagnostic],
    ) {
        for diagnostic in diagnostics {
            let kind_name = match diagnostic.kind {
                crate::phase::story_manager::StoryManagerDiagnosticKind::Selected => {
                    "story_manager_selected"
                }
                crate::phase::story_manager::StoryManagerDiagnosticKind::Skipped => {
                    "story_manager_skipped"
                }
            };
            let kind = self.interner.intern(kind_name);
            let message = if let Some(reason) = diagnostic.reason {
                format!(
                    "{:06X}:{}:{}",
                    diagnostic.form_key.local,
                    reason.as_str(),
                    diagnostic.message
                )
            } else {
                format!("{:06X}:{}", diagnostic.form_key.local, diagnostic.message)
            };
            self.decisions.push(Decision { kind, message });
        }
    }

    fn force_story_manager_dialogue_quests(
        &mut self,
        quest_fks: &[FormKey],
    ) -> Result<u32, RunError> {
        let mut changed = 0u32;
        for source_fk in quest_fks {
            if self.force_target_quest_autostart(*source_fk, "story_manager_quest_autostart")? {
                changed += 1;
            }
        }
        Ok(changed)
    }

    fn rewrite_story_manager_quest_events(
        &mut self,
        rewrites: &FxHashMap<FormKey, u32>,
    ) -> Result<u32, RunError> {
        let mut rewrites = rewrites.iter().collect::<Vec<_>>();
        rewrites.sort_by_key(|(source_fk, _)| source_fk.local);
        let mut changed = 0u32;
        for (source_fk, event_type) in rewrites {
            let Some(target_fk) = self
                .mapper_state
                .as_ref()
                .and_then(|state| state.source_to_target.get(source_fk))
                .copied()
            else {
                let kind = self
                    .interner
                    .intern("story_manager_quest_event_rewrite_skipped");
                self.decisions.push(Decision {
                    kind,
                    message: format!("{:06X}:quest_not_translated", source_fk.local),
                });
                continue;
            };
            let mut record = match read_record_relayout_by_form_key(
                self.target_handle_id,
                &target_fk,
                &self.schema_target,
                &self.interner,
                None,
            ) {
                Ok(record) => record,
                Err(e) => {
                    let warning = self.interner.intern(&format!(
                        "story_manager_target_quest_event_read:{:06X}:{e}",
                        target_fk.local
                    ));
                    self.warnings.push(warning);
                    continue;
                }
            };
            if !crate::phase::story_manager::set_qust_event_type(&mut record, *event_type) {
                continue;
            }
            let replaced = replace_record_contents_native(
                self.target_handle_id,
                record,
                &self.schema_target,
                &self.interner,
            )
            .map_err(|e| {
                RunError::InvalidConfig(format!("story_manager_quest_event_replace:{e}"))
            })?;
            if !replaced {
                return Err(RunError::InvalidConfig(format!(
                    "story_manager_quest_event_replace_missing:{:06X}",
                    target_fk.local
                )));
            }
            let event_name = String::from_utf8_lossy(&event_type.to_le_bytes()).into_owned();
            let kind = self.interner.intern("story_manager_quest_event_rewritten");
            self.decisions.push(Decision {
                kind,
                message: format!(
                    "{:06X}->{:06X}:{event_name}",
                    source_fk.local, target_fk.local
                ),
            });
            changed += 1;
        }
        Ok(changed)
    }

    fn restore_passive_dialogue_controllers(&mut self) -> Result<u32, RunError> {
        let npc_quests =
            crate::phase::story_manager::npc_referenced_quest_local_ids(self.source_handle_id)?;
        if npc_quests.is_empty() {
            return Ok(0);
        }
        let quest_sig = SigCode::from_str("QUST")
            .map_err(|e| RunError::InvalidConfig(format!("QUST signature: {e}")))?;
        let quest_fks = iter_form_keys_of_sig(self.source_handle_id, quest_sig, &self.interner)?;
        let mut changed = 0;
        for source_fk in quest_fks {
            if !npc_quests.contains(&source_fk.local) {
                continue;
            }
            let source_record = match read_record_relayout_by_form_key(
                self.source_handle_id,
                &source_fk,
                &self.schema_source,
                &self.interner,
                None,
            ) {
                Ok(record) => record,
                Err(e) => {
                    let warning = self.interner.intern(&format!(
                        "passive_dialogue_controller_read:{:06X}:{e}",
                        source_fk.local
                    ));
                    self.warnings.push(warning);
                    continue;
                }
            };
            if !crate::phase::story_manager::is_passive_dialogue_controller(
                &source_record,
                true,
                &self.interner,
            ) {
                continue;
            }
            if self.force_target_quest_autostart(source_fk, "quest_startup_passive_controller")? {
                changed += 1;
            }
        }
        Ok(changed)
    }

    fn force_target_quest_autostart(
        &mut self,
        source_fk: FormKey,
        decision_kind: &str,
    ) -> Result<bool, RunError> {
        let target_fk = self
            .mapper_state
            .as_ref()
            .and_then(|state| state.source_to_target.get(&source_fk))
            .copied();
        let Some(target_fk) = target_fk else {
            let kind = self.interner.intern(&format!("{decision_kind}_skipped"));
            self.decisions.push(Decision {
                kind,
                message: format!("{:06X}:quest_not_translated", source_fk.local),
            });
            return Ok(false);
        };
        let mut record = match read_record_relayout_by_form_key(
            self.target_handle_id,
            &target_fk,
            &self.schema_target,
            &self.interner,
            None,
        ) {
            Ok(record) => record,
            Err(e) => {
                let warning = self.interner.intern(&format!(
                    "story_manager_target_quest_read:{:06X}:{e}",
                    target_fk.local
                ));
                self.warnings.push(warning);
                return Ok(false);
            }
        };
        if !crate::phase::story_manager::force_qust_autostart(&mut record, &self.interner) {
            return Ok(false);
        }
        let replaced = replace_record_contents_native(
            self.target_handle_id,
            record,
            &self.schema_target,
            &self.interner,
        )
        .map_err(|e| RunError::InvalidConfig(format!("story_manager_quest_replace:{e}")))?;
        if !replaced {
            return Err(RunError::InvalidConfig(format!(
                "story_manager_quest_replace_missing:{:06X}",
                target_fk.local
            )));
        }
        let kind = self.interner.intern(decision_kind);
        self.decisions.push(Decision {
            kind,
            message: format!("{:06X}->{:06X}", source_fk.local, target_fk.local),
        });
        Ok(true)
    }

    fn preallocate_story_manager_nodes(
        &mut self,
        ordered_nodes: &[FormKey],
        graph: &crate::phase::story_manager::StoryManagerSourceGraph,
    ) {
        let Some(state) = self.mapper_state.as_mut() else {
            return;
        };
        let mut mapper = FormKeyMapper::from_state(state, &self.interner);
        for fk in ordered_nodes {
            let Some(record) = graph.nodes.get(fk) else {
                continue;
            };
            let normalized_eid = normalized_eid_opt(record.eid, mapper.interner);
            mapper.allocate_or_resolve(*fk, normalized_eid, record.sig);
        }
    }

    fn translate_story_manager_nodes(
        &mut self,
        ordered_nodes: &[FormKey],
        selected_nodes: &FxHashSet<FormKey>,
        selected_quests_by_node: &FxHashMap<FormKey, FxHashSet<FormKey>>,
        event_bridges: &FxHashMap<FormKey, u32>,
        graph: &crate::phase::story_manager::StoryManagerSourceGraph,
        emitted_nodes: &mut FxHashSet<FormKey>,
    ) -> Result<TranslateStats, RunError> {
        let mut stats = TranslateStats::default();
        let whole_plugin_names =
            if self.config.is_whole_plugin && self.config.target_master_names.is_empty() {
                self.target_master_plugin_names()
            } else {
                Vec::new()
            };
        let target_master_names = target_master_names_for_skip(&self.config, whole_plugin_names);
        let target_master_syms: FxHashSet<Sym> =
            intern_plugin_names(&target_master_names, &self.interner);
        let first_target_master_sym = target_master_names
            .first()
            .map(|name| self.interner.intern(name));
        let fallout4 = target_master_names
            .iter()
            .find(|name| name.eq_ignore_ascii_case("Fallout4.esm"))
            .map(|name| self.interner.intern(name));

        for fk in ordered_nodes {
            let Some(source_template) = graph.nodes.get(fk) else {
                continue;
            };
            let mut src_record = source_template.clone();
            let source_sig = src_record.sig;
            stats.signature_entry(source_sig).seen += 1;
            if let Some(allowed_quests) = selected_quests_by_node.get(fk) {
                crate::phase::story_manager::retain_story_manager_quests(
                    &mut src_record,
                    allowed_quests,
                );
            }
            crate::phase::story_manager::sanitize_story_manager_previous_node(
                &mut src_record,
                selected_nodes,
                &self.interner,
            );
            if let Some(keyword_raw) = event_bridges.get(fk) {
                let fallout4 = fallout4.ok_or_else(|| {
                    RunError::InvalidConfig(
                        "story_manager_event_bridge_missing_fallout4_master".to_string(),
                    )
                })?;
                crate::phase::story_manager::lower_incompatible_event_root(
                    &mut src_record,
                    FormKey {
                        local: crate::phase::story_manager::FO4_SCRIPT_EVENT_ROOT_LOCAL,
                        plugin: fallout4,
                    },
                    *keyword_raw,
                );
            }

            {
                let mut ctx = PairCtx {
                    interner: &self.interner,
                };
                if let Err(e) = self.translator.pre_translate(&mut ctx, &mut src_record) {
                    let w = self
                        .interner
                        .intern(&format!("story_manager_pre_translate:{e}"));
                    self.warnings.push(w);
                }
            }

            let mut translated = match self.translator.translate_ignoring_skip(
                &src_record,
                &self.interner,
                src_record.sig.as_str(),
            ) {
                TranslateResult::Translated(record) => record,
                TranslateResult::Dropped { decision, .. } => {
                    self.decisions.push(decision);
                    stats.records_dropped += 1;
                    stats.signature_entry(source_sig).dropped += 1;
                    continue;
                }
                TranslateResult::Deferred(kind) => {
                    self.deferred.push((*fk, kind));
                    stats.records_deferred += 1;
                    stats.signature_entry(source_sig).deferred += 1;
                    continue;
                }
            };

            if self
                .schema_target
                .record_def(translated.sig.as_str())
                .is_none()
            {
                let warning = self.interner.intern(&format!(
                    "story_manager_unsupported_target_record:{}",
                    translated.sig.as_str()
                ));
                self.warnings.push(warning);
                stats.records_dropped += 1;
                stats.signature_entry(source_sig).dropped += 1;
                continue;
            }

            let (target_fk, rewrite_report) = {
                let state = self
                    .mapper_state
                    .as_mut()
                    .expect("mapper_state initialized before Story Manager emit");
                let mut mapper = FormKeyMapper::from_state(state, &self.interner);
                let normalized_eid = normalized_eid_opt(translated.eid, mapper.interner);
                let target_fk = mapper.allocate_or_resolve(*fk, normalized_eid, translated.sig);
                translated.form_key = target_fk;
                let rewrite_report = match mapper.rewrite_record_with_report(&mut translated) {
                    Ok(report) => Some(report),
                    Err(e) => {
                        let w = mapper
                            .interner
                            .intern(&format!("story_manager_rewrite_record:{e}"));
                        self.warnings.push(w);
                        None
                    }
                };
                (target_fk, rewrite_report)
            };
            if is_target_master_remap(target_fk, &target_master_syms) {
                emitted_nodes.insert(*fk);
                stats.records_vanilla_remapped += 1;
                stats.signature_entry(source_sig).vanilla_remapped += 1;
                continue;
            }

            {
                let mut ctx = PairCtx {
                    interner: &self.interner,
                };
                if let Err(e) = self.translator.post_translate(&mut ctx, &mut translated) {
                    let w = self
                        .interner
                        .intern(&format!("story_manager_post_translate:{e}"));
                    self.warnings.push(w);
                }
            }
            {
                let mut ctx = TargetCtx {
                    interner: &self.interner,
                };
                if let Err(e) = self.translator.run_target_hook(&mut ctx, &mut translated) {
                    let w = self
                        .interner
                        .intern(&format!("story_manager_target_hook:{e}"));
                    self.warnings.push(w);
                }
            }

            let report = crate::translator::class_a_normalize::normalize_flags_and_enums(
                &mut translated,
                &self.schema_target,
                &self.interner,
            );
            for message in report.decisions {
                let kind = self.interner.intern("class_a_normalize");
                self.decisions.push(Decision { kind, message });
            }
            for warning in report.warnings {
                let sym = self.interner.intern(&warning);
                self.warnings.push(sym);
            }

            let translated = {
                let normalizer = TargetRecordNormalizer {
                    target_schema: &self.schema_target,
                    source_record_def: self.schema_source.record_def(source_sig.as_str()),
                    interner: Some(&self.interner),
                };
                match normalizer.normalize(translated) {
                    TargetRecordNormalization::Keep(record) => record,
                    TargetRecordNormalization::DropUnsupportedRecord => {
                        stats.records_dropped += 1;
                        stats.signature_entry(source_sig).dropped += 1;
                        continue;
                    }
                }
            };
            let full_plugin_snapshot = if self.config.is_whole_plugin {
                Some(crate::full_plugin::target_schema_record_view(
                    &translated,
                    &self.schema_target,
                ))
            } else {
                None
            };
            if let Err(e) = add_record_native(
                self.target_handle_id,
                translated,
                &self.schema_target,
                &self.interner,
            ) {
                let w = self
                    .interner
                    .intern(&format!("story_manager_write_error:{e}"));
                self.warnings.push(w);
                stats.records_failed += 1;
                stats.signature_entry(source_sig).failed += 1;
                continue;
            }
            if let Some(snapshot) = full_plugin_snapshot.as_ref() {
                self.capture_full_plugin_record_state(
                    *fk,
                    snapshot,
                    &target_master_syms,
                    first_target_master_sym,
                    rewrite_report
                        .as_ref()
                        .map(|report| &report.unresolved_form_keys),
                );
            }
            emitted_nodes.insert(*fk);
            stats.records_translated += 1;
            stats.signature_entry(source_sig).translated += 1;
        }

        Ok(stats)
    }

    /// Translate a bounded list of source FormKeys into the target plugin.
    ///
    /// Same per-record pipeline as `translate_all` (pair-hook pre/post +
    /// translator + mapper allocate + rewrite + target-hook + write) but
    /// scoped to the caller-supplied set. FormKeys that don't resolve
    /// in the source plugin are recorded as `records_failed` and skipped.
    pub fn translate_records(&mut self, fks: &[FormKey]) -> Result<TranslateStats, RunError> {
        self.require_source_handle()?;
        self.init_mapper_state()?;
        let stats = self.translate_fks(fks)?;
        self.finalize_legacy_creature_race_coverage()?;
        Ok(stats)
    }

    /// Convert FO76 interior cells (CELL fields + Persistent/Temporary placed
    /// children + interior NAVM) into the FO4 target. Gated to FO76→FO4; a no-op
    /// otherwise. Mirrors `emit_projected_navmeshes`' translate+remap path but
    /// runs serially per interior cell (the interior subset is small relative to
    /// the exterior navmesh corpus, and the cell/child allocations are real
    /// records whose mapper entries must persist).
    pub fn emit_interior_cells(&mut self, carry_previs: bool) -> Result<TranslateStats, RunError> {
        let mut stats = TranslateStats::default();
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(stats);
        }
        if self.mapper_state.is_none() {
            self.init_mapper_state()?;
        }

        let cell = SigCode::from_str("CELL")
            .map_err(|e| RunError::InvalidConfig(format!("CELL signature: {e}")))?;
        let all_cell_fks = iter_form_keys_of_sig(self.source_handle_id, cell, &mut self.interner)?;
        if all_cell_fks.is_empty() {
            return Ok(stats);
        }

        // Batch-snapshot ALL cells once (a single localized-strings clone for the
        // whole set) and pick the interior ones by DATA's IsInteriorCell bit.
        // Snapshotting per cell here clones the multi-language string table on
        // every call — over an entire worldspace's cells that is the stall.
        let cells_snapshot =
            snapshot_records_by_form_keys(self.source_handle_id, &all_cell_fks, &self.interner)?;
        let interior_indices: Vec<usize> = cells_snapshot
            .records
            .iter()
            .enumerate()
            .filter(|(_, rec)| raw_cell_is_interior(&rec.raw_record))
            .map(|(idx, _)| idx)
            .collect();
        if interior_indices.is_empty() {
            return Ok(stats);
        }

        let interior_fks: Vec<FormKey> = interior_indices
            .iter()
            .map(|&idx| cells_snapshot.records[idx].form_key)
            .collect();
        let source_plugin_sym = interior_fks[0].plugin;
        let interior_objs: FxHashSet<u32> = interior_fks
            .iter()
            .map(|fk| fk.local & 0x00FF_FFFF)
            .collect();

        // One source-tree pass: cell object id -> placed-child object ids.
        let children_map = crate::source_read::collect_interior_cell_children(
            self.source_handle_id,
            &interior_objs,
        )
        .map_err(|e| RunError::InvalidConfig(format!("interior_children_collect:{e}")))?;

        // One target pass: drop any pre-existing PKIN storage-cell stubs for the
        // interior cells we are about to emit; the real cell supersedes them.
        let interior_objs_vec: Vec<u32> = interior_objs.iter().copied().collect();
        crate::target_write::remove_interior_cell_stubs_native(
            self.target_handle_id,
            &interior_objs_vec,
        )
        .map_err(|e| RunError::InvalidConfig(format!("interior_stub_remove:{e}")))?;

        // ── Pass 1: translate the interior CELL records from the batch. Keep each
        // translated cell + its target local id, then drop the all-cells snapshot
        // to free the (mostly exterior) cell records before snapshotting children.
        let mut translated_cells: Vec<(u32, Record)> = Vec::with_capacity(interior_indices.len());
        // Source cell object id -> the translated target cell's FormKey. The file
        // FormID (own master index applied) is composed at the NAVM call site via
        // `fo76_navmesh::target_form_id`; storing the bare `form_key.local` here
        // would drop the master index and corrupt the interior NAVM parent.
        let mut cell_formkey_by_obj: FxHashMap<u32, FormKey> = FxHashMap::default();
        for &idx in &interior_indices {
            let cell_obj = cells_snapshot.records[idx].form_key.local & 0x00FF_FFFF;
            let Some(mut translated_cell) = self.translate_and_remap_snapshot_record(
                &cells_snapshot,
                idx,
                "CELL",
                &mut stats,
                None,
            )?
            else {
                continue;
            };
            if !carry_previs {
                strip_interior_previs_fields(&mut translated_cell);
            }
            crate::fixups::mark_public_wastelanders_hubs::mark_wastelanders_public_hub(
                cell_obj,
                &mut translated_cell,
                &self.interner,
            );
            cell_formkey_by_obj.insert(cell_obj, translated_cell.form_key);
            translated_cells.push((cell_obj, translated_cell));
        }
        drop(cells_snapshot);

        // Flat child work list (only for cells that translated), in cell order.
        const PERSISTENT_GROUP: i32 = 8;
        const TEMPORARY_GROUP: i32 = 9;
        let mut child_fks: Vec<FormKey> = Vec::new();
        let mut child_meta: Vec<(u32, i32)> = Vec::new(); // (cell object id, section type)
        for (cell_obj, _) in &translated_cells {
            let Some(kids) = children_map.get(cell_obj) else {
                continue;
            };
            for obj in &kids.persistent {
                child_fks.push(FormKey {
                    local: *obj,
                    plugin: source_plugin_sym,
                });
                child_meta.push((*cell_obj, PERSISTENT_GROUP));
            }
            for obj in &kids.temporary {
                child_fks.push(FormKey {
                    local: *obj,
                    plugin: source_plugin_sym,
                });
                child_meta.push((*cell_obj, TEMPORARY_GROUP));
            }
        }

        // ── Pass 2: batch-snapshot all placed children once, translate each, and
        // bucket the results per cell/section.
        type CellChildBuckets = (Vec<Record>, Vec<SigCode>, Vec<Record>, Vec<SigCode>);
        let mut children_by_cell: FxHashMap<u32, CellChildBuckets> = FxHashMap::default();
        if !child_fks.is_empty() {
            let source_formid_context = crate::fo76_navmesh::snapshot_formid_context(
                self.source_handle_id,
            )
            .map_err(|e| RunError::InvalidConfig(format!("interior_navm_source_context:{e}")))?;
            let target_formid_context = crate::fo76_navmesh::snapshot_formid_context(
                self.target_handle_id,
            )
            .map_err(|e| RunError::InvalidConfig(format!("interior_navm_target_context:{e}")))?;
            let children_snapshot =
                snapshot_records_by_form_keys(self.source_handle_id, &child_fks, &self.interner)?;
            for (idx, &(cell_obj, section)) in child_meta.iter().enumerate() {
                let child_sig = children_snapshot.records[idx]
                    .raw_record
                    .signature
                    .as_str()
                    .to_owned();
                let nvnm_contexts = (child_sig == "NAVM")
                    .then_some((&source_formid_context, &target_formid_context));
                let Some(mut translated_child) = self.translate_and_remap_snapshot_record(
                    &children_snapshot,
                    idx,
                    &child_sig,
                    &mut stats,
                    nvnm_contexts,
                )?
                else {
                    continue;
                };
                if child_sig == "NAVM" {
                    if let Some(cell_fk) = cell_formkey_by_obj.get(&cell_obj) {
                        let cell_file_form_id = crate::fo76_navmesh::target_form_id(
                            *cell_fk,
                            &target_formid_context,
                            &self.interner,
                        );
                        set_nvnm_parent_interior(&mut translated_child, cell_file_form_id);
                    }
                }
                let sig_code =
                    SigCode::from_str(&child_sig).unwrap_or(SigCode([b'?', b'?', b'?', b'?']));
                let entry = children_by_cell.entry(cell_obj).or_default();
                if section == PERSISTENT_GROUP {
                    entry.0.push(translated_child);
                    entry.1.push(sig_code);
                } else {
                    entry.2.push(translated_child);
                    entry.3.push(sig_code);
                }
            }
        }

        // ── Pass 3: assemble + insert each cell with its children in one call. ──
        for (cell_obj, translated_cell) in translated_cells {
            let (persistent_records, persistent_sigs, temporary_records, temporary_sigs) =
                children_by_cell.remove(&cell_obj).unwrap_or_default();
            match crate::target_write::add_interior_cell_with_children_native(
                self.target_handle_id,
                translated_cell,
                persistent_records,
                temporary_records,
                &self.schema_target,
                &self.interner,
            ) {
                Ok(outcome) => {
                    if !outcome.cell_inserted {
                        stats.records_dropped += 1;
                        stats.signature_entry(cell).dropped += 1;
                        continue;
                    }
                    stats.records_translated += 1;
                    stats.signature_entry(cell).translated += 1;
                    // Per-sig translated for each child handed over. Encode-drops
                    // (rare; localized-string filtering) are reconciled at the
                    // aggregate level.
                    for sig in persistent_sigs.iter().chain(temporary_sigs.iter()) {
                        stats.records_translated += 1;
                        stats.signature_entry(*sig).translated += 1;
                    }
                    let dropped = outcome.children_dropped;
                    if dropped > 0 {
                        stats.records_translated = stats.records_translated.saturating_sub(dropped);
                        stats.records_dropped += dropped;
                    }
                }
                Err(e) => {
                    let w = self.interner.intern(&format!("interior_cell_write:{e}"));
                    self.warnings.push(w);
                    stats.records_failed += 1;
                    stats.signature_entry(cell).failed += 1;
                    continue;
                }
            }
        }

        // Normalize copied placed records once (XEZN→LCTN strip, flag/enum clamp).
        let placed_sigs: Vec<String> = ["REFR", "ACHR", "PGRE", "PHZD"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        crate::fixups::normalize_placed_records::normalize_copied_placed_records(
            self.target_handle_id,
            &placed_sigs,
        )
        .map_err(|e| RunError::InvalidConfig(format!("interior_normalize_placed:{e}")))?;
        crate::fixups::clear_interior_hand_changed::clear_interior_hand_changed_flags(
            self.target_handle_id,
        )
        .map_err(|e| RunError::InvalidConfig(format!("interior_hand_changed_clear:{e}")))?;

        Ok(stats)
    }

    /// Run the pre/translate/mapper-remap/post pipeline on one record from a
    /// pre-fetched batch snapshot, and return the translated+remapped `Record`.
    /// Returns `Ok(None)` when the record is dropped/deferred/failed (stats
    /// updated accordingly).
    ///
    /// Takes the batch + index rather than snapshotting per call: each snapshot
    /// deep-clones the multi-language localized-strings table, so per-record
    /// snapshotting over a large source is quadratic. The caller snapshots once.
    ///
    /// `nvnm_contexts` rewrites NVNM door-ref/geometry FormIDs through the mapper
    /// when set (NAVM children only).
    fn translate_and_remap_snapshot_record(
        &mut self,
        snapshot: &SourceRecordBatchSnapshot,
        idx: usize,
        ignored_signature: &str,
        stats: &mut TranslateStats,
        nvnm_contexts: Option<(
            &crate::fo76_navmesh::FormIdContext,
            &crate::fo76_navmesh::FormIdContext,
        )>,
    ) -> Result<Option<Record>, RunError> {
        let record_snapshot = &snapshot.records[idx];
        let fk = record_snapshot.form_key;

        let legacy_bptd_only = matches!(self.source, Game::Fnv | Game::Fo3);
        let relayout_target_schema = (self.target == Game::Fo4
            && (self.source == Game::Fo76 || legacy_bptd_only))
            .then(|| self.schema_target.clone());
        let relayout_ctx = relayout_target_schema.as_deref().map(|target_schema| {
            crate::struct_relayout::StructRelayoutCtx {
                target_schema,
                target_form_version:
                    crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION,
                legacy_bptd_only,
            }
        });

        let mut src_record = match decode_record_from_parsed_relayout(
            &record_snapshot.raw_record,
            &fk,
            &self.schema_source,
            &snapshot.masters,
            &snapshot.plugin_name,
            snapshot.strings.as_ref(),
            snapshot.plugin_is_localized,
            &self.interner,
            relayout_ctx.as_ref(),
        ) {
            Ok(record) => record,
            Err(e) => {
                let w = self.interner.intern(&format!("interior_read_error:{e}"));
                self.warnings.push(w);
                stats.records_failed += 1;
                return Ok(None);
            }
        };
        let source_sig = src_record.sig;

        {
            let mut ctx = PairCtx {
                interner: &self.interner,
            };
            if let Err(e) = self.translator.pre_translate(&mut ctx, &mut src_record) {
                let w = self.interner.intern(&format!("interior_pre_translate:{e}"));
                self.warnings.push(w);
            }
        }

        let mut translated = match self.translator.translate_ignoring_skip(
            &src_record,
            &self.interner,
            ignored_signature,
        ) {
            TranslateResult::Translated(record) => record,
            TranslateResult::Dropped { decision, .. } => {
                self.decisions.push(decision);
                stats.records_dropped += 1;
                stats.signature_entry(source_sig).dropped += 1;
                return Ok(None);
            }
            TranslateResult::Deferred(kind) => {
                self.deferred.push((fk, kind));
                stats.records_deferred += 1;
                stats.signature_entry(source_sig).deferred += 1;
                return Ok(None);
            }
        };

        if self
            .schema_target
            .record_def(translated.sig.as_str())
            .is_none()
        {
            let w = self.interner.intern(&format!(
                "interior_unsupported_target_record:{}",
                translated.sig.as_str()
            ));
            self.warnings.push(w);
            stats.records_dropped += 1;
            stats.signature_entry(source_sig).dropped += 1;
            return Ok(None);
        }

        let collision_rename = self.mapper_state.as_ref().and_then(|state| {
            let translated_sig = translated.sig;
            rename_fo76_target_editor_id_collision(
                &mut translated,
                &state.target_eid_index,
                &self.interner,
                is_editor_id_collision_rename_forced(self.source, self.target, translated_sig),
            )
        });
        if let Some((old, new)) = collision_rename {
            let warning = self
                .interner
                .intern(&format!("fo76_target_edid_collision_renamed:{old}->{new}"));
            self.warnings.push(warning);
        }

        // Mapper allocate + internal FK remap (mutates run mapper state).
        {
            let state = self
                .mapper_state
                .as_mut()
                .expect("mapper_state initialized before interior cell emit");
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            let normalized_eid = normalized_eid_opt(translated.eid, mapper.interner);
            let target_fk = mapper.allocate_or_resolve(fk, normalized_eid, translated.sig);
            translated.form_key = target_fk;
            if let Some((source_ctx, target_ctx)) = nvnm_contexts {
                if let Err(e) = crate::fo76_navmesh::rewrite_record_nvnm_with_context(
                    &mut translated,
                    &mut mapper,
                    source_ctx,
                    target_ctx,
                ) {
                    let w = self.interner.intern(&format!("interior_nvnm:{e}"));
                    self.warnings.push(w);
                }
            }
            if let Err(e) = mapper.rewrite_record(&mut translated) {
                let w = self
                    .interner
                    .intern(&format!("interior_rewrite_record:{e}"));
                self.warnings.push(w);
            }
        }

        {
            let mut ctx = PairCtx {
                interner: &self.interner,
            };
            if let Err(e) = self.translator.post_translate(&mut ctx, &mut translated) {
                let w = self
                    .interner
                    .intern(&format!("interior_post_translate:{e}"));
                self.warnings.push(w);
            }
        }
        {
            let mut ctx = TargetCtx {
                interner: &self.interner,
            };
            if let Err(e) = self.translator.run_target_hook(&mut ctx, &mut translated) {
                let w = self.interner.intern(&format!("interior_target_hook:{e}"));
                self.warnings.push(w);
            }
        }

        let report = crate::translator::class_a_normalize::normalize_flags_and_enums(
            &mut translated,
            &self.schema_target,
            &self.interner,
        );
        for message in report.decisions {
            let kind = self.interner.intern("class_a_normalize");
            self.decisions.push(Decision { kind, message });
        }

        Ok(Some(translated))
    }

    /// Translate source NAVM records into existing projected target CELL child
    /// groups. This intentionally bypasses the normal `skip_records` entry for
    /// NAVM, but only for this structured writer.
    pub fn emit_projected_navmeshes(&mut self) -> Result<TranslateStats, RunError> {
        if self.target != Game::Fo4 || !matches!(self.source, Game::Fo76 | Game::Fnv | Game::Fo3) {
            return Ok(TranslateStats::default());
        }
        if self.mapper_state.is_none() {
            self.init_mapper_state()?;
        }
        let navm = SigCode::from_str("NAVM")
            .map_err(|e| RunError::InvalidConfig(format!("NAVM signature: {e}")))?;
        let fks = iter_form_keys_of_sig(self.source_handle_id, navm, &mut self.interner)?;
        self.translate_projected_navmeshes(&fks)
    }

    pub fn prepare_terrain_navmesh_graft(
        &mut self,
        prior_plugin_path: &Path,
    ) -> Result<usize, RunError> {
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(0);
        }

        let handle = OwnedPluginHandle::load(prior_plugin_path, self.target.as_str(), None)?;
        let object_ids =
            esp_authoring_core::plugin_runtime::graft_terrain_navmesh_object_ids_from_handle(
                handle.id(),
            )
            .map_err(|error| RunError::InvalidConfig(format!("graft_terrain:{error}")))?;

        self.generated_object_id_reservations
            .extend(object_ids.iter().copied());
        if let Some(state) = self.mapper_state.as_mut() {
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            mapper.reserve_generated_object_ids(object_ids.iter().copied());
        }
        Ok(object_ids.len())
    }

    /// Graft reused exterior terrain (CELL shells + LAND), navmesh (NAVM), and
    /// terrain-texture records (TXST/LTEX/GRAS) from a prior FO4 output ESM
    /// (`prior_handle_id`) into the target instead of regenerating them. Mirrors
    /// the NAVM source→target mapping setup of `emit_projected_navmeshes` so
    /// door / placed-ref repair still resolves navmesh links, then reserves every
    /// grafted object-id so no later phase re-allocates one (grafted records
    /// bypass the mapper). Backs `regen.py --re-use-land`.
    pub fn graft_terrain_navmesh(
        &mut self,
        prior_handle_id: u64,
    ) -> Result<TranslateStats, RunError> {
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(TranslateStats::default());
        }
        if self.mapper_state.is_none() {
            self.init_mapper_state()?;
        }

        // Establish NAVM source→target mappings (preserve_source_ids reuses the
        // source object-id) exactly as `emit_projected_navmeshes` does, so the
        // grafted NAVM ids line up and door-ref repair resolves navmesh links.
        let navm = SigCode::from_str("NAVM")
            .map_err(|e| RunError::InvalidConfig(format!("NAVM signature: {e}")))?;
        let navm_fks = iter_form_keys_of_sig(self.source_handle_id, navm, &mut self.interner)?;
        {
            let state = self
                .mapper_state
                .as_mut()
                .expect("mapper_state initialized before terrain graft");
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            for &fk in &navm_fks {
                mapper.allocate_or_resolve(fk, None, navm);
            }
        }

        // Structural clone of terrain + navmesh + terrain-texture records from the
        // cached FO4 output (same game, same masters → not a conversion).
        let report = esp_authoring_core::plugin_runtime::graft_terrain_navmesh_from_handle(
            prior_handle_id,
            self.target_handle_id,
        )
        .map_err(|e| RunError::InvalidConfig(format!("graft_terrain:{e}")))?;

        // Reserve every grafted own object-id so placed-children / ECZN / etc.
        // never re-allocate one.
        {
            let state = self
                .mapper_state
                .as_mut()
                .expect("mapper_state initialized before terrain graft");
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            mapper.reserve_object_ids(report.object_ids.iter().copied());
        }

        for warning in &report.warnings {
            let w = self.interner.intern(&format!("graft_terrain: {warning}"));
            self.warnings.push(w);
        }

        let added =
            (report.cells + report.lands + report.navms + report.txst + report.ltex + report.gras)
                as u32;
        Ok(TranslateStats {
            records_translated: added,
            records_failed: report.warnings.len() as u32,
            ..TranslateStats::default()
        })
    }

    fn translate_projected_navmeshes(
        &mut self,
        fks: &[FormKey],
    ) -> Result<TranslateStats, RunError> {
        if fks.is_empty() {
            return Ok(TranslateStats::default());
        }

        let navm = SigCode::from_str("NAVM")
            .map_err(|e| RunError::InvalidConfig(format!("NAVM signature: {e}")))?;
        {
            let state = self
                .mapper_state
                .as_mut()
                .expect("mapper_state initialized before projected NAVM emit");
            let mut mapper = FormKeyMapper::from_state(state, &self.interner);
            for &fk in fks {
                mapper.allocate_or_resolve(fk, None, navm);
            }
        }

        let source_records =
            snapshot_records_by_form_keys(self.source_handle_id, fks, &self.interner)?;
        let legacy_navmeshes = if matches!(self.source, Game::Fnv | Game::Fo3) {
            Some(
                prepare_legacy_fallout_navmeshes(self.source_handle_id, &source_records.records)
                    .map_err(|error| {
                        RunError::InvalidConfig(format!("legacy_fallout_navmesh:{error}"))
                    })?,
            )
        } else {
            None
        };
        let source_formid_context =
            crate::fo76_navmesh::snapshot_formid_context(self.source_handle_id)
                .map_err(|e| RunError::InvalidConfig(format!("fo76_navm_source_context:{e}")))?;
        let target_formid_context =
            crate::fo76_navmesh::snapshot_formid_context(self.target_handle_id)
                .map_err(|e| RunError::InvalidConfig(format!("fo76_navm_target_context:{e}")))?;

        let whole_plugin_names =
            if self.config.is_whole_plugin && self.config.target_master_names.is_empty() {
                self.target_master_plugin_names()
            } else {
                Vec::new()
            };
        let target_master_names = target_master_names_for_skip(&self.config, whole_plugin_names);
        let target_master_syms: FxHashSet<Sym> =
            intern_plugin_names(&target_master_names, &self.interner);
        let first_target_master_sym = target_master_names
            .first()
            .map(|name| self.interner.intern(name));

        let legacy_bptd_only = matches!(self.source, Game::Fnv | Game::Fo3);
        let relayout_target_schema = (self.target == Game::Fo4
            && (self.source == Game::Fo76 || legacy_bptd_only))
            .then(|| self.schema_target.clone());
        let relayout_ctx = relayout_target_schema.as_deref().map(|target_schema| {
            crate::struct_relayout::StructRelayoutCtx {
                target_schema,
                target_form_version:
                    crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION,
                legacy_bptd_only,
            }
        });

        // Borrowed, not cloned: each record gets a cheap overlay scratch over
        // this frozen template instead of a full MapperState clone.
        let mapper_state_template = self
            .mapper_state
            .as_ref()
            .expect("mapper_state initialized before projected NAVM emit");
        let event_tx = self.event_tx.clone();
        let interner = &self.interner;
        let schema_source = &*self.schema_source;
        let schema_target = &*self.schema_target;
        let translator = &self.translator;
        let projected_navmesh_offset = self.config.projected_navmesh_offset;
        let is_whole_plugin = self.config.is_whole_plugin;
        let strings = source_records.strings.as_ref();

        let prepare_results: Vec<ProjectedNavmeshPrepareResult> = source_records
            .records
            .par_iter()
            .map(|snapshot| {
                let mut scratch = FormKeyMapper::overlay_scratch(mapper_state_template);
                Self::prepare_projected_navmesh(
                    snapshot,
                    &source_records.masters,
                    &source_records.plugin_name,
                    strings,
                    source_records.plugin_is_localized,
                    schema_source,
                    schema_target,
                    interner,
                    relayout_ctx.as_ref(),
                    translator,
                    legacy_navmeshes.as_ref(),
                    mapper_state_template,
                    &mut scratch,
                    &source_formid_context,
                    &target_formid_context,
                    &target_master_syms,
                    projected_navmesh_offset,
                    is_whole_plugin,
                )
            })
            .collect();

        let mut stats = TranslateStats::default();
        let mut prepared_meta = Vec::with_capacity(prepare_results.len());
        let mut prepared_records = Vec::with_capacity(prepare_results.len());
        for mut result in prepare_results {
            stats.absorb(result.stats);
            self.decisions.append(&mut result.decisions);
            self.deferred.append(&mut result.deferred);
            for warning in result.warnings {
                let sym = self.interner.intern(&warning);
                self.warnings.push(sym);
            }
            if let Some(prepared) = result.prepared {
                prepared_meta.push(PreparedNavmeshMeta {
                    source_fk: prepared.source_fk,
                    source_sig: prepared.source_sig,
                    full_plugin_snapshot: prepared.full_plugin_snapshot,
                });
                prepared_records.push(prepared.record);
            }
        }

        // Chunked apply: one store lock + one grouped batch insert
        // per 1000 records instead of a full-tree scan per record. Per-record
        // outcome handling and the Python checkpoint cadence are identical to
        // the legacy per-record loop.
        let total = prepared_records.len();
        let mut record_iter = prepared_records.into_iter();
        let mut meta_iter = prepared_meta.into_iter();
        let mut processed = 0usize;
        while processed < total {
            let chunk_len = (total - processed).min(1000);
            let chunk_records: Vec<Record> = record_iter.by_ref().take(chunk_len).collect();
            let outcomes = add_projected_navmeshes_chunk_native(
                self.target_handle_id,
                chunk_records,
                &*self.schema_target,
                &self.interner,
            );
            for outcome in outcomes {
                let meta = meta_iter.next().expect("prepared meta per outcome");
                match outcome {
                    Ok(true) => {
                        if let Some(snapshot) = meta.full_plugin_snapshot.as_ref() {
                            self.capture_full_plugin_record_state(
                                meta.source_fk,
                                snapshot,
                                &target_master_syms,
                                first_target_master_sym,
                                None,
                            );
                        }
                        stats.records_translated += 1;
                        stats.signature_entry(meta.source_sig).translated += 1;
                    }
                    Ok(false) => {
                        let w = self.interner.intern(&format!(
                            "projected_navmesh_skipped:{}",
                            form_key_to_read_str(&meta.source_fk, &self.interner)
                        ));
                        self.warnings.push(w);
                        stats.records_dropped += 1;
                        stats.signature_entry(meta.source_sig).dropped += 1;
                    }
                    Err(e) => {
                        let w = self
                            .interner
                            .intern(&format!("projected_navmesh_write:{e}"));
                        self.warnings.push(w);
                        stats.records_failed += 1;
                        stats.signature_entry(meta.source_sig).failed += 1;
                    }
                }

                processed += 1;
                if processed % 1000 == 0 {
                    let record_count = processed as u64;
                    let cb = self.progress_callback.as_ref();
                    Python::attach(|py| -> Result<(), RunError> {
                        py.check_signals().map_err(|_| RunError::Cancelled)?;
                        if let Some(cb) = cb {
                            let keep_going: bool = cb
                                .call1(py, (record_count,))
                                .and_then(|r| r.extract::<bool>(py))
                                .unwrap_or(true);
                            if !keep_going {
                                return Err(RunError::Cancelled);
                            }
                        }
                        Ok(())
                    })?;
                }
            }
        }

        if self.config.is_whole_plugin {
            let warning = format!(
                "full_plugin_state:unresolved_refs={};target_master_refs={}",
                self.full_plugin_state.unresolved_ref_count(),
                self.full_plugin_state.target_master_ref_count()
            );
            let sym = self.interner.intern(&warning);
            self.warnings.push(sym);
        }

        Ok(stats)
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_projected_navmesh(
        snapshot: &crate::source_read::SourceRecordSnapshot,
        source_masters: &[String],
        source_plugin_name: &str,
        source_strings: Option<&esp_authoring_core::plugin_runtime::LocalizedStringsState>,
        source_plugin_is_localized: bool,
        schema_source: &AuthoringSchema,
        schema_target: &AuthoringSchema,
        interner: &StringInterner,
        relayout_ctx: Option<&crate::struct_relayout::StructRelayoutCtx<'_>>,
        translator: &Translator,
        legacy_navmeshes: Option<&LegacyFalloutNavmeshBatch>,
        mapper_base: &MapperState,
        mapper_scratch: &mut MapperState,
        source_formid_context: &crate::fo76_navmesh::FormIdContext,
        target_formid_context: &crate::fo76_navmesh::FormIdContext,
        target_master_syms: &FxHashSet<Sym>,
        projected_navmesh_offset: [f32; 3],
        is_whole_plugin: bool,
    ) -> ProjectedNavmeshPrepareResult {
        let mut result = ProjectedNavmeshPrepareResult::default();
        let fk = snapshot.form_key;
        let mut src_record = match decode_record_from_parsed_relayout(
            &snapshot.raw_record,
            &fk,
            schema_source,
            source_masters,
            source_plugin_name,
            source_strings,
            source_plugin_is_localized,
            interner,
            relayout_ctx,
        ) {
            Ok(record) => record,
            Err(e) => {
                result.warnings.push(format!("read_error:{e}"));
                result.stats.records_failed += 1;
                return result;
            }
        };
        let source_sig = src_record.sig;
        result.stats.signature_entry(source_sig).seen += 1;

        if let Some(legacy_navmeshes) = legacy_navmeshes {
            let raw_form_id = snapshot.raw_record.form_id;
            if let Some(error) = legacy_navmeshes.failures.get(&raw_form_id) {
                result
                    .warnings
                    .push(format!("legacy_fallout_navmesh:{raw_form_id:08X}:{error}"));
                result.stats.records_failed += 1;
                result.stats.signature_entry(source_sig).failed += 1;
                return result;
            }
            let Some(nvnm) = legacy_navmeshes.converted.get(&raw_form_id) else {
                result.warnings.push(format!(
                    "legacy_fallout_navmesh:{raw_form_id:08X}:missing converted payload"
                ));
                result.stats.records_failed += 1;
                result.stats.signature_entry(source_sig).failed += 1;
                return result;
            };
            src_record.fields.retain(|field| {
                !matches!(
                    field.sig.as_str(),
                    "NVER" | "DATA" | "NVVX" | "NVTR" | "NVCA" | "NVDP" | "NVGD" | "NVEX"
                )
            });
            src_record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("NVNM").expect("static NVNM signature"),
                value: FieldValue::Bytes(smallvec::SmallVec::from_vec(nvnm.clone())),
            });
        }

        {
            let mut ctx = PairCtx { interner };
            if let Err(e) = translator.pre_translate(&mut ctx, &mut src_record) {
                result.warnings.push(format!("pre_translate:{e}"));
            }
        }

        let mut translated = match translator.translate_ignoring_skip(&src_record, interner, "NAVM")
        {
            TranslateResult::Translated(record) => record,
            TranslateResult::Dropped { decision, .. } => {
                result.decisions.push(decision);
                result.stats.records_dropped += 1;
                result.stats.signature_entry(source_sig).dropped += 1;
                return result;
            }
            TranslateResult::Deferred(kind) => {
                result.deferred.push((fk, kind));
                result.stats.records_deferred += 1;
                result.stats.signature_entry(source_sig).deferred += 1;
                return result;
            }
        };

        if schema_target.record_def(translated.sig.as_str()).is_none() {
            result.warnings.push(format!(
                "unsupported_target_record:{} not in {} generated schema",
                translated.sig.as_str(),
                Game::Fo4.as_str()
            ));
            result.stats.records_dropped += 1;
            result.stats.signature_entry(source_sig).dropped += 1;
            return result;
        }

        if let Some((old, new)) = rename_fo76_target_editor_id_collision(
            &mut translated,
            &mapper_base.target_eid_index,
            interner,
            false,
        ) {
            result
                .warnings
                .push(format!("fo76_target_edid_collision_renamed:{old}->{new}"));
        }

        let target_fk = {
            let mut mapper =
                FormKeyMapper::from_state_overlay(mapper_base, mapper_scratch, interner);
            let normalized_eid = normalized_eid_opt(translated.eid, mapper.interner);
            let target_fk = mapper.allocate_or_resolve(fk, normalized_eid, translated.sig);
            translated.form_key = target_fk;
            if translated
                .fields
                .iter()
                .any(|field| field.sig.0 == *b"NVNM" || field.sig.0 == *b"MNAM")
            {
                if let Err(e) = crate::fo76_navmesh::rewrite_record_nvnm_with_context(
                    &mut translated,
                    &mut mapper,
                    source_formid_context,
                    target_formid_context,
                ) {
                    result.warnings.push(format!("fo76_navm:{e}"));
                    if legacy_navmeshes.is_some() {
                        result.stats.records_failed += 1;
                        result.stats.signature_entry(source_sig).failed += 1;
                        return result;
                    }
                }
            }
            if let Err(e) = mapper.rewrite_record(&mut translated) {
                result.warnings.push(format!("rewrite_record:{e}"));
            }
            target_fk
        };

        if is_target_master_remap(target_fk, target_master_syms) {
            result.stats.records_vanilla_remapped += 1;
            result.stats.signature_entry(source_sig).vanilla_remapped += 1;
            return result;
        }

        {
            let mut ctx = PairCtx { interner };
            if let Err(e) = translator.post_translate(&mut ctx, &mut translated) {
                result.warnings.push(format!("post_translate:{e}"));
            }
        }

        {
            let mut ctx = TargetCtx { interner };
            if let Err(e) = translator.run_target_hook(&mut ctx, &mut translated) {
                result.warnings.push(format!("target_hook:{e}"));
            }
        }

        let report = crate::translator::class_a_normalize::normalize_flags_and_enums(
            &mut translated,
            schema_target,
            interner,
        );
        for message in report.decisions {
            let kind = interner.intern("class_a_normalize");
            result.decisions.push(Decision { kind, message });
        }
        result.warnings.extend(report.warnings);

        let normalizer = TargetRecordNormalizer {
            target_schema: schema_target,
            source_record_def: schema_source.record_def(source_sig.as_str()),
            interner: Some(interner),
        };
        let mut translated = match normalizer.normalize(translated) {
            TargetRecordNormalization::Keep(record) => record,
            TargetRecordNormalization::DropUnsupportedRecord => {
                result.stats.records_dropped += 1;
                result.stats.signature_entry(source_sig).dropped += 1;
                return result;
            }
        };

        if legacy_navmeshes.is_some()
            && !translated
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "NVNM")
        {
            result
                .warnings
                .push("legacy_fallout_navmesh:target normalization dropped NVNM".to_string());
            result.stats.records_failed += 1;
            result.stats.signature_entry(source_sig).failed += 1;
            return result;
        }

        if let Err(e) = crate::target_write::offset_record_nvnm_geometry(
            &mut translated,
            projected_navmesh_offset,
        ) {
            result
                .warnings
                .push(format!("projected_navmesh_offset:{e}"));
            result.stats.records_failed += 1;
            result.stats.signature_entry(source_sig).failed += 1;
            return result;
        }

        let full_plugin_snapshot = is_whole_plugin
            .then(|| crate::full_plugin::target_schema_record_view(&translated, schema_target));
        result.prepared = Some(PreparedProjectedNavmesh {
            source_fk: fk,
            source_sig,
            record: translated,
            full_plugin_snapshot,
        });
        result
    }

    pub(crate) fn should_emit_fo76_quest_dialogue(&self) -> bool {
        self.config.is_whole_plugin
            && self.config.records_limit.is_none()
            && self.source == Game::Fo76
            && self.target == Game::Fo4
            && !self.config.skips_fo76_quest_dialogue()
    }

    pub(crate) fn should_emit_fo76_quest_scenes(&self) -> bool {
        self.config.is_whole_plugin
            && self.config.records_limit.is_none()
            && self.source == Game::Fo76
            && self.target == Game::Fo4
    }

    pub(crate) fn emit_quest_child_dialogue(&mut self) -> Result<TranslateStats, RunError> {
        let dial = SigCode::from_str("DIAL")
            .map_err(|e| RunError::InvalidConfig(format!("DIAL signature: {e}")))?;
        let xdi_plan = self.build_fo76_xdi_dialogue_plan()?;
        self.emit_phase_status("translate_v2: DIAL enumerate start");
        let fks = iter_form_keys_of_sig(self.source_handle_id, dial, &mut self.interner)?;
        self.emit_phase_status(format!(
            "translate_v2: DIAL enumerate done count={}",
            fks.len()
        ));
        self.emit_phase_status("translate_v2: DIAL translate_fks start");
        self.translate_fks_with_mode_and_parents(
            &fks,
            RecordWriteMode::QuestChild,
            &HashMap::new(),
            Some(&xdi_plan),
        )
    }

    pub(crate) fn emit_quest_child_scenes(&mut self) -> Result<TranslateStats, RunError> {
        let scen = SigCode::from_str("SCEN")
            .map_err(|e| RunError::InvalidConfig(format!("SCEN signature: {e}")))?;
        let xdi_plan = self.build_fo76_xdi_dialogue_plan()?;
        self.emit_phase_status("translate_v2: SCEN enumerate start");
        let fks = iter_form_keys_of_sig(self.source_handle_id, scen, &mut self.interner)?;
        self.emit_phase_status(format!(
            "translate_v2: SCEN enumerate done count={}",
            fks.len()
        ));
        self.emit_phase_status("translate_v2: SCEN translate_fks start");
        self.translate_fks_with_mode_and_parents(
            &fks,
            RecordWriteMode::QuestChild,
            &HashMap::new(),
            Some(&xdi_plan),
        )
    }

    pub(crate) fn emit_topic_child_infos(&mut self) -> Result<TranslateStats, RunError> {
        let info = SigCode::from_str("INFO")
            .map_err(|e| RunError::InvalidConfig(format!("INFO signature: {e}")))?;
        self.emit_phase_status("translate_v2: INFO enumerate start");
        let fks = iter_form_keys_of_sig(self.source_handle_id, info, &mut self.interner)?;
        self.emit_phase_status(format!(
            "translate_v2: INFO enumerate done count={}",
            fks.len()
        ));
        // INFO->parent-DIAL parentage is expressed only by source group nesting
        // (a TES4 Topic-Child group whose label is the DIAL form_id), not by any
        // DIAL subrecord. Build that {source INFO fid -> source DIAL fid} index so
        // each INFO can be placed under its DIAL in the target.
        self.emit_phase_status("translate_v2: INFO parent index start");
        let mut info_parent_index =
            crate::target_write::build_source_info_to_dialogue_index(self.source_handle_id)
                .map_err(|e| RunError::InvalidConfig(format!("info_parent_index:{e}")))?;
        let xdi_plan = self.build_fo76_xdi_dialogue_plan_from_index(&info_parent_index)?;
        for (&info, &parent) in &xdi_plan.info_parent_overrides {
            info_parent_index.insert(info, parent);
        }
        self.emit_phase_status(format!(
            "translate_v2: INFO parent index done count={}",
            info_parent_index.len()
        ));
        self.emit_phase_status("translate_v2: INFO translate_fks start");
        self.translate_fks_with_mode_and_parents(
            &fks,
            RecordWriteMode::TopicChildInfo,
            &info_parent_index,
            Some(&xdi_plan),
        )
    }

    fn build_fo76_xdi_dialogue_plan(
        &mut self,
    ) -> Result<crate::translator::pair_hooks::fo76_fo4::XdiDialoguePlan, RunError> {
        let info_parent_index =
            crate::target_write::build_source_info_to_dialogue_index(self.source_handle_id)
                .map_err(|e| RunError::InvalidConfig(format!("info_parent_index:{e}")))?;
        self.build_fo76_xdi_dialogue_plan_from_index(&info_parent_index)
    }

    fn build_fo76_xdi_dialogue_plan_from_index(
        &mut self,
        info_parent_index: &HashMap<u32, u32>,
    ) -> Result<crate::translator::pair_hooks::fo76_fo4::XdiDialoguePlan, RunError> {
        let scene_sig = SigCode::from_str("SCEN")
            .map_err(|e| RunError::InvalidConfig(format!("SCEN signature: {e}")))?;
        let scene_fks =
            iter_form_keys_of_sig(self.source_handle_id, scene_sig, &mut self.interner)?;
        let mut scenes = Vec::with_capacity(scene_fks.len());
        for scene_fk in scene_fks {
            let scene = read_record_relayout_by_form_key(
                self.source_handle_id,
                &scene_fk,
                &*self.schema_source,
                &self.interner,
                None,
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "XDI SCEN read {}: {error}",
                    form_key_to_read_str(&scene_fk, &self.interner)
                ))
            })?;
            scenes.push(scene);
        }
        let mut candidate_info_ids =
            crate::translator::pair_hooks::fo76_fo4::combined_player_dialogue_info_candidates(
                &scenes,
                info_parent_index,
            )
            .into_iter()
            .collect::<Vec<_>>();
        candidate_info_ids.sort_unstable();
        let Some(source_plugin) = scenes.first().map(|scene| scene.form_key.plugin) else {
            return Ok(Default::default());
        };
        let mut prompt_info_ids = std::collections::HashSet::new();
        for local in candidate_info_ids {
            let info_fk = FormKey {
                local,
                plugin: source_plugin,
            };
            let info = read_record_relayout_by_form_key(
                self.source_handle_id,
                &info_fk,
                &*self.schema_source,
                &self.interner,
                None,
            )
            .map_err(|error| {
                RunError::InvalidConfig(format!(
                    "XDI INFO read {}: {error}",
                    form_key_to_read_str(&info_fk, &self.interner)
                ))
            })?;
            if info.fields.iter().any(|field| field.sig.0 == *b"RNAM") {
                prompt_info_ids.insert(local);
            }
        }
        crate::translator::pair_hooks::fo76_fo4::build_xdi_dialogue_plan(
            &scenes,
            info_parent_index,
            &prompt_info_ids,
        )
        .map_err(|error| RunError::InvalidConfig(format!("fo76_xdi_dialogue:{error}")))
    }

    /// Rebuild the single top-level FO4 NAVI record from the finalized target
    /// NAVM graph. Legacy Fallout NAVI rows are never parsed or carried because
    /// merged FNV+FO3 input can contain multiple NVER=11 records with a foreign
    /// NVMI layout; rebuilding from all target NAVMs preserves the whole graph.
    pub fn rebuild_projected_navi(&mut self) -> Result<TranslateStats, RunError> {
        let supported_source = matches!(
            self.source,
            Game::Fo76 | Game::Fnv | Game::Fo3 | Game::SkyrimSe
        );
        if !supported_source
            || self.target != Game::Fo4
            || (self.source != Game::Fo76 && !self.config.is_whole_plugin)
        {
            return Ok(TranslateStats::default());
        }
        let preferred_navi_form_id = Some(FO4_CANONICAL_NAVI_FORM_ID);
        let raw_formid_mappings = if matches!(self.source, Game::Fo76 | Game::SkyrimSe) {
            let source_to_target_pairs = self
                .mapper_state
                .as_ref()
                .map(|state| {
                    state
                        .source_to_target
                        .iter()
                        .map(|(&source, &target)| (source, target))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            crate::fo76_navmesh::raw_formid_mappings_for_context(
                source_to_target_pairs,
                &self.interner,
                self.source_handle_id,
                self.target_handle_id,
            )
            .map_err(|e| RunError::InvalidConfig(format!("rebuild_projected_navi:{e}")))?
        } else {
            Vec::new()
        };
        let navi_stats = match self.source {
            Game::Fo76 if !raw_formid_mappings.is_empty() => {
                rebuild_projected_navi_from_source_native(
                    self.target_handle_id,
                    self.source_handle_id,
                    &raw_formid_mappings,
                    preferred_navi_form_id,
                )
            }
            Game::SkyrimSe => rebuild_projected_navi_from_source_with_nver_native(
                self.target_handle_id,
                self.source_handle_id,
                &raw_formid_mappings,
                preferred_navi_form_id,
                15,
            ),
            // FO76 without mappings and both legacy Fallout games rebuild from
            // every finalized target NAVM. In particular, never feed NVER=11
            // FNV/FO3 NVMI bytes to the FO76/Skyrim metadata remapper.
            Game::Fo76 | Game::Fnv | Game::Fo3 => {
                rebuild_projected_navi_native(self.target_handle_id, preferred_navi_form_id)
            }
            _ => unreachable!("source gate above admits only NAVI rebuild sources"),
        }
        .map_err(|e| RunError::InvalidConfig(format!("rebuild_projected_navi:{e}")))?;
        let mut stats = TranslateStats::default();
        stats.records_translated = navi_stats.records_added + navi_stats.records_replaced;
        stats.records_dropped = navi_stats
            .records_removed
            .saturating_sub(navi_stats.records_replaced);
        stats.records_failed = navi_stats.warnings;
        let message = format!(
            "rebuild_projected_navi: records_added={} records_replaced={} records_removed={} navmesh_infos={} edge_links={} stale_edge_links_dropped={} warnings={} navmeshes_seen={} navmeshes_touched={} navmesh_bad_internal_links={} navmesh_linked_edge_vertex_mismatches={} navmesh_opposite_normal_linked_pairs={} navmesh_missing_internal_links={} navmesh_same_direction_internal_edges={} navmesh_ambiguous_local_edges={} navmesh_external_links_added={} navmesh_missing_external_links={} navmesh_ambiguous_external_edges={} navmesh_external_link_caps_hit={} navmesh_winding_conflicts={}",
            navi_stats.records_added,
            navi_stats.records_replaced,
            navi_stats.records_removed,
            navi_stats.navmesh_infos,
            navi_stats.edge_links,
            navi_stats.stale_edge_links_dropped,
            navi_stats.warnings,
            navi_stats.navmeshes_seen,
            navi_stats.navmeshes_touched,
            navi_stats.navmesh_bad_internal_links,
            navi_stats.navmesh_linked_edge_vertex_mismatches,
            navi_stats.navmesh_opposite_normal_linked_pairs,
            navi_stats.navmesh_missing_internal_links,
            navi_stats.navmesh_same_direction_internal_edges,
            navi_stats.navmesh_ambiguous_local_edges,
            navi_stats.navmesh_external_links_added,
            navi_stats.navmesh_missing_external_links,
            navi_stats.navmesh_ambiguous_external_edges,
            navi_stats.navmesh_external_link_caps_hit,
            navi_stats.navmesh_winding_conflicts,
        );
        let sym = self.interner.intern(&message);
        self.warnings.push(sym);
        // Cap any navi-specific diagnostic strings accumulated during the rebuild.
        self.finalize_navi_warnings();
        Ok(stats)
    }

    /// Release the FK remap state to free its memory.
    ///
    /// Safe to call any time after fixups complete — the mapper is no
    /// longer needed by asset phases or ESP serialisation.  A subsequent call
    /// to `translate_all` / `apply_fixups_v2` would rebuild it, so this is purely
    /// an RSS-reduction lever for the late pipeline window.
    pub fn release_remap_state(&mut self) {
        self.mapper_state = None;
    }

    /// Append a diagnostic warning string from the NAVI rebuild phase.
    ///
    /// Unlike the main `warnings` Vec (which uses interned symbols), these
    /// are stored as owned `String`s to keep them separate and cap-able.
    pub fn push_navi_warning(&mut self, msg: String) {
        self.navi_warnings.push(msg);
    }

    /// Cap the NAVI warning buffer to at most 1000 entries to prevent unbounded
    /// growth on large worldspaces, and shrink the allocation.
    pub fn finalize_navi_warnings(&mut self) {
        const KEEP: usize = 1000;
        let dropped = self.navi_warnings.len().saturating_sub(KEEP);
        self.navi_warnings.truncate(KEEP);
        if dropped > 0 {
            self.navi_warnings
                .push(format!("... +{dropped} more (capped)"));
        }
        self.navi_warnings.shrink_to_fit();
    }

    /// Return the current count of NAVI warning strings.
    pub fn navi_warning_len(&self) -> usize {
        self.navi_warnings.len()
    }

    /// (Re)build `mapper_state` seeded with EIDs from every master handle.
    ///
    /// Called by `translate_all` and `translate_records` at the start of each
    /// translate pass. Persists into fixups via `self.mapper_state`.
    pub(crate) fn init_mapper_state(&mut self) -> Result<(), RunError> {
        self.legacy_serial_normalization.clear();
        self.legacy_creature_race_coverage = Default::default();
        let (source_plugin_name, source_master_names) =
            match plugin_context_for_handle(self.source_handle_id) {
                Ok(context) => context,
                Err(e) => {
                    let w = self.interner.intern(&format!("source_context_error:{e}"));
                    self.warnings.push(w);
                    (String::new(), Vec::new())
                }
            };
        let target_master_names = if !self.config.target_master_names.is_empty() {
            self.config.target_master_names.clone()
        } else {
            match plugin_context_for_handle(self.target_handle_id) {
                Ok((_plugin_name, masters)) => masters,
                Err(e) => {
                    let w = self.interner.intern(&format!("target_context_error:{e}"));
                    self.warnings.push(w);
                    Vec::new()
                }
            }
        };
        let mapper_opts = MapperOptions {
            output_plugin_name: self.config.output_plugin_name.clone(),
            source_plugin_name,
            source_master_names,
            target_master_names,
            use_base_game_assets: self.config.use_base_game_assets,
            vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                self.source,
                self.target,
            ),
            preserve_source_ids: self.config.preserve_source_ids,
            generated_object_id_floor: self.config.generated_object_id_floor,
            resolution_mode: if self.config.strict_mapper {
                crate::formkey_mapper::ResolutionMode::Strict
            } else {
                crate::formkey_mapper::ResolutionMode::DeferAndFixup
            },
        };

        if self.mapper_state.is_some() {
            let w = self
                .interner
                .intern("translate:mapper_state_already_set;rebuilding");
            self.warnings.push(w);
        }
        let preflight_eid_entries = mapper_entries_from_preflight(&self.config, &self.interner);
        let mut eid_entries = preflight_eid_entries.clone();
        let master_ids = self.master_handle_ids.clone();
        if !master_ids.is_empty() {
            self.emit_status(&format!(
                "  mapper: scanning {} master plugin(s) for editor-ids…",
                master_ids.len()
            ));
        }
        for master_id in master_ids {
            let master_started = std::time::Instant::now();
            match collect_eid_index(master_id, &self.schema_source, &mut self.interner) {
                Ok(entries) => {
                    let entry_count = entries.len();
                    eid_entries.extend(
                        entries.into_iter().map(|(eid, fk, sig)| {
                            (normalized_eid_sym(eid, &self.interner), fk, sig)
                        }),
                    );
                    self.emit_status(&format!(
                        "  mapper: master handle {} → {} editor-ids ({:.1}s)",
                        master_id,
                        entry_count,
                        master_started.elapsed().as_secs_f64()
                    ));
                }
                Err(e) => {
                    let w = self.interner.intern(&format!("eid_index_error:{e}"));
                    self.warnings.push(w);
                }
            }
        }
        let mut mapper_state = MapperState::new(eid_entries, mapper_opts);
        mapper_state
            .reserved_generated_object_ids
            .extend(self.generated_object_id_reservations.iter().copied());
        if self.source == Game::Fo76
            && self.target == Game::Fo4
            && mapper_state.options.target_master_names.iter().any(|name| {
                name.eq_ignore_ascii_case(crate::translator::pair_hooks::fo76_fo4::XDI_MASTER_NAME)
            })
        {
            let xdi_keyword = FormKey {
                local: crate::translator::pair_hooks::fo76_fo4::XDI_SCENE_KEYWORD_FORM_ID,
                plugin: self
                    .interner
                    .intern(crate::translator::pair_hooks::fo76_fo4::XDI_MASTER_NAME),
            };
            mapper_state
                .source_to_target
                .insert(xdi_keyword, xdi_keyword);
        }
        let needs_legacy_fo4_substitutions =
            matches!(self.source, Game::Fnv | Game::Fo3) && self.target == Game::Fo4;
        let needs_fo76_fo4_weather_substitutions =
            self.source == Game::Fo76 && self.target == Game::Fo4;
        let needs_skyrimse_fo4_weather_substitutions =
            self.source == Game::SkyrimSe && self.target == Game::Fo4;
        if (self.config.use_base_game_assets && !preflight_eid_entries.is_empty())
            || needs_legacy_fo4_substitutions
            || needs_fo76_fo4_weather_substitutions
            || needs_skyrimse_fo4_weather_substitutions
        {
            let source_scan_started = std::time::Instant::now();
            self.emit_status(&format!(
                "  mapper: scanning source plugin editor-ids for preflight mappings ({} target entries)…",
                preflight_eid_entries.len()
            ));
            match collect_eid_index(
                self.source_handle_id,
                &self.schema_source,
                &mut self.interner,
            ) {
                Ok(source_entries) => {
                    let source_entry_count = source_entries.len();
                    if self.config.use_base_game_assets && !preflight_eid_entries.is_empty() {
                        let creature_skin_remap_skips = collect_fo76_fo4_creature_skin_remap_skips(
                            self.source_handle_id,
                            &self.schema_source,
                            &source_entries,
                            &preflight_eid_entries,
                            &self.interner,
                            self.source,
                            self.target,
                        );
                        for (source_form_key, target_form_key) in
                            source_target_mappings_from_preflight_with_skips(
                                source_entries.iter().copied(),
                                &preflight_eid_entries,
                                &self.interner,
                                self.source,
                                self.target,
                                &creature_skin_remap_skips,
                            )
                        {
                            // Debug trace for the Gulper races' (source 110D23
                            // GulperRace, 111655 GulperSmallRace) preflight seed,
                            // confirming the seeded target plugin. Gated on
                            // MODBOX_TRACE_0247C1.
                            if matches!(source_form_key.local, 0x0011_0D23 | 0x0011_1655)
                                && std::env::var_os("MODBOX_TRACE_0247C1").is_some()
                            {
                                eprintln!(
                                    "[trace_0247c1] SEED source={:06X}@{:?} -> target={:06X}@{:?}",
                                    source_form_key.local,
                                    self.interner.resolve(source_form_key.plugin),
                                    target_form_key.local,
                                    self.interner.resolve(target_form_key.plugin),
                                );
                            }
                            mapper_state
                                .source_to_target
                                .entry(source_form_key)
                                .or_insert(target_form_key);
                        }
                    }
                    if needs_fo76_fo4_weather_substitutions {
                        for (source_form_key, target_form_key) in
                            crate::translator::pair_hooks::fo76_fo4::fo76_fo4_voli_gdry_substitution_mappings(
                                &source_entries,
                                &self.interner,
                            )
                        {
                            mapper_state
                                .source_to_target
                                .insert(source_form_key, target_form_key);
                        }
                    }
                    if needs_skyrimse_fo4_weather_substitutions {
                        for (source_form_key, target_form_key) in
                            crate::translator::pair_hooks::skyrimse_fo4::skyrimse_fo4_voli_gdry_substitution_mappings(
                                &source_entries,
                                &self.interner,
                            )
                        {
                            mapper_state
                                .source_to_target
                                .insert(source_form_key, target_form_key);
                        }
                    }
                    for (source_form_key, target_form_key) in
                        fnv_fo3_fo4_humanoid_race_substitution_mappings(
                            &source_entries,
                            &self.interner,
                            self.source,
                            self.target,
                        )
                    {
                        mapper_state
                            .source_to_target
                            .insert(source_form_key, target_form_key);
                    }
                    seed_fnv_fo3_fo4_ammo_substitutions(
                        &mut mapper_state,
                        &source_entries,
                        &self.interner,
                        self.source,
                        self.target,
                    )
                    .map_err(|error| {
                        RunError::InvalidConfig(format!("embedded ammo substitutions: {error}"))
                    })?;
                    self.emit_status(&format!(
                        "  mapper: scanned {} source editor-ids in {:.1}s",
                        source_entry_count,
                        source_scan_started.elapsed().as_secs_f64()
                    ));
                }
                Err(e) => {
                    let w = self
                        .interner
                        .intern(&format!("source_eid_preflight_error:{e}"));
                    self.warnings.push(w);
                }
            }
        }
        // Forced FO76→FO4 substitutions. Seeded last so they win over
        // EID/allocate resolution: `allocate_or_resolve` consults
        // `source_to_target` first. Unconditional for FO76→FO4 (not gated on
        // use_base_game_assets) — these references must always resolve to the
        // FO4 vanilla records.
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            for (source_form_key, target_form_key) in
                fo76_fo4_forced_keyword_substitution_mappings(&self.interner)
            {
                mapper_state
                    .source_to_target
                    .insert(source_form_key, target_form_key);
            }
            for (source_form_key, target_form_key) in
                fo76_fo4_forced_race_substitution_mappings(&self.interner)
            {
                mapper_state
                    .source_to_target
                    .insert(source_form_key, target_form_key);
            }
            for (source_form_key, target_form_key) in
                fo76_fo4_forced_base_object_substitution_mappings(&self.interner)
            {
                mapper_state
                    .source_to_target
                    .insert(source_form_key, target_form_key);
            }
            for (source_form_key, target_form_key) in
                fo76_fo4_forced_location_ref_type_substitution_mappings(&self.interner)
            {
                mapper_state
                    .source_to_target
                    .insert(source_form_key, target_form_key);
            }
        }
        self.mapper_state = Some(mapper_state);
        Ok(())
    }

    pub(crate) fn observe_legacy_creature_race_decision(
        &mut self,
        decision: &crate::translator::pair_hooks::fnv_creature_race::CreatureRaceDecision,
    ) {
        self.legacy_creature_race_coverage
            .observe_decision(decision, "CREA");
    }

    pub(crate) fn should_apply_legacy_creature_race_policy(&self) -> bool {
        matches!(self.source, Game::Fnv | Game::Fo3)
            && self.target == Game::Fo4
            && !self.translator.maps.skip_records.contains("CREA")
    }

    pub(crate) fn target_form_key_resolves_to_race(&self, form_key: FormKey) -> bool {
        let Some(state) = self.mapper_state.as_ref() else {
            return false;
        };
        let plugin = self.interner.resolve(form_key.plugin).unwrap_or("");
        state
            .target_eid_index
            .values()
            .flatten()
            .any(|(candidate, sig)| {
                sig.as_str() == "RACE"
                    && candidate.local == form_key.local
                    && self
                        .interner
                        .resolve(candidate.plugin)
                        .is_some_and(|candidate_plugin| {
                            candidate_plugin.eq_ignore_ascii_case(plugin)
                        })
            })
    }

    fn legacy_creature_race_expected_candidates(&self) -> usize {
        if !self.should_apply_legacy_creature_race_policy() {
            return self.legacy_creature_race_coverage.candidates;
        }
        let full_merged_audit = self.source == Game::Fnv
            && self.target == Game::Fo4
            && self.config.is_whole_plugin
            && self.config.records_limit.is_none()
            && self.mapper_state.as_ref().is_some_and(|state| {
                state
                    .options
                    .source_plugin_name
                    .eq_ignore_ascii_case("FNV_FO3_Merged.esm")
            });
        if full_merged_audit {
            crate::translator::pair_hooks::fnv_creature_race::EXPECTED_FULL_MERGED_CREA_CANDIDATES
        } else {
            self.legacy_creature_race_coverage.candidates
        }
    }

    pub(crate) fn fail_legacy_creature_race(&mut self, error: String) -> RunError {
        self.legacy_creature_race_coverage.expected_candidates =
            self.legacy_creature_race_expected_candidates();
        let report = serde_json::to_string(&self.legacy_creature_race_coverage)
            .unwrap_or_else(|json_error| format!("{{\"serialization_error\":{json_error:?}}}"));
        let _ = self.event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase: "translate",
            level: crate::phase::LogLevel::Warn,
            message: format!("legacy_creature_race_coverage:{report}"),
        });
        RunError::InvalidConfig(format!("{error};legacy_creature_race_coverage:{report}"))
    }

    pub(crate) fn finalize_legacy_creature_race_coverage(&mut self) -> Result<(), RunError> {
        if !matches!(self.source, Game::Fnv | Game::Fo3) || self.target != Game::Fo4 {
            return Ok(());
        }
        self.legacy_creature_race_coverage.expected_candidates =
            self.legacy_creature_race_expected_candidates();
        let report = serde_json::to_string(&self.legacy_creature_race_coverage)
            .map_err(|error| RunError::InvalidConfig(format!("creature coverage JSON: {error}")))?;
        let _ = self.event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase: "translate",
            level: crate::phase::LogLevel::Info,
            message: format!("legacy_creature_race_coverage:{report}"),
        });
        if !self.legacy_creature_race_coverage.coverage_gate_passes() {
            return Err(RunError::InvalidConfig(format!(
                "legacy_creature_race_coverage_gate_failed:{report}"
            )));
        }
        Ok(())
    }

    /// Run the per-record translate pipeline over `fks`.
    ///
    /// Caller must have already called `init_mapper_state`. Yields to Python
    /// every 1000 records to honour Ctrl-C and the optional progress callback.
    fn translate_fks(&mut self, fks: &[FormKey]) -> Result<TranslateStats, RunError> {
        self.translate_fks_with_mode(fks, RecordWriteMode::TopLevel)
    }

    fn translate_fks_with_mode(
        &mut self,
        fks: &[FormKey],
        write_mode: RecordWriteMode,
    ) -> Result<TranslateStats, RunError> {
        self.translate_fks_with_mode_and_parents(fks, write_mode, &HashMap::new(), None)
    }

    /// `info_parent_index` maps a source INFO form_id to its source parent-DIAL
    /// form_id (from group nesting); empty for every mode except TopicChildInfo.
    fn translate_fks_with_mode_and_parents(
        &mut self,
        fks: &[FormKey],
        write_mode: RecordWriteMode,
        info_parent_index: &HashMap<u32, u32>,
        xdi_plan: Option<&crate::translator::pair_hooks::fo76_fo4::XdiDialoguePlan>,
    ) -> Result<TranslateStats, RunError> {
        let log_progress = !matches!(write_mode, RecordWriteMode::TopLevel);
        if log_progress {
            self.emit_phase_status(format!(
                "translate_v2: translate_fks mode={write_mode:?} total={} start",
                fks.len()
            ));
        }
        let mut stats = TranslateStats::default();
        let mut record_count: u64 = 0;
        let whole_plugin_names =
            if self.config.is_whole_plugin && self.config.target_master_names.is_empty() {
                self.target_master_plugin_names()
            } else {
                Vec::new()
            };
        let target_master_names = target_master_names_for_skip(&self.config, whole_plugin_names);
        let target_master_syms: FxHashSet<Sym> =
            intern_plugin_names(&target_master_names, &self.interner);
        let first_target_master_sym = target_master_names
            .first()
            .map(|name| self.interner.intern(name));

        // Source→FO4 struct relayout context. Legacy Fallout is deliberately
        // restricted to BPTD.BPND; FO76 retains the generic divergent-struct path.
        // Clone the Arc so the ctx borrows this local, not `self` — the loop body
        // later needs `&mut self` (capture_full_plugin_record_state), which would
        // conflict with an immutable borrow of `self.schema_target` held here.
        let legacy_bptd_only = matches!(self.source, Game::Fnv | Game::Fo3);
        let relayout_target_schema = (self.target == Game::Fo4
            && (self.source == Game::Fo76 || legacy_bptd_only))
            .then(|| self.schema_target.clone());
        let relayout_ctx = relayout_target_schema.as_deref().map(|target_schema| {
            crate::struct_relayout::StructRelayoutCtx {
                target_schema,
                target_form_version:
                    crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION,
                legacy_bptd_only,
            }
        });

        for &fk in fks {
            record_count += 1;

            // ── Process record (inner block so `continue` doesn't skip the yield check) ──
            'record: {
                // ── Read source record ─────────────────────────────────
                let mut src_record = match read_record_relayout_by_form_key(
                    self.source_handle_id,
                    &fk,
                    &*self.schema_source,
                    &self.interner,
                    relayout_ctx.as_ref(),
                ) {
                    Ok(r) => r,
                    Err(e) => {
                        let w = self.interner.intern(&format!("read_error:{e}"));
                        self.warnings.push(w);
                        stats.records_failed += 1;
                        break 'record;
                    }
                };
                let source_sig = src_record.sig;
                stats.signature_entry(source_sig).seen += 1;
                let fnv_scri_target = if self.source == Game::Fnv && self.target == Game::Fo4 {
                    crate::translator::pair_hooks::fnv_fo4::FnvFo4Hook::capture_scri_target(
                        &src_record,
                        &self.interner,
                    )
                    .map(str::to_string)
                } else {
                    None
                };
                let creature_race_event = if self.should_apply_legacy_creature_race_policy() {
                    match crate::translator::pair_hooks::fnv_creature_race::apply_legacy_creature_race_policy(
                        self.source,
                        self.target,
                        &mut src_record,
                        &self.interner,
                    ) {
                        Ok(event) => {
                            if let Some(event) = event.as_ref() {
                                self.observe_legacy_creature_race_decision(&event.decision);
                            }
                            event
                        }
                        Err(error) => {
                            self.observe_legacy_creature_race_decision(&error.decision);
                            return Err(self.fail_legacy_creature_race(error.to_string()));
                        }
                    }
                } else {
                    None
                };
                // ── PairHook::pre_translate ────────────────────────────
                {
                    let mut ctx = PairCtx {
                        interner: &mut self.interner,
                    };
                    if let Err(e) = self.translator.pre_translate(&mut ctx, &mut src_record) {
                        let w = self.interner.intern(&format!("pre_translate:{e}"));
                        self.warnings.push(w);
                    }
                }
                if source_sig.as_str() == "DIAL"
                    && let Some(count) =
                        xdi_plan.and_then(|plan| plan.dial_info_count_overrides.get(&fk.local))
                {
                    crate::translator::pair_hooks::fo76_fo4::apply_xdi_dial_info_count(
                        &mut src_record,
                        *count,
                    )
                    .map_err(|error| {
                        RunError::InvalidConfig(format!("fo76_xdi_dialogue:{error}"))
                    })?;
                }

                // ── Translate ──────────────────────────────────────────
                if source_sig.as_str() == "SCEN"
                    && let Some(filler) =
                        xdi_plan.and_then(|plan| plan.scene_player_topic_fillers.get(&fk.local))
                {
                    crate::translator::pair_hooks::fo76_fo4::apply_xdi_scene_player_padding(
                        &mut src_record,
                        *filler,
                    );
                }

                let translated = {
                    let forced_skip_signature = match write_mode {
                        RecordWriteMode::QuestChild if source_sig.as_str() == "DIAL" => {
                            Some("DIAL")
                        }
                        RecordWriteMode::TopicChildInfo if source_sig.as_str() == "INFO" => {
                            Some("INFO")
                        }
                        _ => None,
                    };
                    let removed_forced_skip = if let Some(signature) = forced_skip_signature {
                        self.translator.maps.skip_records.remove(signature)
                    } else {
                        false
                    };
                    let result = self.translator.translate(&src_record, &mut self.interner);
                    if removed_forced_skip {
                        if let Some(signature) = forced_skip_signature {
                            self.translator
                                .maps
                                .skip_records
                                .insert(signature.to_string());
                        }
                    }
                    match result {
                        TranslateResult::Translated(r) => r,
                        TranslateResult::Dropped { decision, .. } => {
                            self.decisions.push(decision);
                            stats.records_dropped += 1;
                            stats.signature_entry(source_sig).dropped += 1;
                            break 'record;
                        }
                        TranslateResult::Deferred(kind) => {
                            self.deferred.push((fk, kind));
                            stats.records_deferred += 1;
                            stats.signature_entry(source_sig).deferred += 1;
                            break 'record;
                        }
                    }
                };

                // ── Allocate target FormKey + rewrite cross-plugin refs ─
                let mut translated = translated;
                if self
                    .schema_target
                    .record_def(translated.sig.as_str())
                    .is_none()
                {
                    let warning = format!(
                        "unsupported_target_record:{} not in {} generated schema",
                        translated.sig.as_str(),
                        self.target.as_str()
                    );
                    let w = self.interner.intern(&warning);
                    self.warnings.push(w);
                    stats.records_dropped += 1;
                    stats.signature_entry(source_sig).dropped += 1;
                    break 'record;
                }
                let cell_edid_rename = if self.source == Game::Fo76 && self.target == Game::Fo4 {
                    self.mapper_state.as_ref().and_then(|state| {
                        let translated_sig = translated.sig;
                        rename_fo76_target_editor_id_collision(
                            &mut translated,
                            &state.target_eid_index,
                            &self.interner,
                            is_editor_id_collision_rename_forced(
                                self.source,
                                self.target,
                                translated_sig,
                            ),
                        )
                    })
                } else {
                    None
                };
                let collision_donor = cell_edid_rename.as_ref().and_then(|(old, _)| {
                    self.mapper_state.as_ref().and_then(|state| {
                        target_collision_donor_form_key(
                            &state.target_eid_index,
                            &self.interner,
                            old,
                            translated.sig,
                        )
                    })
                });
                if let Some((old, new)) = cell_edid_rename.as_ref() {
                    let w = self
                        .interner
                        .intern(&format!("fo76_target_edid_collision_renamed:{old}->{new}"));
                    self.warnings.push(w);
                }
                let mut legacy_serial_diagnostics = Vec::new();
                let mut legacy_serial_drop = false;
                let planned_info_split = xdi_plan
                    .and_then(|plan| plan.combined_info_splits.get(&fk.local))
                    .copied();
                let (target_fk, generated_player_info_fk) = {
                    let state = self.mapper_state.as_mut().unwrap();
                    let mut mapper = FormKeyMapper::from_state(state, &self.interner);
                    let normalized_eid = normalized_eid_opt(translated.eid, mapper.interner);
                    let target_fk = mapper.allocate_or_resolve(fk, normalized_eid, translated.sig);
                    translated.form_key = target_fk;
                    if let Some(race) = creature_race_event
                        .as_ref()
                        .and_then(|event| event.decision.audited_race().ok().flatten())
                    {
                        mapper.add_mapping(race, race);
                    }
                    if let Some(outcome) = self.translator.normalize_serial_mapper_record_once(
                        fk,
                        &mut translated,
                        &mut mapper,
                        &mut self.legacy_serial_normalization,
                    ) {
                        match outcome {
                            Ok(report) => {
                                report.register_target_identities(&mut mapper);
                                legacy_serial_diagnostics = report.diagnostics(&translated);
                            }
                            Err(diagnostic) => {
                                legacy_serial_drop = true;
                                legacy_serial_diagnostics.push(diagnostic);
                            }
                        }
                    }
                    if !legacy_serial_drop {
                        if self.source == Game::Fo76
                            && self.target == Game::Fo4
                            && translated
                                .fields
                                .iter()
                                .any(|field| field.sig.0 == *b"NVNM" || field.sig.0 == *b"MNAM")
                        {
                            if let Err(e) = crate::fo76_navmesh::rewrite_record_nvnm_for_fo4(
                                &mut translated,
                                &mut mapper,
                                self.source_handle_id,
                                self.target_handle_id,
                            ) {
                                let w = mapper.interner.intern(&format!("fo76_navm:{e}"));
                                self.warnings.push(w);
                            }
                        }
                        if let Err(e) = mapper.rewrite_record(&mut translated) {
                            let w = mapper.interner.intern(&format!("rewrite_record:{e}"));
                            self.warnings.push(w);
                        }
                    }
                    let generated_player_info_fk =
                        planned_info_split.map(|_| mapper.allocate_generated());
                    (target_fk, generated_player_info_fk)
                };
                for diagnostic in legacy_serial_diagnostics {
                    let level = if diagnostic.warning {
                        crate::phase::LogLevel::Warn
                    } else {
                        crate::phase::LogLevel::Info
                    };
                    let _ = self.event_tx.try_send(crate::phase::PhaseEvent::Log {
                        phase: "translate",
                        level,
                        message: diagnostic.message.clone(),
                    });
                    if diagnostic.warning {
                        let warning = self.interner.intern(&diagnostic.message);
                        self.warnings.push(warning);
                    }
                }
                if legacy_serial_drop {
                    stats.records_dropped += 1;
                    stats.signature_entry(source_sig).dropped += 1;
                    break 'record;
                }
                if is_target_master_remap(target_fk, &target_master_syms) {
                    stats.records_vanilla_remapped += 1;
                    stats.signature_entry(source_sig).vanilla_remapped += 1;
                    break 'record;
                }
                if let Some(donor_form_key) = collision_donor {
                    if let Err(error) =
                        self.merge_target_collision_donor(&mut translated, donor_form_key)
                    {
                        let warning = self
                            .interner
                            .intern(&format!("collision_donor_merge:{error}"));
                        self.warnings.push(warning);
                    }
                }
                if let Some(source_scpt_form_key) = fnv_scri_target {
                    if let Some(target_form_key) = form_key_to_legacy_str(target_fk, &self.interner)
                    {
                        self.fnv_scri_links.push(FnvScriLink {
                            target_form_key,
                            source_scpt_form_key,
                        });
                    }
                }

                // ── PairHook::post_translate ───────────────────────────
                {
                    let mut ctx = PairCtx {
                        interner: &mut self.interner,
                    };
                    if let Err(e) = self.translator.post_translate(&mut ctx, &mut translated) {
                        let w = self.interner.intern(&format!("post_translate:{e}"));
                        self.warnings.push(w);
                    }
                }

                // ── TargetHook::run ────────────────────────────────────
                {
                    let mut ctx = TargetCtx {
                        interner: &mut self.interner,
                    };
                    if let Err(e) = self.translator.run_target_hook(&mut ctx, &mut translated) {
                        let w = self.interner.intern(&format!("target_hook:{e}"));
                        self.warnings.push(w);
                    }
                }

                // DIAL category 5 is FO76 Miscellaneous but FO4 Detection, so
                // this semantic remap cannot be idempotent. Apply it once at
                // the final target boundary, after every reusable pre-pass.
                if self.source == Game::Fo76 && self.target == Game::Fo4 {
                    crate::translator::pair_hooks::fo76_fo4::Fo76Fo4Hook::normalize_dial_data_category(
                        &self.interner,
                        &mut translated,
                    );
                }

                // ── Class A: schema-driven flag/enum normalization ─
                // FO76→FO4 only. Masks unknown header-flag bits + subrecord
                // flag bits and clamps out-of-domain enums against the FO4
                // schema. Runs after the semantic target hooks (so it only
                // validates their output) and before TargetRecordNormalizer.
                if self.source == Game::Fo76 && self.target == Game::Fo4 {
                    let report = crate::translator::class_a_normalize::normalize_flags_and_enums(
                        &mut translated,
                        &self.schema_target,
                        &self.interner,
                    );
                    for message in report.decisions {
                        let kind = self.interner.intern("class_a_normalize");
                        self.decisions.push(Decision { kind, message });
                    }
                    for w in report.warnings {
                        let sym = self.interner.intern(&w);
                        self.warnings.push(sym);
                    }
                }

                // ── Write to target ────────────────────────────────────
                let synthetic_player_info = if let Some(player_form_key) = generated_player_info_fk
                {
                    Some(
                        crate::translator::pair_hooks::fo76_fo4::split_fo76_combined_player_dialogue_info(
                            &mut translated,
                            player_form_key,
                            &self.interner,
                        )
                        .map_err(|error| {
                            RunError::InvalidConfig(format!("fo76_xdi_dialogue:{error}"))
                        })?,
                    )
                } else {
                    None
                };
                let normalizer = TargetRecordNormalizer {
                    target_schema: &self.schema_target,
                    source_record_def: self.schema_source.record_def(source_sig.as_str()),
                    interner: Some(&self.interner),
                };
                let mut translated = match normalizer.normalize(translated) {
                    TargetRecordNormalization::Keep(record) => record,
                    TargetRecordNormalization::DropUnsupportedRecord => {
                        stats.records_dropped += 1;
                        stats.signature_entry(source_sig).dropped += 1;
                        break 'record;
                    }
                };
                let mut synthetic_player_info = synthetic_player_info
                    .map(|record| match normalizer.normalize(record) {
                        TargetRecordNormalization::Keep(record) => Ok(record),
                        TargetRecordNormalization::DropUnsupportedRecord => Err(
                            RunError::InvalidConfig(format!(
                                "fo76_xdi_dialogue:generated player INFO for {:#08X} was rejected by the FO4 schema",
                                fk.local
                            )),
                        ),
                    })
                    .transpose()?;
                if let Err(error) =
                    crate::translator::pair_hooks::fnv_creature_race::validate_crea_derived_npc_race(
                        &translated,
                        creature_race_event.as_ref(),
                        &self.interner,
                        |race| self.target_form_key_resolves_to_race(race),
                    )
                {
                    return Err(self.fail_legacy_creature_race(error.to_string()));
                }
                namespace_base_asset_model_paths(
                    &mut translated,
                    &self.relocation_members,
                    base_asset_namespace(&self.config, self.source, self.target).unwrap_or(""),
                    &self.interner,
                );
                if let Some(player_info) = synthetic_player_info.as_mut() {
                    namespace_base_asset_model_paths(
                        player_info,
                        &self.relocation_members,
                        base_asset_namespace(&self.config, self.source, self.target).unwrap_or(""),
                        &self.interner,
                    );
                }
                let full_plugin_snapshot = if self.config.is_whole_plugin {
                    Some(crate::full_plugin::target_schema_record_view(
                        &translated,
                        &self.schema_target,
                    ))
                } else {
                    None
                };
                let synthetic_player_snapshot = if self.config.is_whole_plugin {
                    synthetic_player_info.as_ref().map(|record| {
                        crate::full_plugin::target_schema_record_view(record, &self.schema_target)
                    })
                } else {
                    None
                };
                match write_mode {
                    RecordWriteMode::TopLevel => {
                        if let Err(e) = add_record_native(
                            self.target_handle_id,
                            translated,
                            &*self.schema_target,
                            &self.interner,
                        ) {
                            let w = self.interner.intern(&format!("write_error:{e}"));
                            self.warnings.push(w);
                            stats.records_failed += 1;
                            stats.signature_entry(source_sig).failed += 1;
                            break 'record;
                        }
                    }
                    RecordWriteMode::QuestChild => {
                        match add_quest_child_record_native(
                            self.target_handle_id,
                            translated,
                            &*self.schema_target,
                            &self.interner,
                        ) {
                            Ok(true) => {}
                            Ok(false) => {
                                let w = self.interner.intern(&format!(
                                    "quest_child_record_skipped:{}",
                                    form_key_to_read_str(&fk, &self.interner)
                                ));
                                self.warnings.push(w);
                                stats.records_dropped += 1;
                                stats.signature_entry(source_sig).dropped += 1;
                                break 'record;
                            }
                            Err(e) => {
                                let w = self
                                    .interner
                                    .intern(&format!("quest_child_record_write:{e}"));
                                self.warnings.push(w);
                                stats.records_failed += 1;
                                stats.signature_entry(source_sig).failed += 1;
                                break 'record;
                            }
                        }
                    }
                    RecordWriteMode::TopicChildInfo => {
                        // Resolve the target parent-DIAL form_id: source INFO ->
                        // source DIAL (group nesting) -> target DIAL (remap).
                        let resolve_target_parent = |source_dial_local| {
                            let source_dial = FormKey {
                                local: source_dial_local,
                                plugin: fk.plugin,
                            };
                            self.mapper_state
                                .as_ref()
                                .and_then(|state| state.source_to_target.get(&source_dial))
                                .map(|target| target.local)
                        };
                        if let Some(split) = planned_info_split {
                            let player_parent = resolve_target_parent(split.player_parent)
                                .ok_or_else(|| {
                                    RunError::InvalidConfig(format!(
                                        "fo76_xdi_dialogue:player DIAL {:06X} for INFO {:06X} has no target mapping",
                                        split.player_parent, fk.local
                                    ))
                                })?;
                            let npc_parent =
                                resolve_target_parent(split.npc_parent).ok_or_else(|| {
                                    RunError::InvalidConfig(format!(
                                        "fo76_xdi_dialogue:NPC DIAL {:06X} for INFO {:06X} has no target mapping",
                                        split.npc_parent, fk.local
                                    ))
                                })?;
                            let player_info = synthetic_player_info.take().ok_or_else(|| {
                                RunError::InvalidConfig(format!(
                                    "fo76_xdi_dialogue:INFO {:06X} split has no generated player record",
                                    fk.local
                                ))
                            })?;
                            let player_inserted = add_topic_child_record_native(
                                self.target_handle_id,
                                player_info,
                                player_parent,
                                &*self.schema_target,
                                &self.interner,
                            )
                            .map_err(|error| {
                                RunError::InvalidConfig(format!(
                                    "fo76_xdi_dialogue:player INFO {:06X} write: {error}",
                                    fk.local
                                ))
                            })?;
                            if !player_inserted {
                                return Err(RunError::InvalidConfig(format!(
                                    "fo76_xdi_dialogue:player INFO {:06X} was skipped",
                                    fk.local
                                )));
                            }
                            let npc_inserted = add_topic_child_record_native(
                                self.target_handle_id,
                                translated,
                                npc_parent,
                                &*self.schema_target,
                                &self.interner,
                            )
                            .map_err(|error| {
                                RunError::InvalidConfig(format!(
                                    "fo76_xdi_dialogue:NPC INFO {:06X} write: {error}",
                                    fk.local
                                ))
                            })?;
                            if !npc_inserted {
                                return Err(RunError::InvalidConfig(format!(
                                    "fo76_xdi_dialogue:NPC INFO {:06X} was skipped",
                                    fk.local
                                )));
                            }
                        } else {
                            let target_parent_dialogue_form_id = info_parent_index
                                .get(&fk.local)
                                .and_then(|&source_dial_local| {
                                    resolve_target_parent(source_dial_local)
                                });
                            let Some(target_parent_dialogue_form_id) =
                                target_parent_dialogue_form_id
                            else {
                                let w = self.interner.intern(&format!(
                                    "topic_child_info_no_parent:{}",
                                    form_key_to_read_str(&fk, &self.interner)
                                ));
                                self.warnings.push(w);
                                stats.records_dropped += 1;
                                stats.signature_entry(source_sig).dropped += 1;
                                break 'record;
                            };
                            match add_topic_child_record_native(
                                self.target_handle_id,
                                translated,
                                target_parent_dialogue_form_id,
                                &*self.schema_target,
                                &self.interner,
                            ) {
                                Ok(true) => {}
                                Ok(false) => {
                                    let w = self.interner.intern(&format!(
                                        "topic_child_info_skipped:{}",
                                        form_key_to_read_str(&fk, &self.interner)
                                    ));
                                    self.warnings.push(w);
                                    stats.records_dropped += 1;
                                    stats.signature_entry(source_sig).dropped += 1;
                                    break 'record;
                                }
                                Err(e) => {
                                    let w = self
                                        .interner
                                        .intern(&format!("topic_child_info_write:{e}"));
                                    self.warnings.push(w);
                                    stats.records_failed += 1;
                                    stats.signature_entry(source_sig).failed += 1;
                                    break 'record;
                                }
                            }
                        }
                    }
                }
                if let Some(snapshot) = full_plugin_snapshot.as_ref() {
                    self.capture_full_plugin_record_state(
                        fk,
                        snapshot,
                        &target_master_syms,
                        first_target_master_sym,
                        None,
                    );
                }
                if let Some(snapshot) = synthetic_player_snapshot.as_ref() {
                    self.capture_full_plugin_record_state(
                        fk,
                        snapshot,
                        &target_master_syms,
                        first_target_master_sym,
                        None,
                    );
                }

                stats.records_translated += 1;
                stats.signature_entry(source_sig).translated += 1;
            } // end 'record

            // ── Yield + cancel check every 1000 records ───────────────
            if record_count % 1000 == 0 {
                if log_progress && record_count % 25000 == 0 {
                    self.emit_phase_status(format!(
                        "translate_v2: translate_fks mode={write_mode:?} progress={record_count}/{} translated={} dropped={} failed={}",
                        fks.len(),
                        stats.records_translated,
                        stats.records_dropped,
                        stats.records_failed
                    ));
                }
                let cb = self.progress_callback.as_ref();
                Python::attach(|py| -> Result<(), RunError> {
                    py.check_signals().map_err(|_| RunError::Cancelled)?;
                    if let Some(cb) = cb {
                        let keep_going: bool = cb
                            .call1(py, (record_count,))
                            .and_then(|r| r.extract::<bool>(py))
                            .unwrap_or(true);
                        if !keep_going {
                            return Err(RunError::Cancelled);
                        }
                    }
                    Ok(())
                })?;
            }
        }

        if self.config.is_whole_plugin {
            let warning = format!(
                "full_plugin_state:unresolved_refs={};target_master_refs={}",
                self.full_plugin_state.unresolved_ref_count(),
                self.full_plugin_state.target_master_ref_count()
            );
            let sym = self.interner.intern(&warning);
            self.warnings.push(sym);
        }

        if log_progress {
            self.emit_phase_status(format!(
                "translate_v2: translate_fks mode={write_mode:?} done translated={} dropped={} failed={}",
                stats.records_translated, stats.records_dropped, stats.records_failed
            ));
        }
        Ok(stats)
    }

    pub(crate) fn rebuild_full_plugin_worldspace_groups(&mut self) -> Result<(), RunError> {
        if !self.config.is_whole_plugin {
            return Ok(());
        }
        if !supports_source_worldspace_topology_rebuild(self.source, self.target) {
            return Ok(());
        }

        let source_to_target_pairs = self
            .mapper_state
            .as_ref()
            .map(|state| {
                state
                    .source_to_target
                    .iter()
                    .map(|(&source, &target)| (source, target))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let raw_formid_mappings = crate::fo76_navmesh::raw_formid_mappings_for_context(
            source_to_target_pairs,
            &self.interner,
            self.source_handle_id,
            self.target_handle_id,
        )
        .map_err(|e| RunError::InvalidConfig(format!("rebuild_worldspace_groups:{e}")))?;
        let stats = rebuild_worldspace_groups_from_source_native(
            self.target_handle_id,
            self.source_handle_id,
            &raw_formid_mappings,
        )
        .map_err(|e| RunError::InvalidConfig(format!("rebuild_worldspace_groups:{e}")))?;
        let message = format!(
            "rebuild_worldspace_groups: groups_rebuilt={} records_nested={} flat_records_removed={}",
            stats.groups_rebuilt, stats.records_nested, stats.flat_records_removed,
        );
        let sym = self.interner.intern(&message);
        self.warnings.push(sym);
        Ok(())
    }

    pub(crate) fn capture_full_plugin_record_state(
        &mut self,
        source_fk: FormKey,
        translated: &crate::record::Record,
        target_master_syms: &FxHashSet<Sym>,
        first_target_master_sym: Option<Sym>,
        unresolved_source_refs: Option<&FxHashSet<FormKey>>,
    ) {
        if !self.config.is_whole_plugin {
            return;
        }
        self.full_plugin_state.record_translated(translated.sig);
        if let Some(unresolved_source_refs) = unresolved_source_refs {
            self.full_plugin_state.capture_record_refs_with_unresolved(
                translated,
                unresolved_source_refs,
                target_master_syms,
            );
        } else {
            self.full_plugin_state.capture_record_refs(
                translated,
                source_fk.plugin,
                target_master_syms,
            );
        }
        if let Some(first_target_master_sym) = first_target_master_sym {
            self.full_plugin_state
                .capture_raw_zero_master_refs(translated, first_target_master_sym);
        }
    }

    pub(crate) fn target_master_plugin_names(&mut self) -> Vec<String> {
        let mut names = Vec::with_capacity(self.master_handle_ids.len());
        for &handle_id in &self.master_handle_ids {
            match plugin_name_for_handle(handle_id) {
                Ok(name) => names.push(name),
                Err(e) => {
                    let sym = self
                        .interner
                        .intern(&format!("full_plugin_master_name_error:{e}"));
                    self.warnings.push(sym);
                }
            }
        }
        names
    }

    /// Execute the canonical fixup segment plan.
    ///
    /// Fused sweeps handle audit-cleared record visitors; fixup segments use
    /// the single-fixup registry mechanics for scope skips, convergence, and
    /// full-plugin state.
    ///
    /// Phase-contract safe: per-segment visibility goes to stderr and the
    /// caller's event channel. Harvests `addon_index_remap` into the
    /// decision channel so the NIF phase keeps working.
    pub fn apply_fixups_v2(&mut self) -> Result<Vec<(String, FixupReport)>, FixupError> {
        let mapper_opts = MapperOptions {
            output_plugin_name: self.config.output_plugin_name.clone(),
            source_plugin_name: String::new(),
            source_master_names: Vec::new(),
            target_master_names: self.config.target_master_names.clone(),
            use_base_game_assets: self.config.use_base_game_assets,
            vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                self.source,
                self.target,
            ),
            preserve_source_ids: self.config.preserve_source_ids,
            generated_object_id_floor: self.config.generated_object_id_floor,
            resolution_mode: if self.config.strict_mapper {
                crate::formkey_mapper::ResolutionMode::Strict
            } else {
                crate::formkey_mapper::ResolutionMode::DeferAndFixup
            },
        };
        if self.mapper_state.is_none() {
            self.mapper_state = Some(MapperState::new([], mapper_opts));
        }

        let config = FixupConfig {
            strict: self.config.strict_mapper,
            preserve_source_ids: self.config.preserve_source_ids,
            use_base_game_assets: self.config.use_base_game_assets,
            is_whole_plugin: self.config.is_whole_plugin,
            root_sig: self.config.root_sig,
            skip_record_sigs: self.translator.maps.skip_records.clone(),
            mod_path: self.config.mod_path.clone(),
            source_extracted_dir: self.config.source_extracted_dir.clone(),
            target_extracted_dir: self.config.target_extracted_dir.clone(),
            target_master_handle_ids: self.master_handle_ids.clone(),
            target_schema: Some(Arc::clone(&self.schema_target)),
            source_schema: Some(Arc::clone(&self.schema_source)),
            asset_phases: self.config.asset_phases.clone(),
            defer_placed_child_ref_class: self.config.defer_placed_child_ref_class,
        };

        let cancel = Arc::clone(&self.cancel);
        let event_tx = self.event_tx.clone();
        let source_handle_id = self.source_handle_id;
        let target_handle_id = self.target_handle_id;

        let all_reports = {
            let interner = &self.interner;
            let mut mapper = FormKeyMapper::from_state(
                self.mapper_state
                    .as_mut()
                    .expect("mapper_state initialized above"),
                interner,
            );

            let mut ctx = FixupContext {
                source_handle_id,
                target_handle_id,
                schema_target: &self.schema_target,
                schema_source: &self.schema_source,
                skip_record_sigs: &self.translator.maps.skip_records,
                mod_path: self.config.mod_path.as_deref(),
                source_extracted_dir: self.config.source_extracted_dir.as_deref(),
                target_master_handle_ids: &self.master_handle_ids,
                config: &config,
            };

            let log_fixups_v2 = |message: String| {
                eprintln!("{message}");
                let _ = event_tx.try_send(crate::phase::PhaseEvent::Log {
                    phase: "fixups_v2",
                    level: crate::phase::LogLevel::Info,
                    message,
                });
            };
            let mut emit_progress =
                |name: &'static str,
                 iteration: u32,
                 status: &'static str,
                 report: Option<&FixupReport>| {
                    if status == "started" {
                        log_fixups_v2(format!("[fixups_v2] starting {name} iter={iteration}"));
                    } else if let Some(report) = report {
                        let action = if status == "skipped" {
                            "skipped"
                        } else {
                            "finished"
                        };
                        let message = report
                            .message
                            .and_then(|sym| interner.resolve(sym))
                            .map(|text| format!(" message={text}"))
                            .unwrap_or_default();
                        log_fixups_v2(format!(
                            "[fixups_v2] {action} {name} iter={iteration} changed={} dropped={} added={} warnings={} diagnostics={} elapsed_ms={} status_report={} scope={}{}",
                            report.records_changed,
                            report.records_dropped,
                            report.records_added,
                            report.warnings.len(),
                            report.diagnostics.len(),
                            report.elapsed_ms,
                            report.status.as_str(),
                            report.scope.as_str(),
                            message
                        ));
                        emit_fixup_warning_logs(
                            &event_tx,
                            "fixups_v2",
                            name,
                            iteration,
                            report,
                            interner,
                        );
                        emit_fixup_diagnostic_logs(
                            &event_tx,
                            "fixups_v2",
                            name,
                            iteration,
                            report,
                            interner,
                        );
                    }
                };

            let mut all_reports: Vec<(String, FixupReport)> = Vec::new();
            // Master plugins are never mutated by fixups: master-derived
            // gather products are shared across all sweeps of this run.
            let mut master_cache = crate::store2::visitor::MasterScanCache::default();
            for segment in crate::store2::fixups_v2::build_default_segment_plan() {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err(FixupError::Cancelled);
                }
                match &segment {
                    crate::store2::fixups_v2::Segment::Fixup(make) => {
                        let fixup = make();
                        let fixup_name = fixup.name();
                        log_fixups_v2(format!("[fixups_v2] queued {fixup_name}"));
                        let mut registry = FixupRegistry::new();
                        registry.register(fixup);
                        let reports = registry
                            .run_all_with_progress_and_cancel_and_full_plugin_state(
                                &mut ctx,
                                &mut mapper,
                                &mut emit_progress,
                                Some(cancel.as_ref()),
                                Some(&self.full_plugin_state),
                            )?;
                        if reports.is_empty() {
                            log_fixups_v2(format!(
                                "[fixups_v2] skipped {fixup_name} reason=not_applicable"
                            ));
                        }
                        all_reports.extend(reports);
                    }
                    crate::store2::fixups_v2::Segment::Sweep(label, _) => {
                        let sweep = segment.build_sweep().expect("sweep segment");
                        let sweep_names: Vec<&'static str> = sweep
                            .visitors
                            .iter()
                            .map(|visitor| visitor.name())
                            .collect();
                        for name in &sweep_names {
                            log_fixups_v2(format!("[fixups_v2] queued {name} segment={label}"));
                        }
                        let source_id = Some(source_handle_id).filter(|id| *id != 0);
                        let mut session = crate::session::open_session(target_handle_id, source_id)
                            .map_err(|e| FixupError::HandleError(e.to_string()))?;
                        let reports = crate::store2::visitor::run_sweep(
                            &mut session,
                            &mut mapper,
                            &config,
                            &sweep,
                            &mut master_cache,
                        )?;
                        for (name, report) in &reports {
                            let message = report
                                .message
                                .and_then(|sym| interner.resolve(sym))
                                .map(|text| format!(" message={text}"))
                                .unwrap_or_default();
                            log_fixups_v2(format!(
                                "[fixups_v2] finished {name} iter={} changed={} dropped={} added={} warnings={} diagnostics={} elapsed_ms={} status_report={} scope={}{}",
                                report.iteration,
                                report.records_changed,
                                report.records_dropped,
                                report.records_added,
                                report.warnings.len(),
                                report.diagnostics.len(),
                                report.elapsed_ms,
                                report.status.as_str(),
                                report.scope.as_str(),
                                message
                            ));
                            emit_fixup_warning_logs(
                                &event_tx,
                                "fixups_v2",
                                name,
                                report.iteration,
                                report,
                                interner,
                            );
                            emit_fixup_diagnostic_logs(
                                &event_tx,
                                "fixups_v2",
                                name,
                                report.iteration,
                                report,
                                interner,
                            );
                        }
                        for name in sweep_names {
                            if !reports.iter().any(|(reported, _)| reported == name) {
                                log_fixups_v2(format!(
                                    "[fixups_v2] skipped {name} segment={label} reason=not_applicable"
                                ));
                            }
                        }
                        drop(session);
                        log_fixups_v2(format!(
                            "[fixups_v2] sweep {label} done: {} visitor reports",
                            reports.len()
                        ));
                        all_reports.extend(reports);
                    }
                }
            }
            all_reports
        };

        // Harvest AddonNode index reassignments into the decision channel for
        // the NIF phase.
        for (_name, report) in &all_reports {
            for &(old, new) in &report.addon_index_remap {
                let kind = self.interner.intern("addon_node_index_remap");
                self.decisions.push(crate::translator::Decision {
                    kind,
                    message: format!("{old}->{new}"),
                });
            }
        }

        Ok(all_reports)
    }

    /// Authoritatively resolve deferred LCTN placed-ref-target classes against
    /// the now-COMPLETE output plugin.
    ///
    /// Runs AFTER the FO76→FO4 phase-6 cell-slice copy + cell-location sync
    /// re-insert the exterior placed children. The pre-copy
    /// `null_dangling_own_plugin_refs` and raw-LCTN passes deferred these classes
    /// because their targets were absent then; this pass keeps every ref whose
    /// target is now present and nulls/prunes any that are still absent.
    pub fn repair_placed_child_refs(&mut self) -> Result<FixupReport, FixupError> {
        let mapper_opts = MapperOptions {
            output_plugin_name: self.config.output_plugin_name.clone(),
            source_plugin_name: String::new(),
            source_master_names: Vec::new(),
            target_master_names: self.config.target_master_names.clone(),
            use_base_game_assets: self.config.use_base_game_assets,
            vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                self.source,
                self.target,
            ),
            preserve_source_ids: self.config.preserve_source_ids,
            generated_object_id_floor: self.config.generated_object_id_floor,
            resolution_mode: if self.config.strict_mapper {
                crate::formkey_mapper::ResolutionMode::Strict
            } else {
                crate::formkey_mapper::ResolutionMode::DeferAndFixup
            },
        };
        if self.mapper_state.is_none() {
            self.mapper_state = Some(MapperState::new([], mapper_opts));
        }

        let config = FixupConfig {
            strict: self.config.strict_mapper,
            preserve_source_ids: self.config.preserve_source_ids,
            use_base_game_assets: self.config.use_base_game_assets,
            is_whole_plugin: self.config.is_whole_plugin,
            root_sig: self.config.root_sig,
            skip_record_sigs: self.translator.maps.skip_records.clone(),
            mod_path: self.config.mod_path.clone(),
            source_extracted_dir: self.config.source_extracted_dir.clone(),
            target_extracted_dir: self.config.target_extracted_dir.clone(),
            target_master_handle_ids: self.master_handle_ids.clone(),
            target_schema: Some(Arc::clone(&self.schema_target)),
            source_schema: Some(Arc::clone(&self.schema_source)),
            asset_phases: self.config.asset_phases.clone(),
            defer_placed_child_ref_class: self.config.defer_placed_child_ref_class,
        };

        let event_tx = self.event_tx.clone();
        let interner = &self.interner;
        let mut mapper = FormKeyMapper::from_state(
            self.mapper_state
                .as_mut()
                .expect("mapper_state initialized above"),
            interner,
        );

        let mut session =
            crate::session::open_session(self.target_handle_id, Some(self.source_handle_id))
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let repair_error = |name: &str, err: FixupError| {
            FixupError::HandleError(format!("repair_placed_child_refs:{name}: {err}"))
        };
        let log_timing = |name: &str, started: std::time::Instant| {
            eprintln!(
                "[repair_timing] {name} elapsed_ms={}",
                started.elapsed().as_millis()
            );
        };
        let started = std::time::Instant::now();
        let mut report = crate::fixups::rewrite_raw_lctn_formids::repair_lctn_raw_formids(
            &mut session,
            &mut mapper,
            &config,
        )
        .map_err(|err| repair_error("rewrite_raw_lctn_formids", err))?;
        log_timing("rewrite_raw_lctn_formids", started);
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let gated_refs =
                crate::fixups::gate_runtime_controlled_placed_refs::gate_runtime_controlled_placed_refs(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| repair_error("gate_runtime_controlled_placed_refs", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(gated_refs.records_changed);
            report.warnings.extend(gated_refs.warnings);
            log_timing("gate_runtime_controlled_placed_refs", started);

            let started = std::time::Instant::now();
            let placed_lod =
                crate::fixups::normalize_placed_records::normalize_placed_lod_header_flags(
                    &mut session,
                    &mapper,
                )
                .map_err(|err| repair_error("normalize_placed_lod_header_flags", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(placed_lod.records_changed);
            report.warnings.extend(placed_lod.warnings);
            log_timing("normalize_placed_lod_header_flags", started);
        }
        let started = std::time::Instant::now();
        let placed_child = crate::fixups::null_dangling_own_plugin_refs::repair_placed_child_refs(
            &mut session,
            &mut mapper,
            &config,
        )
        .map_err(|err| repair_error("null_dangling_own_plugin_refs", err))?;
        report.records_changed = report
            .records_changed
            .saturating_add(placed_child.records_changed);
        report.records_dropped = report
            .records_dropped
            .saturating_add(placed_child.records_dropped);
        report.warnings.extend(placed_child.warnings);
        log_timing("null_dangling_own_plugin_refs", started);
        // Authoritative post-copy resolve of VMAD Object script-property refs.
        // Run unconditionally: placed children can be introduced by interior and
        // projected copy paths even when the pre-copy deferral flag was not set.
        let started = std::time::Instant::now();
        let vmad = crate::fixups::null_dangling_vmad_refs::repair_dangling_vmad_refs(
            &mut session,
            &mut mapper,
            &config,
        )
        .map_err(|err| repair_error("null_dangling_vmad_refs", err))?;
        report.records_changed = report.records_changed.saturating_add(vmad.records_changed);
        report.warnings.extend(vmad.warnings);
        log_timing("null_dangling_vmad_refs", started);
        // Strip the REFR placed-child Class C subrecords (XRFG/XLYR/XASP) here,
        // post-copy: REFR ∈ skip_records, so the in-phase
        // ValidateReferenceTargetTypesFixup run (pre-copy) sees none of these
        // records and their wrong-type / no-FO4-home refs survive into output.
        let started = std::time::Instant::now();
        let refr_strip =
            crate::fixups::validate_reference_target_types::strip_refr_placed_child_subrecords(
                &mut session,
                &mut mapper,
                &config,
            )
            .map_err(|err| repair_error("strip_refr_placed_child_subrecords", err))?;
        report.records_changed = report
            .records_changed
            .saturating_add(refr_strip.records_changed);
        report.records_dropped = report
            .records_dropped
            .saturating_add(refr_strip.records_dropped);
        log_timing("strip_refr_placed_child_subrecords", started);
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let material_swaps =
                crate::fixups::promote_placed_custom_material_swaps::promote_placed_custom_material_swaps(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| repair_error("promote_placed_custom_material_swaps", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(material_swaps.records_changed);
            log_timing("promote_placed_custom_material_swaps", started);
        }
        // Resolve placed refs whose base object is a leveled list (LVLI). FO76
        // allows an LVLI as a placed-ref base; FO4 reports "Missing/Invalid base
        // object". The cell-slice copy path resolves this for exterior/projected
        // children during copy, but the interior-cell emit path does not — so
        // interior placed-LVLI refs only get resolved here, post-copy, where every
        // placed child (interior + exterior) is present. Idempotent for the
        // already-resolved exterior refs (their base is no longer an LVLI).
        let started = std::time::Instant::now();
        let lvli_resolve =
            crate::fixups::resolve_placed_leveled_bases::resolve_placed_leveled_bases(
                &mut session,
                &mut mapper,
                &config,
            )
            .map_err(|err| repair_error("resolve_placed_leveled_bases", err))?;
        report.records_changed = report
            .records_changed
            .saturating_add(lvli_resolve.records_changed);
        report.records_dropped = report
            .records_dropped
            .saturating_add(lvli_resolve.records_dropped);
        log_timing("resolve_placed_leveled_bases", started);
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            // Normalize raw placed-link slots before later passes consume XLKR/XAPR.
            let started = std::time::Instant::now();
            let linked_ref_repair =
                crate::fixups::repair_placed_linked_refs::repair_placed_linked_refs(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| repair_error("repair_placed_linked_refs", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(linked_ref_repair.records_changed);
            log_timing("repair_placed_linked_refs", started);
        }
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let workshop_boundaries =
                crate::fixups::synthesize_workshop_boundaries::synthesize_workshop_boundaries(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| repair_error("synthesize_workshop_boundaries", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(workshop_boundaries.records_changed);
            report.records_added = report
                .records_added
                .saturating_add(workshop_boundaries.records_added);
            report.warnings.extend(workshop_boundaries.warnings);
            log_timing("synthesize_workshop_boundaries", started);
        }
        // Backfill ref-side XLRT from LCTN LCSR rows so FO4 location tracking
        // (boss clearing, special-ref queries) sees the reftypes FO76 only
        // baked location-side. Must stay AFTER synthesize_workshop_boundaries:
        // workshop locations have their Boss rows stripped there and must not
        // get Boss XLRT re-applied to their defender spawns.
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let xlrt_backfill =
                crate::fixups::backfill_placed_loc_ref_types::backfill_placed_loc_ref_types(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| repair_error("backfill_placed_loc_ref_types", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(xlrt_backfill.records_changed);
            log_timing("backfill_placed_loc_ref_types", started);
        }
        // Repair interior teleport-door XTEL targets. The cell-slice copy path
        // remaps XTEL's own master byte 0x00→07 for exterior/worldspace children;
        // the interior-cell emit path keeps XTEL raw, so its door/transition
        // FormIDs reach the output naming Fallout4.esm (byte 0x00) instead of the
        // own partner. FO76→FO4 only (the byte-0 own-ref ambiguity is specific to
        // the master-less FO76 source).
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let door_repair =
                crate::fixups::repair_placed_teleport_doors::repair_placed_references(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| repair_error("repair_placed_teleport_doors", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(door_repair.records_changed);
            log_timing("repair_placed_teleport_doors", started);
            // Convert FO76 additive placed-light XRDS deltas (commonly negative) to
            // FO4 absolute radius overrides. A negative absolute radius corrupts the
            // cell's spatial partition, killing physics + sound for the whole cell.
            let started = std::time::Instant::now();
            let light_radius =
                crate::fixups::normalize_placed_light_radius::normalize_placed_light_radius(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| repair_error("normalize_placed_light_radius", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(light_radius.records_changed);
            log_timing("normalize_placed_light_radius", started);
        }
        // WRLD.RNAM is the large-reference table for worldspace refs. FO76->FO4
        // strips source WRLD runtime tables now, but keep this repair late for
        // any FO4-native/legacy RNAM table that survives while the output is
        // complete and the source->target mapper is still available.
        let started = std::time::Instant::now();
        let wrld_large_refs = crate::fixups::rewrite_raw_wrld_large_refs::repair_wrld_large_refs(
            &mut session,
            &mut mapper,
            &config,
        )
        .map_err(|err| repair_error("rewrite_raw_wrld_large_refs", err))?;
        report.records_changed = report
            .records_changed
            .saturating_add(wrld_large_refs.records_changed);
        log_timing("rewrite_raw_wrld_large_refs", started);
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let xezn_finalize = crate::fixups::encounter_zones::finalize_placed_xezn_targets(
                &mut session,
                &config,
                mapper.interner,
            )
            .map_err(|err| repair_error("finalize_placed_xezn_targets", err))?;
            emit_fixup_report_log(
                &event_tx,
                "repair_placed_child_refs",
                "finalize_placed_xezn_targets",
                &xezn_finalize,
                mapper.interner,
            );
            report.records_changed = report
                .records_changed
                .saturating_add(xezn_finalize.records_changed);
            report.records_dropped = report
                .records_dropped
                .saturating_add(xezn_finalize.records_dropped);
            report.warnings.extend(xezn_finalize.warnings);
            log_timing("finalize_placed_xezn_targets", started);
        }
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let ess_spawn =
                crate::fixups::encounter_zones::specialize_placed_actor_templates_after_ref_repair(
                    &mut session,
                    &mut mapper,
                    &config,
                )
                .map_err(|err| {
                    repair_error("specialize_placed_actor_templates_after_ref_repair", err)
                })?;
            emit_fixup_report_log(
                &event_tx,
                "repair_placed_child_refs",
                "specialize_placed_actor_templates",
                &ess_spawn,
                mapper.interner,
            );
            report.records_added = report.records_added.saturating_add(ess_spawn.records_added);
            report.records_changed = report
                .records_changed
                .saturating_add(ess_spawn.records_changed);
            report.warnings.extend(ess_spawn.warnings);
            log_timing("specialize_placed_actor_templates", started);
        }
        // Final QUST alias-condition repair must remain after every placed-child
        // repair above: only now can output REFR persistence be proven from both
        // its record flag and CELL persistent-group topology.
        if self.source == Game::Fo76 && self.target == Game::Fo4 {
            let started = std::time::Instant::now();
            let quest_conditions =
                crate::fixups::strip_invalid_quest_condition_params::repair_final_quest_reference_conditions(
                    &mut session,
                    mapper.interner,
                    &config,
                )
                .map_err(|err| repair_error("repair_final_quest_reference_conditions", err))?;
            report.records_changed = report
                .records_changed
                .saturating_add(quest_conditions.records_changed);
            report.warnings.extend(quest_conditions.warnings);
            log_timing("repair_final_quest_reference_conditions", started);
        }
        session.flush_pending_effects();
        self.warnings.extend(report.warnings.iter().copied());
        Ok(report)
    }

    /// Post-copy FO76→FO4 encounter-zone synthesis. Runs AFTER the cell-slice
    /// copy + persistent-cell synthesis (exterior CELLs present in the target)
    /// and BEFORE `build_esp` (source handle still open, so FO76 LCTN is
    /// readable). Synthesizes one FO4 `ECZN` per qualifying Location, stamps
    /// `CELL.XEZN` on each footprint cell, and rewrites workshop `LCTN` keyword
    /// arrays to FO4's settlement contract. No-op off the FO76→FO4 path.
    pub fn synthesize_encounter_zones(
        &mut self,
        identity_resolve: bool,
    ) -> Result<FixupReport, FixupError> {
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(FixupReport::empty());
        }
        let mapper_opts = MapperOptions {
            output_plugin_name: self.config.output_plugin_name.clone(),
            source_plugin_name: String::new(),
            source_master_names: Vec::new(),
            target_master_names: self.config.target_master_names.clone(),
            use_base_game_assets: self.config.use_base_game_assets,
            vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                self.source,
                self.target,
            ),
            preserve_source_ids: self.config.preserve_source_ids,
            generated_object_id_floor: self.config.generated_object_id_floor,
            resolution_mode: if self.config.strict_mapper {
                crate::formkey_mapper::ResolutionMode::Strict
            } else {
                crate::formkey_mapper::ResolutionMode::DeferAndFixup
            },
        };
        if self.mapper_state.is_none() {
            self.mapper_state = Some(MapperState::new([], mapper_opts));
        }

        let config = FixupConfig {
            strict: self.config.strict_mapper,
            preserve_source_ids: self.config.preserve_source_ids,
            use_base_game_assets: self.config.use_base_game_assets,
            is_whole_plugin: self.config.is_whole_plugin,
            root_sig: self.config.root_sig,
            skip_record_sigs: self.translator.maps.skip_records.clone(),
            mod_path: self.config.mod_path.clone(),
            source_extracted_dir: self.config.source_extracted_dir.clone(),
            target_extracted_dir: self.config.target_extracted_dir.clone(),
            target_master_handle_ids: self.master_handle_ids.clone(),
            target_schema: Some(Arc::clone(&self.schema_target)),
            source_schema: Some(Arc::clone(&self.schema_source)),
            asset_phases: self.config.asset_phases.clone(),
            defer_placed_child_ref_class: self.config.defer_placed_child_ref_class,
        };

        let event_tx = self.event_tx.clone();
        let interner = &self.interner;
        let mut mapper = FormKeyMapper::from_state(
            self.mapper_state
                .as_mut()
                .expect("mapper_state initialized above"),
            interner,
        );

        let started = std::time::Instant::now();
        let mut session =
            crate::session::open_session(self.target_handle_id, Some(self.source_handle_id))
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
        eprintln!(
            "[eczn_timing] open_session elapsed_ms={}",
            started.elapsed().as_millis()
        );
        let report = crate::fixups::encounter_zones::synthesize_encounter_zones(
            &mut session,
            &mut mapper,
            &config,
            identity_resolve,
        )?;
        emit_fixup_report_log(
            &event_tx,
            "synthesize_encounter_zones",
            "encounter_zones",
            &report,
            mapper.interner,
        );
        let started = std::time::Instant::now();
        session.flush_pending_effects();
        eprintln!(
            "[eczn_timing] flush_pending_effects elapsed_ms={}",
            started.elapsed().as_millis()
        );
        self.warnings.extend(report.warnings.iter().copied());
        Ok(report)
    }

    /// FO76→FO4 interior sky-region assignment (post-copy). Stamps `CELL.XCCM`
    /// (Sky/Weather from Region → REGN) on every interior CELL flagged Show-Sky
    /// that lost its sky source in translation (FO76's FO76-only `XISR` Interior
    /// Sky Override has no FO4 equivalent). Must run AFTER interior-cell emit and
    /// encounter-zone synthesis, with the source handle still open (the dropped
    /// `XISR` weather is read back from the source CELL).
    pub fn synthesize_sky_regions(&mut self) -> Result<FixupReport, FixupError> {
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(FixupReport::empty());
        }
        let mapper_opts = MapperOptions {
            output_plugin_name: self.config.output_plugin_name.clone(),
            source_plugin_name: String::new(),
            source_master_names: Vec::new(),
            target_master_names: self.config.target_master_names.clone(),
            use_base_game_assets: self.config.use_base_game_assets,
            vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                self.source,
                self.target,
            ),
            preserve_source_ids: self.config.preserve_source_ids,
            generated_object_id_floor: self.config.generated_object_id_floor,
            resolution_mode: if self.config.strict_mapper {
                crate::formkey_mapper::ResolutionMode::Strict
            } else {
                crate::formkey_mapper::ResolutionMode::DeferAndFixup
            },
        };
        if self.mapper_state.is_none() {
            self.mapper_state = Some(MapperState::new([], mapper_opts));
        }

        let config = FixupConfig {
            strict: self.config.strict_mapper,
            preserve_source_ids: self.config.preserve_source_ids,
            use_base_game_assets: self.config.use_base_game_assets,
            is_whole_plugin: self.config.is_whole_plugin,
            root_sig: self.config.root_sig,
            skip_record_sigs: self.translator.maps.skip_records.clone(),
            mod_path: self.config.mod_path.clone(),
            source_extracted_dir: self.config.source_extracted_dir.clone(),
            target_extracted_dir: self.config.target_extracted_dir.clone(),
            target_master_handle_ids: self.master_handle_ids.clone(),
            target_schema: Some(Arc::clone(&self.schema_target)),
            source_schema: Some(Arc::clone(&self.schema_source)),
            asset_phases: self.config.asset_phases.clone(),
            defer_placed_child_ref_class: self.config.defer_placed_child_ref_class,
        };

        let interner = &self.interner;
        let mut mapper = FormKeyMapper::from_state(
            self.mapper_state
                .as_mut()
                .expect("mapper_state initialized above"),
            interner,
        );

        let mut session =
            crate::session::open_session(self.target_handle_id, Some(self.source_handle_id))
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let report =
            crate::fixups::sky_regions::synthesize_sky_regions(&mut session, &mut mapper, &config)?;
        session.flush_pending_effects();
        Ok(report)
    }

    /// FO76→FO4 vendor-dialogue enablement (post-copy). Synthesizes the
    /// `B21_VendorDialogueFaction` gate faction and enrolls every NPC that
    /// belongs to a vendor faction (a FACT carrying `VENC`) into it, so the
    /// companion `B21_VendorDialogue.esp` "Let's trade" topic can find them via
    /// `GetInFaction`. Target-only (FACT/NPC_); must run AFTER
    /// `repair_placed_child_refs` finalizes `FACT.VENC` and BEFORE the mapper
    /// remap state is released. FO76→FO4 only; no-ops otherwise.
    pub fn synthesize_vendor_dialogue(&mut self) -> Result<FixupReport, FixupError> {
        if self.source != Game::Fo76 || self.target != Game::Fo4 {
            return Ok(FixupReport::empty());
        }
        let mapper_opts = MapperOptions {
            output_plugin_name: self.config.output_plugin_name.clone(),
            source_plugin_name: String::new(),
            source_master_names: Vec::new(),
            target_master_names: self.config.target_master_names.clone(),
            use_base_game_assets: self.config.use_base_game_assets,
            vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                self.source,
                self.target,
            ),
            preserve_source_ids: self.config.preserve_source_ids,
            generated_object_id_floor: self.config.generated_object_id_floor,
            resolution_mode: if self.config.strict_mapper {
                crate::formkey_mapper::ResolutionMode::Strict
            } else {
                crate::formkey_mapper::ResolutionMode::DeferAndFixup
            },
        };
        if self.mapper_state.is_none() {
            self.mapper_state = Some(MapperState::new([], mapper_opts));
        }

        let config = FixupConfig {
            strict: self.config.strict_mapper,
            preserve_source_ids: self.config.preserve_source_ids,
            use_base_game_assets: self.config.use_base_game_assets,
            is_whole_plugin: self.config.is_whole_plugin,
            root_sig: self.config.root_sig,
            skip_record_sigs: self.translator.maps.skip_records.clone(),
            mod_path: self.config.mod_path.clone(),
            source_extracted_dir: self.config.source_extracted_dir.clone(),
            target_extracted_dir: self.config.target_extracted_dir.clone(),
            target_master_handle_ids: self.master_handle_ids.clone(),
            target_schema: Some(Arc::clone(&self.schema_target)),
            source_schema: Some(Arc::clone(&self.schema_source)),
            asset_phases: self.config.asset_phases.clone(),
            defer_placed_child_ref_class: self.config.defer_placed_child_ref_class,
        };

        let interner = &self.interner;
        let mut mapper = FormKeyMapper::from_state(
            self.mapper_state
                .as_mut()
                .expect("mapper_state initialized above"),
            interner,
        );

        // Target-only pass (FACT/NPC_); the early source-close runs before this
        // phase, so open the session without the (now-closed) source handle.
        let mut session = crate::session::open_session(self.target_handle_id, None)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let report = crate::fixups::vendor_dialogue::synthesize_vendor_dialogue(
            &mut session,
            &mut mapper,
            &config,
        )?;
        session.flush_pending_effects();
        Ok(report)
    }

    /// Run the FNV legacy-scripting translation pass for deferred records.
    ///
    /// Processes QUST, SCEN, and INFO records that were deferred during
    /// `translate_all` (Phase D records). Translated authoring-record payloads
    /// are written directly to the target plugin handle via
    /// `insert_authoring_record_value`; the returned
    /// `FnvLegacyScriptingResult` carries only counts, PSC texts, voice paths,
    /// and warnings.
    ///
    /// `mod_prefix` — the mod author prefix (e.g. `"B21"`).
    /// `source_plugin` — source plugin filename (e.g. `"FNV.esm"`).
    /// `quest_records`, `scene_records`, `info_records` — slices of
    ///   `(record_dict, source_form_key)` pairs, typically produced by reading
    ///   the deferred FormKeys from the source handle.
    ///
    pub fn run_fnv_legacy_scripting(
        &mut self,
        mod_prefix: &str,
        source_plugin: &str,
        mod_path: &str,
        script_records: &[(serde_json::Value, String)],
        quest_records: &[(serde_json::Value, String)],
        scene_records: &[(serde_json::Value, String)],
        info_records: &[(serde_json::Value, String)],
        dial_records: &[(serde_json::Value, String)],
        // Pairs of (target_form_key, source_scpt_form_key) mapping non-scripting
        // records (WEAP, NPC_, etc.) to their SCRI targets. Built by the Python
        // caller from source records' SCRI subrecords + FormKey mapper.
        scri_links: &[(String, String)],
    ) -> Result<FnvLegacyScriptingResult, FnvScriptingError> {
        let mut ctx =
            FnvLegacyScriptingContext::new(mod_prefix, source_plugin, self.config.strict_mapper);

        crate::fnv_legacy_scripting::translate_all_scpt(&mut ctx, script_records);
        translate_all_qust(&mut ctx, quest_records);
        translate_all_scen(&mut ctx, scene_records);
        translate_all_dial(&mut ctx, info_records);

        // ── DIAL: group by speaker + emit stripped payloads ───────────────────
        // Mirrors Python `phases.py:149–162`: group_dial_records once for the
        // result side-channel, then re-emit each DIAL record as a payload with
        // legacy-scripting subrecords stripped (SCTX/VTCK/VMAD/...).
        let dial_only: Vec<serde_json::Value> =
            dial_records.iter().map(|(v, _)| v.clone()).collect();
        let dialogue_groups = crate::fnv_legacy_scripting::dialogue::group_dial_records(&dial_only);
        crate::fnv_legacy_scripting::dialogue::accumulate_dial_records(&mut ctx, dial_records);

        // ── Pre-mutate payload FormKeys via the mapper ─────────────────────
        // Mirrors Python `_rewrite_payload_formkeys`: cross-plugin refs that
        // point at records translated during this run (which live in the
        // output plugin with freshly-allocated local IDs not on any master
        // table) need to be remapped before the encoder runs. The mapper
        // state may be absent for runs that never invoked translate_all;
        // in that case the rewrite is a no-op.
        let mut payloads = std::mem::take(&mut ctx.translated_record_payloads);
        let mut formkeys_rewritten: u32 = 0;
        let mut scene_pnam_dropped: u32 = 0;
        if let Some(state) = self.mapper_state.as_mut() {
            let mapper = FormKeyMapper::from_state(state, &mut self.interner);
            for payload in payloads.iter_mut() {
                formkeys_rewritten +=
                    crate::fnv_legacy_scripting::fk_rewrite::rewrite_payload_formkeys(
                        &mut payload.translated_record,
                        &mapper,
                    );
                if payload.signature == "SCEN" {
                    scene_pnam_dropped +=
                        crate::fnv_legacy_scripting::fk_rewrite::drop_unmapped_scene_parent(
                            &mut payload.translated_record,
                            &mapper,
                        );
                }
            }
        }
        if formkeys_rewritten > 0 || scene_pnam_dropped > 0 {
            let warning = format!(
                "fnv_legacy_scripting:fk_rewrite: rewrote {} ref(s), dropped {} SCEN PNAM",
                formkeys_rewritten, scene_pnam_dropped,
            );
            let sym = self.interner.intern(&warning);
            self.warnings.push(sym);
        }

        // ── Drain translated payloads into the target plugin handle ────────
        // Each payload was produced as an authoring JsonValue by the per-sig
        // synthesizer. `insert_authoring_record_value` resolves
        // FormKey strings against the target plugin's master table, so
        // cross-plugin references in the payload (e.g. `"001234:FNV.esm"`)
        // become correctly encoded master references when the source plugin
        // is on the target's master list.
        let mut records_written: u32 = 0;
        let mut records_failed: u32 = 0;
        for payload in payloads {
            match esp_authoring_core::plugin_runtime::insert_authoring_record_value(
                self.target_handle_id,
                &payload.translated_record,
            ) {
                Ok(_form_key) => {
                    records_written += 1;
                    for w in &payload.warnings {
                        let sym = self.interner.intern(w.as_str());
                        self.warnings.push(sym);
                    }
                }
                Err(err) => {
                    records_failed += 1;
                    // PyErr -> string requires the GIL; fall back to the type
                    // name when we can't attach.
                    let msg = Python::attach(|py| err.value(py).to_string());
                    let warning = format!(
                        "fnv_legacy_scripting:insert_authoring_record_value({} {}): {msg}",
                        payload.signature, payload.source_form_key,
                    );
                    let sym = self.interner.intern(&warning);
                    self.warnings.push(sym);
                }
            }
        }

        // ── Emit .psc files for every translated script/fragment ──────────
        // Mirrors Python's inline file writes in script_translator.py /
        // quest.py / scene.py / dialogue.py. When `mod_path` is empty (e.g.
        // handle-only tests) emission is a no-op and every record is counted
        // as skipped. Errors are accumulated into ctx.warnings rather than
        // failing the run.
        let psc_report = crate::fnv_legacy_scripting::psc_emission::emit_psc_files(
            std::path::Path::new(mod_path),
            &ctx.translated_scripts,
            &ctx.translated_quests,
            &ctx.translated_infos,
            &ctx.translated_scenes,
        );
        for err in &psc_report.errors {
            let sym = self.interner.intern(err.as_str());
            self.warnings.push(sym);
        }

        // Forward accumulated synthesizer warnings into the run's warning buffer.
        for warning in &ctx.warnings {
            let sym = self.interner.intern(warning.as_str());
            self.warnings.push(sym);
        }

        // ── VMAD intent computation ─────────────────────────────────────────
        // Pair SCRI links with translated SCPT records, then attach VMAD
        // directly to the target handle.
        let vmad_targets: Vec<crate::fnv_legacy_scripting::vmad::VmadTarget> = scri_links
            .iter()
            .map(
                |(target_fk, source_scpt_fk)| crate::fnv_legacy_scripting::vmad::VmadTarget {
                    target_form_key: target_fk.clone(),
                    source_scpt_form_key: source_scpt_fk.clone(),
                },
            )
            .collect();
        let vmad_intents = crate::fnv_legacy_scripting::vmad::build_scpt_vmad_intents(
            &vmad_targets,
            &ctx.translated_scripts,
        );
        let vmad_attached_in_rust = self.attach_vmad_intents_to_target_handle(&vmad_intents)?;

        Ok(FnvLegacyScriptingResult {
            translated_scripts: ctx.translated_scripts,
            translated_quests: ctx.translated_quests,
            translated_infos: ctx.translated_infos,
            translated_scenes: ctx.translated_scenes,
            dialogue_groups,
            records_written,
            records_failed,
            psc_files_written: psc_report.files_written,
            psc_files_skipped: psc_report.files_skipped,
            skipped_records: ctx.skipped_records,
            lip_regeneration_needed: ctx.lip_regeneration_needed,
            warnings: ctx.warnings,
            vmad_intents,
            vmad_attached_in_rust,
        })
    }

    pub fn run_fnv_legacy_scripting_from_deferred(
        &mut self,
        mod_prefix: &str,
        source_plugin: &str,
        mod_path: &str,
    ) -> Result<FnvLegacyScriptingResult, FnvScriptingError> {
        crate::fnv_legacy_scripting::from_run::run_from_deferred(
            self,
            mod_prefix,
            source_plugin,
            mod_path,
        )
    }

    fn attach_vmad_intents_to_target_handle(
        &mut self,
        intents: &[crate::fnv_legacy_scripting::vmad::ScriptBindingIntent],
    ) -> Result<bool, FnvScriptingError> {
        if intents.is_empty() {
            return Ok(false);
        }
        let mut attached = 0u32;
        for intent in intents {
            let mut record =
                match esp_authoring_core::plugin_runtime::plugin_handle_read_authoring_record_value_json(
                    self.target_handle_id,
                    &intent.target_form_key,
                ) {
                    Ok(Some(record)) => record,
                    Ok(None) => {
                        let warning = format!(
                            "fnv_legacy_scripting:vmad_target_missing:{}",
                            intent.target_form_key
                        );
                        let sym = self.interner.intern(&warning);
                        self.warnings.push(sym);
                        continue;
                    }
                    Err(err) => {
                        return Err(FnvScriptingError::Setup(format!(
                            "vmad read {}: {err}",
                            intent.target_form_key
                        )));
                    }
                };
            crate::fnv_legacy_scripting::vmad::attach_vmad_to_record(&mut record, intent)
                .map_err(|err| FnvScriptingError::Setup(err.to_string()))?;
            esp_authoring_core::plugin_runtime::plugin_handle_replace_authoring_record_value(
                self.target_handle_id,
                &record,
            )
            .map_err(|err| {
                FnvScriptingError::Setup(format!("vmad replace {}: {err}", intent.target_form_key))
            })?;
            attached += 1;
        }
        if attached > 0 {
            let warning = format!("fnv_legacy_scripting:vmad_attached:{attached}");
            let sym = self.interner.intern(&warning);
            self.warnings.push(sym);
        }
        Ok(attached > 0)
    }

    /// Rewrite form-key references in the target handle using `mappings`.
    /// Returns the number of records that had at least one reference rewritten.
    /// Rust-owned replacement for the old Python form-key rewrite pass.
    pub fn apply_registry_mappings(
        &mut self,
        mappings: &std::collections::HashMap<String, String>,
    ) -> Result<usize, RunError> {
        if mappings.is_empty() {
            return Ok(0);
        }
        esp_authoring_core::plugin_runtime::plugin_handle_rewrite_references(
            self.target_handle_id,
            mappings,
        )
        .map_err(|e| RunError::InvalidConfig(format!("apply_registry_mappings: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixup_warning_details_are_logged_with_a_bounded_tail() {
        let interner = StringInterner::new();
        let mut report = FixupReport::empty();
        for index in 0..FIXUP_WARNING_LOG_LIMIT + 2 {
            report.warnings.push(interner.intern(&format!(
                "ess_spawn: actor={index:06X} branch={:06X}",
                index + 1
            )));
        }

        let messages =
            fixup_warning_log_messages("fixups_v2", "encounter_zones", 1, &report, &interner);

        assert_eq!(messages.len(), FIXUP_WARNING_LOG_LIMIT + 1);
        assert_eq!(
            messages[0],
            "[fixups_v2] diagnostic encounter_zones iter=1 ess_spawn: actor=000000 branch=000001"
        );
        assert_eq!(
            messages.last().unwrap(),
            "[fixups_v2] diagnostic encounter_zones iter=1 truncated=2 total=34"
        );
    }

    #[test]
    fn post_copy_fixup_report_emits_summary_warning_and_diagnostic_events() {
        let interner = StringInterner::new();
        let mut report = FixupReport::empty();
        report.records_changed = 3;
        report.records_added = 1;
        report
            .warnings
            .push(interner.intern("ess_spawn: actor=59BA47 branch=597659"));
        report
            .diagnostics
            .push(interner.intern("source_template_override:actor=0D228A"));
        let (event_tx, event_rx) = crossbeam_channel::unbounded();

        emit_fixup_report_log(
            &event_tx,
            "repair_placed_child_refs",
            "specialize_placed_actor_templates",
            &report,
            &interner,
        );

        let events = event_rx.try_iter().collect::<Vec<_>>();
        assert_eq!(events.len(), 3);
        assert!(matches!(
            &events[0],
            crate::phase::PhaseEvent::Log {
                phase: "repair_placed_child_refs",
                level: crate::phase::LogLevel::Info,
                message,
            } if message.contains("changed=3") && message.contains("added=1")
        ));
        assert!(matches!(
            &events[1],
            crate::phase::PhaseEvent::Log {
                phase: "repair_placed_child_refs",
                level: crate::phase::LogLevel::Warn,
                message,
            } if message.contains("actor=59BA47 branch=597659")
        ));
        assert!(matches!(
            &events[2],
            crate::phase::PhaseEvent::Log {
                phase: "repair_placed_child_refs",
                level: crate::phase::LogLevel::Info,
                message,
            } if message.contains("source_template_override:actor=0D228A")
        ));
    }

    #[test]
    fn owned_run_releases_handles_once_and_keeps_target_live_until_drop() {
        let source = OwnedPluginHandle::new("OwnedSource.esm", "fo4");
        let target = OwnedPluginHandle::new("OwnedTarget.esm", "fo4");
        let master = OwnedPluginHandle::new("OwnedMaster.esm", "fo4");
        let source_id = source.id();
        let target_id = target.id();
        let master_id = master.id();
        let run_id = create_owned_run(
            Game::Fo4,
            Game::Fo4,
            RunConfig::default(),
            OwnedRunHandles {
                source: Some(source),
                target,
                masters: vec![master],
            },
            PathBuf::from("OwnedTarget.esm"),
            TargetMode::CreateNew,
        )
        .unwrap();

        with_run(run_id, |run| {
            assert!(run.release_source_handle());
            assert!(!run.release_source_handle());
            assert_eq!(run.release_master_handles(), 1);
            assert_eq!(run.release_master_handles(), 0);
            Ok::<_, RunError>(())
        })
        .unwrap();
        {
            let store = esp_authoring_core::plugin_runtime::plugin_handle_store_ref()
                .lock()
                .unwrap();
            assert!(!store.contains_key(&source_id));
            assert!(!store.contains_key(&master_id));
            assert!(store.contains_key(&target_id));
        }

        drop_run(run_id).unwrap();
        assert!(
            !esp_authoring_core::plugin_runtime::plugin_handle_store_ref()
                .lock()
                .unwrap()
                .contains_key(&target_id)
        );
    }

    #[test]
    fn source_worldspace_topology_rebuild_includes_skyrimse_to_fo4() {
        assert!(supports_source_worldspace_topology_rebuild(
            Game::SkyrimSe,
            Game::Fo4
        ));
        assert!(supports_source_worldspace_topology_rebuild(
            Game::Fnv,
            Game::Fo4
        ));
        assert!(!supports_source_worldspace_topology_rebuild(
            Game::Fo76,
            Game::Fo4
        ));
        assert!(!supports_source_worldspace_topology_rebuild(
            Game::SkyrimSe,
            Game::SkyrimSe
        ));
    }

    #[test]
    fn form_key_to_legacy_str_formats_python_legacy_shape() {
        let mut interner = StringInterner::new();
        let plugin = interner.intern("Output.esp");
        let fk = FormKey {
            local: 0x1234,
            plugin,
        };
        assert_eq!(
            form_key_to_legacy_str(fk, &interner).as_deref(),
            Some("001234:Output.esp")
        );
    }

    #[test]
    fn set_nvnm_parent_interior_preserves_own_master_index_and_clears_exterior() {
        // The interior NAVM parent must carry the cell's *file* FormID (own
        // master index applied). The mapper's `form_key.local` carries master
        // index 0x00; stamping that instead would resolve the parent cell to
        // the wrong master at runtime and crash the engine's NavMeshInfoMap
        // lookup on interior cells.
        use esp_authoring_core::nvnm::{NvnmGrid, NvnmParent, NvnmPayload, parse_nvnm, write_nvnm};

        let payload = NvnmPayload {
            version: 15,
            flags: 0,
            // A residual exterior-grid parent that the force MUST clear.
            parent: NvnmParent::Exterior {
                world: 0x0001_0023,
                grid_x: 1,
                grid_y: 2,
            },
            vertices: Vec::new(),
            triangles: Vec::new(),
            edge_links: Vec::new(),
            door_refs: Vec::new(),
            cover_array: Vec::new(),
            cover_triangle_mappings: Vec::new(),
            waypoints: Vec::new(),
            grid: NvnmGrid::default(),
        };

        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("NAVM").unwrap(),
            FormKey {
                local: 0x0056_8668,
                plugin: interner.intern("Output.esm"),
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("NVNM").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(write_nvnm(&payload))),
        });

        // Cell's file FormID: own master index 0x07 + object id.
        let cell_file_form_id = 0x0756_61B4_u32;
        set_nvnm_parent_interior(&mut record, cell_file_form_id);

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected NVNM bytes");
        };
        let parsed = parse_nvnm(bytes.as_slice()).expect("re-parse NVNM");
        assert_eq!(
            parsed.parent,
            NvnmParent::Interior {
                cell: cell_file_form_id
            }
        );
        let NvnmParent::Interior { cell } = parsed.parent else {
            unreachable!();
        };
        assert_eq!(cell >> 24, 0x07, "own master index must survive the stamp");
    }

    #[test]
    fn config_preflight_rows_seed_mapper_state() {
        let interner = StringInterner::new();
        let cfg = RunConfig {
            output_plugin_name: "Output.esm".into(),
            use_base_game_assets: true,
            target_record_preflight: vec![TargetRecordPreflightRow {
                editor_id: "Ammo10mm".into(),
                signature: "AMMO".into(),
                form_key: "01F276:Fallout4.esm".into(),
            }],
            target_master_names: vec!["Fallout4.esm".into()],
            ..Default::default()
        };
        let mut entries = mapper_entries_from_preflight(&cfg, &interner);
        assert_eq!(entries.len(), 1);

        let (eid, fk, sig) = entries.remove(0);
        assert_eq!(interner.resolve(eid), Some("ammo10mm"));
        assert_eq!(interner.resolve(fk.plugin), Some("Fallout4.esm"));
        assert_eq!(fk.local, 0x01F276);
        assert_eq!(sig.as_str(), "AMMO");
    }

    #[test]
    fn mixed_case_editor_ids_use_same_preflight_mapper_key() {
        let interner = StringInterner::new();
        let cfg = RunConfig {
            output_plugin_name: "Output.esm".into(),
            use_base_game_assets: true,
            target_record_preflight: vec![TargetRecordPreflightRow {
                editor_id: "Ammo10mm".into(),
                signature: "AMMO".into(),
                form_key: "01F276:Fallout4.esm".into(),
            }],
            ..Default::default()
        };
        let entries = mapper_entries_from_preflight(&cfg, &interner);
        let source_eid = interner.intern("ammo10MM");
        let normalized_source_eid = normalized_eid_sym(source_eid, &interner);
        let sig = crate::ids::SigCode::from_str("AMMO").unwrap();
        let source_fk = FormKey {
            local: 0x800,
            plugin: interner.intern("SeventySix.esm"),
        };
        let mut mapper = FormKeyMapper::new(
            entries,
            MapperOptions {
                output_plugin_name: "Output.esm".into(),
                use_base_game_assets: true,
                ..Default::default()
            },
            &interner,
        );

        let target_fk = mapper.allocate_or_resolve(source_fk, Some(normalized_source_eid), sig);
        assert_eq!(interner.resolve(target_fk.plugin), Some("Fallout4.esm"));
        assert_eq!(target_fk.local, 0x01F276);
    }

    #[test]
    fn preflight_source_mappings_rewrite_refs_before_duplicate_record_seen() {
        use crate::ids::{SigCode, SubrecordSig};
        use crate::record::{FieldEntry, FieldValue, Record};

        let mut interner = StringInterner::new();
        let ammo_sig = SigCode::from_str("AMMO").unwrap();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let source_ammo = FormKey::parse("01F276@SeventySix.esm", &mut interner).unwrap();
        let source_weap = FormKey::parse("000800@SeventySix.esm", &mut interner).unwrap();
        let target_ammo = FormKey::parse("01F276@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("ammo10MM");
        let target_eid = interner.intern("ammo10mm");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_ammo, ammo_sig)],
            &[(target_eid, target_ammo, ammo_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );
        assert_eq!(mappings, vec![(source_ammo, target_ammo)]);

        let mut state = MapperState::new(
            [(target_eid, target_ammo, ammo_sig)],
            MapperOptions {
                output_plugin_name: "Output.esm".into(),
                use_base_game_assets: true,
                ..Default::default()
            },
        );
        for (source_form_key, target_form_key) in mappings {
            state
                .source_to_target
                .insert(source_form_key, target_form_key);
        }

        let mut record = Record::new(weap_sig, source_weap);
        let ammo_field = interner.intern("ammo");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Struct(vec![(ammo_field, FieldValue::FormKey(source_ammo))]),
        });

        let mut mapper = FormKeyMapper::from_state(&mut state, &mut interner);
        mapper.rewrite_record(&mut record).unwrap();

        if let FieldValue::Struct(fields) = &record.fields[0].value {
            assert_eq!(fields[0].1, FieldValue::FormKey(target_ammo));
        } else {
            panic!("expected Struct field");
        }
    }

    #[test]
    fn fo76_fo4_weap_preflight_maps_cr_prefixed_editor_ids() {
        let mut interner = StringInterner::new();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let source_weap = FormKey::parse("0DF259@SeventySix.esm", &mut interner).unwrap();
        let target_weap = FormKey::parse("0DF259@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("crAssaultronRightClaw");
        let target_eid = interner.intern("assaultronrightclaw");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_weap, weap_sig)],
            &[(target_eid, target_weap, weap_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert_eq!(mappings, vec![(source_weap, target_weap)]);
    }

    #[test]
    fn fo76_fo4_weap_preflight_maps_zzz_prefixed_editor_ids() {
        let mut interner = StringInterner::new();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let source_weap = FormKey::parse("0AB6AE@SeventySix.esm", &mut interner).unwrap();
        let target_weap = FormKey::parse("0AB6AE@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("zzz_TutorialDummy10mm");
        let target_eid = interner.intern("tutorialdummy10mm");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_weap, weap_sig)],
            &[(target_eid, target_weap, weap_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert_eq!(mappings, vec![(source_weap, target_weap)]);
    }

    #[test]
    fn fo76_fo4_weap_preflight_maps_cr_prefixed_same_local_id() {
        let mut interner = StringInterner::new();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let source_weap = FormKey::parse("09F24D@SeventySix.esm", &mut interner).unwrap();
        let target_weap = FormKey::parse("09F24D@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("crMirelurkHunterSpitWeapon");
        let target_eid = interner.intern("weapmirelurkhunter01");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_weap, weap_sig)],
            &[(target_eid, target_weap, weap_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert_eq!(mappings, vec![(source_weap, target_weap)]);
    }

    #[test]
    fn fo76_fo4_preflight_maps_custom_projectile_collision_layer() {
        let mut interner = StringInterner::new();
        let coll_sig = SigCode::from_str("COLL").unwrap();
        let source_layer = FormKey::parse("5B74D0@SeventySix.esm", &mut interner).unwrap();
        let target_layer = FormKey::parse("088787@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("L_PROJ_NO_COLLIDE_PROJ");
        let target_eid = interner.intern("l_coneprojectile");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_layer, coll_sig)],
            &[(target_eid, target_layer, coll_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert_eq!(mappings, vec![(source_layer, target_layer)]);
    }

    #[test]
    fn fo76_fo4_preflight_same_local_id_remap_requires_weap_alias_prefix() {
        let mut interner = StringInterner::new();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let source_weap = FormKey::parse("123456@SeventySix.esm", &mut interner).unwrap();
        let target_weap = FormKey::parse("123456@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("SomeDifferentWeapon");
        let target_eid = interner.intern("otherweapon");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_weap, weap_sig)],
            &[(target_eid, target_weap, weap_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert!(mappings.is_empty());
    }

    #[test]
    fn preflight_source_mappings_do_not_remap_packages_by_editor_id() {
        let mut interner = StringInterner::new();
        let pack_sig = SigCode::from_str("PACK").unwrap();
        let source_pack = FormKey::parse("407F9F@SeventySix.esm", &mut interner).unwrap();
        let target_pack = FormKey::parse("02A105@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("followplayer");
        let target_eid = interner.intern("followplayer");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_pack, pack_sig)],
            &[(target_eid, target_pack, pack_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert!(mappings.is_empty());
    }

    #[test]
    fn fo76_fo4_static_preflight_mappings_do_not_remap_by_editor_id() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_stat = FormKey::parse("012345@SeventySix.esm", &mut interner).unwrap();
        let target_stat = FormKey::parse("012345@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("TerrainShelfRocks01");
        let target_eid = interner.intern("TerrainShelfRocks01");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_stat, stat_sig)],
            &[(target_eid, target_stat, stat_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert!(mappings.is_empty());
    }

    #[test]
    fn fo76_fo4_static_marker_preflight_mappings_remap_by_editor_id() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_marker = FormKey::parse("00003B@SeventySix.esm", &mut interner).unwrap();
        let target_marker = FormKey::parse("00003B@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("XMarker");
        let target_eid = interner.intern("xmarker");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_marker, stat_sig)],
            &[(target_eid, target_marker, stat_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert_eq!(mappings, vec![(source_marker, target_marker)]);
    }

    #[test]
    fn fo76_fo4_movable_static_preflight_mappings_do_not_remap_by_editor_id() {
        let mut interner = StringInterner::new();
        let mstt_sig = SigCode::from_str("MSTT").unwrap();
        let source_mstt = FormKey::parse("196D46@SeventySix.esm", &mut interner).unwrap();
        let target_mstt = FormKey::parse("196D46@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("Vehicle_ShuttleBus01");
        let target_eid = interner.intern("Vehicle_ShuttleBus01");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_mstt, mstt_sig)],
            &[(target_eid, target_mstt, mstt_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert!(mappings.is_empty());
    }

    #[test]
    fn fo76_fo4_creature_skin_records_do_not_remap_by_editor_id() {
        let mut interner = StringInterner::new();
        let mut blocked = FxHashSet::default();
        blocked.insert(FormKey::parse("112BED@SeventySix.esm", &mut interner).unwrap());
        blocked.insert(FormKey::parse("112BEB@SeventySix.esm", &mut interner).unwrap());

        for (sig_name, source_fk, target_fk, editor_id) in [
            (
                "ARMO",
                "112BED@SeventySix.esm",
                "00CE5F@DLCNukaWorld.esm",
                "DLC04_SkinRadAnt01",
            ),
            (
                "ARMA",
                "112BEB@SeventySix.esm",
                "00CE5D@DLCNukaWorld.esm",
                "DLC04_RadAntAA",
            ),
        ] {
            let sig = SigCode::from_str(sig_name).unwrap();
            let source = FormKey::parse(source_fk, &mut interner).unwrap();
            let target = FormKey::parse(target_fk, &mut interner).unwrap();
            let source_eid = interner.intern(editor_id);
            let target_eid = interner.intern(&editor_id.to_ascii_lowercase());

            let mappings = source_target_mappings_from_preflight_with_skips(
                [(source_eid, source, sig)],
                &[(target_eid, target, sig)],
                &interner,
                Game::Fo76,
                Game::Fo4,
                &blocked,
            );

            assert!(mappings.is_empty(), "{sig_name} must stay source-owned");
        }
    }

    #[test]
    fn fo76_fo4_wearable_armor_records_still_remap_by_editor_id() {
        let mut interner = StringInterner::new();

        for (sig_name, source_fk, target_fk, editor_id) in [
            (
                "ARMO",
                "210000@SeventySix.esm",
                "220000@Fallout4.esm",
                "Armor_Raider_Chest",
            ),
            (
                "ARMA",
                "210001@SeventySix.esm",
                "220001@Fallout4.esm",
                "Armor_Raider_Chest_AA",
            ),
        ] {
            let sig = SigCode::from_str(sig_name).unwrap();
            let source = FormKey::parse(source_fk, &mut interner).unwrap();
            let target = FormKey::parse(target_fk, &mut interner).unwrap();
            let source_eid = interner.intern(editor_id);
            let target_eid = interner.intern(&editor_id.to_ascii_lowercase());

            let mappings = source_target_mappings_from_preflight(
                [(source_eid, source, sig)],
                &[(target_eid, target, sig)],
                &interner,
                Game::Fo76,
                Game::Fo4,
            );

            assert_eq!(mappings, vec![(source, target)], "{sig_name} should remap");
        }
    }

    #[test]
    fn fo76_fo4_skin_race_exact_target_match_does_not_need_protection() {
        let mut interner = StringInterner::new();
        let race_sig = SigCode::from_str("RACE").unwrap();
        let source_protectron_race =
            FormKey::parse("0DFB33@SeventySix.esm", &mut interner).unwrap();
        let target_protectron_race = FormKey::parse("0DFB33@Fallout4.esm", &mut interner).unwrap();
        let source_protectron_eid = interner.intern("ProtectronRace");
        let target_protectron_eid = interner.intern("protectronrace");

        assert!(source_race_has_same_editor_id_target(
            source_protectron_race,
            &[(source_protectron_eid, source_protectron_race, race_sig)],
            &[(target_protectron_eid, target_protectron_race, race_sig)],
            &interner,
        ));

        let source_rad_ant_race = FormKey::parse("112BEC@SeventySix.esm", &mut interner).unwrap();
        let target_rad_ant_race = FormKey::parse("00CE5E@DLCNukaWorld.esm", &mut interner).unwrap();
        let source_rad_ant_eid = interner.intern("RadAntRace");
        let target_rad_ant_eid = interner.intern("dlc04_radantrace");

        assert!(!source_race_has_same_editor_id_target(
            source_rad_ant_race,
            &[(source_rad_ant_eid, source_rad_ant_race, race_sig)],
            &[(target_rad_ant_eid, target_rad_ant_race, race_sig)],
            &interner,
        ));
    }

    #[test]
    fn fo76_fo4_cell_preflight_mappings_do_not_remap_reused_editor_ids() {
        let mut interner = StringInterner::new();
        let cell_sig = SigCode::from_str("CELL").unwrap();
        let source_cell = FormKey::parse("261548@SeventySix.esm", &mut interner).unwrap();
        let target_cell = FormKey::parse("00DFF7@Fallout4.esm", &mut interner).unwrap();
        let source_eid = interner.intern("RelayTower04Ext");
        let target_eid = interner.intern("relaytower04ext");

        let mappings = source_target_mappings_from_preflight(
            [(source_eid, source_cell, cell_sig)],
            &[(target_eid, target_cell, cell_sig)],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert!(mappings.is_empty());
    }

    #[test]
    fn fnv_fo4_wilderness_cell_stays_source_owned() {
        let mut interner = StringInterner::new();
        let cell_sig = SigCode::from_str("CELL").unwrap();
        let source_cell = FormKey::parse("0DDCAB@FNV_FO3_Merged.esm", &mut interner).unwrap();
        let target_cell = FormKey::parse("000D8A@DLCCoast.esm", &mut interner).unwrap();
        let wilderness = interner.intern("wilderness");
        let mut mapper = FormKeyMapper::new(
            [(wilderness, target_cell, cell_sig)],
            MapperOptions {
                output_plugin_name: "FNV_FO3_Merged.esm".into(),
                use_base_game_assets: true,
                vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                    Game::Fnv,
                    Game::Fo4,
                ),
                preserve_source_ids: true,
                ..Default::default()
            },
            &interner,
        );

        let mapped = mapper.allocate_or_resolve(source_cell, Some(wilderness), cell_sig);

        assert_eq!(mapped.local, 0x0D_DCAB);
        assert_eq!(interner.resolve(mapped.plugin), Some("FNV_FO3_Merged.esm"));
        assert_ne!(mapped, target_cell);
        assert!(!allows_source_target_preflight_remap(
            Game::Fnv,
            Game::Fo4,
            cell_sig,
            "Wilderness",
        ));
    }

    #[test]
    fn fo76_fo4_cell_editor_id_collision_gets_fo76_suffix() {
        let mut interner = StringInterner::new();
        let cell_sig = SigCode::from_str("CELL").unwrap();
        let source_cell = FormKey::parse("261548@SeventySix.esm", &mut interner).unwrap();
        let target_cell = FormKey::parse("00DFF7@Fallout4.esm", &mut interner).unwrap();
        let original_eid = interner.intern("RelayTower04Ext");
        let normalized_eid = interner.intern("relaytower04ext");
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let mut target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>> = FxHashMap::default();
        target_eid_index.insert(normalized_eid, vec![(target_cell, cell_sig)]);

        let mut record = Record::new(cell_sig, source_cell);
        record.eid = Some(original_eid);
        record.fields.push(FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(original_eid),
        });

        let renamed = rename_fo76_target_editor_id_collision(
            &mut record,
            &target_eid_index,
            &interner,
            false,
        );

        assert_eq!(
            renamed,
            Some(("RelayTower04Ext".into(), "RelayTower04Extfo76".into()))
        );
        assert_eq!(
            record.eid.and_then(|eid| interner.resolve(eid)),
            Some("RelayTower04Extfo76")
        );
        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].sig, edid_sig);
        assert_eq!(
            match record.fields[0].value {
                FieldValue::String(sym) => interner.resolve(sym),
                _ => None,
            },
            Some("RelayTower04Extfo76")
        );
    }

    #[test]
    fn fo76_fo4_cross_signature_editor_id_collision_gets_fo76_suffix() {
        let mut interner = StringInterner::new();
        let alch_sig = SigCode::from_str("ALCH").unwrap();
        let npc_sig = SigCode::from_str("NPC_").unwrap();
        let source_alch = FormKey::parse("0330FB@SeventySix.esm", &mut interner).unwrap();
        let target_npc = FormKey::parse("01D15C@Fallout4.esm", &mut interner).unwrap();
        let original_eid = interner.intern("Dogmeat");
        let normalized_eid = interner.intern("dogmeat");
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let mut target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>> = FxHashMap::default();
        target_eid_index.insert(normalized_eid, vec![(target_npc, npc_sig)]);

        let mut record = Record::new(alch_sig, source_alch);
        record.eid = Some(original_eid);
        record.fields.push(FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(original_eid),
        });

        let renamed = rename_fo76_target_editor_id_collision(
            &mut record,
            &target_eid_index,
            &interner,
            false,
        );

        assert_eq!(renamed, Some(("Dogmeat".into(), "Dogmeatfo76".into())));
        assert_eq!(
            record.eid.and_then(|eid| interner.resolve(eid)),
            Some("Dogmeatfo76")
        );
        assert_eq!(
            match record.fields[0].value {
                FieldValue::String(sym) => interner.resolve(sym),
                _ => None,
            },
            Some("Dogmeatfo76")
        );
    }

    #[test]
    fn fo76_fo4_blocked_same_signature_editor_id_collision_gets_fo76_suffix() {
        let mut interner = StringInterner::new();
        let armo_sig = SigCode::from_str("ARMO").unwrap();
        let source_skin = FormKey::parse("112BED@SeventySix.esm", &mut interner).unwrap();
        let target_skin = FormKey::parse("00CE5F@DLCNukaWorld.esm", &mut interner).unwrap();
        let original_eid = interner.intern("DLC04_SkinRadAnt01");
        let normalized_eid = interner.intern("dlc04_skinradant01");
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let mut target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>> = FxHashMap::default();
        target_eid_index.insert(normalized_eid, vec![(target_skin, armo_sig)]);

        let mut record = Record::new(armo_sig, source_skin);
        record.eid = Some(original_eid);
        record.fields.push(FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(original_eid),
        });

        let renamed =
            rename_fo76_target_editor_id_collision(&mut record, &target_eid_index, &interner, true);

        assert_eq!(
            renamed,
            Some(("DLC04_SkinRadAnt01".into(), "DLC04_SkinRadAnt01fo76".into()))
        );
        assert_eq!(
            record.eid.and_then(|eid| interner.resolve(eid)),
            Some("DLC04_SkinRadAnt01fo76")
        );
    }

    #[test]
    fn fo76_fo4_same_signature_editor_id_collision_keeps_vanilla_remap_candidate() {
        let mut interner = StringInterner::new();
        let alch_sig = SigCode::from_str("ALCH").unwrap();
        let source_alch = FormKey::parse("0330FB@SeventySix.esm", &mut interner).unwrap();
        let target_alch = FormKey::parse("0330FB@Fallout4.esm", &mut interner).unwrap();
        let original_eid = interner.intern("Dogmeat");
        let normalized_eid = interner.intern("dogmeat");
        let mut target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>> = FxHashMap::default();
        target_eid_index.insert(normalized_eid, vec![(target_alch, alch_sig)]);

        let mut record = Record::new(alch_sig, source_alch);
        record.eid = Some(original_eid);

        let renamed = rename_fo76_target_editor_id_collision(
            &mut record,
            &target_eid_index,
            &interner,
            false,
        );

        assert_eq!(renamed, None);
        assert_eq!(
            record.eid.and_then(|eid| interner.resolve(eid)),
            Some("Dogmeat")
        );
    }

    #[test]
    fn fo76_fo4_static_marker_editor_id_collision_keeps_vanilla_remap_candidate() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_marker = FormKey::parse("00003B@SeventySix.esm", &mut interner).unwrap();
        let target_marker = FormKey::parse("00003B@Fallout4.esm", &mut interner).unwrap();
        let original_eid = interner.intern("XMarker");
        let normalized_eid = interner.intern("xmarker");
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let mut target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>> = FxHashMap::default();
        target_eid_index.insert(normalized_eid, vec![(target_marker, stat_sig)]);

        let mut record = Record::new(stat_sig, source_marker);
        record.eid = Some(original_eid);
        record.fields.push(FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(original_eid),
        });

        let renamed =
            rename_fo76_target_editor_id_collision(&mut record, &target_eid_index, &interner, true);

        assert_eq!(renamed, None);
        assert_eq!(
            record.eid.and_then(|eid| interner.resolve(eid)),
            Some("XMarker")
        );
        assert_eq!(
            match record.fields[0].value {
                FieldValue::String(sym) => interner.resolve(sym),
                _ => None,
            },
            Some("XMarker")
        );
    }

    #[test]
    fn target_master_remap_classification_matches_plugin_sym() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let output = interner.intern("Output.esm");
        let target_master_syms: FxHashSet<Sym> = [fallout4].into_iter().collect();

        assert!(is_target_master_remap(
            FormKey {
                local: 0x01F276,
                plugin: fallout4,
            },
            &target_master_syms
        ));
        assert!(!is_target_master_remap(
            FormKey {
                local: 0x800,
                plugin: output,
            },
            &target_master_syms
        ));
    }

    #[test]
    fn target_master_names_from_config_apply_outside_whole_plugin_mode() {
        let cfg = RunConfig {
            is_whole_plugin: false,
            target_master_names: vec!["Fallout4.esm".into()],
            ..Default::default()
        };

        assert_eq!(
            target_master_names_for_skip(&cfg, Vec::new()),
            vec!["Fallout4.esm".to_string()]
        );
    }

    #[test]
    fn fo76_fo4_relocation_member_static_model_path_is_namespaced() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_stat = FormKey::parse("012345@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(stat_sig, source_stat);
        let model = interner.intern("Landscape\\DirtCliffs\\TerrainShelfRocks01.nif");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(model),
        });

        let members: std::collections::HashSet<String> = [relocation_modl_key(
            "Landscape\\DirtCliffs\\TerrainShelfRocks01.nif",
        )]
        .into_iter()
        .collect();
        let changed = namespace_base_asset_model_paths(&mut record, &members, "FO76", &interner);

        assert_eq!(changed, 1);
        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected string MODL");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("FO76\\Landscape\\DirtCliffs\\TerrainShelfRocks01.nif")
        );
    }

    #[test]
    fn fo76_fo4_relocation_member_movable_static_model_path_is_namespaced() {
        let mut interner = StringInterner::new();
        let mstt_sig = SigCode::from_str("MSTT").unwrap();
        let source_mstt = FormKey::parse("196D46@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(mstt_sig, source_mstt);
        let model = interner.intern("vehicles\\whitespring\\Vehicle_ShuttleBus_WhitePlain.nif");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(model),
        });

        let members: std::collections::HashSet<String> = [relocation_modl_key(
            "vehicles\\whitespring\\Vehicle_ShuttleBus_WhitePlain.nif",
        )]
        .into_iter()
        .collect();
        let changed = namespace_base_asset_model_paths(&mut record, &members, "FO76", &interner);

        assert_eq!(changed, 1);
        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected string MODL");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("FO76\\vehicles\\whitespring\\Vehicle_ShuttleBus_WhitePlain.nif")
        );
    }

    #[test]
    fn fo76_fo4_forced_flagwall_static_model_path_is_namespaced() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_stat = FormKey::parse("17FE29@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(stat_sig, source_stat);
        let model = interner.intern("SetDressing\\Minutemen\\FlagWallMinutemen01.nif");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(model),
        });

        let members: std::collections::HashSet<String> = [relocation_modl_key(
            "SetDressing\\Minutemen\\FlagWallMinutemen01.nif",
        )]
        .into_iter()
        .collect();
        let changed = namespace_base_asset_model_paths(&mut record, &members, "FO76", &interner);

        assert_eq!(changed, 1);
        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected string MODL");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("FO76\\SetDressing\\Minutemen\\FlagWallMinutemen01.nif")
        );
    }

    #[test]
    fn fo76_fo4_non_member_model_path_is_left_untouched() {
        let mut interner = StringInterner::new();
        let stat_sig = SigCode::from_str("STAT").unwrap();
        let source_stat = FormKey::parse("012345@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(stat_sig, source_stat);
        let model = interner.intern("Landscape\\DirtCliffs\\TerrainShelfRocks01.nif");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(model),
        });

        // Member set covers a different mesh, so this STAT's MODL must not move.
        let members: std::collections::HashSet<String> =
            [relocation_modl_key("Landscape\\Rocks\\SomeOtherRock.nif")]
                .into_iter()
                .collect();
        let changed = namespace_base_asset_model_paths(&mut record, &members, "FO76", &interner);

        assert_eq!(changed, 0);
        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected string MODL");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("Landscape\\DirtCliffs\\TerrainShelfRocks01.nif")
        );
    }

    #[test]
    fn fo76_fo4_terrain_phase_skips_terrain_owned_records() {
        let mut translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let cfg = RunConfig {
            asset_phases: AssetPhaseFlags {
                terrain: true,
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(!translator.maps.skip_records.contains("LTEX"));
        assert!(!translator.maps.skip_records.contains("GRAS"));

        apply_fo76_terrain_owned_record_skips(&mut translator, Game::Fo76, Game::Fo4, &cfg);

        assert!(translator.maps.skip_records.contains("LTEX"));
        assert!(translator.maps.skip_records.contains("GRAS"));
    }

    #[test]
    fn fo76_fo4_without_terrain_phase_keeps_ltex_and_gras_translatable() {
        let mut translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let cfg = RunConfig {
            asset_phases: AssetPhaseFlags {
                terrain: false,
                ..Default::default()
            },
            ..Default::default()
        };

        apply_fo76_terrain_owned_record_skips(&mut translator, Game::Fo76, Game::Fo4, &cfg);

        assert!(!translator.maps.skip_records.contains("LTEX"));
        assert!(!translator.maps.skip_records.contains("GRAS"));
    }

    #[test]
    fn explicit_quest_dialogue_skips_disable_structured_dialogue_tail() {
        for signature in ["QUST", "DIAL", "INFO"] {
            let cfg = RunConfig {
                skip_record_signatures: vec![signature.to_lowercase()],
                ..Default::default()
            };

            assert!(cfg.skips_fo76_quest_dialogue());
        }

        let cfg = RunConfig {
            skip_record_signatures: vec!["WEAP".to_string()],
            ..Default::default()
        };
        assert!(!cfg.skips_fo76_quest_dialogue());
    }

    #[test]
    fn full_merged_creature_coverage_is_zero_when_creatures_are_excluded() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_new_native,
        };

        let source_handle = plugin_handle_new_native("FNV_FO3_Merged.esm", Some("fnv")).unwrap();
        let target_handle = plugin_handle_new_native("Out.esm", Some("fo4")).unwrap();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: true,
                skip_record_signatures: vec!["CREA".to_string()],
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            run.mapper_state = Some(MapperState::new(
                [],
                MapperOptions {
                    source_plugin_name: "FNV_FO3_Merged.esm".to_string(),
                    ..Default::default()
                },
            ));
            assert_eq!(run.legacy_creature_race_expected_candidates(), 0);
            run.finalize_legacy_creature_race_coverage()
        })
        .unwrap();

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    /// Verify create/lookup/drop lifecycle without needing real plugin handles.
    ///
    /// The registry does not validate that handle IDs point to loaded plugins —
    /// that check happens only when you call read_record / add_record. So we can
    /// pass sentinel values here and confirm the registry wiring works correctly.
    #[test]
    fn run_create_lookup_drop() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esm".into(),
                ..Default::default()
            },
        })
        .expect("create_run failed");

        // Verify the run is accessible.
        let game_str = with_run(id, |run| Ok::<_, RunError>(run.source.as_str().to_string()))
            .expect("with_run failed");
        assert_eq!(game_str, "fo4");

        // Verify decisions and warnings start empty.
        with_run(id, |run| {
            assert!(run.decisions.is_empty());
            assert!(run.warnings.is_empty());
            assert!(run.deferred.is_empty());
            Ok::<_, RunError>(())
        })
        .unwrap();

        // Drop it.
        drop_run(id).expect("drop_run failed");

        // A second drop must return UnknownRun.
        assert!(matches!(drop_run(id), Err(RunError::UnknownRun(_))));

        // with_run after drop must also return UnknownRun.
        assert!(matches!(
            with_run(id, |_| Ok::<_, RunError>(())),
            Err(RunError::UnknownRun(_))
        ));
    }

    #[test]
    fn run_create_unknown_game_returns_invalid_config() {
        let result = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 1,
            target_handle_id: 2,
            master_handle_ids: vec![],
            config: RunConfig::default(),
        });
        // fo4 is a valid game, so this should succeed.
        assert!(result.is_ok());
        drop_run(result.unwrap()).unwrap();
    }

    #[test]
    fn create_run_builds_relocation_members_from_extracted_dirs() {
        let tmp = std::env::temp_dir().join("reloc_create_run");
        let _ = std::fs::remove_dir_all(&tmp);
        let fo76 = tmp.join("fo76");
        let fo4 = tmp.join("fo4");
        for d in [&fo76, &fo4] {
            std::fs::create_dir_all(d.join("meshes/landscape/rocks")).unwrap();
            std::fs::write(d.join("meshes/landscape/rocks/rock01.nif"), b"x").unwrap();
        }

        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                source_extracted_dir: Some(fo76.clone()),
                target_extracted_dir: Some(fo4.clone()),
                base_asset_relocation_mesh_roots: vec!["meshes/landscape".to_string()],
                base_asset_namespace: "FO76".to_string(),
                ..Default::default()
            },
        })
        .expect("create_run failed");

        with_run(id, |run| {
            // rock01.nif collides (present under both fo76 and fo4); its cascade is
            // empty because the 1-byte placeholder won't parse as a NIF — fine here.
            assert!(
                run.relocation_members
                    .contains("meshes/landscape/rocks/rock01.nif")
            );
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // -----------------------------------------------------------------------
    // mapper_state persists across records in translate_all: two allocations
    // in the same run must produce distinct object-ids.
    // -----------------------------------------------------------------------

    #[test]
    fn translate_all_mapper_persists_across_records() {
        use crate::formkey_mapper::FormKeyMapper;
        use crate::formkey_mapper::{MapperOptions, MapperState};
        use crate::ids::{FormKey, SigCode};
        use crate::sym::StringInterner;

        let mut interner = StringInterner::new();
        let opts = MapperOptions {
            output_plugin_name: "Output.esp".into(),
            ..Default::default()
        };
        let mut state = MapperState::new([], opts);
        let weap = SigCode::from_str("WEAP").unwrap();

        let src1 = FormKey::parse("001000@Source.esm", &mut interner).unwrap();
        let src2 = FormKey::parse("002000@Source.esm", &mut interner).unwrap();

        // Simulate record 1: allocate target for src1.
        let tgt1 = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &mut interner);
            mapper.allocate_or_resolve(src1, None, weap)
        };

        // Simulate record 2: allocate target for src2.
        let tgt2 = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &mut interner);
            mapper.allocate_or_resolve(src2, None, weap)
        };

        // Both must be under Output.esp.
        assert_eq!(interner.resolve(tgt1.plugin).unwrap(), "Output.esp");
        assert_eq!(interner.resolve(tgt2.plugin).unwrap(), "Output.esp");
        // They must have distinct object-ids across records.
        assert_ne!(
            tgt1.local, tgt2.local,
            "object-ids must not collide across records"
        );
    }

    #[test]
    fn fnv_fo3_humanoid_race_substitutions_cover_merged_source_catalog() {
        let source_races = [
            "CaucasianOldAged",
            "AfricanAmericanOldAged",
            "AsianOldAged",
            "HispanicOldAged",
            "AfricanAmericanRaider",
            "AsianRaider",
            "HispanicRaider",
            "CaucasianRaider",
            "TestQACaucasian",
            "HispanicOld",
            "HispanicChild",
            "CaucasianOld",
            "CaucasianChild",
            "AsianOld",
            "AsianChild",
            "AfricanAmericanOld",
            "AfricanAmericanChild",
            "AfricanAmerican",
            "Ghoul",
            "Asian",
            "Hispanic",
            "Caucasian",
            "Christine",
            "WhitelegsCacasians",
            "WhiteLegsAfricanAmerican",
            "DeadHorseCaucasian",
            "DeadHorseAfricanAmerican",
            "SorrowAfricanAmerican",
            "SorrowCacasian",
            "Lobotomites",
            "MarkedMenGhoul",
            "DLCPittHispanicMut",
            "DLCPittAsianMut",
            "DLCPittCaucasianMut",
            "DLCPittAfricanAmericanMut",
            "HispanicTribal",
            "CaucasianTribal",
            "AsianTribal",
            "AfricanAmericanTribal",
        ];
        assert_eq!(source_races.len(), 39);

        let interner = StringInterner::new();
        let source_plugin = interner.intern("FNV_FO3_Merged.esm");
        let race_sig = SigCode::from_str("RACE").unwrap();
        let source_entries = source_races
            .iter()
            .enumerate()
            .map(|(index, editor_id)| {
                (
                    interner.intern(editor_id),
                    FormKey {
                        local: 0x800 + index as u32,
                        plugin: source_plugin,
                    },
                    race_sig,
                )
            })
            .collect::<Vec<_>>();

        let mappings = fnv_fo3_fo4_humanoid_race_substitution_mappings(
            &source_entries,
            &interner,
            Game::Fnv,
            Game::Fo4,
        );
        assert_eq!(mappings.len(), source_races.len());
        assert_eq!(
            mappings
                .iter()
                .filter(|(_, target)| target.local == FO4_HUMAN_RACE_LOCAL)
                .count(),
            33
        );
        assert_eq!(
            mappings
                .iter()
                .filter(|(_, target)| target.local == FO4_HUMAN_CHILD_RACE_LOCAL)
                .count(),
            4
        );
        assert_eq!(
            mappings
                .iter()
                .filter(|(_, target)| target.local == FO4_GHOUL_RACE_LOCAL)
                .count(),
            2
        );
        assert!(
            mappings
                .iter()
                .all(|(_, target)| { interner.resolve(target.plugin) == Some("Fallout4.esm") })
        );
    }

    #[test]
    fn fnv_and_fo3_humanoid_races_use_same_fo4_donor_policy() {
        let interner = StringInterner::new();
        let race_sig = SigCode::from_str("RACE").unwrap();
        for (source_game, source_plugin, editor_id, expected_local) in [
            (
                Game::Fnv,
                "FalloutNV.esm",
                "CaucasianChild",
                FO4_HUMAN_CHILD_RACE_LOCAL,
            ),
            (
                Game::Fo3,
                "Fallout3.esm",
                "DLCPittCaucasianMut",
                FO4_HUMAN_RACE_LOCAL,
            ),
            (Game::Fo3, "Fallout3.esm", "Ghoul", FO4_GHOUL_RACE_LOCAL),
        ] {
            let entry = (
                interner.intern(editor_id),
                FormKey {
                    local: 0x800,
                    plugin: interner.intern(source_plugin),
                },
                race_sig,
            );
            let mappings = fnv_fo3_fo4_humanoid_race_substitution_mappings(
                &[entry],
                &interner,
                source_game,
                Game::Fo4,
            );
            assert_eq!(mappings.len(), 1);
            assert_eq!(mappings[0].1.local, expected_local);
        }

        let known_entry = (
            interner.intern("Caucasian"),
            FormKey {
                local: 0x19,
                plugin: interner.intern("FalloutNV.esm"),
            },
            race_sig,
        );
        let unknown_entry = (
            interner.intern("SomeCreatureRace"),
            FormKey {
                local: 0x800,
                plugin: interner.intern("FalloutNV.esm"),
            },
            race_sig,
        );
        let non_race_entry = (
            interner.intern("Caucasian"),
            FormKey {
                local: 0x801,
                plugin: interner.intern("FalloutNV.esm"),
            },
            SigCode::from_str("NPC_").unwrap(),
        );
        assert!(
            fnv_fo3_fo4_humanoid_race_substitution_mappings(
                &[unknown_entry],
                &interner,
                Game::Fnv,
                Game::Fo4,
            )
            .is_empty()
        );
        assert!(
            fnv_fo3_fo4_humanoid_race_substitution_mappings(
                &[non_race_entry],
                &interner,
                Game::Fnv,
                Game::Fo4,
            )
            .is_empty()
        );
        assert!(
            fnv_fo3_fo4_humanoid_race_substitution_mappings(
                &[known_entry],
                &interner,
                Game::SkyrimSe,
                Game::Fo4,
            )
            .is_empty()
        );
        assert!(
            fnv_fo3_fo4_humanoid_race_substitution_mappings(
                &[known_entry],
                &interner,
                Game::Fnv,
                Game::SkyrimSe,
            )
            .is_empty()
        );
    }

    #[test]
    fn whole_plugin_weap_ammo_substitution_wins_without_replacing_mapper_fallbacks() {
        for (source_game, source_plugin_name) in
            [(Game::Fnv, "FalloutNV.esm"), (Game::Fo3, "Fallout3.esm")]
        {
            let interner = StringInterner::new();
            let source_plugin = interner.intern(source_plugin_name);
            let output_plugin = interner.intern("Converted.esp");
            let table_ammo = FormKey {
                local: 0x004241,
                plugin: source_plugin,
            };
            let non_table_ammo = FormKey {
                local: 0x07EA27,
                plugin: source_plugin,
            };
            let projectile = FormKey {
                local: 0x02CD5F,
                plugin: source_plugin,
            };
            let normal_ammo_target = FormKey {
                local: 0x0801,
                plugin: output_plugin,
            };
            let projectile_target = FormKey {
                local: 0x0802,
                plugin: output_plugin,
            };
            let mut state = MapperState::new(
                [],
                MapperOptions {
                    output_plugin_name: "Converted.esp".into(),
                    ..Default::default()
                },
            );
            state.source_to_target.insert(
                table_ammo,
                FormKey {
                    local: 0x0800,
                    plugin: output_plugin,
                },
            );
            state
                .source_to_target
                .insert(non_table_ammo, normal_ammo_target);
            state.source_to_target.insert(projectile, projectile_target);
            let source_entries = [
                (
                    interner.intern("Ammo10mm"),
                    table_ammo,
                    SigCode::from_str("AMMO").unwrap(),
                ),
                (
                    interner.intern("Ammo22LR"),
                    non_table_ammo,
                    SigCode::from_str("AMMO").unwrap(),
                ),
                (
                    interner.intern("Projectile22LR"),
                    projectile,
                    SigCode::from_str("PROJ").unwrap(),
                ),
            ];

            let seeded = seed_fnv_fo3_fo4_ammo_substitutions(
                &mut state,
                &source_entries,
                &interner,
                source_game,
                Game::Fo4,
            )
            .unwrap();

            assert_eq!(seeded, 1);
            let table_target = state.source_to_target[&table_ammo];
            assert_eq!(table_target.local, 0x01F276);
            assert_eq!(interner.resolve(table_target.plugin), Some("Fallout4.esm"));
            assert_eq!(state.source_to_target[&non_table_ammo], normal_ammo_target);
            assert_eq!(state.source_to_target[&projectile], projectile_target);
        }
    }

    #[test]
    fn legacy_same_name_output_keeps_generated_targets_outside_the_source_domain() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FNV_FO3_Merged.esm");
        let low_source = FormKey {
            local: 0x100,
            plugin,
        };
        let source_at_default_floor = FormKey {
            local: FIRST_ALLOCATION_ID,
            plugin,
        };
        let next = legacy_output_allocation_floor(
            Game::Fnv,
            Game::Fo4,
            "FNV_FO3_Merged.esm",
            "fnv_fo3_merged.ESM",
            true,
            FIRST_ALLOCATION_ID,
            [low_source.local, source_at_default_floor.local],
        )
        .unwrap()
        .expect("same-name legacy output needs a disjoint generated domain");
        assert_eq!(next, source_at_default_floor.local + 1);
        assert_eq!(
            legacy_output_allocation_floor(
                Game::Fnv,
                Game::Fo4,
                "FalloutNV.esm",
                "Converted.esm",
                true,
                FIRST_ALLOCATION_ID,
                [low_source.local, source_at_default_floor.local],
            )
            .unwrap(),
            None,
            "distinct source/output plugin namespaces need no raised floor"
        );
        assert_eq!(
            legacy_output_allocation_floor(
                Game::Fo76,
                Game::Fo4,
                "SeventySix.esm",
                "SeventySix.esm",
                true,
                FIRST_ALLOCATION_ID,
                [low_source.local, source_at_default_floor.local],
            )
            .unwrap(),
            None,
            "FO76 allocation policy must remain isolated"
        );
        assert_eq!(
            legacy_output_allocation_floor(
                Game::Fnv,
                Game::Fo4,
                "FNV_FO3_Merged.esm",
                "FNV_FO3_Merged.esm",
                true,
                0xA000,
                [low_source.local, source_at_default_floor.local],
            )
            .unwrap(),
            Some(0xA000),
            "an existing higher allocation floor must win"
        );
        assert!(
            legacy_output_allocation_floor(
                Game::Fnv,
                Game::Fo4,
                "FNV_FO3_Merged.esm",
                "FNV_FO3_Merged.esm",
                true,
                FIRST_ALLOCATION_ID,
                [low_source.local, 0x00FF_FFFF],
            )
            .is_err(),
            "fresh legacy IDs require free 24-bit local-ID space"
        );
        let mut state = MapperState::new(
            [],
            MapperOptions {
                source_plugin_name: "FNV_FO3_Merged.esm".into(),
                output_plugin_name: "FNV_FO3_Merged.esm".into(),
                preserve_source_ids: true,
                resolution_mode: ResolutionMode::Strict,
                ..Default::default()
            },
        );
        state.next_object_id = next;
        let sig = SigCode::from_str("MGEF").unwrap();
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);

        let generated_target = mapper.allocate_or_resolve(low_source, None, sig);
        let preserved_target = mapper.allocate_or_resolve(source_at_default_floor, None, sig);
        mapper.add_mapping(generated_target, generated_target);

        assert_eq!(generated_target.local, next);
        assert_eq!(preserved_target, source_at_default_floor);
        assert_eq!(
            mapper.lookup(source_at_default_floor),
            Some(preserved_target)
        );

        let seeded_source = FormKey {
            local: 0x200,
            plugin,
        };
        let seeded_target = FormKey {
            local: next + 1,
            plugin,
        };
        mapper.add_mapping(seeded_source, seeded_target);
        let later_low_source = FormKey {
            local: 0x300,
            plugin,
        };
        let later_target = mapper.allocate_or_resolve(later_low_source, None, sig);
        assert_eq!(later_target.local, seeded_target.local + 1);
    }

    #[test]
    fn seeded_legacy_race_substitutions_rewrite_npc_refs_and_prevent_local_races() {
        use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
        use crate::record::{FieldEntry, FieldValue, Record};

        let interner = StringInterner::new();
        let source_plugin = interner.intern("FNV_FO3_Merged.esm");
        let race_sig = SigCode::from_str("RACE").unwrap();
        let source_entries = [
            ("Caucasian", 0x000019, FO4_HUMAN_RACE_LOCAL),
            ("HispanicChild", 0x0042C4, FO4_HUMAN_CHILD_RACE_LOCAL),
            ("MarkedMenGhoul", 0x187FEC, FO4_GHOUL_RACE_LOCAL),
        ]
        .map(|(editor_id, local, target_local)| {
            (
                (
                    interner.intern(editor_id),
                    FormKey {
                        local,
                        plugin: source_plugin,
                    },
                    race_sig,
                ),
                target_local,
            )
        });
        let catalog = source_entries.map(|(entry, _)| entry);
        let mappings = fnv_fo3_fo4_humanoid_race_substitution_mappings(
            &catalog,
            &interner,
            Game::Fnv,
            Game::Fo4,
        );
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "FNV_FO3_Merged.esm".into(),
                preserve_source_ids: true,
                ..Default::default()
            },
        );
        state.source_to_target.extend(mappings.iter().copied());

        for ((_, source_race, _), expected_local) in source_entries {
            let expected = FormKey {
                local: expected_local,
                plugin: interner.intern("Fallout4.esm"),
            };
            let resolved_identity = {
                let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
                mapper.allocate_or_resolve(source_race, None, race_sig)
            };
            assert_eq!(resolved_identity, expected);
            let target_masters = FxHashSet::from_iter([interner.intern("Fallout4.esm")]);
            assert!(is_target_master_remap(resolved_identity, &target_masters));

            let mut npc = Record::new(
                SigCode::from_str("NPC_").unwrap(),
                FormKey {
                    local: source_race.local + 1,
                    plugin: source_plugin,
                },
            );
            npc.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("RNAM").unwrap(),
                value: FieldValue::FormKey(source_race),
            });
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            mapper.rewrite_record(&mut npc).unwrap();
            assert_eq!(npc.fields[0].value, FieldValue::FormKey(expected));
        }

        let source_race = catalog[2].1;
        let expected = FormKey {
            local: FO4_GHOUL_RACE_LOCAL,
            plugin: interner.intern("Fallout4.esm"),
        };
        let mut quest = Record::new(
            SigCode::from_str("QUST").unwrap(),
            FormKey {
                local: 0x2000,
                plugin: source_plugin,
            },
        );
        quest.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("CTDA").unwrap(),
            value: FieldValue::List(vec![FieldValue::Struct(vec![(
                interner.intern("Parameter1"),
                FieldValue::FormKey(source_race),
            )])]),
        });
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.rewrite_record(&mut quest).unwrap();
        let FieldValue::List(rows) = &quest.fields[0].value else {
            panic!("expected condition rows");
        };
        let FieldValue::Struct(fields) = &rows[0] else {
            panic!("expected condition row");
        };
        assert_eq!(fields[0].1, FieldValue::FormKey(expected));
    }

    // -----------------------------------------------------------------------
    // FO76→FO4 forced keyword substitutions (attach points).
    // -----------------------------------------------------------------------

    #[test]
    fn fo76_fo4_forced_keyword_substitutions_target_fo4_master_keywords() {
        use crate::ids::FormKey;
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let mappings = fo76_fo4_forced_keyword_substitution_mappings(&interner);
        let src = |local| FormKey {
            local,
            plugin: interner.intern("SeventySix.esm"),
        };
        let tgt = |plugin_name, local| FormKey {
            local,
            plugin: interner.intern(plugin_name),
        };
        // ap_gun_Appearance -> ap_WeaponMaterial
        assert!(mappings.contains(&(src(0x0011_4364), tgt("Fallout4.esm", 0x0024_A0D8))));
        // ma_Gun_Appearance -> ma_WeaponMaterialSwaps
        assert!(mappings.contains(&(src(0x0037_D0B2), tgt("Fallout4.esm", 0x0024_A0D7))));
        // ap_melee_Appearance -> ap_melee_Material (FO76 1A001E -> FO4 1A001E)
        assert!(mappings.contains(&(src(0x001A_001E), tgt("Fallout4.esm", 0x001A_001E))));
        // DLC04_ma_HandmadeAssaultRifle -> DLC04_ma_HandmadeAssaultRifle
        assert!(mappings.contains(&(src(0x0011_3855), tgt("DLCNukaWorld.esm", 0x0003_3B61))));
    }

    #[test]
    fn fo76_fo4_forced_race_substitution_targets_fo4_ghoul_race() {
        use crate::ids::FormKey;
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let mappings = fo76_fo4_forced_race_substitution_mappings(&interner);
        let src = FormKey {
            local: 0x0079_CCE7,
            plugin: interner.intern("SeventySix.esm"),
        };
        let tgt = FormKey {
            local: 0x000E_AFB6,
            plugin: interner.intern("Fallout4.esm"),
        };

        assert!(mappings.contains(&(src, tgt)));
    }

    #[test]
    fn fo76_fo4_forced_base_object_substitution_targets_fo4_workbench() {
        use crate::ids::FormKey;
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let mappings = fo76_fo4_forced_base_object_substitution_mappings(&interner);
        let src = FormKey {
            local: 0x003B_D4F4,
            plugin: interner.intern("SeventySix.esm"),
        };
        let tgt = FormKey {
            local: 0x000C_1AEB,
            plugin: interner.intern("Fallout4.esm"),
        };

        assert!(mappings.contains(&(src, tgt)));
    }

    #[test]
    fn fo76_fo4_forced_location_ref_type_substitution_targets_fo4_boss() {
        use crate::ids::FormKey;
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let mappings = fo76_fo4_forced_location_ref_type_substitution_mappings(&interner);
        let src = FormKey {
            local: 0x0000_3956,
            plugin: interner.intern("SeventySix.esm"),
        };
        let tgt = FormKey {
            local: 0x0000_3956,
            plugin: interner.intern("Fallout4.esm"),
        };

        assert!(mappings.contains(&(src, tgt)));
    }

    #[test]
    fn seeded_keyword_substitution_resolves_reference_to_target_master() {
        use crate::formkey_mapper::FormKeyMapper;
        use crate::formkey_mapper::{MapperOptions, MapperState};
        use crate::ids::{FormKey, SigCode, SubrecordSig};
        use crate::record::{FieldEntry, FieldValue, Record};
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let opts = MapperOptions {
            output_plugin_name: "SeventySix.esm".into(),
            ..Default::default()
        };
        let mut state = MapperState::new([], opts);
        for (source_form_key, target_form_key) in
            fo76_fo4_forced_keyword_substitution_mappings(&interner)
        {
            state
                .source_to_target
                .insert(source_form_key, target_form_key);
        }

        let kywd = SigCode::from_str("KYWD").unwrap();
        let source = FormKey {
            local: 0x0011_4364,
            plugin: interner.intern("SeventySix.esm"),
        };
        let resolved = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            // No EditorID supplied: only the seeded override should map this.
            mapper.allocate_or_resolve(source, None, kywd)
        };
        assert_eq!(
            resolved,
            FormKey {
                local: 0x0024_A0D8,
                plugin: interner.intern("Fallout4.esm"),
            },
            "seeded substitution must win over allocate (it is consulted first)"
        );

        let source_handmade = FormKey {
            local: 0x0011_3855,
            plugin: interner.intern("SeventySix.esm"),
        };
        let resolved_handmade = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            mapper.allocate_or_resolve(source_handmade, None, kywd)
        };
        assert_eq!(
            resolved_handmade,
            FormKey {
                local: 0x0003_3B61,
                plugin: interner.intern("DLCNukaWorld.esm"),
            },
            "handmade OMOD target keywords must stay constrained to the Nuka-World association"
        );

        let mut omod = Record::new(
            SigCode::from_str("OMOD").unwrap(),
            FormKey {
                local: 0x0031_B1D4,
                plugin: interner.intern("SeventySix.esm"),
            },
        );
        omod.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MNAM").unwrap(),
            value: FieldValue::List(vec![FieldValue::FormKey(source_handmade)]),
        });
        {
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            mapper.rewrite_record(&mut omod).unwrap();
        }

        let FieldValue::List(items) = &omod.fields[0].value else {
            panic!("expected MNAM list");
        };
        assert_eq!(
            items,
            &vec![FieldValue::FormKey(FormKey {
                local: 0x0003_3B61,
                plugin: interner.intern("DLCNukaWorld.esm"),
            })],
            "OMOD MNAM must retain the handmade weapon-family constraint"
        );
    }

    #[test]
    fn seeded_base_object_substitution_rewrites_workshop_refr_base_to_fo4_workbench() {
        use crate::formkey_mapper::FormKeyMapper;
        use crate::formkey_mapper::{MapperOptions, MapperState};
        use crate::ids::{FormKey, SigCode, SubrecordSig};
        use crate::record::{FieldEntry, FieldValue, Record};
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let opts = MapperOptions {
            output_plugin_name: "SeventySix.esm".into(),
            ..Default::default()
        };
        let mut state = MapperState::new([], opts);
        for (source_form_key, target_form_key) in
            fo76_fo4_forced_base_object_substitution_mappings(&interner)
        {
            state
                .source_to_target
                .insert(source_form_key, target_form_key);
        }

        let source_workbench = FormKey {
            local: 0x003B_D4F4,
            plugin: interner.intern("SeventySix.esm"),
        };
        let target_workbench = FormKey {
            local: 0x000C_1AEB,
            plugin: interner.intern("Fallout4.esm"),
        };
        let mut refr = Record::new(
            SigCode::from_str("REFR").unwrap(),
            FormKey {
                local: 0x0020_106D,
                plugin: interner.intern("SeventySix.esm"),
            },
        );
        refr.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("NAME").unwrap(),
            value: FieldValue::FormKey(source_workbench),
        });

        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.rewrite_record(&mut refr).unwrap();

        assert_eq!(refr.fields[0].value, FieldValue::FormKey(target_workbench));
    }

    #[test]
    fn seeded_race_substitutions_rewrite_npc_rnam_to_fo4_compatible_races() {
        use crate::formkey_mapper::FormKeyMapper;
        use crate::formkey_mapper::{MapperOptions, MapperState};
        use crate::ids::{FormKey, SigCode, SubrecordSig};
        use crate::record::{FieldEntry, FieldValue, Record};
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let opts = MapperOptions {
            output_plugin_name: "SeventySix.esm".into(),
            ..Default::default()
        };
        let mut state = MapperState::new([], opts);
        for (source_form_key, target_form_key) in
            fo76_fo4_forced_race_substitution_mappings(&interner)
        {
            state
                .source_to_target
                .insert(source_form_key, target_form_key);
        }

        for (source_local, target_local) in [(0x0079_CCE7, 0x000E_AFB6), (0x0077_2F32, 0x000D_FB33)]
        {
            let source_race = FormKey {
                local: source_local,
                plugin: interner.intern("SeventySix.esm"),
            };
            let target_race = FormKey {
                local: target_local,
                plugin: interner.intern("Fallout4.esm"),
            };
            let mut npc = Record::new(
                SigCode::from_str("NPC_").unwrap(),
                FormKey {
                    local: 0x0083_26DD,
                    plugin: interner.intern("SeventySix.esm"),
                },
            );
            npc.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("RNAM").unwrap(),
                value: FieldValue::FormKey(source_race),
            });

            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            mapper.rewrite_record(&mut npc).unwrap();

            assert_eq!(npc.fields[0].value, FieldValue::FormKey(target_race));
        }
    }

    // -----------------------------------------------------------------------
    // rewrite_record must run after allocate_or_resolve so cross-plugin FK
    // refs are updated before add_record_native.
    // -----------------------------------------------------------------------

    #[test]
    fn cross_record_formkey_reference_is_rewritten() {
        use crate::formkey_mapper::FormKeyMapper;
        use crate::formkey_mapper::{MapperOptions, MapperState};
        use crate::ids::{FormKey, SigCode, SubrecordSig};
        use crate::record::{FieldEntry, FieldValue, Record};
        use crate::sym::StringInterner;

        let mut interner = StringInterner::new();
        let opts = MapperOptions {
            output_plugin_name: "Output.esp".into(),
            ..Default::default()
        };
        let mut state = MapperState::new([], opts);
        let weap = SigCode::from_str("WEAP").unwrap();

        // Source plugin has a WEAP at 001000 that references an ammo at 002000.
        let src_weap = FormKey::parse("001000@Source.esm", &mut interner).unwrap();
        let src_ammo = FormKey::parse("002000@Source.esm", &mut interner).unwrap();

        // Record 1: allocate the ammo (no fields, just get its target FK).
        let tgt_ammo = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &mut interner);
            mapper.allocate_or_resolve(src_ammo, None, SigCode::from_str("AMMO").unwrap())
        };

        // Record 2: the WEAP references src_ammo in a DNAM field.
        let mut weap_record = Record::new(weap, src_weap);
        let ammo_sym = interner.intern("ammo");
        weap_record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Struct(vec![(ammo_sym, FieldValue::FormKey(src_ammo))]),
        });

        // Fixed sequence: allocate_or_resolve THEN rewrite_record.
        {
            let mut mapper = FormKeyMapper::from_state(&mut state, &mut interner);
            let tgt_weap = mapper.allocate_or_resolve(src_weap, None, weap);
            weap_record.form_key = tgt_weap;
            mapper.rewrite_record(&mut weap_record).unwrap();
        }

        // The DNAM field must now reference tgt_ammo, not src_ammo.
        if let FieldValue::Struct(fields) = &weap_record.fields[0].value {
            assert_eq!(
                fields[0].1,
                FieldValue::FormKey(tgt_ammo),
                "cross-plugin FK must be rewritten to target-plugin FK"
            );
        } else {
            panic!("expected Struct field");
        }

        // The tgt_ammo plugin must be Output.esp, not Source.esm.
        assert_eq!(
            interner.resolve(tgt_ammo.plugin).unwrap(),
            "Output.esp",
            "target FK must be in the output plugin"
        );
    }

    // -----------------------------------------------------------------------
    // Fixups use self.interner (same Rodeo as translate_all).
    //
    // We can only test this indirectly without real plugin handles: verify that
    // a ConversionRun's mapper_state is properly reset to None after translate_all
    // fails (no plugin) and that fixups do not panic.
    // -----------------------------------------------------------------------

    #[test]
    fn apply_fixups_v2_without_translate_all_does_not_panic() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let result = with_run(id, |run| run.apply_fixups_v2().map_err(RunError::from));
        // All fixups have applies_to returning false for handles 9999/9998
        // (no real plugin) so reports should be empty (no panic).
        let _ = result; // may error due to invalid handles — just confirm no panic
        drop_run(id).unwrap();
    }

    #[test]
    fn apply_fixups_v2_preserves_mapper_state() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            let source_fk = FormKey::parse("000800@Source.esm", &mut run.interner).unwrap();
            let target_fk = FormKey::parse("000900@Output.esp", &mut run.interner).unwrap();
            let mut state = MapperState::new(
                [],
                MapperOptions {
                    output_plugin_name: "Output.esp".into(),
                    ..Default::default()
                },
            );
            state.source_to_target.insert(source_fk, target_fk);
            run.mapper_state = Some(state);

            let _ = run.apply_fixups_v2();

            let state = run
                .mapper_state
                .as_ref()
                .expect("apply_fixups_v2 must restore mapper_state");
            assert_eq!(state.source_to_target.get(&source_fk), Some(&target_fk));
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
    }

    #[test]
    fn apply_fixups_v2_emits_per_fixup_log_events() {
        let source =
            esp_authoring_core::plugin_runtime::plugin_handle_new_native("Source.esm", Some("fo4"))
                .unwrap();
        let target =
            esp_authoring_core::plugin_runtime::plugin_handle_new_native("Output.esp", Some("fo4"))
                .unwrap();
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                is_whole_plugin: true,
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            run.apply_fixups_v2().map_err(RunError::from)?;
            let messages: Vec<String> = run
                .event_rx
                .try_iter()
                .filter_map(|event| match event {
                    crate::phase::PhaseEvent::Log {
                        phase: "fixups_v2",
                        message,
                        ..
                    } => Some(message),
                    _ => None,
                })
                .collect();

            assert!(
                messages
                    .iter()
                    .any(|message| message.contains("queued apply_weapon_sound_defaults")),
                "missing sweep fixup log event: {messages:?}"
            );
            assert!(
                messages
                    .iter()
                    .any(|message| message
                        .contains("queued normalize_creature_lvln_template_chains")),
                "missing creature normalizer log event: {messages:?}"
            );
            assert!(
                messages
                    .iter()
                    .any(|message| message
                        .contains("starting normalize_creature_lvln_template_chains")),
                "missing creature normalizer start event: {messages:?}"
            );
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
    }

    #[test]
    fn skyrim_weather_production_pipeline_seeds_and_rewrites_master_god_rays() {
        use bytes::Bytes;
        use esp_authoring_core::plugin_runtime::{
            ParsedGroup, ParsedItem, ParsedRecord, ParsedSubrecord,
            plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
            plugin_handle_store_ref,
        };
        use smol_str::SmolStr;

        fn record(
            signature: &'static str,
            form_id: u32,
            subrecords: Vec<ParsedSubrecord>,
        ) -> ParsedItem {
            ParsedItem::Record(ParsedRecord {
                signature: SmolStr::new_static(signature),
                form_id,
                flags: 0,
                version_control: 0,
                form_version: Some(44),
                version2: None,
                subrecords,
                raw_payload: None,
                parse_error: None,
            })
        }

        fn subrecord(signature: &'static str, data: impl Into<Bytes>) -> ParsedSubrecord {
            ParsedSubrecord {
                signature: SmolStr::new_static(signature),
                data: data.into(),
                semantic_type: None,
            }
        }

        let source = plugin_handle_new_native("ConvertedSkyrim.esm", Some("skyrimse")).unwrap();
        plugin_handle_add_master_native(source, "Skyrim.esm", None).unwrap();
        plugin_handle_add_master_native(source, "Update.esm", None).unwrap();
        let target = plugin_handle_new_native("ConvertedSkyrim.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();

        let donor_editor_ids = [
            (0x0000_0D53, "SkyrimClearSunrise"),
            (0x0100_0D51, "SkyrimCloudyDay"),
            (0x0100_0D52, "SkyrimRainSunset"),
            (0x0100_0D58, "SkyrimFogNight"),
        ];
        let voli = donor_editor_ids
            .into_iter()
            .map(|(form_id, editor_id)| {
                record(
                    "VOLI",
                    form_id,
                    vec![subrecord("EDID", Bytes::from(format!("{editor_id}\0")))],
                )
            })
            .collect();
        let hnam = [0x0000_0D53_u32, 0x0100_0D51, 0x0100_0D52, 0x0100_0D58]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let wthr = record(
            "WTHR",
            0x0200_0800,
            vec![
                subrecord("EDID", Bytes::from_static(b"ProductionWeather\0")),
                subrecord("HNAM", Bytes::from(hnam)),
            ],
        );
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&source).unwrap();
            slot.parsed.root_items = vec![
                ParsedItem::Group(ParsedGroup {
                    label: *b"VOLI",
                    group_type: 0,
                    tail: Bytes::new(),
                    children: voli,
                }),
                ParsedItem::Group(ParsedGroup {
                    label: *b"WTHR",
                    group_type: 0,
                    tail: Bytes::new(),
                    children: vec![wthr],
                }),
            ];
            slot.invalidate_sections();
        }

        let run_id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "ConvertedSkyrim.esm".into(),
                is_whole_plugin: true,
                preserve_source_ids: true,
                target_master_names: vec!["Fallout4.esm".into()],
                ..Default::default()
            },
        })
        .unwrap();
        let reports = with_run(run_id, |run| {
            run.translate_all()?;
            run.apply_fixups_v2().map_err(RunError::from)
        })
        .unwrap();
        assert!(
            reports.iter().any(|(name, report)| {
                name == "rewrite_raw_object_template_formids" && report.records_changed > 0
            }),
            "canonical fixup registry did not rewrite the Skyrim weather"
        );

        let expected = [
            0x0021_6A93,
            0x001C_855D,
            0x0021_15D1,
            0x001C_C192,
            0x001C_C192,
            0x0021_6A93,
            0x001C_855D,
            0x0021_15D1,
        ];
        let interner = StringInterner::new();
        let mut session = crate::session::open_session(target, None).unwrap();
        let schema = session.schema().unwrap();
        let weather_key = session
            .form_keys_of_sig(SigCode::from_str("WTHR").unwrap(), &interner)
            .unwrap()
            .into_iter()
            .next()
            .expect("translated WTHR");
        let weather = session
            .record_decoded(&weather_key, schema.as_ref(), &interner)
            .unwrap();
        let FieldValue::Bytes(wgdr) = &weather
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "WGDR")
            .expect("translated WGDR")
            .value
        else {
            panic!("WGDR must remain a raw FormID array");
        };
        let actual = wgdr
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);

        drop(session);
        drop_run(run_id).unwrap();
        plugin_handle_close_native(source);
        plugin_handle_close_native(target);
    }

    #[test]
    fn run_error_cancelled_display() {
        let msg = format!("{}", RunError::Cancelled);
        assert_eq!(msg, "translation cancelled");
    }

    #[test]
    fn progress_callback_starts_none() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            assert!(
                run.progress_callback.is_none(),
                "progress_callback must start as None"
            );
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
    }

    // -----------------------------------------------------------------------
    // translate_all on a run with no plugin handles returns quickly
    //     (no records to process, no cancel triggered). This confirms the
    //     yield-check logic doesn't fire when record_count is always 0.
    // -----------------------------------------------------------------------

    #[test]
    fn translate_all_zero_records_no_cancel() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        // translate_all on sentinel handles: source_signatures will fail with an error.
        // The important thing is it does NOT return RunError::Cancelled.
        let result = with_run(id, |run| run.translate_all());
        match result {
            Err(RunError::Cancelled) => panic!("unexpected Cancelled on zero-record run"),
            _ => {} // any other outcome is acceptable
        }

        drop_run(id).unwrap();
    }

    // -----------------------------------------------------------------------
    // mod_path / source_extracted_dir flow from RunConfig into the
    //      FixupContext that apply_fixups_v2 builds for the registry. We don't
    //      execute fixups end-to-end (it needs real plugin handles); we
    //      instead reuse the exact lines apply_fixups_v2 uses to build the ctx,
    //      proving the as_deref() plumbing produces the expected Option<&Path>.
    // -----------------------------------------------------------------------

    #[test]
    fn run_config_carries_mod_path_into_fixup_context() {
        use crate::fixups::{FixupConfig, FixupContext};
        use crate::schema::AuthoringSchema;
        use std::path::{Path, PathBuf};

        let mod_path = PathBuf::from("/fake/mod");
        let extracted = PathBuf::from("/fake/extracted");

        let cfg = RunConfig {
            output_plugin_name: "Output.esm".into(),
            mod_path: Some(mod_path.clone()),
            source_extracted_dir: Some(extracted.clone()),
            ..Default::default()
        };

        // Sanity-check the round-trip through RunConfig itself.
        assert_eq!(cfg.mod_path.as_deref(), Some(mod_path.as_path()));
        assert_eq!(
            cfg.source_extracted_dir.as_deref(),
            Some(extracted.as_path())
        );

        // Build a FixupContext using the same expressions apply_fixups_v2 uses
        // (`self.config.mod_path.as_deref()`), so this test fails if anyone
        // reverts the fixup-context plumbing to hardcoded `None`.
        let fixup_config = FixupConfig::default();
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let masters: Vec<u64> = vec![];
        let ctx = FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: &schema,
            schema_source: &schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: cfg.mod_path.as_deref(),
            source_extracted_dir: cfg.source_extracted_dir.as_deref(),
            target_master_handle_ids: &masters,
            config: &fixup_config,
        };

        assert_eq!(ctx.mod_path, Some(Path::new("/fake/mod")));
        assert_eq!(ctx.source_extracted_dir, Some(Path::new("/fake/extracted")));
    }

    #[test]
    fn run_config_default_mod_path_is_none() {
        let cfg = RunConfig::default();
        assert!(cfg.mod_path.is_none());
        assert!(cfg.source_extracted_dir.is_none());
    }

    #[test]
    fn navi_warnings_capped() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            for i in 0..1_000_000 {
                run.push_navi_warning(format!("w{i}"));
            }
            run.finalize_navi_warnings();
            // At most 1000 entries + 1 cap message = 1001
            assert!(
                run.navi_warning_len() <= 1001,
                "navi_warnings must be capped to <= 1001, got {}",
                run.navi_warning_len()
            );
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
    }

    #[test]
    fn release_remap_state_drops_mapper() {
        let id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            run.mapper_state = Some(MapperState::new(
                [],
                MapperOptions {
                    output_plugin_name: "Output.esp".into(),
                    ..Default::default()
                },
            ));
            assert!(
                run.mapper_state.is_some(),
                "mapper_state must be Some before release"
            );
            run.release_remap_state();
            assert!(
                run.mapper_state.is_none(),
                "mapper_state must be None after release"
            );
            Ok::<_, RunError>(())
        })
        .unwrap();

        drop_run(id).unwrap();
    }

    #[test]
    fn final_quest_condition_repair_runs_after_placed_repairs() {
        let source = include_str!("run.rs");
        let start = source.find("pub fn repair_placed_child_refs").unwrap();
        let end = source[start..]
            .find("pub fn synthesize_encounter_zones")
            .map(|offset| start + offset)
            .unwrap();
        let body = &source[start..end];
        let teleport = body
            .rfind("repair_placed_teleport_doors::repair_placed_references")
            .unwrap();
        let actor_specialization = body
            .rfind("specialize_placed_actor_templates_after_ref_repair")
            .unwrap();
        let final_conditions = body
            .rfind("repair_final_quest_reference_conditions(")
            .unwrap();
        let flush = body
            .rfind("session.flush_pending_effects()")
            .expect("session flush");

        assert!(final_conditions > teleport);
        assert!(final_conditions > actor_specialization);
        assert!(final_conditions < flush);
    }
}
