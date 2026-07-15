//! Fixup trait and registry for the FO76→FO4 conversion pipeline.
//!
//! A `Fixup` is a single post-translation pass that inspects or rewrites
//! records in the target plugin (via the `ConversionRun` handle). The
//! `FixupRegistry` runs them in registration order, with optional convergence
//! looping for fixups that declare `convergent() == true`.
//!
//! Lifecycle:
//!   1. Caller builds a `FixupContext` (owns `source_handle_id`,
//!      `target_handle_id`, path hints, config).
//!   2. Caller holds `FormKeyMapper` separately — the mapper owns the
//!      `&StringInterner` and fixups reach it via `mapper.interner`.
//!   3. `FixupRegistry::run_all(ctx, mapper)` iterates fixups in order.

pub mod apply_fo76_workshop_catalog;
pub mod apply_weapon_sound_defaults;
pub mod backfill_placed_loc_ref_types;
pub mod clean_leveled_item_entries;
pub mod clear_interior_hand_changed;
pub mod creature;
pub mod drop_incompatible_player_idles;
pub mod drop_untranslatable_loadscreen_records;
pub mod encounter_zones;
pub mod expand_arma_races_from_armor_race;
pub mod face;
pub mod filter_non_vanilla_races_for_weapon_roots;
pub mod fix_invalid_target_formkeys;
pub mod fix_stag_sound_refs;
pub mod fix_water_spell_refs;
pub mod flatten_omod_includes;
pub mod gate_runtime_controlled_placed_refs;
pub mod harvest_modt;
pub mod havok;
pub mod inject_cobjs_for_omods;
pub mod inject_required_child_blocks;
pub mod inject_weap_extra_data;
pub mod ltex_txst_synth;
pub(crate) mod mark_public_wastelanders_hubs;
pub mod normalize_fo76_pack_templates;
pub mod normalize_fo76_weather;
pub mod normalize_placed_light_radius;
pub mod normalize_placed_records;
pub mod null_dangling_misc_refs;
pub mod null_dangling_own_plugin_refs;
pub mod null_dangling_vmad_refs;
pub mod null_invalid_qust_alla_keywords;
pub mod preserve_packin_storage_cells;
pub mod prune_faction_relations;
pub mod prune_orphaned_records;
pub mod recover_fo76_leveled_list_values;
pub mod ref_index;
pub mod remap_idle_anchor_actions;
pub mod remap_light_gobo_to_fo4_base;
pub mod remap_struct_internal_formids;
pub mod repair_placed_linked_refs;
pub mod repair_placed_teleport_doors;
pub mod repair_scen_htid_sound_refs;
pub mod resolve_addon_node_indices;
pub mod resolve_injected_stub_refs;
pub mod resolve_placed_leveled_bases;
pub mod restrict_translated_npc_for_slice;
pub mod rewrite_raw_lctn_formids;
pub mod rewrite_raw_object_template_formids;
pub mod rewrite_raw_wrld_large_refs;
pub mod sky_regions;
pub mod strip_atx_cobj_conditions;
pub mod strip_invalid_quest_condition_params;
pub mod strip_orphan_race_properties;
pub mod strip_perk_leveled_lists_from_containers;
pub mod stub_injection;
pub mod sweep_unmapped_formkeys;
pub mod sync_armo_hand_slots_from_addons;
pub mod synthesize_weap_data_blocks;
pub mod synthesize_workshop_boundaries;
pub mod validate_reference_target_types;
pub mod vendor_dialogue;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope, FixupStatus, FullPluginRunState};
use crate::schema::AuthoringSchema;
use crate::session::{PluginSession, open_session};
use crate::sym::Sym;

// ---------------------------------------------------------------------------
// FixupConfig
// ---------------------------------------------------------------------------

/// Configuration that flows through every fixup in a single conversion run.
#[derive(Default, Clone)]
pub struct FixupConfig {
    /// Abort immediately when a structural problem is detected instead of
    /// demoting it to a warning.
    pub strict: bool,
    /// Prefer reusing source object-ids in the output plugin where possible.
    pub preserve_source_ids: bool,
    /// Allow fixups to pull in base-game assets (e.g. vanilla FO4 records) to
    /// satisfy references that have no direct FO76 equivalent.
    pub use_base_game_assets: bool,
    /// True when this is a whole-plugin conversion (all records from a plugin,
    /// not a per-record sub-graph conversion). Fixups that are only meaningful
    /// for per-record conversions (e.g. `prune_orphaned_records`) skip
    /// themselves when this flag is set.
    pub is_whole_plugin: bool,
    /// The 4-byte record signature of the conversion root (e.g. `NPC_`,
    /// `LVLN`). Creature-only fixups check this to decide whether to run.
    /// `None` means unknown / whole-plugin (no single root type).
    pub root_sig: Option<crate::ids::SigCode>,
    /// Set of source-record signatures the translator skips entirely.
    pub skip_record_sigs: rustc_hash::FxHashSet<String>,
    /// Optional path to the mod directory being converted.
    pub mod_path: Option<std::path::PathBuf>,
    /// Optional path to the directory holding extracted source assets.
    pub source_extracted_dir: Option<std::path::PathBuf>,
    /// Optional path to the directory holding extracted FO4 base-game assets.
    /// Used by fixups that may substitute a vanilla asset for a FO76 reference
    /// (e.g. `remap_light_gobo_to_fo4_base`).
    pub target_extracted_dir: Option<std::path::PathBuf>,
    /// Handle IDs of every master plugin loaded for the target game.
    pub target_master_handle_ids: Vec<u64>,
    /// Schema for the target game.
    pub target_schema: Option<Arc<AuthoringSchema>>,
    /// Schema for the source game.
    pub source_schema: Option<Arc<AuthoringSchema>>,
    /// Whole-plugin asset phases that are allowed to run during fixups.
    pub asset_phases: AssetPhaseFlags,
    /// True only for whole-plugin FO76→FO4 worldspace runs, where the phase-6
    /// cell-slice copy re-inserts exterior placed children (ACHR/REFR/...) AFTER
    /// the fixup phase. When set, pre-copy fixups defer the placed-ref-target
    /// class (LCTN LCUN/LCEP/ACEP) and the raw LCTN special-ref arrays
    /// (LCPR/LCSR) because their targets are not yet present. The authoritative
    /// resolution runs post-copy via
    /// `ConversionRun::repair_placed_child_refs`. False for every other pipeline
    /// (the pre-copy pass resolves the whole class as before).
    pub defer_placed_child_ref_class: bool,
}

// ---------------------------------------------------------------------------
// FixupContext
// ---------------------------------------------------------------------------

/// All mutable state a single fixup invocation may need, except `FormKeyMapper`
/// which is passed separately to `FixupRegistry::run_all` to avoid nested
/// lifetime issues (`FormKeyMapper<'a>` already borrows a `StringInterner`).
pub struct FixupContext<'a> {
    /// Handle ID of the source plugin (FO76).
    pub source_handle_id: u64,
    /// Handle ID of the output/target plugin (FO4).
    pub target_handle_id: u64,
    /// Schema for the target game (FO4).
    pub schema_target: &'a Arc<AuthoringSchema>,
    /// Schema for the source game (read-side).
    pub schema_source: &'a Arc<AuthoringSchema>,
    /// Set of source-record signatures the translator skips entirely.
    pub skip_record_sigs: &'a rustc_hash::FxHashSet<String>,
    /// Optional path to the mod directory being converted.
    pub mod_path: Option<&'a Path>,
    /// Optional path to the directory holding extracted source assets.
    pub source_extracted_dir: Option<&'a Path>,
    /// Handle IDs of every master plugin loaded for the target game.
    pub target_master_handle_ids: &'a [u64],
    /// Run-level configuration.
    pub config: &'a FixupConfig,
}

/// Empty skip-records set, useful for unit tests that don't need a real
/// translator. Hand to `FixupContext::skip_record_sigs` for test literals.
pub fn empty_skip_record_sigs() -> &'static rustc_hash::FxHashSet<String> {
    use std::sync::OnceLock;
    static EMPTY: OnceLock<rustc_hash::FxHashSet<String>> = OnceLock::new();
    EMPTY.get_or_init(rustc_hash::FxHashSet::default)
}

// ---------------------------------------------------------------------------
// FixupReport
// ---------------------------------------------------------------------------

/// Summary of what one fixup invocation did.
#[derive(Clone, Debug)]
pub struct FixupReport {
    pub records_changed: u32,
    pub records_dropped: u32,
    pub records_added: u32,
    pub warnings: Vec<Sym>,
    pub elapsed_ms: u64,
    pub iteration: u32,
    pub status: FixupStatus,
    pub scope: FixupScope,
    pub message: Option<Sym>,
    /// AddonNode `NodeIndex` reassignments `(old_index, new_index)` produced by
    /// `resolve_addon_node_indices`. Harvested by `apply_fixups_py` into the
    /// run's decision channel so the NIF phase can repoint `BSValueNode` blocks.
    /// Empty for every other fixup.
    pub addon_index_remap: Vec<(i64, i64)>,
}

impl FixupReport {
    /// Returns `true` when the fixup made no structural changes (convergence
    /// check: a convergent fixup stops looping when `is_no_op()` is `true`).
    pub fn is_no_op(&self) -> bool {
        self.records_changed + self.records_dropped + self.records_added == 0
    }

    /// Construct an empty (no-op) report.
    pub fn empty() -> Self {
        Self {
            records_changed: 0,
            records_dropped: 0,
            records_added: 0,
            warnings: Vec::new(),
            elapsed_ms: 0,
            iteration: 1,
            status: FixupStatus::Ran,
            scope: FixupScope::WholePluginSafe,
            message: None,
            addon_index_remap: Vec::new(),
        }
    }
}

fn skipped_report(scope: FixupScope, message: Sym) -> FixupReport {
    let mut report = FixupReport::empty();
    report.status = FixupStatus::Skipped;
    report.scope = scope;
    report.message = Some(message);
    report.iteration = 0;
    report
}

fn whole_plugin_skip_message(
    fixup: &dyn Fixup,
    config: &FixupConfig,
) -> Option<(FixupScope, String)> {
    if !config.is_whole_plugin {
        return None;
    }

    let scope = fixup.scope();
    let asset_allowed =
        matches!(scope, FixupScope::AssetOnly) && fixup.asset_phase_allowed(&config.asset_phases);
    let should_skip = matches!(scope, FixupScope::GraphOnly | FixupScope::DisabledPending)
        || (matches!(scope, FixupScope::AssetOnly) && !asset_allowed);

    should_skip.then(|| {
        (
            scope,
            format!("fixup_skipped:{}:{}", fixup.name(), scope.as_str()),
        )
    })
}

// ---------------------------------------------------------------------------
// FixupError
// ---------------------------------------------------------------------------

/// Errors that abort a fixup run.
#[derive(Debug)]
pub enum FixupError {
    /// A convergent fixup did not reach a fixed point within the maximum
    /// iteration budget (64 iterations). The `&'static str` is the fixup name.
    ConvergenceFailure(&'static str),
    /// A handle could not be resolved (e.g. plugin no longer loaded).
    HandleError(String),
    /// The target schema rejected an operation.
    SchemaError(String),
    /// Caller signalled cancellation via the shared `AtomicBool`.
    Cancelled,
    /// Any other unrecoverable fixup error.
    Other(String),
}

impl std::fmt::Display for FixupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FixupError::ConvergenceFailure(name) => {
                write!(f, "fixup '{name}' failed to converge within 64 iterations")
            }
            FixupError::HandleError(msg) => write!(f, "handle error: {msg}"),
            FixupError::SchemaError(msg) => write!(f, "schema error: {msg}"),
            FixupError::Cancelled => write!(f, "fixup run cancelled"),
            FixupError::Other(msg) => write!(f, "fixup error: {msg}"),
        }
    }
}

impl std::error::Error for FixupError {}

// ---------------------------------------------------------------------------
// Fixup trait
// ---------------------------------------------------------------------------

/// A single post-translation fixup pass.
///
/// Implementors must be `Send + Sync` so the registry can be stored in
/// shared contexts (e.g. `Arc<FixupRegistry>`).
///
/// `FormKeyMapper` is accepted directly by `run` (not via `FixupContext`) to
/// sidestep nested lifetime conflicts — see module-level docs.
pub trait Fixup: Send + Sync {
    /// Stable identifier for this fixup, used in reports and error messages.
    fn name(&self) -> &'static str;

    /// Scope used to decide whether this fixup can run for whole-plugin conversion.
    fn scope(&self) -> FixupScope {
        FixupScope::WholePluginSafe
    }

    /// Return true when this asset-only fixup may run for the enabled asset phases.
    fn asset_phase_allowed(&self, _phases: &AssetPhaseFlags) -> bool {
        false
    }

    /// Return `true` when this fixup should run for the given context.
    ///
    /// Called once per fixup per `run_all` invocation, before any iteration.
    fn applies_to(&self, _ctx: &FixupContext) -> bool {
        true
    }

    /// Execute one pass of the fixup.
    ///
    /// May mutate records reachable through `ctx.target_handle_id` and
    /// consult `mapper` for FormKey resolution.
    fn run(
        &self,
        _ctx: &mut FixupContext,
        _mapper: &mut FormKeyMapper,
    ) -> Result<FixupReport, FixupError> {
        Err(FixupError::Other(format!(
            "fixup '{}' does not implement the legacy API",
            self.name()
        )))
    }

    /// Return `true` when this fixup should execute through `PluginSession`.
    fn uses_session(&self) -> bool {
        false
    }

    /// Return `true` when this fixup should run for the given session/config.
    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        self.uses_session()
    }

    /// Execute one pass of the fixup through the held-lock session API.
    fn run_with_session(
        &self,
        _session: &mut PluginSession,
        _mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        Err(FixupError::Other(format!(
            "fixup '{}' does not implement the session API",
            self.name()
        )))
    }

    /// Optional whole-plugin worklist override for fixups whose generic
    /// session implementation would scan the full target plugin.
    fn run_full_plugin_worklist(
        &self,
        _session: &mut PluginSession,
        _mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
        _state: &FullPluginRunState,
    ) -> Result<Option<FixupReport>, FixupError> {
        Ok(None)
    }

    /// When `true`, the registry re-runs this fixup until `FixupReport::is_no_op()`
    /// or the 64-iteration budget is exhausted.
    ///
    /// Use for fixups whose output feeds into their own input (e.g. transitive
    /// FormKey resolution).
    fn convergent(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// FixupRegistry
// ---------------------------------------------------------------------------

/// Ordered collection of fixups.  Run them in registration order via
/// `run_all`.
pub struct FixupRegistry {
    in_run_order: Vec<Box<dyn Fixup>>,
}

impl FixupRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            in_run_order: Vec::new(),
        }
    }

    /// Append a fixup to the end of the run list.
    pub fn register(&mut self, f: Box<dyn Fixup>) {
        self.in_run_order.push(f);
    }

    /// Registered fixup names, in run order.
    pub fn fixup_names(&self) -> Vec<&'static str> {
        self.in_run_order.iter().map(|f| f.name()).collect()
    }

    /// Run every applicable fixup in order, returning a report per fixup.
    ///
    /// Convergent fixups loop until their report is a no-op or the iteration
    /// budget (64) is exhausted.  A budget overrun returns
    /// `Err(FixupError::ConvergenceFailure)`.
    pub fn run_all_in_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<Vec<(String, FixupReport)>, FixupError> {
        self.run_all_in_session_with_progress(
            session,
            mapper,
            config,
            |_name, _iteration, _status, _report| {},
        )
    }

    /// Run every session-capable fixup using the same refresh model as
    /// `run_all_with_progress`.
    ///
    /// The passed session is reopened before returning so callers can keep
    /// using it after the helper completes.
    pub fn run_all_in_session_with_progress<F>(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
        progress: F,
    ) -> Result<Vec<(String, FixupReport)>, FixupError>
    where
        F: FnMut(&'static str, u32, &'static str, Option<&FixupReport>),
    {
        self.run_all_in_session_with_progress_and_cancel(session, mapper, config, progress, None)
    }

    /// Same as `run_all_in_session_with_progress` but checks `cancel` between
    /// fixups and between iterations of a convergent fixup.
    pub fn run_all_in_session_with_progress_and_cancel<F>(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
        mut progress: F,
        cancel: Option<&AtomicBool>,
    ) -> Result<Vec<(String, FixupReport)>, FixupError>
    where
        F: FnMut(&'static str, u32, &'static str, Option<&FixupReport>),
    {
        let mut reports: Vec<(String, FixupReport)> = Vec::new();

        session.flush_pending_effects();
        for fixup in &self.in_run_order {
            if let Some(c) = cancel {
                if c.load(Ordering::Relaxed) {
                    return Err(FixupError::Cancelled);
                }
            }
            if !fixup.uses_session() {
                continue;
            }

            if let Some((scope, message)) = whole_plugin_skip_message(fixup.as_ref(), config) {
                let report = skipped_report(scope, mapper.interner.intern(&message));
                progress(fixup.name(), 0, "skipped", Some(&report));
                reports.push((fixup.name().to_string(), report));
                continue;
            }

            session.flush_pending_effects();
            if !fixup.applies_to_session(session, config) {
                continue;
            }

            let mut iter = 0u32;
            loop {
                if let Some(c) = cancel {
                    if c.load(Ordering::Relaxed) {
                        return Err(FixupError::Cancelled);
                    }
                }
                session.flush_pending_effects();
                let iteration = iter + 1;
                let start = Instant::now();
                progress(fixup.name(), iteration, "started", None);
                let mut report = fixup.run_with_session(session, mapper, config)?;
                report.elapsed_ms = start.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
                report.iteration = iteration;
                report.status = FixupStatus::Ran;
                report.scope = fixup.scope();
                let no_op = report.is_no_op();
                session.flush_pending_effects();
                progress(fixup.name(), iteration, "finished", Some(&report));
                reports.push((fixup.name().to_string(), report));

                if !fixup.convergent() || no_op {
                    break;
                }

                iter += 1;
                if iter > 64 {
                    return Err(FixupError::ConvergenceFailure(fixup.name()));
                }
            }
        }

        Ok(reports)
    }

    /// Run every applicable fixup in order, dispatching to either the
    /// legacy `FixupContext` path or the session API depending on the fixup.
    pub fn run_all(
        &self,
        ctx: &mut FixupContext,
        mapper: &mut FormKeyMapper,
    ) -> Result<Vec<(String, FixupReport)>, FixupError> {
        self.run_all_with_progress(ctx, mapper, |_name, _iteration, _status, _report| {})
    }

    /// Run every applicable fixup and call `progress` before and after each
    /// iteration. Session-capable fixups open a fresh `PluginSession` per
    /// iteration so pending write effects flush before the next pass.
    pub fn run_all_with_progress<F>(
        &self,
        ctx: &mut FixupContext,
        mapper: &mut FormKeyMapper,
        progress: F,
    ) -> Result<Vec<(String, FixupReport)>, FixupError>
    where
        F: FnMut(&'static str, u32, &'static str, Option<&FixupReport>),
    {
        self.run_all_with_progress_and_cancel(ctx, mapper, progress, None)
    }

    /// Same as `run_all_with_progress` but checks `cancel` between fixups
    /// and convergent-fixup iterations. Returns `FixupError::Cancelled` on a
    /// cancelled run.
    pub fn run_all_with_progress_and_cancel<F>(
        &self,
        ctx: &mut FixupContext,
        mapper: &mut FormKeyMapper,
        progress: F,
        cancel: Option<&AtomicBool>,
    ) -> Result<Vec<(String, FixupReport)>, FixupError>
    where
        F: FnMut(&'static str, u32, &'static str, Option<&FixupReport>),
    {
        self.run_all_with_progress_and_cancel_and_full_plugin_state(
            ctx, mapper, progress, cancel, None,
        )
    }

    pub fn run_all_with_progress_and_cancel_and_full_plugin_state<F>(
        &self,
        ctx: &mut FixupContext,
        mapper: &mut FormKeyMapper,
        mut progress: F,
        cancel: Option<&AtomicBool>,
        full_plugin_state: Option<&FullPluginRunState>,
    ) -> Result<Vec<(String, FixupReport)>, FixupError>
    where
        F: FnMut(&'static str, u32, &'static str, Option<&FixupReport>),
    {
        let config = ctx.config;
        let mut reports: Vec<(String, FixupReport)> = Vec::new();

        for fixup in &self.in_run_order {
            if let Some(c) = cancel {
                if c.load(Ordering::Relaxed) {
                    return Err(FixupError::Cancelled);
                }
            }

            if let Some((scope, message)) = whole_plugin_skip_message(fixup.as_ref(), config) {
                let report = skipped_report(scope, mapper.interner.intern(&message));
                progress(fixup.name(), 0, "skipped", Some(&report));
                reports.push((fixup.name().to_string(), report));
                continue;
            }

            if fixup.uses_session() {
                let source_id = Some(ctx.source_handle_id).filter(|id| *id != 0);
                let applies = {
                    let session = open_session(ctx.target_handle_id, source_id)
                        .map_err(|err| FixupError::HandleError(err.to_string()))?;
                    fixup.applies_to_session(&session, config)
                };
                if !applies {
                    continue;
                }

                let mut iter = 0u32;
                loop {
                    let iteration = iter + 1;
                    progress(fixup.name(), iteration, "started", None);
                    let start = Instant::now();
                    let mut report = {
                        let mut session = open_session(ctx.target_handle_id, source_id)
                            .map_err(|err| FixupError::HandleError(err.to_string()))?;
                        if let Some(state) = full_plugin_state.filter(|_| config.is_whole_plugin) {
                            match fixup.run_full_plugin_worklist(
                                &mut session,
                                mapper,
                                config,
                                state,
                            )? {
                                Some(report) => report,
                                None => fixup.run_with_session(&mut session, mapper, config)?,
                            }
                        } else {
                            fixup.run_with_session(&mut session, mapper, config)?
                        }
                    };
                    report.elapsed_ms = start.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
                    report.iteration = iteration;
                    report.status = FixupStatus::Ran;
                    report.scope = fixup.scope();
                    let no_op = report.is_no_op();
                    progress(fixup.name(), iteration, "finished", Some(&report));
                    reports.push((fixup.name().to_string(), report));

                    if !fixup.convergent() || no_op {
                        break;
                    }

                    iter += 1;
                    if iter > 64 {
                        return Err(FixupError::ConvergenceFailure(fixup.name()));
                    }
                }
                continue;
            }

            if !fixup.applies_to(ctx) {
                continue;
            }

            let mut iter = 0u32;
            loop {
                let iteration = iter + 1;
                let start = Instant::now();
                progress(fixup.name(), iteration, "started", None);
                let mut report = fixup.run(ctx, mapper)?;
                report.elapsed_ms = start.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
                report.iteration = iteration;
                report.status = FixupStatus::Ran;
                report.scope = fixup.scope();
                let no_op = report.is_no_op();
                progress(fixup.name(), iteration, "finished", Some(&report));
                reports.push((fixup.name().to_string(), report));

                if !fixup.convergent() || no_op {
                    break;
                }

                iter += 1;
                if iter > 64 {
                    return Err(FixupError::ConvergenceFailure(fixup.name()));
                }
            }
        }

        Ok(reports)
    }
}

impl Default for FixupRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;
    use std::sync::Arc as StdArc;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn create_test_plugin_handle() -> u64 {
        plugin_handle_new_native("FixupRegistryTest.esp", Some("fo4")).expect("test plugin handle")
    }

    fn make_mapper_and_config() -> (StringInterner, MapperState, FixupConfig) {
        let interner = StringInterner::new();
        let config = FixupConfig::default();
        let state = MapperState::new(std::iter::empty(), MapperOptions::default());
        (interner, state, config)
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    struct CountingFixup {
        fixup_name: &'static str,
        calls: AtomicU32,
    }

    impl CountingFixup {
        fn new(name: &'static str) -> Self {
            Self {
                fixup_name: name,
                calls: AtomicU32::new(0),
            }
        }
    }

    impl Fixup for CountingFixup {
        fn name(&self) -> &'static str {
            self.fixup_name
        }

        fn uses_session(&self) -> bool {
            true
        }

        fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
            true
        }

        fn run_with_session(
            &self,
            _session: &mut PluginSession,
            _mapper: &mut FormKeyMapper,
            _config: &FixupConfig,
        ) -> Result<FixupReport, FixupError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(FixupReport::empty())
        }
    }

    #[test]
    fn registry_runs_no_op_fixup_with_session() {
        let target_handle = create_test_plugin_handle();
        let (mapper_interner, mut mapper_state, config) = make_mapper_and_config();
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        let fixup = CountingFixup::new("counting");
        let mut registry = FixupRegistry::new();
        registry.register(Box::new(fixup));

        let reports = registry
            .run_all_in_session(&mut session, &mut mapper, &config)
            .expect("run_all should succeed");

        // One report for the one fixup.
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].0, "counting");
        assert_eq!(reports[0].1.iteration, 1);
        assert!(reports[0].1.is_no_op());
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    /// Fixup that reports a change for the first N calls, then no-op.
    struct ConvergingFixup {
        fixup_name: &'static str,
        /// Total calls remaining that report a change.
        changes_remaining: AtomicU32,
    }

    impl ConvergingFixup {
        fn new(name: &'static str, changes: u32) -> Self {
            Self {
                fixup_name: name,
                changes_remaining: AtomicU32::new(changes),
            }
        }
    }

    impl Fixup for ConvergingFixup {
        fn name(&self) -> &'static str {
            self.fixup_name
        }

        fn uses_session(&self) -> bool {
            true
        }

        fn convergent(&self) -> bool {
            true
        }

        fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
            true
        }

        fn run_with_session(
            &self,
            _session: &mut PluginSession,
            _mapper: &mut FormKeyMapper,
            _config: &FixupConfig,
        ) -> Result<FixupReport, FixupError> {
            let prev = self
                .changes_remaining
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
                    if v > 0 { Some(v - 1) } else { Some(0) }
                })
                .unwrap();

            if prev > 0 {
                Ok(FixupReport {
                    records_changed: 1,
                    ..FixupReport::empty()
                })
            } else {
                Ok(FixupReport::empty())
            }
        }
    }

    #[test]
    fn convergent_fixup_loops_until_no_op() {
        let target_handle = create_test_plugin_handle();
        let (mapper_interner, mut mapper_state, config) = make_mapper_and_config();
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        // Will report changes for 3 calls, then no-op on the 4th.
        let fixup = ConvergingFixup::new("converging", 3);
        let mut registry = FixupRegistry::new();
        registry.register(Box::new(fixup));

        let reports = registry
            .run_all_in_session(&mut session, &mut mapper, &config)
            .expect("should converge");

        // 3 change reports + 1 no-op = 4 total entries.
        assert_eq!(reports.len(), 4);
        assert_eq!(reports[0].1.iteration, 1);
        assert_eq!(reports[3].1.iteration, 4);
        assert!(reports[3].1.is_no_op());
        for report in &reports[..3] {
            assert!(!report.1.is_no_op());
        }
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    /// Fixup that always reports a change — never converges.
    struct NeverConvergesFixup;

    impl Fixup for NeverConvergesFixup {
        fn name(&self) -> &'static str {
            "never_converges"
        }

        fn uses_session(&self) -> bool {
            true
        }

        fn convergent(&self) -> bool {
            true
        }

        fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
            true
        }

        fn run_with_session(
            &self,
            _session: &mut PluginSession,
            _mapper: &mut FormKeyMapper,
            _config: &FixupConfig,
        ) -> Result<FixupReport, FixupError> {
            Ok(FixupReport {
                records_changed: 1,
                ..FixupReport::empty()
            })
        }
    }

    #[test]
    fn convergence_failure_after_64_iters() {
        let target_handle = create_test_plugin_handle();
        let (mapper_interner, mut mapper_state, config) = make_mapper_and_config();
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        let mut registry = FixupRegistry::new();
        registry.register(Box::new(NeverConvergesFixup));

        let result = registry.run_all_in_session(&mut session, &mut mapper, &config);
        assert!(
            matches!(
                result,
                Err(FixupError::ConvergenceFailure("never_converges"))
            ),
            "expected ConvergenceFailure, got: {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    struct NeverAppliesFixup;

    impl Fixup for NeverAppliesFixup {
        fn name(&self) -> &'static str {
            "never_applies"
        }

        fn uses_session(&self) -> bool {
            true
        }

        fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
            false
        }

        fn run_with_session(
            &self,
            _session: &mut PluginSession,
            _mapper: &mut FormKeyMapper,
            _config: &FixupConfig,
        ) -> Result<FixupReport, FixupError> {
            panic!("run() must not be called when applies_to returns false");
        }
    }

    #[test]
    fn applies_to_false_skips_fixup() {
        let target_handle = create_test_plugin_handle();
        let (mapper_interner, mut mapper_state, config) = make_mapper_and_config();
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        let mut registry = FixupRegistry::new();
        registry.register(Box::new(NeverAppliesFixup));

        let reports = registry
            .run_all_in_session(&mut session, &mut mapper, &config)
            .expect("run_all should succeed");
        // Skipped fixup produces no report.
        assert!(reports.is_empty());
    }

    struct HavokAssetFixup {
        calls: StdArc<AtomicU32>,
    }

    impl Fixup for HavokAssetFixup {
        fn name(&self) -> &'static str {
            "havok_asset"
        }

        fn scope(&self) -> FixupScope {
            FixupScope::AssetOnly
        }

        fn asset_phase_allowed(&self, phases: &AssetPhaseFlags) -> bool {
            phases.havok
        }

        fn uses_session(&self) -> bool {
            true
        }

        fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
            true
        }

        fn run_with_session(
            &self,
            _session: &mut PluginSession,
            _mapper: &mut FormKeyMapper,
            _config: &FixupConfig,
        ) -> Result<FixupReport, FixupError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(FixupReport::empty())
        }
    }

    #[test]
    fn whole_plugin_skips_asset_fixup_when_required_phase_disabled() {
        let target_handle = create_test_plugin_handle();
        let (mapper_interner, mut mapper_state, mut config) = make_mapper_and_config();
        config.is_whole_plugin = true;
        config.asset_phases.animations = true;
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");
        let calls = StdArc::new(AtomicU32::new(0));

        let mut registry = FixupRegistry::new();
        registry.register(Box::new(HavokAssetFixup {
            calls: StdArc::clone(&calls),
        }));

        let reports = registry
            .run_all_in_session(&mut session, &mut mapper, &config)
            .expect("run_all should succeed");

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].0, "havok_asset");
        assert_eq!(reports[0].1.status, FixupStatus::Skipped);
        assert_eq!(reports[0].1.scope, FixupScope::AssetOnly);
        assert_eq!(reports[0].1.iteration, 0);
        let message = reports[0]
            .1
            .message
            .and_then(|sym| mapper_interner.resolve(sym))
            .unwrap();
        assert_eq!(message, "fixup_skipped:havok_asset:asset_only");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn whole_plugin_runs_asset_fixup_when_required_phase_enabled() {
        let target_handle = create_test_plugin_handle();
        let (mapper_interner, mut mapper_state, mut config) = make_mapper_and_config();
        config.is_whole_plugin = true;
        config.asset_phases.havok = true;
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");
        let calls = StdArc::new(AtomicU32::new(0));

        let mut registry = FixupRegistry::new();
        registry.register(Box::new(HavokAssetFixup {
            calls: StdArc::clone(&calls),
        }));

        let reports = registry
            .run_all_in_session(&mut session, &mut mapper, &config)
            .expect("run_all should succeed");

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].1.status, FixupStatus::Ran);
        assert_eq!(reports[0].1.scope, FixupScope::AssetOnly);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    struct AddRecordUntilVisibleFixup;

    impl Fixup for AddRecordUntilVisibleFixup {
        fn name(&self) -> &'static str {
            "add_record_until_visible"
        }

        fn uses_session(&self) -> bool {
            true
        }

        fn convergent(&self) -> bool {
            true
        }

        fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
            true
        }

        fn run_with_session(
            &self,
            session: &mut PluginSession,
            mapper: &mut FormKeyMapper,
            _config: &FixupConfig,
        ) -> Result<FixupReport, FixupError> {
            let ammo_sig = SigCode::from_str("AMMO").expect("valid AMMO sig");
            if !session
                .form_keys_of_sig(ammo_sig, mapper.interner)
                .map_err(|err| FixupError::HandleError(err.to_string()))?
                .is_empty()
            {
                return Ok(FixupReport::empty());
            }

            let schema = session
                .schema()
                .map_err(|err| FixupError::HandleError(err.to_string()))?;
            let edid_sig = SubrecordSig::from_str("EDID").expect("valid EDID sig");
            let edid = mapper.interner.intern("AddedByFixup");
            let fk = FormKey::parse("000800@FixupRegistryTest.esp", mapper.interner)
                .expect("valid form key");
            let record = Record {
                sig: ammo_sig,
                form_key: fk,
                eid: Some(edid),
                flags: RecordFlags::empty(),
                fields: smallvec::smallvec![FieldEntry {
                    sig: edid_sig,
                    value: FieldValue::String(edid),
                }],
                warnings: smallvec::SmallVec::new(),
            };
            session
                .add_record(record, schema.as_ref(), mapper.interner)
                .map_err(|err| FixupError::HandleError(err.to_string()))?;

            Ok(FixupReport {
                records_added: 1,
                ..FixupReport::empty()
            })
        }
    }

    #[test]
    fn run_all_in_session_refreshes_session_between_iterations() {
        let target_handle = create_test_plugin_handle();
        let (mapper_interner, mut mapper_state, config) = make_mapper_and_config();
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        let mut registry = FixupRegistry::new();
        registry.register(Box::new(AddRecordUntilVisibleFixup));

        let reports = registry
            .run_all_in_session(&mut session, &mut mapper, &config)
            .expect("run_all should converge");

        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].0, "add_record_until_visible");
        assert_eq!(reports[0].1.records_added, 1);
        assert_eq!(reports[0].1.iteration, 1);
        assert!(reports[1].1.is_no_op());
        assert_eq!(reports[1].1.iteration, 2);

        let ammo_sig = SigCode::from_str("AMMO").expect("valid AMMO sig");
        let ammo_records = session
            .form_keys_of_sig(ammo_sig, &mapper_interner)
            .expect("fresh session should see added record");
        assert_eq!(ammo_records.len(), 1);
    }

    struct ErrorAfterStructuralWriteFixup;

    impl Fixup for ErrorAfterStructuralWriteFixup {
        fn name(&self) -> &'static str {
            "error_after_structural_write"
        }

        fn uses_session(&self) -> bool {
            true
        }

        fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
            true
        }

        fn run_with_session(
            &self,
            session: &mut PluginSession,
            mapper: &mut FormKeyMapper,
            _config: &FixupConfig,
        ) -> Result<FixupReport, FixupError> {
            let ammo_sig = SigCode::from_str("AMMO").expect("valid AMMO sig");
            let schema = session
                .schema()
                .map_err(|err| FixupError::HandleError(err.to_string()))?;
            let edid_sig = SubrecordSig::from_str("EDID").expect("valid EDID sig");
            let edid = mapper.interner.intern("ErrorPathAmmo");
            let fk = FormKey::parse("000801@FixupRegistryTest.esp", mapper.interner)
                .expect("valid form key");
            let record = Record {
                sig: ammo_sig,
                form_key: fk,
                eid: Some(edid),
                flags: RecordFlags::empty(),
                fields: smallvec::smallvec![FieldEntry {
                    sig: edid_sig,
                    value: FieldValue::String(edid),
                }],
                warnings: smallvec::SmallVec::new(),
            };
            session
                .add_record(record, schema.as_ref(), mapper.interner)
                .map_err(|err| FixupError::HandleError(err.to_string()))?;
            Err(FixupError::HandleError("expected test error".to_string()))
        }
    }

    #[test]
    fn run_all_in_session_flushes_and_keeps_session_usable_on_error() {
        let target_handle = create_test_plugin_handle();
        let (mapper_interner, mut mapper_state, config) = make_mapper_and_config();
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        let mut registry = FixupRegistry::new();
        registry.register(Box::new(ErrorAfterStructuralWriteFixup));

        let result = registry.run_all_in_session(&mut session, &mut mapper, &config);
        assert!(
            matches!(result, Err(FixupError::HandleError(message)) if message == "expected test error")
        );

        let ammo_sig = SigCode::from_str("AMMO").expect("valid AMMO sig");
        let ammo_records = session
            .form_keys_of_sig(ammo_sig, &mapper_interner)
            .expect("session should remain usable after helper error");
        assert_eq!(ammo_records.len(), 1);
    }
}
