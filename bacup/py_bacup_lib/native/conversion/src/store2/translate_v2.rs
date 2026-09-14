//! translate_all_v2 — the parallel translate pipeline.
//!
//! Passes per chunk of `CHUNK` source FormKeys (enumeration = legacy order):
//!   P (parallel): mmap read → decode → pre_translate → translate → EDID rename
//!   A (serial):   allocate_or_resolve → NVNM rewrite → rewrite_record   [mapper]
//!   F (parallel): post hooks → class-A → normalizer → namespacing → snapshot
//!   E (serial):   add_record_in_slot (encode + insert; lstring ids in order)
//! Serial passes preserve legacy determinism (FormKey + lstring allocation
//! order). FO76/FNV/FO3→FO4 first preallocates every eligible translated FormKey
//! so raw VMAD and legacy magic references are independent of signature order.
//! Tail (structured dialogue + worldspace rebuild) reuses the legacy methods.
//! No Python contact.

use std::path::Path;
use std::time::{Duration, Instant};

use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

use esp_authoring_core::plugin_runtime::{LocalizedStringsState, plugin_handle_store_ref};

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::phase::progress::ProgressReporter;
use crate::phase::{LogLevel, PhaseEvent};
use crate::record::Record;
use crate::run::{
    ConversionRun, FnvScriLink, LegacyFormKeyAllocationIntent, LegacyFormKeyPreallocationCoverage,
    RunError, TranslateStats, base_asset_namespace, capture_target_master_context,
    is_mandatory_skyrim_actor_record, is_target_master_remap, namespace_base_asset_model_paths,
    normalized_eid_opt, rename_fo76_target_editor_id_collision, target_collision_donor_form_key,
};
use crate::source_read::decode_record_from_parsed_relayout;
use crate::store2::source::SourceEsm;
use crate::sym::{StringInterner, Sym};
use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
use crate::target_write::{add_record_in_slot, add_skyrim_navmesh_record_in_slot};
use crate::translator::{Decision, DeferredKind, Game, TranslateResult};

pub const CHUNK: usize = 4096;

/// What Pass P decided for one source record; carries everything the serial
/// passes and the final merge need to reproduce legacy stat/warning/decision
/// bookkeeping in position order.
enum PassPOutcome {
    /// Decoded + translated; flows into A/F/E.
    Translated {
        record: Record,
        fnv_scri_target: Option<String>,
        lowered_acre: bool,
    },
    /// translate() returned Dropped — count dropped against source sig.
    Dropped(Decision),
    /// translate() returned Deferred — count deferred against source sig.
    Deferred { kind: DeferredKind, record: Record },
    /// Target sig not in the target schema — count dropped, push warning.
    DroppedUnsupported,
    /// Read/decode error — count failed, NO seen for the (unknown) sig.
    ReadFailed,
}

/// Disposition assigned after the serial/parallel passes; drives the final
/// stat merge in position order (mirrors the legacy per-record counters).
enum Disposition {
    Translated,
    Dropped,
    Deferred,
    Failed,
    VanillaRemapped,
}

struct Candidate {
    source_fk: FormKey,
    source_sig: SigCode,
    /// True once the source record decoded (legacy increments `seen` then).
    seen: bool,
    /// The working record as it flows P → A → F → E (None once dropped/failed).
    record: Option<Record>,
    fnv_scri_target: Option<String>,
    lowered_acre: bool,
    collision_donor: Option<FormKey>,
    /// Final disposition for the stat merge.
    disposition: Disposition,
    /// Warnings in intra-record order (P, then A, then F, then E).
    warnings: Vec<String>,
    /// Decisions in intra-record order.
    decisions: Vec<Decision>,
    /// Deferred entries (translate Deferred only).
    deferred: Vec<(FormKey, DeferredKind)>,
    /// FNV SCRI links captured for this record (FNV→FO4 only).
    fnv_scri_links: Vec<FnvScriLink>,
    /// Full-plugin snapshot built in Pass F (whole-plugin runs only).
    full_plugin_snapshot: Option<Record>,
    /// Exact source FormKeys left unresolved by Pass A's mapper rewrite.
    unresolved_source_refs: FxHashSet<FormKey>,
    creature_race_event:
        Option<crate::translator::pair_hooks::fnv_creature_race::CreatureRaceGateEvent>,
    creature_race_failure_decision:
        Option<crate::translator::pair_hooks::fnv_creature_race::CreatureRaceDecision>,
    creature_race_fatal_error: Option<String>,
    acre_fatal_error: Option<String>,
}

struct SkyrimNavmeshSet {
    converted: FxHashMap<FormKey, Vec<u8>>,
    failures: FxHashMap<FormKey, String>,
}

#[derive(Default)]
struct ChunkPassTiming {
    pass_p: Duration,
    pre_allocation_gates: Duration,
    pass_a: Duration,
    pass_f: Duration,
    post_translate_gates: Duration,
    pass_e: Duration,
    target_write_gate: Duration,
    merge: Duration,
    chunks: usize,
}

impl Candidate {
    fn from_pass_p(source_fk: FormKey, source_sig: SigCode, outcome: PassPOutcome) -> Self {
        let mut c = Candidate {
            source_fk,
            source_sig,
            seen: false,
            record: None,
            fnv_scri_target: None,
            lowered_acre: false,
            collision_donor: None,
            disposition: Disposition::Failed,
            warnings: Vec::new(),
            decisions: Vec::new(),
            deferred: Vec::new(),
            fnv_scri_links: Vec::new(),
            full_plugin_snapshot: None,
            unresolved_source_refs: FxHashSet::default(),
            creature_race_event: None,
            creature_race_failure_decision: None,
            creature_race_fatal_error: None,
            acre_fatal_error: None,
        };
        match outcome {
            PassPOutcome::Translated {
                record,
                fnv_scri_target,
                lowered_acre,
            } => {
                c.seen = true;
                c.record = Some(record);
                c.fnv_scri_target = fnv_scri_target;
                c.lowered_acre = lowered_acre;
                c.disposition = Disposition::Translated;
            }
            PassPOutcome::Dropped(decision) => {
                c.seen = true;
                c.decisions.push(decision);
                c.disposition = Disposition::Dropped;
            }
            PassPOutcome::Deferred { kind, record } => {
                c.seen = true;
                c.record = Some(record);
                c.deferred.push((source_fk, kind));
                c.disposition = Disposition::Deferred;
            }
            PassPOutcome::DroppedUnsupported => {
                c.seen = true;
                c.disposition = Disposition::Dropped;
            }
            PassPOutcome::ReadFailed => {
                c.seen = false;
                c.disposition = Disposition::Failed;
            }
        }
        c
    }
}

/// Source decode context — captured once under a short lock, mirroring the
/// values the legacy `read_record_relayout_by_form_key` reads from the slot.
struct SourceCtx {
    masters: Vec<String>,
    plugin_name: String,
    strings: Option<LocalizedStringsState>,
    plugin_is_localized: bool,
}

fn capture_source_ctx(handle_id: u64) -> Result<SourceCtx, RunError> {
    const TES4_FLAG_LOCALIZED: u32 = 0x0000_0080;
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| RunError::InvalidConfig(format!("no source plugin handle: {handle_id}")))?;
    let plugin_is_localized = (slot.parsed.header.flags & TES4_FLAG_LOCALIZED) != 0;
    Ok(SourceCtx {
        masters: slot.parsed.header.masters.clone(),
        plugin_name: slot.parsed.plugin_name.clone(),
        strings: plugin_is_localized.then(|| slot.strings_ref().clone()),
        plugin_is_localized,
    })
}

/// Build a `crate::ids::FormKey` from a raw source form_id with the same
/// master-index → plugin-name mapping the legacy CoreSection index uses
/// (`form_key_for_record` + `form_key_from_index_entry`).
fn form_key_for_raw(
    raw_form_id: u32,
    own_plugin_name: &str,
    masters: &[String],
    interner: &StringInterner,
) -> FormKey {
    let master_index = ((raw_form_id >> 24) & 0xFF) as usize;
    let own_index = masters.len() & 0xFF;
    let plugin = if master_index != own_index && master_index < masters.len() {
        interner.intern(masters[master_index].as_str())
    } else {
        interner.intern(own_plugin_name)
    };
    FormKey {
        local: raw_form_id & 0x00FF_FFFF,
        plugin,
    }
}

pub fn translate_all_v2(
    run: &mut ConversionRun,
    source_esm_path: &Path,
) -> Result<TranslateStats, RunError> {
    let translate_started = Instant::now();
    log_translate_v2(run, "translate_v2: opening source esm");
    let source_open_started = Instant::now();
    let esm = SourceEsm::open(source_esm_path).map_err(|e| {
        RunError::InvalidConfig(format!("store2 source open {source_esm_path:?}: {e}"))
    })?;
    let source_open = source_open_started.elapsed();
    log_translate_v2(run, "translate_v2: source esm opened");
    log_translate_v2(run, "translate_v2: capture source context start");
    let source_context_started = Instant::now();
    let source_ctx = capture_source_ctx(run.source_handle_id)?;
    let source_context = source_context_started.elapsed();
    log_translate_v2(run, "translate_v2: capture source context done");
    let pack_preflight_started = Instant::now();
    if let Some(mut preflight) = run.begin_legacy_pack_preflight(&source_ctx.plugin_name) {
        for position in esm.positions_of_sig(*b"PACK") {
            let Some(view) = esm.view_at(position) else {
                continue;
            };
            let form_key = form_key_for_raw(
                view.form_id(),
                &source_ctx.plugin_name,
                &source_ctx.masters,
                &run.interner,
            );
            let decoded = view
                .to_parsed_record()
                .map_err(|error| error.to_string())
                .and_then(|raw_record| {
                    decode_record_from_parsed_relayout(
                        &raw_record,
                        &form_key,
                        &run.schema_source,
                        &source_ctx.masters,
                        &source_ctx.plugin_name,
                        source_ctx.strings.as_ref(),
                        source_ctx.plugin_is_localized,
                        &run.interner,
                        None,
                    )
                    .map_err(|error| error.to_string())
                });
            match decoded {
                Ok(record) => preflight.observe_decoded(&record, &run.interner),
                Err(error) => preflight.observe_decode_error(form_key, error, &run.interner),
            }
        }
        run.finish_legacy_pack_preflight(preflight)?;
    }
    let pack_preflight = pack_preflight_started.elapsed();
    log_translate_v2(run, "translate_v2: init mapper state start");
    let mapper_started = Instant::now();
    run.prepare_mapper_state_for_translation()?;
    let mapper = mapper_started.elapsed();
    log_translate_v2(run, "translate_v2: init mapper state done");
    if run.config.records_limit == Some(0) {
        // Nothing to translate; still run the tail exactly like translate_all.
        let mut stats = TranslateStats::default();
        run.validate_creature_dependency_accounting()?;
        run.finalize_legacy_creature_race_coverage()?;
        if run.should_emit_fo76_quest_dialogue() {
            stats.absorb(run.emit_quest_child_dialogue()?);
            stats.absorb(run.emit_topic_child_infos()?);
        }
        if run.should_emit_fo76_quest_scenes() {
            stats.absorb(run.emit_quest_child_scenes()?);
        }
        run.rebuild_full_plugin_worldspace_groups()?;
        return Ok(stats);
    }
    let navmesh_preparation_started = Instant::now();
    let skyrim_navmeshes = if run.source == Game::SkyrimSe && run.target == Game::Fo4 {
        log_translate_v2(run, "translate_v2: prepare Skyrim NAVM set start");
        let prepared = crate::skyrim_navmesh::prepare_skyrim_navmeshes(&esm);
        let converted = prepared
            .converted
            .into_iter()
            .map(|(raw_form_id, bytes)| {
                (
                    form_key_for_raw(
                        raw_form_id,
                        &source_ctx.plugin_name,
                        &source_ctx.masters,
                        &run.interner,
                    ),
                    bytes,
                )
            })
            .collect::<FxHashMap<_, _>>();
        let failures = prepared
            .failures
            .into_iter()
            .map(|(raw_form_id, error)| {
                (
                    form_key_for_raw(
                        raw_form_id,
                        &source_ctx.plugin_name,
                        &source_ctx.masters,
                        &run.interner,
                    ),
                    error,
                )
            })
            .collect::<FxHashMap<_, _>>();
        let report = prepared.report;
        log_translate_v2(
            run,
            format!(
                "translate_v2: prepare Skyrim NAVM set done seen={} converted={} failed={} missing_geometry={} edge_links_resolved={} edge_links_dropped={} cover_triangles_dropped={}",
                report.records_seen,
                report.records_converted,
                report.records_failed,
                report.records_without_geometry,
                report.edge_links_resolved,
                report.edge_links_dropped,
                report.cover_triangles_dropped,
            ),
        );
        Some(SkyrimNavmeshSet {
            converted,
            failures,
        })
    } else {
        None
    };
    let navmesh_preparation = navmesh_preparation_started.elapsed();

    // Target master context (skip set + first master sym), built once.
    log_translate_v2(run, "translate_v2: capture target master context start");
    let target_context_started = Instant::now();
    let (target_master_syms, first_target_master_sym) = capture_target_master_context(run);
    let target_context = target_context_started.elapsed();
    log_translate_v2(run, "translate_v2: capture target master context done");

    // The structured quest-child tail owns SCEN and, when dialogue is enabled,
    // DIAL/INFO. Exclude those signatures from the main enumeration.
    let emit_structured_dialogue = run.should_emit_fo76_quest_dialogue();
    let emit_structured_scenes = run.should_emit_fo76_quest_scenes();
    let dial_sig = SigCode::from_str("DIAL").map(|s| s.0).unwrap_or([0; 4]);
    let info_sig = SigCode::from_str("INFO").map(|s| s.0).unwrap_or([0; 4]);
    let scen_sig = SigCode::from_str("SCEN").map(|s| s.0).unwrap_or([0; 4]);

    // Enumerate (sig, position) in legacy order with the same per-sig-batch cap.
    let enumeration_started = Instant::now();
    let enumeration: Vec<(SigCode, usize)> = esm
        .enumerate_positions_sorted_sig(run.config.records_limit)
        .into_iter()
        .filter(|(sig, _)| {
            !((emit_structured_dialogue && (*sig == dial_sig || *sig == info_sig))
                || (emit_structured_scenes && *sig == scen_sig))
        })
        .map(|(sig_bytes, pos)| (SigCode(sig_bytes), pos))
        .collect();
    let enumeration_time = enumeration_started.elapsed();
    let total = enumeration.len() as u32;
    log_translate_v2(run, format!("translate_v2: enumerated records={total}"));
    let runtime_plan_started = Instant::now();
    plan_skyrim_runtime_components_v2(run, &esm, &source_ctx, &enumeration)?;
    let runtime_plan = runtime_plan_started.elapsed();
    let excluded_scan_started = Instant::now();
    let excluded_source_formkeys =
        collect_excluded_source_formkeys(run, &esm, &enumeration, &source_ctx);
    let excluded_scan = excluded_scan_started.elapsed();

    let preallocation_started = Instant::now();
    if matches!(run.source, Game::Fo76 | Game::Fnv | Game::Fo3) && run.target == Game::Fo4 {
        preallocate_legacy_form_keys(
            run,
            &esm,
            &source_ctx,
            &excluded_source_formkeys,
            &enumeration,
        )?;
    }
    let preallocation = preallocation_started.elapsed();
    log_translate_v2(
        run,
        format!(
            "translate_v2: setup_timing source_open_ms={:.3} source_context_ms={:.3} pack_preflight_ms={:.3} mapper_ms={:.3} navmesh_preparation_ms={:.3} target_context_ms={:.3} enumeration_ms={:.3} runtime_plan_ms={:.3} excluded_scan_ms={:.3} preallocation_ms={:.3}",
            source_open.as_secs_f64() * 1000.0,
            source_context.as_secs_f64() * 1000.0,
            pack_preflight.as_secs_f64() * 1000.0,
            mapper.as_secs_f64() * 1000.0,
            navmesh_preparation.as_secs_f64() * 1000.0,
            target_context.as_secs_f64() * 1000.0,
            enumeration_time.as_secs_f64() * 1000.0,
            runtime_plan.as_secs_f64() * 1000.0,
            excluded_scan.as_secs_f64() * 1000.0,
            preallocation.as_secs_f64() * 1000.0,
        ),
    );

    let reporter = ProgressReporter::new("translate_v2", total, run.event_tx.clone());
    let mut stats = TranslateStats::default();
    let mut chunk_timing = ChunkPassTiming::default();

    for chunk in enumeration.chunks(CHUNK) {
        if run.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(RunError::Cancelled);
        }

        chunk_timing.chunks += 1;
        let pass_started = Instant::now();
        let mut candidates = run_pass_p(run, &esm, &source_ctx, &excluded_source_formkeys, chunk);
        chunk_timing.pass_p += pass_started.elapsed();
        let gates_started = Instant::now();
        enforce_creature_dependency_terminals(run, &candidates)?;
        enforce_acre_gate(&candidates)?;
        enforce_creature_race_gate(run, &candidates, true)?;
        chunk_timing.pre_allocation_gates += gates_started.elapsed();
        let pass_started = Instant::now();
        run_pass_a(
            run,
            &target_master_syms,
            skyrim_navmeshes.as_ref(),
            &mut candidates,
        )?;
        chunk_timing.pass_a += pass_started.elapsed();
        let pass_started = Instant::now();
        run_pass_f(run, &source_ctx, &mut candidates);
        chunk_timing.pass_f += pass_started.elapsed();
        let gates_started = Instant::now();
        enforce_creature_dependency_post_translate_terminals(run, &candidates, "post_translate")?;
        validate_creature_race_targets(run, &candidates)?;
        chunk_timing.post_translate_gates += gates_started.elapsed();
        let pass_started = Instant::now();
        run_pass_e(
            run,
            &target_master_syms,
            first_target_master_sym,
            &mut candidates,
        );
        chunk_timing.pass_e += pass_started.elapsed();
        let gate_started = Instant::now();
        enforce_creature_dependency_post_translate_terminals(run, &candidates, "target_write")?;
        chunk_timing.target_write_gate += gate_started.elapsed();

        let merge_started = Instant::now();
        merge_chunk(run, &mut stats, candidates);
        chunk_timing.merge += merge_started.elapsed();
        reporter.inc(chunk.len() as u32);
    }
    log_translate_v2(run, "translate_v2: per-record chunks done");
    log_translate_v2(
        run,
        format!(
            "translate_v2: chunk_timing chunks={} pass_p_ms={:.3} pre_allocation_gates_ms={:.3} pass_a_ms={:.3} pass_f_ms={:.3} post_translate_gates_ms={:.3} pass_e_ms={:.3} target_write_gate_ms={:.3} merge_ms={:.3}",
            chunk_timing.chunks,
            chunk_timing.pass_p.as_secs_f64() * 1000.0,
            chunk_timing.pre_allocation_gates.as_secs_f64() * 1000.0,
            chunk_timing.pass_a.as_secs_f64() * 1000.0,
            chunk_timing.pass_f.as_secs_f64() * 1000.0,
            chunk_timing.post_translate_gates.as_secs_f64() * 1000.0,
            chunk_timing.pass_e.as_secs_f64() * 1000.0,
            chunk_timing.target_write_gate.as_secs_f64() * 1000.0,
            chunk_timing.merge.as_secs_f64() * 1000.0,
        ),
    );
    run.validate_creature_dependency_accounting()?;
    run.finalize_legacy_creature_race_coverage()?;
    log_translate_v2(run, "translate_v2: reporter finish start");
    reporter.finish();
    log_translate_v2(run, "translate_v2: reporter finish done");

    // Tail — mirror translate_all exactly.
    let mut dialogue_tail = Duration::ZERO;
    let mut info_tail = Duration::ZERO;
    let mut scene_tail = Duration::ZERO;
    let other_tail_started = Instant::now();
    if emit_structured_dialogue {
        log_translate_v2(run, "translate_v2: emit quest child dialogue start");
        let tail_started = Instant::now();
        stats.absorb(run.emit_quest_child_dialogue()?);
        dialogue_tail = tail_started.elapsed();
        log_translate_v2(run, "translate_v2: emit quest child dialogue done");
        log_translate_v2(run, "translate_v2: emit topic child infos start");
        let tail_started = Instant::now();
        stats.absorb(run.emit_topic_child_infos()?);
        info_tail = tail_started.elapsed();
        log_translate_v2(run, "translate_v2: emit topic child infos done");
    }
    if emit_structured_scenes {
        log_translate_v2(run, "translate_v2: emit quest child scenes start");
        let tail_started = Instant::now();
        stats.absorb(run.emit_quest_child_scenes()?);
        scene_tail = tail_started.elapsed();
        log_translate_v2(run, "translate_v2: emit quest child scenes done");
    }
    log_translate_v2(run, "translate_v2: emit admitted Skyrim quests start");
    stats.absorb(run.emit_skyrim_minimal_quest_projections()?);
    log_translate_v2(run, "translate_v2: emit admitted Skyrim quests done");
    let legacy_fallout_navmesh_tail =
        matches!(run.source, Game::Fnv | Game::Fo3) && run.target == Game::Fo4;
    if legacy_fallout_navmesh_tail {
        log_translate_v2(run, "translate_v2: rebuild legacy worldspace groups start");
        run.rebuild_full_plugin_worldspace_groups()?;
        log_translate_v2(run, "translate_v2: emit legacy projected NAVM start");
        stats.absorb(run.emit_projected_navmeshes()?);
        log_translate_v2(run, "translate_v2: emit legacy projected NAVM done");
    }
    if (legacy_fallout_navmesh_tail || run.source == Game::SkyrimSe) && run.target == Game::Fo4 {
        log_translate_v2(run, "translate_v2: rebuild source NAVI start");
        stats.absorb(run.rebuild_projected_navi()?);
        log_translate_v2(run, "translate_v2: rebuild source NAVI done");
    }
    if !legacy_fallout_navmesh_tail {
        log_translate_v2(run, "translate_v2: rebuild worldspace groups start");
        run.rebuild_full_plugin_worldspace_groups()?;
        log_translate_v2(run, "translate_v2: rebuild worldspace groups done");
    }

    if run.config.is_whole_plugin {
        log_translate_v2(run, "translate_v2: full plugin warning summary start");
        let warning = format!(
            "full_plugin_state:unresolved_refs={};target_master_refs={}",
            run.full_plugin_state.unresolved_ref_count(),
            run.full_plugin_state.target_master_ref_count()
        );
        let sym = run.interner.intern(&warning);
        run.warnings.push(sym);
        log_translate_v2(run, "translate_v2: full plugin warning summary done");
    }

    run.validate_skyrim_actor_translation(&stats)?;

    let other_tail = other_tail_started
        .elapsed()
        .saturating_sub(dialogue_tail + info_tail + scene_tail);
    log_translate_v2(
        run,
        format!(
            "translate_v2: tail_timing dialogue_ms={:.3} info_ms={:.3} scenes_ms={:.3} other_ms={:.3} translate_total_ms={:.3}",
            dialogue_tail.as_secs_f64() * 1000.0,
            info_tail.as_secs_f64() * 1000.0,
            scene_tail.as_secs_f64() * 1000.0,
            other_tail.as_secs_f64() * 1000.0,
            translate_started.elapsed().as_secs_f64() * 1000.0,
        ),
    );

    log_translate_v2(run, "translate_v2: returning stats");
    Ok(stats)
}

fn plan_skyrim_runtime_components_v2(
    run: &mut ConversionRun,
    esm: &SourceEsm,
    source_ctx: &SourceCtx,
    enumeration: &[(SigCode, usize)],
) -> Result<(), RunError> {
    if run.source != Game::SkyrimSe || run.target != Game::Fo4 {
        return Ok(());
    }
    let managed = enumeration
        .iter()
        .filter(|(signature, position)| {
            esm.view_at(*position).is_some_and(|view| {
                let form_key = form_key_for_raw(
                    view.form_id(),
                    &source_ctx.plugin_name,
                    &source_ctx.masters,
                    &run.interner,
                );
                run.is_skyrim_runtime_planned_record(form_key, *signature)
            })
        })
        .collect::<Vec<_>>();
    let mut records = Vec::with_capacity(managed.len());
    for (signature, position) in managed {
        let view = esm.view_at(*position).ok_or_else(|| {
            RunError::InvalidConfig(format!(
                "Skyrim runtime preflight source position {position} is missing"
            ))
        })?;
        let form_key = form_key_for_raw(
            view.form_id(),
            &source_ctx.plugin_name,
            &source_ctx.masters,
            &run.interner,
        );
        let raw_record = view.to_parsed_record().map_err(|error| {
            RunError::InvalidConfig(format!(
                "parse Skyrim runtime {} {:06X}: {error}",
                signature.as_str(),
                form_key.local
            ))
        })?;
        let record = decode_record_from_parsed_relayout(
            &raw_record,
            &form_key,
            &run.schema_source,
            &source_ctx.masters,
            &source_ctx.plugin_name,
            source_ctx.strings.as_ref(),
            source_ctx.plugin_is_localized,
            &run.interner,
            None,
        )
        .map_err(|error| {
            RunError::InvalidConfig(format!(
                "decode Skyrim runtime {} {:06X}: {error}",
                signature.as_str(),
                form_key.local
            ))
        })?;
        records.push(record);
    }
    run.plan_skyrim_runtime_records(records)?;
    for (signature, position) in enumeration {
        let raw = esm
            .view_at(*position)
            .map(|view| view.form_id())
            .ok_or_else(|| {
                RunError::InvalidConfig(format!(
                    "Skyrim runtime decision source position {position} is missing"
                ))
            })?;
        let form_key = form_key_for_raw(
            raw,
            &source_ctx.plugin_name,
            &source_ctx.masters,
            &run.interner,
        );
        if !run.is_skyrim_runtime_planned_record(form_key, *signature) {
            continue;
        }
        if run.skyrim_runtime_component_disposition(form_key).is_none() {
            return Err(RunError::InvalidConfig(format!(
                "managed Skyrim runtime {} {:06X} has no atomic component decision",
                signature.as_str(),
                form_key.local
            )));
        }
    }
    Ok(())
}

fn log_translate_v2(run: &ConversionRun, message: impl Into<String>) {
    let _ = run.event_tx.try_send(PhaseEvent::Log {
        phase: "translate_v2",
        level: LogLevel::Info,
        message: message.into(),
    });
}

fn preallocate_translated_candidates(
    candidates: &[Candidate],
    run: &mut ConversionRun,
) -> LegacyFormKeyPreallocationCoverage {
    let intents = candidates.iter().filter_map(legacy_preallocation_intent);
    run.preallocate_legacy_form_key_intents(intents)
}

fn legacy_preallocation_intent(candidate: &Candidate) -> Option<LegacyFormKeyAllocationIntent> {
    let eligible = matches!(candidate.disposition, Disposition::Translated)
        || (matches!(candidate.disposition, Disposition::Deferred)
            && candidate
                .deferred
                .iter()
                .any(|(_, kind)| matches!(kind, DeferredKind::FnvLegacyScripting)));
    if !eligible {
        return None;
    }
    let record = candidate.record.as_ref()?;
    Some(LegacyFormKeyAllocationIntent {
        source_fk: candidate.source_fk,
        editor_id: record.eid,
        target_sig: record.sig,
    })
}

fn preallocate_legacy_form_keys(
    run: &mut ConversionRun,
    esm: &SourceEsm,
    source_ctx: &SourceCtx,
    excluded_source_formkeys: &FxHashSet<FormKey>,
    enumeration: &[(SigCode, usize)],
) -> Result<(), RunError> {
    log_translate_v2(
        run,
        format!(
            "translate_v2: forward-reference preallocation start records={}",
            enumeration.len()
        ),
    );
    run.prepare_legacy_output_allocation_domain(enumeration.iter().filter_map(|(_, position)| {
        esm.view_at(*position)
            .map(|record| record.form_id() & 0x00FF_FFFF)
    }))?;
    run.preallocate_fnv_quest_slice_pack_identities()?;
    let mut total = LegacyFormKeyPreallocationCoverage::default();
    for chunk in enumeration.chunks(CHUNK) {
        if run.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(RunError::Cancelled);
        }
        let candidates = run_pass_p(run, esm, source_ctx, excluded_source_formkeys, chunk);
        enforce_creature_dependency_terminals(run, &candidates)?;
        enforce_acre_gate(&candidates)?;
        enforce_creature_race_gate(run, &candidates, false)?;
        let coverage = preallocate_translated_candidates(&candidates, run);
        total.eligible += coverage.eligible;
        total.mapped += coverage.mapped;
        total.missing += coverage.missing;
    }
    log_translate_v2(
        run,
        format!(
            "translate_v2: forward-reference preallocation done eligible={} mapped={} missing={}",
            total.eligible, total.mapped, total.missing
        ),
    );
    if total.missing != 0 || total.mapped != total.eligible {
        return Err(RunError::InvalidConfig(format!(
            "forward-reference preallocation incomplete: eligible={} mapped={} missing={}",
            total.eligible, total.mapped, total.missing
        )));
    }
    Ok(())
}

fn is_selected_fnv_quest_slice_mgef(
    run: &ConversionRun,
    source_fk: FormKey,
    source_plugin: crate::sym::Sym,
    source_sig: SigCode,
) -> bool {
    run.config.fnv_quest_slice
        && run.source == Game::Fnv
        && run.target == Game::Fo4
        && source_sig.0 == *b"MGEF"
        && source_fk.plugin == source_plugin
        && run
            .config
            .fnv_quest_slice_records
            .iter()
            .find(|(signature, _)| signature.eq_ignore_ascii_case("MGEF"))
            .is_some_and(|(_, locals)| {
                locals
                    .iter()
                    .any(|local| local & 0x00FF_FFFF == source_fk.local)
            })
}

/// Pass P — parallel: mmap read → decode → pre_translate → translate → rename.
fn run_pass_p(
    run: &ConversionRun,
    esm: &SourceEsm,
    source_ctx: &SourceCtx,
    excluded_source_formkeys: &FxHashSet<FormKey>,
    chunk: &[(SigCode, usize)],
) -> Vec<Candidate> {
    let interner = &run.interner;
    let translator = &run.translator;
    let schema_source = &*run.schema_source;
    let schema_target = &*run.schema_target;
    let source = run.source;
    let target = run.target;
    let mapper_state = run.mapper_state.as_ref();
    let strings = source_ctx.strings.as_ref();
    let source_plugin = interner.intern(&source_ctx.plugin_name);

    // Legacy Fallout is restricted to BPTD.BPND; FO76 retains generic relayout.
    let legacy_bptd_only = matches!(source, Game::Fnv | Game::Fo3);
    let relayout_target_schema = (target == Game::Fo4
        && (source == Game::Fo76 || legacy_bptd_only))
        .then(|| run.schema_target.clone());
    let relayout_ctx = relayout_target_schema.as_deref().map(|target_schema| {
        crate::struct_relayout::StructRelayoutCtx {
            target_schema,
            target_form_version:
                crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION,
            legacy_bptd_only,
        }
    });

    chunk
        .par_iter()
        .map(|&(source_sig, pos)| {
            let source_fk = {
                let raw = esm.view_at(pos).map(|v| v.form_id()).unwrap_or(0);
                form_key_for_raw(raw, &source_ctx.plugin_name, &source_ctx.masters, interner)
            };

            let view = match esm.view_at(pos) {
                Some(v) => v,
                None => {
                    let mut c =
                        Candidate::from_pass_p(source_fk, source_sig, PassPOutcome::ReadFailed);
                    c.warnings
                        .push(format!("read_error:position {pos} missing"));
                    return c;
                }
            };
            let parsed = match view.to_parsed_record() {
                Ok(p) => p,
                Err(e) => {
                    let mut c =
                        Candidate::from_pass_p(source_fk, source_sig, PassPOutcome::ReadFailed);
                    c.warnings.push(format!("read_error:{e}"));
                    return c;
                }
            };
            let mut src_record = match decode_record_from_parsed_relayout(
                &parsed,
                &source_fk,
                schema_source,
                &source_ctx.masters,
                &source_ctx.plugin_name,
                strings,
                source_ctx.plugin_is_localized,
                interner,
                relayout_ctx.as_ref(),
            ) {
                Ok(r) => r,
                Err(e) => {
                    let mut c =
                        Candidate::from_pass_p(source_fk, source_sig, PassPOutcome::ReadFailed);
                    c.warnings.push(format!("read_error:{e}"));
                    return c;
                }
            };
            // source_sig from the enumeration equals src_record.sig.
            let source_sig = src_record.sig;
            let mut pass_p_warnings: Vec<String> = Vec::new();
            let admitted_creature_dependency =
                run.is_admitted_creature_dependency(source_fk, source_sig);

            if run.is_skyrim_creature_batch_owned(source_fk) {
                return Candidate::from_pass_p(
                    source_fk,
                    source_sig,
                    PassPOutcome::Deferred {
                        kind: DeferredKind::V2Pipeline,
                        record: src_record,
                    },
                );
            }

            if run.config.mvp_melee_only && !admitted_creature_dependency {
                let kind = interner.intern("mvp_melee_only");
                return Candidate::from_pass_p(
                    source_fk,
                    source_sig,
                    PassPOutcome::Dropped(Decision {
                        kind,
                        message: format!(
                            "{} {:06X} deferred to the dedicated melee corpus phase",
                            source_sig.as_str(),
                            source_fk.local
                        ),
                    }),
                );
            }
            let admitted_mvp_weapon_record = run.is_admitted_mvp_weapon_record(&src_record);

            if run.is_skyrim_runtime_planned_record(source_fk, source_sig) {
                let disposition = run
                    .skyrim_runtime_component_disposition(source_fk)
                    .expect("Skyrim runtime preflight validated every managed record");
                if unsupported_skyrim_runtime_record_blocks_translation(
                    disposition.supported,
                    admitted_creature_dependency,
                ) {
                    let kind = interner.intern("skyrim_runtime_component_unsupported");
                    return Candidate::from_pass_p(
                        source_fk,
                        source_sig,
                        PassPOutcome::Dropped(Decision {
                            kind,
                            message: format!(
                                "{} {:06X}: components=[{}]: {}",
                                source_sig.as_str(),
                                source_fk.local,
                                disposition.component_ids.join(","),
                                disposition.reason.as_deref().unwrap_or("unsupported")
                            ),
                        }),
                    );
                }
            }
            if run.is_skyrim_component_projection_owned(source_fk) {
                return Candidate::from_pass_p(
                    source_fk,
                    source_sig,
                    PassPOutcome::Deferred {
                        kind: DeferredKind::V2Pipeline,
                        record: src_record,
                    },
                );
            }

            if placed_record_has_excluded_base(&src_record, excluded_source_formkeys) {
                let kind = interner.intern("excluded_base");
                return Candidate::from_pass_p(
                    source_fk,
                    source_sig,
                    PassPOutcome::Dropped(Decision {
                        kind,
                        message: format!(
                            "{} {:06X} base record excluded from output",
                            source_sig.as_str(),
                            source_fk.local
                        ),
                    }),
                );
            }

            // FNV SCRI capture (FNV→FO4 only) — captured before the FNV pair
            // hook drops SCRI in pre_translate.
            let fnv_scri_target = if source == Game::Fnv && target == Game::Fo4 {
                let raw_form_id_resolver = |raw| {
                    Some(form_key_for_raw(
                        raw,
                        &source_ctx.plugin_name,
                        &source_ctx.masters,
                        interner,
                    ))
                };
                match crate::run::capture_fnv_scri_target_text(
                    &src_record,
                    interner,
                    &raw_form_id_resolver,
                ) {
                    Ok(target) => target,
                    Err(error) if run.config.fnv_quest_slice => {
                        let mut candidate = Candidate::from_pass_p(
                            source_fk,
                            source_sig,
                            PassPOutcome::ReadFailed,
                        );
                        candidate
                            .warnings
                            .push(format!("strict FNV quest slice SCRI capture failed: {error}"));
                        return candidate;
                    }
                    Err(error) => {
                        pass_p_warnings.push(format!("fnv_scri:{error}"));
                        None
                    }
                }
            } else {
                None
            };
            let creature_race_event = if run.should_apply_legacy_creature_race_policy() {
                match crate::translator::pair_hooks::fnv_creature_race::apply_legacy_creature_race_policy(
                    source,
                    target,
                    &mut src_record,
                    interner,
                ) {
                    Ok(event) => event,
                    Err(error) => {
                        let message = error.to_string();
                        let mut c = Candidate::from_pass_p(
                            source_fk,
                            source_sig,
                            PassPOutcome::DroppedUnsupported,
                        );
                        c.creature_race_failure_decision = Some(error.decision);
                        c.creature_race_fatal_error = Some(message);
                        return c;
                    }
                }
            } else {
                None
            };
            let lowered_acre = matches!(source, Game::Fnv | Game::Fo3)
                && target == Game::Fo4
                && source_sig.as_str() == "ACRE"
                && !translator
                    .maps
                    .skip_records
                    .contains(source_sig.as_str());
            if lowered_acre
                && let Err(error) =
                    crate::translator::pair_hooks::fnv_fo4::lower_acre_signature(&mut src_record)
            {
                let mut candidate = Candidate::from_pass_p(
                    source_fk,
                    source_sig,
                    PassPOutcome::DroppedUnsupported,
                );
                candidate.acre_fatal_error = Some(error.to_string());
                return candidate;
            }

            // pre_translate (errors → warning, record continues).
            {
                let warning_start = src_record.warnings.len();
                let mut ctx = crate::translator::pair_hook::PairCtx::with_source_and_target(
                    interner,
                    &source_ctx.plugin_name,
                    &source_ctx.masters,
                    &run.config.target_master_names,
                );
                if let Err(e) = translator.pre_translate(&mut ctx, &mut src_record) {
                    pass_p_warnings.push(format!("pre_translate:{e}"));
                }
                pass_p_warnings.extend(
                    src_record.warnings[warning_start..]
                        .iter()
                        .filter_map(|warning| interner.resolve(*warning))
                        .map(str::to_owned),
                );
            }

            // translate (TopLevel mode — no forced-skip-signature mutation).
            let translated = match if admitted_mvp_weapon_record
                || admitted_creature_dependency
                || is_mandatory_skyrim_actor_record(source, target, source_sig)
                || run.is_skyrim_runtime_planned_record(source_fk, source_sig)
            {
                translator.translate_ignoring_skip(
                    &src_record,
                    interner,
                    src_record.sig.as_str(),
                )
            } else if is_selected_fnv_quest_slice_mgef(run, source_fk, source_plugin, source_sig) {
                translator.translate_ignoring_skip(&src_record, interner, "MGEF")
            } else {
                translator.translate(&src_record, interner)
            } {
                TranslateResult::Translated(r) => r,
                TranslateResult::Dropped { decision, .. } => {
                    let mut c = Candidate::from_pass_p(
                        source_fk,
                        source_sig,
                        PassPOutcome::Dropped(decision),
                    );
                    c.warnings = pass_p_warnings;
                    return c;
                }
                TranslateResult::Deferred(kind) => {
                    let mut c = Candidate::from_pass_p(
                        source_fk,
                        source_sig,
                        PassPOutcome::Deferred {
                            kind,
                            record: src_record,
                        },
                    );
                    c.warnings = pass_p_warnings;
                    return c;
                }
            };

            // Unsupported target sig.
            if schema_target.record_def(translated.sig.as_str()).is_none() {
                let mut c =
                    Candidate::from_pass_p(source_fk, source_sig, PassPOutcome::DroppedUnsupported);
                c.warnings = pass_p_warnings;
                c.warnings.push(format!(
                    "unsupported_target_record:{} not in {} generated schema",
                    translated.sig.as_str(),
                    target.as_str()
                ));
                return c;
            }

            // FO76→FO4 EDID-collision rename. Gated on mapper_state present
            // (it is — init happens before Pass P).
            let mut translated = translated;
            let mut collision_donor = None;
            if source == Game::Fo76 && target == Game::Fo4 {
                if let Some(state) = mapper_state {
                    let translated_sig = translated.sig;
                    let renamed = rename_fo76_target_editor_id_collision(
                        &mut translated,
                        &state.target_eid_index,
                        &state.options.vanilla_remap_blocked_source_form_keys,
                        interner,
                        crate::run::is_editor_id_collision_rename_forced(
                            source,
                            target,
                            translated_sig,
                        ),
                    );
                    if let Some((old, new)) = renamed {
                        collision_donor = target_collision_donor_form_key(
                            &state.target_eid_index,
                            interner,
                            &old,
                            translated.sig,
                        );
                        pass_p_warnings
                            .push(format!("fo76_target_edid_collision_renamed:{old}->{new}"));
                    }
                }
            }

            let mut c = Candidate::from_pass_p(
                source_fk,
                source_sig,
                PassPOutcome::Translated {
                    record: translated,
                    fnv_scri_target,
                    lowered_acre,
                },
            );
            c.collision_donor = collision_donor;
            c.creature_race_event = creature_race_event;
            c.warnings = pass_p_warnings;
            c
        })
        .collect()
}

fn unsupported_skyrim_runtime_record_blocks_translation(
    runtime_component_supported: bool,
    admitted_creature_dependency: bool,
) -> bool {
    !runtime_component_supported && !admitted_creature_dependency
}

fn enforce_creature_race_gate(
    run: &mut ConversionRun,
    candidates: &[Candidate],
    observe_coverage: bool,
) -> Result<(), RunError> {
    let mut first_error = None;
    for candidate in candidates {
        if observe_coverage {
            if let Some(event) = candidate.creature_race_event.as_ref() {
                run.observe_legacy_creature_race_decision(&event.decision);
            }
            if let Some(decision) = candidate.creature_race_failure_decision.as_ref() {
                run.observe_legacy_creature_race_decision(decision);
            }
        }
        if first_error.is_none() {
            first_error = candidate.creature_race_fatal_error.clone();
        }
    }
    if let Some(error) = first_error {
        return Err(run.fail_legacy_creature_race(error));
    }
    Ok(())
}

fn enforce_acre_gate(candidates: &[Candidate]) -> Result<(), RunError> {
    if let Some(error) = candidates
        .iter()
        .find_map(|candidate| candidate.acre_fatal_error.as_deref())
    {
        return Err(RunError::InvalidConfig(format!("legacy_acre:{error}")));
    }
    Ok(())
}

fn enforce_creature_dependency_terminals(
    run: &ConversionRun,
    candidates: &[Candidate],
) -> Result<(), RunError> {
    for candidate in candidates {
        if !run.is_admitted_creature_dependency(candidate.source_fk, candidate.source_sig) {
            continue;
        }
        let disposition = match candidate.disposition {
            Disposition::Translated => continue,
            Disposition::Dropped => "dropped",
            Disposition::Deferred => "deferred",
            Disposition::Failed => "failed",
            Disposition::VanillaRemapped => "unexpected_preallocation_state",
        };
        return Err(run.creature_dependency_failure(
            candidate.source_fk,
            candidate.source_sig,
            disposition,
        ));
    }
    Ok(())
}

fn enforce_creature_dependency_post_translate_terminals(
    run: &ConversionRun,
    candidates: &[Candidate],
    stage: &str,
) -> Result<(), RunError> {
    for candidate in candidates {
        if !run.is_admitted_creature_dependency(candidate.source_fk, candidate.source_sig)
            || matches!(
                candidate.disposition,
                Disposition::Translated | Disposition::VanillaRemapped
            )
        {
            continue;
        }
        let disposition = match candidate.disposition {
            Disposition::Translated | Disposition::VanillaRemapped => unreachable!(),
            Disposition::Dropped => "dropped",
            Disposition::Deferred => "deferred",
            Disposition::Failed => "failed",
        };
        return Err(run.creature_dependency_failure(
            candidate.source_fk,
            candidate.source_sig,
            &format!("{stage}_{disposition}"),
        ));
    }
    Ok(())
}

fn validate_creature_race_targets(
    run: &mut ConversionRun,
    candidates: &[Candidate],
) -> Result<(), RunError> {
    for candidate in candidates {
        if !matches!(candidate.disposition, Disposition::Translated) {
            continue;
        }
        let Some(record) = candidate.record.as_ref() else {
            continue;
        };
        if let Err(error) =
            crate::translator::pair_hooks::fnv_creature_race::validate_crea_derived_npc_race(
                record,
                candidate.creature_race_event.as_ref(),
                &run.interner,
                |race| run.target_form_key_resolves_to_race(race),
            )
        {
            return Err(run.fail_legacy_creature_race(error.to_string()));
        }
    }
    Ok(())
}

fn collect_excluded_source_formkeys(
    run: &ConversionRun,
    esm: &SourceEsm,
    enumeration: &[(SigCode, usize)],
    source_ctx: &SourceCtx,
) -> FxHashSet<FormKey> {
    enumeration
        .iter()
        .filter(|(sig, _)| run.translator.maps.skip_records.contains(sig.as_str()))
        .filter(|(sig, _)| !is_mandatory_skyrim_actor_record(run.source, run.target, *sig))
        .filter_map(|(signature, pos)| {
            let raw = esm.view_at(*pos)?.form_id();
            let form_key = form_key_for_raw(
                raw,
                &source_ctx.plugin_name,
                &source_ctx.masters,
                &run.interner,
            );
            (!run.config.is_mvp_record_exception(
                run.source,
                run.target,
                form_key,
                *signature,
                &run.interner,
            ) && !run.is_admitted_creature_dependency(form_key, *signature))
            .then_some(form_key)
        })
        .collect()
}

fn placed_record_has_excluded_base(
    record: &Record,
    excluded_source_formkeys: &FxHashSet<FormKey>,
) -> bool {
    if !matches!(record.sig.as_str(), "REFR" | "ACHR") {
        return false;
    }
    record.fields.iter().any(|field| {
        field.sig.as_str() == "NAME"
            && matches!(&field.value, crate::record::FieldValue::FormKey(fk) if excluded_source_formkeys.contains(fk))
    })
}

/// Pass A — serial, position order: resolve FormKeys + serial normalizers +
/// NVNM + generic reference rewrite. Legacy Fallout records were preallocated
/// before this pass, so their raw references can resolve across signatures.
fn run_pass_a(
    run: &mut ConversionRun,
    target_master_syms: &FxHashSet<Sym>,
    skyrim_navmeshes: Option<&SkyrimNavmeshSet>,
    candidates: &mut [Candidate],
) -> Result<(), RunError> {
    let source = run.source;
    let target = run.target;
    let source_handle_id = run.source_handle_id;
    let target_handle_id = run.target_handle_id;
    let interner = &run.interner;
    let translator = &run.translator;
    let event_tx = run.event_tx.clone();
    let serial_normalization = &mut run.legacy_serial_normalization;
    let placed_actor_aliases = &mut run.legacy_placed_actor_aliases;
    let state = run
        .mapper_state
        .as_mut()
        .expect("mapper_state initialized before Pass A");
    let mut mapper = FormKeyMapper::from_state(state, interner);

    for c in candidates.iter_mut() {
        if !matches!(c.disposition, Disposition::Translated) {
            continue;
        }
        let Some(record) = c.record.as_mut() else {
            continue;
        };

        let normalized_eid = normalized_eid_opt(record.eid, mapper.interner);
        let target_fk = mapper.allocate_or_resolve(c.source_fk, normalized_eid, record.sig);
        record.form_key = target_fk;
        if c.lowered_acre {
            crate::translator::pair_hooks::fnv_fo4::finalize_lowered_acre(
                record,
                c.source_fk,
                target_fk,
                &mut mapper,
                placed_actor_aliases,
                interner,
            )
            .map_err(|error| RunError::InvalidConfig(format!("legacy_acre:{error}")))?;
        }
        if let Some(race) = c
            .creature_race_event
            .as_ref()
            .and_then(|event| event.decision.audited_race().ok().flatten())
        {
            mapper.add_mapping(race, race);
        }

        let mut serial_drop = false;
        if let Some(outcome) = translator.normalize_serial_mapper_record_once(
            c.source_fk,
            record,
            &mut mapper,
            serial_normalization,
        ) {
            let diagnostics = match outcome {
                Ok(report) => {
                    report.register_target_identities(&mut mapper);
                    report.diagnostics(record)
                }
                Err(diagnostic) => {
                    serial_drop = true;
                    vec![diagnostic]
                }
            };
            for diagnostic in diagnostics {
                let level = if diagnostic.warning {
                    LogLevel::Warn
                } else {
                    LogLevel::Info
                };
                let _ = event_tx.try_send(PhaseEvent::Log {
                    phase: "translate_v2",
                    level,
                    message: diagnostic.message.clone(),
                });
                if diagnostic.warning {
                    c.warnings.push(diagnostic.message);
                }
            }
        }
        if serial_drop {
            c.disposition = Disposition::Dropped;
            c.record = None;
            continue;
        }

        if source == Game::SkyrimSe && target == Game::Fo4 && record.sig.as_str() == "NAVM" {
            let install_result = skyrim_navmeshes
                .and_then(|navmeshes| navmeshes.converted.get(&c.source_fk))
                .ok_or_else(|| {
                    skyrim_navmeshes
                        .and_then(|navmeshes| navmeshes.failures.get(&c.source_fk))
                        .cloned()
                        .unwrap_or_else(|| {
                            format!(
                                "no converted v15 geometry for Skyrim NAVM {:06X}",
                                c.source_fk.local
                            )
                        })
                })
                .and_then(|bytes| crate::skyrim_navmesh::install_converted_nvnm(record, bytes));
            if let Err(error) = install_result {
                c.warnings.push(format!("skyrim_navm:{error}"));
                c.disposition = Disposition::Failed;
            }
        }
        if matches!(c.disposition, Disposition::Failed) {
            c.record = None;
            continue;
        }

        if matches!(source, Game::Fo76 | Game::SkyrimSe)
            && target == Game::Fo4
            && record
                .fields
                .iter()
                .any(|field| field.sig.0 == *b"NVNM" || field.sig.0 == *b"MNAM")
        {
            if let Err(e) = crate::fo76_navmesh::rewrite_record_nvnm_for_fo4(
                record,
                &mut mapper,
                source_handle_id,
                target_handle_id,
            ) {
                c.warnings.push(format!("{}_navm:{e}", source.as_str()));
            }
        }
        match mapper.rewrite_record_with_report(record) {
            Ok(report) => c.unresolved_source_refs = report.unresolved_form_keys,
            Err(e) => c.warnings.push(format!("rewrite_record:{e}")),
        }

        if is_target_master_remap(target_fk, target_master_syms) {
            c.disposition = Disposition::VanillaRemapped;
            c.record = None;
            continue;
        }

        // FNV SCRI link — needs target_fk.
        if let Some(source_scpt_form_key) = c.fnv_scri_target.take()
            && let Err(error) = crate::run::append_fnv_scri_link(
                &mut c.fnv_scri_links,
                target_fk,
                &source_scpt_form_key,
                interner,
            )
        {
            c.warnings
                .push(format!("translated FNV SCRI attachment: {error}"));
            if run.config.fnv_quest_slice {
                c.disposition = Disposition::Failed;
                c.record = None;
            }
        }
    }
    drop(mapper);

    for candidate in candidates.iter_mut() {
        if !matches!(candidate.disposition, Disposition::Translated) {
            continue;
        }
        let (Some(record), Some(donor_form_key)) =
            (candidate.record.as_mut(), candidate.collision_donor)
        else {
            continue;
        };
        if let Err(error) = run.merge_target_collision_donor(record, donor_form_key) {
            candidate
                .warnings
                .push(format!("collision_donor_merge:{error}"));
        }
    }
    Ok(())
}

/// Pass F — parallel: post hooks → class-A → normalizer → namespacing → snapshot.
fn run_pass_f(run: &mut ConversionRun, source_ctx: &SourceCtx, candidates: &mut [Candidate]) {
    let interner = &run.interner;
    let translator = &run.translator;
    let schema_target = &*run.schema_target;
    let schema_source = &*run.schema_source;
    let source = run.source;
    let target = run.target;
    let is_whole_plugin = run.config.is_whole_plugin;
    let relocation_members = &run.relocation_members;
    let namespace = base_asset_namespace(&run.config, source, target).unwrap_or("");

    candidates.par_iter_mut().for_each(|c| {
        if !matches!(c.disposition, Disposition::Translated) {
            return;
        }
        let Some(mut record) = c.record.take() else {
            return;
        };
        let source_sig = c.source_sig;

        // post_translate.
        {
            let mut ctx = crate::translator::pair_hook::PairCtx::with_source_and_target(
                interner,
                &source_ctx.plugin_name,
                &source_ctx.masters,
                &run.config.target_master_names,
            );
            if let Err(e) = translator.post_translate(&mut ctx, &mut record) {
                c.warnings.push(format!("post_translate:{e}"));
            }
        }
        // target hook.
        {
            let mut ctx = crate::translator::target_hook::TargetCtx { interner };
            if let Err(e) = translator.run_target_hook(&mut ctx, &mut record) {
                c.warnings.push(format!("target_hook:{e}"));
            }
        }
        // Class A flag/enum normalization (FO76→FO4 only).
        if source == Game::Fo76 && target == Game::Fo4 {
            let report = crate::translator::class_a_normalize::normalize_flags_and_enums(
                &mut record,
                schema_target,
                interner,
            );
            for message in report.decisions {
                let kind = interner.intern("class_a_normalize");
                c.decisions.push(Decision { kind, message });
            }
            for w in report.warnings {
                c.warnings.push(w);
            }
        }
        // TargetRecordNormalizer.
        let normalizer = TargetRecordNormalizer {
            target_schema: schema_target,
            source_record_def: schema_source.record_def(source_sig.as_str()),
            interner: Some(interner),
        };
        let mut record = match normalizer.normalize(record) {
            TargetRecordNormalization::Keep(r) => r,
            TargetRecordNormalization::DropUnsupportedRecord => {
                c.disposition = Disposition::Dropped;
                return;
            }
        };
        // Base-asset model-path namespacing.
        namespace_base_asset_model_paths(&mut record, relocation_members, namespace, interner);
        crate::run::namespace_base_asset_decal_material_path(
            &mut record,
            relocation_members,
            namespace,
            interner,
        );

        // Whole-plugin snapshot.
        if is_whole_plugin {
            c.full_plugin_snapshot = Some(crate::full_plugin::target_schema_record_view(
                &record,
                schema_target,
            ));
        }
        c.record = Some(record);
    });
}

/// Pass E — serial, position order, one store-lock per chunk: encode + insert.
/// Preserves localized-string id allocation order bit-for-bit (legacy serial
/// `add_record_native` encode order).
fn run_pass_e(
    run: &mut ConversionRun,
    target_master_syms: &FxHashSet<Sym>,
    first_target_master_sym: Option<Sym>,
    candidates: &mut [Candidate],
) {
    let target_handle_id = run.target_handle_id;
    let schema_target = run.schema_target.clone();
    let is_whole_plugin = run.config.is_whole_plugin;
    let is_skyrim_to_fo4 = run.source == Game::SkyrimSe && run.target == Game::Fo4;

    let mut wrote_any = false;
    {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = match store.get_mut(&target_handle_id) {
            Some(slot) => slot,
            None => {
                for c in candidates.iter_mut() {
                    if matches!(c.disposition, Disposition::Translated) && c.record.is_some() {
                        c.warnings
                            .push(format!("write_error:no plugin handle: {target_handle_id}"));
                        c.disposition = Disposition::Failed;
                        c.record = None;
                    }
                }
                return;
            }
        };

        for c in candidates.iter_mut() {
            if !matches!(c.disposition, Disposition::Translated) {
                continue;
            }
            let Some(record) = c.record.take() else {
                c.disposition = Disposition::Failed;
                continue;
            };
            let is_navmesh = record.sig.as_str() == "NAVM";
            let write_result = if is_skyrim_to_fo4 && is_navmesh {
                add_skyrim_navmesh_record_in_slot(slot, record, &schema_target, &run.interner)
            } else {
                add_record_in_slot(slot, record, &schema_target, &run.interner)
            };
            match write_result {
                Ok(()) => {
                    wrote_any = true;
                    // Defer full-plugin capture until after the lock is dropped
                    // (capture mutates run.full_plugin_state, not the slot) so we
                    // keep `&run.interner` borrow scoping simple. We stash the
                    // snapshot on the candidate (already there) and capture below.
                }
                Err(e) => {
                    c.warnings.push(format!("write_error:{e}"));
                    c.disposition = Disposition::Failed;
                    c.full_plugin_snapshot = None;
                }
            }
        }

        if wrote_any {
            slot.apply_write_effect(
                &esp_authoring_core::plugin_runtime::WriteEffect::RecordsAddedOrRemoved,
            );
        }
    }

    // Full-plugin record-ref capture, in position order, only for records that
    // were successfully written.
    if is_whole_plugin {
        for c in candidates.iter_mut() {
            if !matches!(c.disposition, Disposition::Translated) {
                continue;
            }
            if let Some(snapshot) = c.full_plugin_snapshot.take() {
                run.capture_full_plugin_record_state(
                    c.source_fk,
                    &snapshot,
                    target_master_syms,
                    first_target_master_sym,
                    Some(&c.unresolved_source_refs),
                );
            }
        }
    }
}

/// Merge one chunk's candidates into the run's stats/warnings/decisions/etc.,
/// strictly in position order.
fn merge_chunk(run: &mut ConversionRun, stats: &mut TranslateStats, candidates: Vec<Candidate>) {
    for c in candidates {
        if matches!(
            c.disposition,
            Disposition::Translated | Disposition::VanillaRemapped
        ) {
            run.record_creature_dependency_terminal(c.source_fk);
        }
        if c.seen {
            stats.signature_entry(c.source_sig).seen += 1;
        }
        match c.disposition {
            Disposition::Translated => {
                stats.records_translated += 1;
                stats.signature_entry(c.source_sig).translated += 1;
            }
            Disposition::Dropped => {
                stats.records_dropped += 1;
                stats.signature_entry(c.source_sig).dropped += 1;
            }
            Disposition::Deferred => {
                stats.records_deferred += 1;
                stats.signature_entry(c.source_sig).deferred += 1;
            }
            Disposition::Failed => {
                stats.records_failed += 1;
                if c.seen {
                    stats.signature_entry(c.source_sig).failed += 1;
                }
            }
            Disposition::VanillaRemapped => {
                stats.records_vanilla_remapped += 1;
                stats.signature_entry(c.source_sig).vanilla_remapped += 1;
            }
        }

        for (fk, kind) in c.deferred {
            run.deferred.push((fk, kind));
        }
        for decision in c.decisions {
            run.decisions.push(decision);
        }
        for warning in c.warnings {
            let sym = run.interner.intern(&warning);
            run.warnings.push(sym);
        }
        for link in c.fnv_scri_links {
            run.fnv_scri_links.push(link);
        }
    }
}

#[cfg(test)]
mod equivalence_tests {
    use super::*;
    use crate::legacy_pack_preflight::{
        LegacyPackExpectedCounts, LegacyPackOriginRow, LegacyPackPreflightReport,
    };
    use crate::record::{FieldEntry, FieldValue, Record};
    use crate::run::{
        CreatureDependencyAdmission, MvpRecordExceptionRow, RunConfig, RunParams, create_run,
        drop_run, with_run,
    };
    use crate::source_read::plugin_name_for_handle;
    use crate::store2::source::COMPRESSED_RECORD_FLAG;
    use crate::store2::source::test_fixture::*;
    use crate::store2::test_util::{assert_handles_equal, handle_records};
    use crate::translator::pair_hooks::fnv_pack::{LegacyPackSourceFamily, LegacyPackType};
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedRecord, parse_plugin_file, plugin_handle_add_master_native,
        plugin_handle_close_native, plugin_handle_new_native, plugin_handle_store_ref,
    };

    #[test]
    fn fnv_legacy_deferred_records_are_preallocation_eligible() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let source_fk = FormKey {
            local: 0x13015B,
            plugin,
        };
        let mut record = Record::new(SigCode::from_str("DIAL").unwrap(), source_fk);
        record.eid = Some(interner.intern("VDialogue13015B"));
        let candidate = Candidate::from_pass_p(
            source_fk,
            record.sig,
            PassPOutcome::Deferred {
                kind: DeferredKind::FnvLegacyScripting,
                record,
            },
        );

        let intent = legacy_preallocation_intent(&candidate).expect("deferred DIAL identity");
        assert_eq!(intent.source_fk, source_fk);
        assert_eq!(intent.target_sig.as_str(), "DIAL");
        assert_eq!(intent.editor_id, Some(interner.intern("VDialogue13015B")));
    }

    #[test]
    fn placed_records_with_excluded_bases_are_identified() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("Skyrim.esm");
        let excluded_base = FormKey {
            plugin: source_plugin,
            local: 0x1234,
        };
        let retained_base = FormKey {
            plugin: source_plugin,
            local: 0x5678,
        };
        let excluded = FxHashSet::from_iter([excluded_base]);
        let mut refr = Record::new(
            SigCode::from_str("REFR").unwrap(),
            FormKey {
                plugin: source_plugin,
                local: 0x9000,
            },
        );
        refr.fields.push(FieldEntry {
            sig: crate::ids::SubrecordSig::from_str("NAME").unwrap(),
            value: FieldValue::FormKey(excluded_base),
        });

        assert!(placed_record_has_excluded_base(&refr, &excluded));

        refr.sig = SigCode::from_str("ACHR").unwrap();
        assert!(placed_record_has_excluded_base(&refr, &excluded));

        refr.fields[0].value = FieldValue::FormKey(retained_base);
        assert!(!placed_record_has_excluded_base(&refr, &excluded));

        refr.sig = SigCode::from_str("STAT").unwrap();
        refr.fields[0].value = FieldValue::FormKey(excluded_base);
        assert!(!placed_record_has_excluded_base(&refr, &excluded));
    }

    /// Source plugin exercising: multiple sigs (sorted-sig order != file order),
    /// an own-plugin formid subrecord (PTRN, rewritten by the mapper), an
    /// EDID-less record, a compressed record, and >1 record per sig (allocation
    /// order). Sorted sigs = KYWD < MISC < WEAP, so WEAP allocates last.
    fn gate_fixture() -> Vec<u8> {
        // Own-index form_ids (master byte 0x00 == own_index for a no-master
        // plugin) so the FormKey is the plugin's own, matching both the legacy
        // CoreSection/locator and store2's `form_key_for_raw`.
        let keyw = record(b"KEYW", 0x0000_0901, 0, &subrecord(b"EDID", b"KeywOne\0"));
        // WEAP 0x801: EDID + PTRN (single formid -> a record allocated later;
        // unmapped at rewrite time in both paths, so it stays as-is identically).
        let mut weap1_payload = subrecord(b"EDID", b"WeapOne\0");
        weap1_payload.extend_from_slice(&subrecord(b"PTRN", &0x0000_0903u32.to_le_bytes()));
        let weap1 = record(b"WEAP", 0x0000_0801, 0, &weap1_payload);
        // WEAP 0x802: compressed, EDID only.
        let weap2 = record(
            b"WEAP",
            0x0000_0802,
            COMPRESSED_RECORD_FLAG,
            &compressed_payload(&subrecord(b"EDID", b"WeapTwo\0")),
        );
        // MISC 0x903: EDID; MISC 0x904: EDID-less (no subrecords).
        let misc = record(b"MISC", 0x0000_0903, 0, &subrecord(b"EDID", b"MiscOne\0"));
        let noeid = record(b"MISC", 0x0000_0904, 0, &[]);
        plugin(
            &[],
            &[
                group(b"WEAP", 0, &[weap1, weap2]),
                group(b"KEYW", 0, &[keyw]),
                group(b"MISC", 0, &[misc, noeid]),
            ],
        )
    }

    fn fnv_hatchet_payload(editor_id: &str, attack_shots_sec: f32) -> Vec<u8> {
        let mut payload = subrecord(b"EDID", format!("{editor_id}\0").as_bytes());
        let mut bounds = [0_u8; 12];
        for (index, value) in [-3_i16, -10, -1, 8, 22, 1].into_iter().enumerate() {
            bounds[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
        payload.extend_from_slice(&subrecord(b"OBND", &bounds));
        payload.extend_from_slice(&subrecord(b"FULL", b"Hatchet\0"));
        payload.extend_from_slice(&subrecord(b"MODL", b"weapons\\1handmelee\\Hatchet.NIF\0"));
        payload.extend_from_slice(&subrecord(
            b"ICON",
            b"interface\\icons\\pipboyimages\\weapons\\weapons_hatchet.dds\0",
        ));
        for (signature, local) in [
            (b"EITM", 0x10_9A08_u32),
            (b"REPL", 0x11_BC5E),
            (b"YNAM", 0x08_54E0),
            (b"ZNAM", 0x08_54DF),
        ] {
            payload.extend_from_slice(&subrecord(signature, &local.to_le_bytes()));
        }
        payload.extend_from_slice(&subrecord(b"ETYP", &3_i32.to_le_bytes()));
        payload.extend_from_slice(&subrecord(b"VANM", b"Back Slash\0"));
        for (signature, local) in [
            (b"INAM", 0x06_0A0F_u32),
            (b"WNAM", 0x11_A8E3),
            (b"NAM6", 0x0B_F0EF),
            (b"NAM9", 0x08_B5B2),
            (b"NAM8", 0x08_B5B3),
        ] {
            payload.extend_from_slice(&subrecord(signature, &local.to_le_bytes()));
        }

        let mut data = [0_u8; 15];
        data[0..4].copy_from_slice(&75_i32.to_le_bytes());
        data[4..8].copy_from_slice(&300_i32.to_le_bytes());
        data[8..12].copy_from_slice(&2.0_f32.to_le_bytes());
        data[12..14].copy_from_slice(&16_i16.to_le_bytes());
        data[14] = 1;
        payload.extend_from_slice(&subrecord(b"DATA", &data));

        let mut dnam = [0_u8; 204];
        dnam[0..4].copy_from_slice(&1_u32.to_le_bytes());
        dnam[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
        dnam[8..12].copy_from_slice(&0.7_f32.to_le_bytes());
        dnam[13] = 255;
        dnam[14] = 1;
        dnam[40] = 12;
        dnam[41] = 255;
        dnam[42] = 1;
        dnam[44..48].copy_from_slice(&500.0_f32.to_le_bytes());
        dnam[48..52].copy_from_slice(&2000.0_f32.to_le_bytes());
        dnam[52..56].copy_from_slice(&1_u32.to_le_bytes());
        dnam[56..60].copy_from_slice(&10_u32.to_le_bytes());
        dnam[60..64].copy_from_slice(&1.1_f32.to_le_bytes());
        dnam[68..72].copy_from_slice(&22.0_f32.to_le_bytes());
        dnam[88..92].copy_from_slice(&attack_shots_sec.to_le_bytes());
        dnam[104..108].copy_from_slice(&38_i32.to_le_bytes());
        dnam[116..120].copy_from_slice(&1.5_f32.to_le_bytes());
        dnam[120..124].copy_from_slice(&(-1_i32).to_le_bytes());
        dnam[164..168].copy_from_slice(&99_u32.to_le_bytes());
        dnam[168..172].copy_from_slice(&2_u32.to_le_bytes());
        dnam[200..204].copy_from_slice(&25_u32.to_le_bytes());
        payload.extend_from_slice(&subrecord(b"DNAM", &dnam));

        let mut critical = [0_u8; 16];
        critical[0..2].copy_from_slice(&16_u16.to_le_bytes());
        critical[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
        critical[8] = 1;
        payload.extend_from_slice(&subrecord(b"CRDT", &critical));

        let mut vats = [0_u8; 20];
        vats[4..8].copy_from_slice(&50.0_f32.to_le_bytes());
        vats[8..12].copy_from_slice(&0.7_f32.to_le_bytes());
        vats[12..16].copy_from_slice(&17.0_f32.to_le_bytes());
        vats[16] = 1;
        payload.extend_from_slice(&subrecord(b"VATS", &vats));
        payload.extend_from_slice(&subrecord(b"VNAM", &0_u32.to_le_bytes()));
        payload
    }

    #[test]
    fn fnv_v2_mvp_bypasses_weap_skip_only_for_exact_hatchet() {
        let exact = record(
            b"WEAP",
            0x11_A8E4,
            0,
            &fnv_hatchet_payload("WeapNVHatchet", 2.538_461_2),
        );
        let throwing = record(
            b"WEAP",
            0x14_DE1D,
            0,
            &fnv_hatchet_payload("WeapNVThrowingHatchet", 2.538_461_2),
        );
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("FalloutNV.esm");
        std::fs::write(
            &source_path,
            plugin(&[], &[group(b"WEAP", 0, &[exact, throwing])]),
        )
        .unwrap();
        let source_handle = load_source_handle(&source_path, "fnv");
        let debug_interner = StringInterner::new();
        let debug_schema = crate::schema::AuthoringSchema::for_game("fnv").unwrap();
        let debug_source = crate::source_read::read_record(
            source_handle,
            "11A8E4@FalloutNV.esm",
            &debug_schema,
            &debug_interner,
        )
        .unwrap();
        assert!(
            crate::translator::pair_hooks::fnv_fo4::classify_hatchet(
                &debug_source,
                &debug_interner
            )
            .is_some(),
            "decoded fixture: {debug_source:#?}"
        );
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: false,
                preserve_source_ids: true,
                skip_record_signatures: vec!["WEAP".to_string()],
                mvp_record_exceptions: vec![
                    MvpRecordExceptionRow {
                        signature: "WEAP".to_string(),
                        local_form_id: 0x11_A8E4,
                        source_plugin: "FalloutNV.esm".to_string(),
                    },
                    MvpRecordExceptionRow {
                        signature: "STAT".to_string(),
                        local_form_id: 0x11_A8E3,
                        source_plugin: "FalloutNV.esm".to_string(),
                    },
                ],
                ..Default::default()
            },
        })
        .unwrap();

        let stats = with_run(id, |run| run.translate_all_v2(&source_path)).unwrap();
        let weapons = handle_records(target_handle)
            .into_iter()
            .filter(|record| record.signature.as_str() == "WEAP")
            .collect::<Vec<_>>();
        assert_eq!(stats.records_translated, 1);
        assert_eq!(stats.records_dropped, 1);
        assert_eq!(weapons.len(), 1);
        assert_eq!(subrecord_data(&weapons[0], "DNAM").len(), 132);
        assert_eq!(subrecord_data(&weapons[0], "CRDT").len(), 12);
        assert_eq!(
            u32::from_le_bytes(subrecord_data(&weapons[0], "ETYP").try_into().unwrap())
                & 0x00FF_FFFF,
            0x01_3F42
        );

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn melee_only_suppresses_generic_translation_and_emits_dedicated_melee() {
        let weapon = record(
            b"WEAP",
            0x11_A8E4,
            0,
            &fnv_hatchet_payload("WeapNVHatchet", 2.538_461_2),
        );
        let mut first_person_model_payload = subrecord(b"EDID", b"WeapNVHatchet1st\0");
        first_person_model_payload.extend_from_slice(&subrecord(
            b"MODL",
            b"weapons\\1handmelee\\Hatchet1st.NIF\0",
        ));
        let first_person_model = record(b"STAT", 0x11_A8E3, 0, &first_person_model_payload);
        let misc = record(
            b"MISC",
            0x00_0903,
            0,
            &subrecord(b"EDID", b"UnrelatedWorldRecord\0"),
        );
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("FalloutNV.esm");
        std::fs::write(
            &source_path,
            plugin(
                &[],
                &[
                    group(b"WEAP", 0, &[weapon]),
                    group(b"STAT", 0, &[first_person_model]),
                    group(b"MISC", 0, &[misc]),
                ],
            ),
        )
        .unwrap();
        let source_handle = load_source_handle(&source_path, "fnv");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: false,
                preserve_source_ids: true,
                skip_record_signatures: vec!["WEAP".to_string()],
                mvp_melee_only: true,
                ..Default::default()
            },
        })
        .unwrap();

        let stats = with_run(id, |run| run.translate_all_v2(&source_path)).unwrap();
        assert_eq!(stats.records_translated, 0);
        assert_eq!(stats.records_dropped, 3);
        assert!(handle_records(target_handle).is_empty());

        let report = crate::phase::run_phase(
            id,
            "mvp_melee",
            crate::phase::DispatchParams {
                mod_path: temp.path().to_path_buf(),
                source_extracted_dir: temp.path().to_path_buf(),
                target_extracted_dir: None,
                target_data_dir: None,
                params: serde_json::json!({}),
            },
        )
        .unwrap();
        assert_eq!(report.records_added, 1);
        let output = handle_records(target_handle);
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].signature.as_str(), "WEAP");

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn fnv_v2_partial_mvp_weapon_closure_does_not_bypass_weap_skip() {
        let exact = record(
            b"WEAP",
            0x11_A8E4,
            0,
            &fnv_hatchet_payload("WeapNVHatchet", 2.538_461_2),
        );
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("FalloutNV.esm");
        std::fs::write(&source_path, plugin(&[], &[group(b"WEAP", 0, &[exact])])).unwrap();
        let source_handle = load_source_handle(&source_path, "fnv");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                preserve_source_ids: true,
                skip_record_signatures: vec!["WEAP".to_string()],
                mvp_record_exceptions: vec![MvpRecordExceptionRow {
                    signature: "WEAP".to_string(),
                    local_form_id: 0x11_A8E4,
                    source_plugin: "FalloutNV.esm".to_string(),
                }],
                ..Default::default()
            },
        })
        .unwrap();

        let stats = with_run(id, |run| run.translate_all_v2(&source_path)).unwrap();
        assert_eq!(stats.records_translated, 0);
        assert_eq!(stats.records_dropped, 1);
        assert!(
            handle_records(target_handle)
                .iter()
                .all(|record| record.signature.as_str() != "WEAP")
        );

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn fnv_v2_non_mvp_hatchet_translation_does_not_require_exceptions() {
        let exact = record(
            b"WEAP",
            0x11_A8E4,
            0,
            &fnv_hatchet_payload("WeapNVHatchet", 2.538_461_2),
        );
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("FalloutNV.esm");
        std::fs::write(&source_path, plugin(&[], &[group(b"WEAP", 0, &[exact])])).unwrap();
        let source_handle = load_source_handle(&source_path, "fnv");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                preserve_source_ids: true,
                ..Default::default()
            },
        })
        .unwrap();

        let stats = with_run(id, |run| run.translate_all_v2(&source_path)).unwrap();
        assert_eq!(stats.records_translated, 1);
        assert_eq!(stats.records_dropped, 0);
        assert_eq!(
            handle_records(target_handle)
                .iter()
                .filter(|record| record.signature.as_str() == "WEAP")
                .count(),
            1
        );

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    fn skyrim_steel_battleaxe_fixture() -> Vec<u8> {
        let stat_payload = {
            let mut payload = subrecord(b"EDID", b"1stPersonSteelBattleaxe\0");
            payload.extend_from_slice(&subrecord(
                b"MODL",
                b"Weapons\\Steel\\1stPersonSteelBattleAxe.nif\0",
            ));
            payload
        };
        let mut weapon_payload = subrecord(b"EDID", b"SteelBattleaxe\0");
        weapon_payload.extend_from_slice(&subrecord(b"OBND", &[0_u8; 12]));
        weapon_payload.extend_from_slice(&subrecord(b"FULL", b"Steel Battleaxe\0"));
        weapon_payload
            .extend_from_slice(&subrecord(b"MODL", b"Weapons\\Steel\\SteelBattleAxe.nif\0"));
        weapon_payload.extend_from_slice(&subrecord(b"ETYP", &0x01_3F45_u32.to_le_bytes()));
        weapon_payload.extend_from_slice(&subrecord(b"WNAM", &0x02_0E27_u32.to_le_bytes()));
        let mut data = [0_u8; 10];
        data[0..4].copy_from_slice(&100_u32.to_le_bytes());
        data[4..8].copy_from_slice(&21.0_f32.to_le_bytes());
        data[8..10].copy_from_slice(&18_u16.to_le_bytes());
        weapon_payload.extend_from_slice(&subrecord(b"DATA", &data));
        let mut dnam = [0_u8; 100];
        dnam[0] = 6;
        weapon_payload.extend_from_slice(&subrecord(b"DNAM", &dnam));
        plugin(
            &[],
            &[
                group(
                    b"STAT",
                    0,
                    &[
                        record(b"STAT", 0x02_0E27, 0, &stat_payload),
                        record(b"STAT", 0x02_0E28, 0, &stat_payload),
                    ],
                ),
                group(
                    b"WEAP",
                    0,
                    &[
                        record(b"WEAP", 0x01_3984, 0, &weapon_payload),
                        record(b"WEAP", 0x01_3985, 0, &weapon_payload),
                    ],
                ),
            ],
        )
    }

    #[test]
    fn skyrim_v2_mvp_translates_only_the_exact_weapon_closure_through_skips() {
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("Skyrim.esm");
        std::fs::write(&source_path, skyrim_steel_battleaxe_fixture()).unwrap();
        let source_handle = load_source_handle(&source_path, "skyrimse");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                preserve_source_ids: true,
                skip_record_signatures: vec!["WEAP".to_string(), "STAT".to_string()],
                mvp_record_exceptions: vec![
                    MvpRecordExceptionRow {
                        signature: "WEAP".to_string(),
                        local_form_id: 0x01_3984,
                        source_plugin: "Skyrim.esm".to_string(),
                    },
                    MvpRecordExceptionRow {
                        signature: "STAT".to_string(),
                        local_form_id: 0x02_0E27,
                        source_plugin: "Skyrim.esm".to_string(),
                    },
                ],
                ..Default::default()
            },
        })
        .unwrap();

        let stats = with_run(id, |run| run.translate_all_v2(&source_path)).unwrap();
        let records = handle_records(target_handle);
        assert_eq!(stats.records_translated, 2);
        assert_eq!(stats.records_dropped, 2);
        assert_eq!(
            records
                .iter()
                .filter(|record| matches!(record.signature.as_str(), "WEAP" | "STAT"))
                .count(),
            2
        );

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    fn legacy_effect_payload(base_effect: u32) -> Vec<u8> {
        let mut payload = subrecord(b"EFID", &base_effect.to_le_bytes());
        let mut efit = vec![0_u8; 20];
        efit[0..4].copy_from_slice(&7_u32.to_le_bytes());
        efit[4..8].copy_from_slice(&3_u32.to_le_bytes());
        efit[8..12].copy_from_slice(&11_u32.to_le_bytes());
        payload.extend_from_slice(&subrecord(b"EFIT", &efit));
        payload
    }

    fn legacy_condition(function: u16, parameter_1: u32) -> Vec<u8> {
        let mut condition = vec![0_u8; 28];
        condition[8..10].copy_from_slice(&function.to_le_bytes());
        condition[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        condition
    }

    fn legacy_mgef_data(
        assoc_item: u32,
        casting_light: u32,
        shader: u32,
        archetype: u32,
        actor_value: i32,
    ) -> Vec<u8> {
        let mut data = vec![0_u8; 72];
        data[8..12].copy_from_slice(&assoc_item.to_le_bytes());
        data[16..20].copy_from_slice(&(-1_i32).to_le_bytes());
        data[24..28].copy_from_slice(&casting_light.to_le_bytes());
        data[32..36].copy_from_slice(&shader.to_le_bytes());
        data[36..40].copy_from_slice(&shader.to_le_bytes());
        data[64..68].copy_from_slice(&archetype.to_le_bytes());
        data[68..72].copy_from_slice(&actor_value.to_le_bytes());
        data
    }

    fn legacy_magic_fixture() -> Vec<u8> {
        const ALCH_ID: u32 = 0x0000_1000;
        const ENCH_ID: u32 = 0x0000_1001;
        const SPEL_ID: u32 = 0x0000_1002;
        const PRIMARY_MGEF_ID: u32 = 0x0000_1100;
        const ASSOC_MGEF_ID: u32 = 0x0000_1101;
        const LIGH_ID: u32 = 0x0000_1200;
        const EFSH_ID: u32 = 0x0000_1300;
        const PERK_ID: u32 = 0x0000_1400;
        const WRLD_ID: u32 = 0x0000_1500;

        let mut alch_payload = subrecord(b"EDID", b"ForwardPotion\0");
        alch_payload.extend_from_slice(&subrecord(b"ENIT", &[0_u8; 20]));
        alch_payload.extend_from_slice(&legacy_effect_payload(PRIMARY_MGEF_ID));
        let alch = record(b"ALCH", ALCH_ID, 0, &alch_payload);

        let mut ench_enit = vec![0_u8; 16];
        ench_enit[0..4].copy_from_slice(&3_u32.to_le_bytes());
        ench_enit[12] = 1;
        let mut ench_payload = subrecord(b"EDID", b"ForwardEnchant\0");
        ench_payload.extend_from_slice(&subrecord(b"ENIT", &ench_enit));
        ench_payload.extend_from_slice(&legacy_effect_payload(PRIMARY_MGEF_ID));
        let ench = record(b"ENCH", ENCH_ID, 0, &ench_payload);

        let mut spit = vec![0_u8; 16];
        spit[0..4].copy_from_slice(&2_u32.to_le_bytes());
        spit[4..8].copy_from_slice(&25_u32.to_le_bytes());
        spit[12] = 0x15;
        let mut spel_payload = subrecord(b"EDID", b"ForwardSpell\0");
        spel_payload.extend_from_slice(&subrecord(b"SPIT", &spit));
        spel_payload.extend_from_slice(&legacy_effect_payload(PRIMARY_MGEF_ID));
        let spel = record(b"SPEL", SPEL_ID, 0, &spel_payload);

        let mut primary_payload = subrecord(b"EDID", b"ForwardPrimaryEffect\0");
        primary_payload.extend_from_slice(&subrecord(
            b"DATA",
            &legacy_mgef_data(ASSOC_MGEF_ID, LIGH_ID, EFSH_ID, 18, 0),
        ));
        let primary_mgef = record(b"MGEF", PRIMARY_MGEF_ID, 0, &primary_payload);

        let mut assoc_payload = subrecord(b"EDID", b"ForwardAssocEffect\0");
        assoc_payload.extend_from_slice(&subrecord(b"DATA", &legacy_mgef_data(0, 0, 0, 0, -1)));
        let assoc_mgef = record(b"MGEF", ASSOC_MGEF_ID, 0, &assoc_payload);

        let light = record(b"LIGH", LIGH_ID, 0, &subrecord(b"EDID", b"ForwardLight\0"));
        let shader = record(b"EFSH", EFSH_ID, 0, &subrecord(b"EDID", b"ForwardShader\0"));

        let mut perk_payload = subrecord(b"EDID", b"ForwardPerk\0");
        perk_payload.extend_from_slice(&subrecord(b"CTDA", &legacy_condition(495, 0)));
        perk_payload.extend_from_slice(&subrecord(b"CIS1", b"top\0"));
        perk_payload.extend_from_slice(&subrecord(b"DATA", &[0, 12, 1, 1, 0]));
        perk_payload.extend_from_slice(&subrecord(b"PRKE", &[2, 0, 0]));
        perk_payload.extend_from_slice(&subrecord(b"DATA", &[72, 3, 2]));
        perk_payload.extend_from_slice(&subrecord(b"PRKC", &[0]));
        perk_payload.extend_from_slice(&subrecord(b"CTDA", &legacy_condition(1, SPEL_ID)));
        perk_payload.extend_from_slice(&subrecord(b"CIS1", b"nested\0"));
        perk_payload.extend_from_slice(&subrecord(b"EPFT", &[1]));
        perk_payload.extend_from_slice(&subrecord(b"EPFD", &1.25_f32.to_le_bytes()));
        perk_payload.extend_from_slice(&subrecord(b"PRKF", &[]));
        perk_payload.extend_from_slice(&subrecord(b"PRKE", &[1, 0, 0]));
        perk_payload.extend_from_slice(&subrecord(b"DATA", &SPEL_ID.to_le_bytes()));
        perk_payload.extend_from_slice(&subrecord(b"PRKF", &[]));
        let perk = record(b"PERK", PERK_ID, 0, &perk_payload);

        let mut wrld_payload = subrecord(b"EDID", b"WastelandNV\0");
        wrld_payload.extend_from_slice(&subrecord(b"INAM", &SPEL_ID.to_le_bytes()));
        wrld_payload.extend_from_slice(&subrecord(b"DATA", &[0x80, 0xFF, 0xFF, 0xFF]));
        let world = record(b"WRLD", WRLD_ID, 0, &wrld_payload);

        plugin(
            &[],
            &[
                group(b"ALCH", 0, &[alch]),
                group(b"ENCH", 0, &[ench]),
                group(b"SPEL", 0, &[spel]),
                group(b"MGEF", 0, &[primary_mgef, assoc_mgef]),
                group(b"LIGH", 0, &[light]),
                group(b"EFSH", 0, &[shader]),
                group(b"PERK", 0, &[perk]),
                group(b"WRLD", 0, &[world]),
            ],
        )
    }

    #[test]
    fn fnv_quest_slice_translates_only_exact_own_plugin_mgef_through_world_fence() {
        const SELECTED_MGEF: u32 = 0x0CB05D;
        const UNRELATED_MGEF: u32 = 0x0CB05E;

        let effect = |raw_form_id: u32, editor_id: &str| {
            let mut payload = subrecord(b"EDID", format!("{editor_id}\0").as_bytes());
            payload.extend_from_slice(&subrecord(b"DATA", &legacy_mgef_data(0, 0, 0, 0, -1)));
            record(b"MGEF", raw_form_id, 0, &payload)
        };
        let fixture = plugin(
            &["FalloutNV.esm"],
            &[group(
                b"MGEF",
                0,
                &[
                    effect(0x0100_0000 | SELECTED_MGEF, "SelectedRestoreAllLimbs"),
                    effect(0x0100_0000 | UNRELATED_MGEF, "UnrelatedOwnEffect"),
                    effect(SELECTED_MGEF, "SameLocalMasterOverride"),
                ],
            )],
        );
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("FalloutNV.esm");
        std::fs::write(&source_path, fixture).unwrap();
        let source_handle = load_source_handle(&source_path, "fnv");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: true,
                preserve_source_ids: true,
                strict_mapper: true,
                skip_record_signatures: vec!["CREA".to_string(), "MGEF".to_string()],
                fnv_quest_slice: true,
                fnv_quest_slice_records: std::collections::HashMap::from([(
                    "MGEF".to_string(),
                    vec![SELECTED_MGEF],
                )]),
                ..Default::default()
            },
        })
        .unwrap();

        let stats = with_run(id, |run| run.translate_all_v2(&source_path)).unwrap();
        let target_records = handle_records(target_handle);
        let target_effects = target_records
            .iter()
            .filter(|record| record.signature.as_str() == "MGEF")
            .collect::<Vec<_>>();

        assert_eq!(stats.records_translated, 1);
        assert_eq!(stats.records_dropped, 2);
        assert_eq!(target_effects.len(), 1);
        assert_eq!(target_effects[0].form_id & 0x00FF_FFFF, SELECTED_MGEF);
        assert_eq!(
            subrecord_data(target_effects[0], "EDID"),
            b"SelectedRestoreAllLimbs\0"
        );

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn creature_dependency_admission_bypasses_only_the_exact_source_record() {
        const SELECTED_MGEF: u32 = 0x0011_0100;
        const UNRELATED_MGEF: u32 = 0x0011_0101;
        let effect = |raw_form_id: u32, editor_id: &str| {
            let mut payload = subrecord(b"EDID", format!("{editor_id}\0").as_bytes());
            payload.extend_from_slice(&subrecord(b"DATA", &legacy_mgef_data(0, 0, 0, 0, -1)));
            record(b"MGEF", raw_form_id, 0, &payload)
        };
        let fixture = plugin(
            &[],
            &[group(
                b"MGEF",
                0,
                &[
                    effect(SELECTED_MGEF, "CreatureDependencyEffect"),
                    effect(UNRELATED_MGEF, "UnrelatedEffect"),
                ],
            )],
        );
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("FalloutNV.esm");
        std::fs::write(&source_path, fixture).unwrap();
        let source_handle = load_source_handle(&source_path, "fnv");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: false,
                preserve_source_ids: true,
                strict_mapper: true,
                // This is an explicit two-record slice; production whole-plugin runs retain
                // the fatal PACK census gate.
                skip_record_signatures: vec!["MGEF".to_string(), "PACK".to_string()],
                legacy_pack_expected_counts: Some(LegacyPackExpectedCounts { fnv: 0, fo3: 0 }),
                ..Default::default()
            },
        })
        .unwrap();
        with_run(id, |run| {
            let source_plugin = run.interner.intern("FalloutNV.esm");
            run.install_creature_dependency_admissions(vec![CreatureDependencyAdmission {
                source_form_key: FormKey {
                    local: SELECTED_MGEF,
                    plugin: source_plugin,
                },
                source_signature: SigCode::from_str("MGEF").unwrap(),
                owner_family_ids: vec!["family-a".to_string()],
            }])?;
            run.translate_all_v2(&source_path)
        })
        .unwrap();

        let target_effects = handle_records(target_handle)
            .into_iter()
            .filter(|record| record.signature.as_str() == "MGEF")
            .collect::<Vec<_>>();
        assert_eq!(target_effects.len(), 1);
        assert_eq!(
            subrecord_data(&target_effects[0], "EDID"),
            b"CreatureDependencyEffect\0"
        );

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn creature_dependency_overrides_unsupported_runtime_component_gate() {
        assert!(!unsupported_skyrim_runtime_record_blocks_translation(
            false, true
        ));
        assert!(unsupported_skyrim_runtime_record_blocks_translation(
            false, false
        ));
        assert!(!unsupported_skyrim_runtime_record_blocks_translation(
            true, false
        ));
    }

    #[test]
    fn creature_dependency_admission_survives_pair_hook_signature_rewrite() {
        const SELECTED_LVLC: u32 = 0x0011_0200;
        let mut lvlo = [0_u8; 12];
        lvlo[0..2].copy_from_slice(&1_u16.to_le_bytes());
        lvlo[4..8].copy_from_slice(&0x0000_0201_u32.to_le_bytes());
        lvlo[8..10].copy_from_slice(&1_u16.to_le_bytes());
        let mut payload = subrecord(b"EDID", b"CreatureDependencyList\0");
        payload.extend_from_slice(&subrecord(b"LVLO", &lvlo));
        let fixture = plugin(
            &[],
            &[group(
                b"LVLC",
                0,
                &[record(b"LVLC", SELECTED_LVLC, 0, &payload)],
            )],
        );
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("FalloutNV.esm");
        std::fs::write(&source_path, fixture).unwrap();
        let source_handle = load_source_handle(&source_path, "fnv");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: false,
                preserve_source_ids: true,
                strict_mapper: false,
                skip_record_signatures: vec![
                    "LVLC".to_string(),
                    "LVLN".to_string(),
                    "PACK".to_string(),
                ],
                legacy_pack_expected_counts: Some(LegacyPackExpectedCounts { fnv: 0, fo3: 0 }),
                ..Default::default()
            },
        })
        .unwrap();
        let result = with_run(id, |run| {
            let source_plugin = run.interner.intern("FalloutNV.esm");
            run.install_creature_dependency_admissions(vec![CreatureDependencyAdmission {
                source_form_key: FormKey {
                    local: SELECTED_LVLC,
                    plugin: source_plugin,
                },
                source_signature: SigCode::from_str("LVLC").unwrap(),
                owner_family_ids: vec!["family-a".to_string()],
            }])?;
            run.translate_all_v2(&source_path)
        });

        assert!(result.is_ok(), "{result:?}");
        assert!(
            handle_records(target_handle)
                .iter()
                .any(|record| record.signature.as_str() == "LVLN")
        );

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn fnv_quest_slice_excludes_acre_before_lowering_and_preserves_selected_achr() {
        const SELECTED_ACHR: u32 = 0x0012_319B;
        const NPC_BASE: u32 = 0x0012_3193;
        const EXCLUDED_ACRE: u32 = 0x0017_A0A;
        const EXCLUDED_CREA_BASE: u32 = 0x0017_A03;
        const CELL_ID: u32 = 0x0012_3000;

        let npc = record(
            b"NPC_",
            NPC_BASE,
            0,
            &subrecord(b"EDID", b"TechMineNCRHostage\0"),
        );
        let selected_achr = record(
            b"ACHR",
            SELECTED_ACHR,
            0,
            &subrecord(b"NAME", &NPC_BASE.to_le_bytes()),
        );
        let persistent = group(&CELL_ID.to_le_bytes(), 8, &[selected_achr]);
        let cell_children = group(&CELL_ID.to_le_bytes(), 6, &[persistent]);
        let cell = record(
            b"CELL",
            CELL_ID,
            0,
            &subrecord(b"EDID", b"QuestSliceInterior\0"),
        );
        let acre = record(
            b"ACRE",
            EXCLUDED_ACRE,
            0,
            &subrecord(b"NAME", &EXCLUDED_CREA_BASE.to_le_bytes()),
        );
        let creature = record(
            b"CREA",
            EXCLUDED_CREA_BASE,
            0,
            &subrecord(b"EDID", b"UnrelatedCreature\0"),
        );
        let misc = record(
            b"MISC",
            0x0017_A10,
            0,
            &subrecord(b"EDID", b"UnrelatedWorldRecord\0"),
        );
        let fixture = plugin(
            &[],
            &[
                group(b"NPC_", 0, &[npc]),
                group(b"CELL", 0, &[cell, cell_children]),
                group(b"ACRE", 0, &[acre]),
                group(b"CREA", 0, &[creature]),
                group(b"MISC", 0, &[misc]),
            ],
        );
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("FalloutNV.esm");
        std::fs::write(&source_path, fixture).unwrap();
        let source_handle = load_source_handle(&source_path, "fnv");
        let target_handle = plugin_handle_new_native("FalloutNV.esm", Some("fo4")).unwrap();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "FalloutNV.esm".into(),
                is_whole_plugin: true,
                preserve_source_ids: true,
                strict_mapper: true,
                skip_record_signatures: vec!["ACRE".to_string(), "CREA".to_string()],
                fnv_quest_slice: true,
                fnv_quest_slice_records: std::collections::HashMap::from([(
                    "ACHR".to_string(),
                    vec![0x12319B, 0x12319C, 0x134B9C],
                )]),
                ..Default::default()
            },
        })
        .unwrap();

        let stats = with_run(id, |run| run.translate_all_v2(&source_path)).unwrap();
        let target_records = handle_records(target_handle);

        assert_eq!(stats.records_failed, 0);
        assert_eq!(stats.records_dropped, 2);
        assert!(target_records.iter().any(|record| {
            record.signature.as_str() == "ACHR" && record.form_id & 0x00FF_FFFF == SELECTED_ACHR
        }));
        assert!(target_records.iter().any(|record| {
            record.signature.as_str() == "MISC" && record.form_id & 0x00FF_FFFF == 0x0017_A10
        }));
        assert!(target_records.iter().all(|record| {
            !matches!(record.signature.as_str(), "ACRE" | "CREA")
                && !(record.signature.as_str() == "ACHR"
                    && record.form_id & 0x00FF_FFFF == EXCLUDED_ACRE)
        }));
        {
            let store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get(&target_handle).unwrap();
            assert!(!slot.parsed.root_items.iter().any(|item| {
                matches!(item, ParsedItem::Group(group) if group.group_type == 0 && group.label == *b"ACHR")
            }));
        }

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn fnv_quest_slice_preallocates_selected_pack_refs_before_npc_translation() {
        const RENOLDS_BASE: u32 = 0x0013_00F0;
        const PATROL_PACK: u32 = 0x0013_3F3E;
        const DIALOGUE_PACK: u32 = 0x0013_289E;

        let pack = |form_id: u32, editor_id: &str| {
            record(
                b"PACK",
                form_id,
                0,
                &subrecord(b"EDID", format!("{editor_id}\0").as_bytes()),
            )
        };
        let mut npc_payload = subrecord(b"EDID", b"NVTechatticupNCRRenolds\0");
        npc_payload.extend_from_slice(&subrecord(b"PKID", &PATROL_PACK.to_le_bytes()));
        npc_payload.extend_from_slice(&subrecord(b"PKID", &DIALOGUE_PACK.to_le_bytes()));
        let fixture = plugin(
            &[],
            &[
                group(
                    b"PACK",
                    0,
                    &[
                        pack(PATROL_PACK, "NVTecPatrolTechatticup"),
                        pack(DIALOGUE_PACK, "TechaticupNCRRenoldsDialoguePackage"),
                    ],
                ),
                group(
                    b"NPC_",
                    0,
                    &[record(b"NPC_", RENOLDS_BASE, 0, &npc_payload)],
                ),
            ],
        );
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("FalloutNV.esm");
        std::fs::write(&source_path, fixture).unwrap();
        let source_handle = load_source_handle(&source_path, "fnv");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: true,
                preserve_source_ids: false,
                strict_mapper: true,
                skip_record_signatures: vec!["CREA".to_string(), "PACK".to_string()],
                fnv_quest_slice: true,
                fnv_quest_slice_records: std::collections::HashMap::from([(
                    "PACK".to_string(),
                    vec![PATROL_PACK, DIALOGUE_PACK],
                )]),
                ..Default::default()
            },
        })
        .unwrap();

        let stats = with_run(id, |run| run.translate_all_v2(&source_path)).unwrap();
        let (target_npc, target_patrol, target_dialogue) = with_run(id, |run| {
            let source_plugin = run.interner.intern("FalloutNV.esm");
            let mappings = &run.mapper_state.as_ref().unwrap().source_to_target;
            let mapped = |local| {
                mappings[&FormKey {
                    local,
                    plugin: source_plugin,
                }]
            };
            Ok::<_, RunError>((
                mapped(RENOLDS_BASE),
                mapped(PATROL_PACK),
                mapped(DIALOGUE_PACK),
            ))
        })
        .unwrap();
        let target_records = handle_records(target_handle);
        let npc = target_records
            .iter()
            .find(|record| {
                record.signature.as_str() == "NPC_"
                    && record.form_id & 0x00FF_FFFF == target_npc.local
            })
            .expect("mapped Renolds NPC");
        let packages = npc
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "PKID")
            .map(|subrecord| raw_u32(&subrecord.data, 0) & 0x00FF_FFFF)
            .collect::<Vec<_>>();

        assert_eq!(stats.records_translated, 1);
        assert_eq!(stats.records_dropped, 2);
        assert_eq!(packages, [target_patrol.local, target_dialogue.local]);
        assert_ne!(target_patrol.local, PATROL_PACK);
        assert_ne!(target_dialogue.local, DIALOGUE_PACK);

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn fnv_store2_captures_raw_npc_scri_attachments_before_drop() {
        let npc = |form_id: u32, editor_id: &str, script_form_id: u32| {
            let mut payload = subrecord(b"EDID", format!("{editor_id}\0").as_bytes());
            payload.extend_from_slice(&subrecord(b"SCRI", &script_form_id.to_le_bytes()));
            record(b"NPC_", form_id, 0, &payload)
        };
        let fixture = plugin(
            &[],
            &[group(
                b"NPC_",
                0,
                &[
                    npc(0x0012_3193, "TechMineNCRHostage", 0x0012_3191),
                    npc(0x0013_00F0, "NCRPrivateRenolds", 0x0016_6305),
                ],
            )],
        );
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("FalloutNV.esm");
        std::fs::write(&source_path, fixture).unwrap();
        let source_handle = load_source_handle(&source_path, "fnv");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: false,
                preserve_source_ids: true,
                strict_mapper: true,
                ..Default::default()
            },
        })
        .unwrap();

        let stats = with_run(id, |run| run.translate_all_v2(&source_path)).unwrap();
        let links = with_run(id, |run| Ok::<_, RunError>(run.fnv_scri_links.clone())).unwrap();
        assert_eq!(stats.records_translated, 2);
        assert_eq!(
            links,
            [
                FnvScriLink {
                    target_form_key: "123193:Out.esm".into(),
                    source_scpt_form_key: "123191:FalloutNV.esm".into(),
                },
                FnvScriLink {
                    target_form_key: "1300F0:Out.esm".into(),
                    source_scpt_form_key: "166305:FalloutNV.esm".into(),
                },
            ]
        );
        for target in handle_records(target_handle)
            .into_iter()
            .filter(|record| record.signature.as_str() == "NPC_")
        {
            assert!(
                target
                    .subrecords
                    .iter()
                    .all(|subrecord| subrecord.signature.as_str() != "SCRI")
            );
        }

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn fnv_store2_preserves_exact_acti_scri_and_persistent_refr_topology() {
        const ACTI_ID: u32 = 0x0013_3F41;
        const REFR_ID: u32 = 0x0013_3F42;
        const SCPT_ID: u32 = 0x0013_4491;
        const CELL_ID: u32 = 0x0013_3F40;

        let mut acti_payload = subrecord(b"EDID", b"TecRenoldsDialogueActivator\0");
        acti_payload.extend_from_slice(&subrecord(b"SCRI", &SCPT_ID.to_le_bytes()));
        let acti = record(b"ACTI", ACTI_ID, 0, &acti_payload);
        let mut refr_payload = subrecord(b"NAME", &ACTI_ID.to_le_bytes());
        let mut primitive = Vec::new();
        for value in [624.0_f32, 1032.0, 176.0, 0.8, 0.3, 0.2, 0.15] {
            primitive.extend_from_slice(&value.to_le_bytes());
        }
        primitive.extend_from_slice(&1_u32.to_le_bytes());
        refr_payload.extend_from_slice(&subrecord(b"XPRM", &primitive));
        let mut bounds = Vec::new();
        for value in [624.0_f32, 1032.0, 176.0] {
            bounds.extend_from_slice(&value.to_le_bytes());
        }
        refr_payload.extend_from_slice(&subrecord(b"XMBO", &bounds));
        let mut transform = Vec::new();
        for value in [54928.0_f32, -47392.0, 4760.0, 0.0, 0.0, 2.383187] {
            transform.extend_from_slice(&value.to_le_bytes());
        }
        refr_payload.extend_from_slice(&subrecord(b"DATA", &transform));
        let refr = record(b"REFR", REFR_ID, 0x0000_0400, &refr_payload);
        let persistent = group(&CELL_ID.to_le_bytes(), 8, &[refr]);
        let cell_children = group(&CELL_ID.to_le_bytes(), 6, &[persistent]);
        let cell = record(
            b"CELL",
            CELL_ID,
            0,
            &subrecord(b"EDID", b"TecRenoldsDialogueCell\0"),
        );
        let fixture = plugin(
            &[],
            &[
                group(b"ACTI", 0, &[acti]),
                group(b"CELL", 0, &[cell, cell_children]),
            ],
        );
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("FalloutNV.esm");
        std::fs::write(&source_path, fixture).unwrap();
        let source_handle = load_source_handle(&source_path, "fnv");
        let target_handle = plugin_handle_new_native("FalloutNV.esm", Some("fo4")).unwrap();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "FalloutNV.esm".into(),
                is_whole_plugin: true,
                preserve_source_ids: true,
                strict_mapper: true,
                skip_record_signatures: vec!["CREA".to_string()],
                fnv_quest_slice: true,
                fnv_quest_slice_records: std::collections::HashMap::from([
                    ("ACTI".to_string(), vec![ACTI_ID]),
                    ("REFR".to_string(), vec![REFR_ID]),
                ]),
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| {
            let source_plugin = run.interner.intern("FalloutNV.esm");
            let source_record = crate::source_read::read_record_relayout_by_form_key(
                run.source_handle_id,
                &FormKey {
                    local: REFR_ID,
                    plugin: source_plugin,
                },
                &run.schema_source,
                &run.interner,
                None,
            )
            .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
            let xprm = source_record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "XPRM")
                .expect("imported XPRM");
            let FieldValue::Bytes(bytes) = &xprm.value else {
                panic!("live-shaped generic struct import must be Bytes");
            };
            assert_eq!(bytes.len(), 32);
            assert_eq!(u32::from_le_bytes(bytes[28..32].try_into().unwrap()), 1);
            Ok::<_, RunError>(())
        })
        .unwrap();

        let stats = with_run(id, |run| run.translate_all_v2(&source_path)).unwrap();
        let (target_acti, target_refr, links, topology) = with_run(id, |run| {
            let source_plugin = run.interner.intern("FalloutNV.esm");
            let mappings = &run.mapper_state.as_ref().unwrap().source_to_target;
            let target_acti = mappings[&FormKey {
                local: ACTI_ID,
                plugin: source_plugin,
            }];
            let target_refr = mappings[&FormKey {
                local: REFR_ID,
                plugin: source_plugin,
            }];
            let topology = crate::target_write::existing_record_topology_native(
                run.target_handle_id,
                target_refr,
                "REFR",
                &run.interner,
            )
            .map_err(|error| RunError::InvalidConfig(error.to_string()))?;
            Ok::<_, RunError>((
                target_acti,
                target_refr,
                run.fnv_scri_links.clone(),
                topology,
            ))
        })
        .unwrap();
        let target_records = handle_records(target_handle);
        let target_acti_record = target_records
            .iter()
            .find(|record| {
                record.signature.as_str() == "ACTI"
                    && record.form_id & 0x00FF_FFFF == target_acti.local
            })
            .expect("mapped dialogue activator");
        let target_refr_record = target_records
            .iter()
            .find(|record| {
                record.signature.as_str() == "REFR"
                    && record.form_id & 0x00FF_FFFF == target_refr.local
            })
            .expect("mapped persistent dialogue activator reference");

        assert_eq!(stats.records_failed, 0);
        assert!(
            target_acti_record
                .subrecords
                .iter()
                .all(|subrecord| subrecord.signature.as_str() != "SCRI")
        );
        assert_eq!(
            target_refr_record
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature.as_str() == "NAME")
                .map(|subrecord| raw_u32(&subrecord.data, 0) & 0x00FF_FFFF),
            Some(target_acti.local)
        );
        assert_eq!(
            links,
            [FnvScriLink {
                target_form_key: format!("{:06X}:FalloutNV.esm", target_acti.local),
                source_scpt_form_key: "134491:FalloutNV.esm".into(),
            }]
        );
        assert_eq!(topology.occurrences, 1);
        assert!(!topology.top_level);
        assert!(topology.persistent);
        with_run(id, |run| {
            let source_plugin = run.interner.intern("FalloutNV.esm");
            crate::fnv_legacy_scripting::from_run::validate_exact_persistent_refr(
                run,
                source_plugin,
            )
            .map_err(RunError::from)
        })
        .unwrap();

        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&target_handle).unwrap();
            for item in &mut slot.parsed.root_items {
                if let ParsedItem::Group(group) = item
                    && group.group_type == 0
                    && group.label == *b"ACTI"
                {
                    group.children.retain(|item| {
                        !matches!(item, ParsedItem::Record(record) if record.form_id & 0x00FF_FFFF == target_acti.local)
                    });
                }
            }
        }
        with_run(id, |run| {
            let source_plugin = run.interner.intern("FalloutNV.esm");
            crate::fnv_legacy_scripting::from_run::ensure_exact_acti_vmad_target(
                run,
                source_plugin,
                &std::collections::HashMap::from([("ACTI".to_string(), vec![ACTI_ID])]),
            )
            .map_err(RunError::from)
        })
        .unwrap();
        assert!(handle_records(target_handle).iter().any(|record| {
            record.signature.as_str() == "ACTI" && record.form_id & 0x00FF_FFFF == target_acti.local
        }));
        let recovered_links =
            with_run(id, |run| Ok::<_, RunError>(run.fnv_scri_links.clone())).unwrap();
        assert_eq!(recovered_links, links);

        fn mutate_trigger_primitive(
            items: &mut [ParsedItem],
            target_local: u32,
            bounds_x: f32,
            primitive_type: u32,
        ) -> bool {
            for item in items {
                match item {
                    ParsedItem::Record(record)
                        if record.signature.as_str() == "REFR"
                            && record.form_id & 0x00FF_FFFF == target_local =>
                    {
                        let primitive = record
                            .subrecords
                            .iter_mut()
                            .find(|subrecord| subrecord.signature.as_str() == "XPRM")
                            .unwrap();
                        let mut bytes = primitive.data.to_vec();
                        bytes[..4].copy_from_slice(&bounds_x.to_le_bytes());
                        bytes[28..32].copy_from_slice(&primitive_type.to_le_bytes());
                        primitive.data = bytes::Bytes::from(bytes);
                        return true;
                    }
                    ParsedItem::Group(group) => {
                        if mutate_trigger_primitive(
                            &mut group.children,
                            target_local,
                            bounds_x,
                            primitive_type,
                        ) {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            false
        }
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&target_handle).unwrap();
            assert!(mutate_trigger_primitive(
                &mut slot.parsed.root_items,
                target_refr.local,
                625.0,
                1,
            ));
        }
        let geometry_error = with_run(id, |run| {
            let source_plugin = run.interner.intern("FalloutNV.esm");
            crate::fnv_legacy_scripting::from_run::validate_exact_persistent_refr(
                run,
                source_plugin,
            )
            .map_err(RunError::from)
        })
        .unwrap_err();
        assert!(
            geometry_error
                .to_string()
                .contains("XPRM.BoundsX expected 624")
        );
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&target_handle).unwrap();
            assert!(mutate_trigger_primitive(
                &mut slot.parsed.root_items,
                target_refr.local,
                624.0,
                0,
            ));
        }
        let primitive_type_error = with_run(id, |run| {
            let source_plugin = run.interner.intern("FalloutNV.esm");
            crate::fnv_legacy_scripting::from_run::validate_exact_persistent_refr(
                run,
                source_plugin,
            )
            .map_err(RunError::from)
        })
        .unwrap_err();
        let primitive_type_error = primitive_type_error.to_string();
        assert!(primitive_type_error.contains("XPRM.Type is not Box"));
        assert!(primitive_type_error.contains("observed=Bytes(len=32)"));
        assert!(primitive_type_error.contains("raw_len=32"));
        assert!(primitive_type_error.contains("raw_type_u32=Some(0)"));
        assert!(primitive_type_error.contains("expected FNV XPRM enum Box=1"));

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    fn legacy_unsupported_creature_fixture() -> Vec<u8> {
        let mut payload = subrecord(b"EDID", b"CazadorFixture\0");
        payload.extend_from_slice(&subrecord(b"MODL", b"Creatures\\Cazador\\cazador.nif\0"));
        payload.extend_from_slice(&subrecord(b"RNAM", &[0x4A]));
        let creature = record(b"CREA", 0x0000_1A01, 0, &payload);
        plugin(&[], &[group(b"CREA", 0, &[creature])])
    }

    fn legacy_humanoid_creature_fixture() -> Vec<u8> {
        let mut payload = subrecord(b"EDID", b"LegionCreature\0");
        payload.extend_from_slice(&subrecord(b"MODL", b"characters\\_male\\skeleton.nif\0"));
        payload.extend_from_slice(&subrecord(b"RNAM", &[0x60]));
        let creature = record(b"CREA", 0x0014_0D4B, 0, &payload);
        plugin(&[], &[group(b"CREA", 0, &[creature])])
    }

    fn legacy_unused_ingredient_sentinel_fixture() -> Vec<u8> {
        let mut payload = subrecord(
            b"EDID",
            b"DoNotCreateNewIngredientsWeArentUsingThemInFallout\0",
        );
        payload.extend_from_slice(&subrecord(b"ETYP", &0_u32.to_le_bytes()));
        payload.extend_from_slice(&subrecord(b"DATA", &0_f32.to_le_bytes()));
        payload.extend_from_slice(&subrecord(b"ENIT", &[0, 0, 0, 0, 0, 0xCD, 0xCD, 0xCD]));
        payload.extend_from_slice(&subrecord(b"EFID", &0x0000_014E_u32.to_le_bytes()));
        let mut efit = [0_u8; 20];
        efit[0..4].copy_from_slice(&1_u32.to_le_bytes());
        payload.extend_from_slice(&subrecord(b"EFIT", &efit));
        let ingredient = record(b"INGR", 0x0003_135B, 0, &payload);
        plugin(&[], &[group(b"INGR", 0, &[ingredient])])
    }

    fn legacy_duplicate_wrld_fixture() -> Vec<u8> {
        let mut payload = subrecord(b"EDID", b"DuplicateLegacyWorld\0");
        payload.extend_from_slice(&subrecord(b"DATA", &[0x10]));
        payload.extend_from_slice(&subrecord(b"DATA", &[0x20]));
        plugin(
            &[],
            &[group(b"WRLD", 0, &[record(b"WRLD", 0x1600, 0, &payload)])],
        )
    }

    fn legacy_same_name_identity_collision_fixture() -> Vec<u8> {
        const LOW_MGEF_ID: u32 = 0x0000_0100;
        const FLOOR_MGEF_ID: u32 = 0x0000_0800;
        const ALCH_ID: u32 = 0x0000_0900;

        let mut potion_payload = subrecord(b"EDID", b"LowEffectPotion\0");
        potion_payload.extend_from_slice(&subrecord(b"ENIT", &[0_u8; 20]));
        potion_payload.extend_from_slice(&legacy_effect_payload(LOW_MGEF_ID));
        let potion = record(b"ALCH", ALCH_ID, 0, &potion_payload);

        let mut low_payload = subrecord(b"EDID", b"LowGeneratedEffect\0");
        low_payload.extend_from_slice(&subrecord(b"DATA", &legacy_mgef_data(0, 0, 0, 0, -1)));
        let low = record(b"MGEF", LOW_MGEF_ID, 0, &low_payload);

        let mut floor_payload = subrecord(b"EDID", b"PreservedFloorEffect\0");
        floor_payload.extend_from_slice(&subrecord(b"DATA", &legacy_mgef_data(0, 0, 0, 0, -1)));
        let floor = record(b"MGEF", FLOOR_MGEF_ID, 0, &floor_payload);

        plugin(
            &[],
            &[
                group(b"ALCH", 0, &[potion]),
                group(b"MGEF", 0, &[low, floor]),
            ],
        )
    }

    const SAME_NAME_CLIMATE_SOURCE_ID: u32 = 0x0000_0812;
    const SAME_NAME_COLLIDING_PATH_SOURCE_ID: u32 = 0x0000_08E1;
    const SAME_NAME_WORLD_SOURCE_ID: u32 = 0x0000_003C;

    fn skyrim_same_name_reference_collision_fixture() -> Vec<u8> {
        let climate = record(
            b"CLMT",
            SAME_NAME_CLIMATE_SOURCE_ID,
            0,
            &subrecord(b"EDID", b"SameNameClimate\0"),
        );
        let path = record(
            b"CPTH",
            SAME_NAME_COLLIDING_PATH_SOURCE_ID,
            0,
            &subrecord(b"EDID", b"SameNamePath\0"),
        );
        let mut world_payload = subrecord(b"EDID", b"SameNameWorld\0");
        world_payload.extend_from_slice(&subrecord(
            b"CNAM",
            &SAME_NAME_CLIMATE_SOURCE_ID.to_le_bytes(),
        ));
        let world = record(b"WRLD", SAME_NAME_WORLD_SOURCE_ID, 0, &world_payload);

        plugin(
            &[],
            &[
                group(b"CLMT", 0, &[climate]),
                group(b"CPTH", 0, &[path]),
                group(b"WRLD", 0, &[world]),
            ],
        )
    }

    fn story_manager_fixture() -> Vec<u8> {
        story_manager_fixture_with_event(b"SCPT")
    }

    fn story_manager_fixture_with_event(event_type: &[u8; 4]) -> Vec<u8> {
        let root_id = 0x0000_0200u32;
        let branch_id = 0x0000_0201u32;
        let smqn_id = 0x0000_0202u32;
        let quest_id = 0x0000_0300u32;

        let mut qust_data = vec![0u8; 16];
        qust_data[0..4].copy_from_slice(&1u32.to_le_bytes());
        let mut quest_payload = subrecord(b"EDID", b"SyntheticRadioQuest\0");
        quest_payload.extend_from_slice(&subrecord(b"DATA", &qust_data));
        let quest = record(b"QUST", quest_id, 0, &quest_payload);

        let mut root_payload = subrecord(b"EDID", b"SyntheticEventRoot\0");
        root_payload.extend_from_slice(&subrecord(b"ENAM", event_type));
        let root = record(b"SMEN", root_id, 0, &root_payload);

        let mut branch_payload = subrecord(b"EDID", b"SyntheticRadioBranch\0");
        branch_payload.extend_from_slice(&subrecord(b"PNAM", &root_id.to_le_bytes()));
        let branch = record(b"SMBN", branch_id, 0, &branch_payload);

        let mut smqn_payload = subrecord(b"EDID", b"GeneralRadio_Synthetic\0");
        smqn_payload.extend_from_slice(&subrecord(b"PNAM", &branch_id.to_le_bytes()));
        smqn_payload.extend_from_slice(&subrecord(b"NNAM", &quest_id.to_le_bytes()));
        let smqn = record(b"SMQN", smqn_id, 0, &smqn_payload);

        plugin(
            &[],
            &[
                group(b"QUST", 0, &[quest]),
                group(b"SMEN", 0, &[root]),
                group(b"SMBN", 0, &[branch]),
                group(b"SMQN", 0, &[smqn]),
            ],
        )
    }

    fn quest_scene_fixture() -> Vec<u8> {
        let quest_id = 0x0000_0300u32;
        let scene_id = 0x0000_0301u32;
        let dialogue_id = 0x0000_0302u32;

        let mut quest_data = vec![0u8; 16];
        quest_data[0..4].copy_from_slice(&1u32.to_le_bytes());
        let mut quest_payload = subrecord(b"EDID", b"SyntheticQuest\0");
        quest_payload.extend_from_slice(&subrecord(b"DATA", &quest_data));
        let quest = record(b"QUST", quest_id, 0, &quest_payload);

        let mut dialogue_payload = subrecord(b"PNAM", &50_f32.to_le_bytes());
        dialogue_payload.extend_from_slice(&subrecord(b"QNAM", &quest_id.to_le_bytes()));
        dialogue_payload.extend_from_slice(&subrecord(b"DATA", &[0_u8; 4]));
        dialogue_payload.extend_from_slice(&subrecord(b"SNAM", b"SCEN"));
        dialogue_payload.extend_from_slice(&subrecord(b"TIFC", &0_u32.to_le_bytes()));
        let dialogue = record(b"DIAL", dialogue_id, 0, &dialogue_payload);

        let mut scene_payload = subrecord(b"EDID", b"SyntheticScene\0");
        scene_payload.extend_from_slice(&subrecord(b"FNAM", &0u32.to_le_bytes()));
        scene_payload.extend_from_slice(&subrecord(b"PNAM", &quest_id.to_le_bytes()));
        scene_payload.extend_from_slice(&subrecord(b"INAM", &0u32.to_le_bytes()));
        scene_payload.extend_from_slice(&subrecord(b"VNAM", &[0u8; 16]));
        scene_payload.extend_from_slice(&subrecord(b"XNAM", &0u32.to_le_bytes()));
        let scene = record(b"SCEN", scene_id, 0, &scene_payload);
        let quest_children = group(&quest_id.to_le_bytes(), 10, &[dialogue, scene]);

        plugin(&[], &[group(b"QUST", 0, &[quest, quest_children])])
    }

    fn perk_fragment_forward_quest_fixture() -> (Vec<u8>, usize) {
        const QUEST_ID: u32 = 0x003F_28C3;
        const PERK_ID: u32 = 0x0040_483A;

        let mut vmad = Vec::new();
        vmad.extend_from_slice(&6_u16.to_le_bytes());
        vmad.extend_from_slice(&2_u16.to_le_bytes());
        vmad.extend_from_slice(&0_u16.to_le_bytes());
        vmad.push(4);
        let push_string = |buffer: &mut Vec<u8>, value: &str| {
            buffer.extend_from_slice(&(value.len() as u16).to_le_bytes());
            buffer.extend_from_slice(value.as_bytes());
        };
        push_string(
            &mut vmad,
            "Fragments:Perks:PRKF_W05_MQS_201P_EyebotPart_0040483A",
        );
        vmad.push(0);
        vmad.extend_from_slice(&1_u16.to_le_bytes());
        push_string(&mut vmad, "W05_MQS_201P_Industrialist");
        vmad.push(1);
        vmad.push(1);
        vmad.extend_from_slice(&0_u16.to_le_bytes());
        vmad.extend_from_slice(&(-1_i16).to_le_bytes());
        let quest_form_id_offset = vmad.len();
        vmad.extend_from_slice(&QUEST_ID.to_le_bytes());
        vmad.extend_from_slice(&1_u16.to_le_bytes());
        vmad.extend_from_slice(&1_u16.to_le_bytes());
        vmad.extend_from_slice(&0_i16.to_le_bytes());
        vmad.push(1);
        push_string(
            &mut vmad,
            "Fragments:Perks:PRKF_W05_MQS_201P_EyebotPart_0040483A",
        );
        push_string(&mut vmad, "Fragment_Entry_01");

        let mut perk_payload = subrecord(b"EDID", b"W05_MQS_201P_EyebotPartPerk\0");
        perk_payload.extend_from_slice(&subrecord(b"VMAD", &vmad));
        perk_payload.extend_from_slice(&subrecord(b"DATA", &[1, 0, 1, 1, 0]));
        perk_payload.extend_from_slice(&subrecord(b"PRKE", &[2, 0]));
        perk_payload.extend_from_slice(&subrecord(b"DATA", &[0x0E, 0x09, 0x02, 0]));
        perk_payload.extend_from_slice(&subrecord(b"PRKF", &[]));
        let perk = record(b"PERK", PERK_ID, 0, &perk_payload);

        let mut quest_data = vec![0_u8; 16];
        quest_data[0..4].copy_from_slice(&1_u32.to_le_bytes());
        let mut quest_payload = subrecord(b"EDID", b"W05_MQS_201P\0");
        quest_payload.extend_from_slice(&subrecord(b"DATA", &quest_data));
        let quest = record(b"QUST", QUEST_ID, 0, &quest_payload);

        (
            plugin(
                &[],
                &[group(b"PERK", 0, &[perk]), group(b"QUST", 0, &[quest])],
            ),
            quest_form_id_offset,
        )
    }

    fn frog_collecting_perk_fixture() -> (Vec<u8>, usize) {
        const PERK_ID: u32 = 0x0053_6ECF;
        const FROG_ITEM_ID: u32 = 0x0042_8C48;

        let mut vmad = Vec::new();
        vmad.extend_from_slice(&6_u16.to_le_bytes());
        vmad.extend_from_slice(&2_u16.to_le_bytes());
        vmad.extend_from_slice(&1_u16.to_le_bytes());
        let push_string = |buffer: &mut Vec<u8>, value: &str| {
            buffer.extend_from_slice(&(value.len() as u16).to_le_bytes());
            buffer.extend_from_slice(value.as_bytes());
        };
        push_string(&mut vmad, "Perks:FrogCollectingPerkScript");
        vmad.push(0);
        vmad.extend_from_slice(&1_u16.to_le_bytes());
        push_string(&mut vmad, "FrogItem");
        vmad.push(1);
        vmad.push(1);
        vmad.extend_from_slice(&0_u16.to_le_bytes());
        vmad.extend_from_slice(&(-1_i16).to_le_bytes());
        let frog_item_form_id_offset = vmad.len();
        vmad.extend_from_slice(&FROG_ITEM_ID.to_le_bytes());

        let mut perk_payload = subrecord(b"EDID", b"FrogCollectingPerk\0");
        perk_payload.extend_from_slice(&subrecord(b"VMAD", &vmad));
        perk_payload.extend_from_slice(&subrecord(b"DATA", &[1, 1, 1]));
        perk_payload.extend_from_slice(&subrecord(b"PRKE", &[2, 0]));
        perk_payload.extend_from_slice(&subrecord(b"DATA", &[0x0E, 0x09, 0x02, 0]));
        perk_payload.extend_from_slice(&subrecord(b"PRKC", &[0]));
        perk_payload.extend_from_slice(&subrecord(b"CTDA", &[0_u8; 32]));
        perk_payload.extend_from_slice(&subrecord(b"PRKC", &[1]));
        perk_payload.extend_from_slice(&subrecord(b"CTDA", &[1_u8; 32]));
        perk_payload.extend_from_slice(&subrecord(b"CTDA", &[2_u8; 32]));
        perk_payload.extend_from_slice(&subrecord(b"EPFT", &[2]));
        perk_payload.extend_from_slice(&subrecord(b"EPFB", &[0, 0]));
        perk_payload.extend_from_slice(&subrecord(b"EPF2", &0x6102_0203_u32.to_le_bytes()));
        perk_payload.extend_from_slice(&subrecord(b"EPF3", &[2, 0]));
        perk_payload.extend_from_slice(&subrecord(b"PRKF", &[]));
        let perk = record(b"PERK", PERK_ID, 0, &perk_payload);

        let frog_item = record(
            b"MISC",
            FROG_ITEM_ID,
            0,
            &subrecord(b"EDID", b"CapturedFrog\0"),
        );

        (
            plugin(
                &[],
                &[group(b"PERK", 0, &[perk]), group(b"MISC", 0, &[frog_item])],
            ),
            frog_item_form_id_offset,
        )
    }

    fn legacy_pack_payload(editor_id: &str, package_type: LegacyPackType) -> Vec<u8> {
        let mut payload = subrecord(b"EDID", format!("{editor_id}\0").as_bytes());
        let mut pkdt = [0_u8; 12];
        pkdt[4] = package_type.code();
        payload.extend_from_slice(&subrecord(b"PKDT", &pkdt));
        payload.extend_from_slice(&subrecord(b"PSDT", &[0_u8; 8]));
        match package_type {
            LegacyPackType::Travel => {
                let mut location = [0_u8; 12];
                location[..4].copy_from_slice(&3_u32.to_le_bytes());
                payload.extend_from_slice(&subrecord(b"PLDT", &location));
            }
            LegacyPackType::Patrol => {
                let mut location = [0_u8; 12];
                location[..4].copy_from_slice(&6_u32.to_le_bytes());
                payload.extend_from_slice(&subrecord(b"PLDT", &location));
                payload.extend_from_slice(&subrecord(b"PKPT", &[1, 0]));
            }
            LegacyPackType::Follow => {
                let mut target = [0_u8; 16];
                target[..4].copy_from_slice(&3_u32.to_le_bytes());
                target[12..16].copy_from_slice(&128_i32.to_le_bytes());
                payload.extend_from_slice(&subrecord(b"PTDT", &target));
                payload.extend_from_slice(&subrecord(b"PKFD", &0_f32.to_le_bytes()));
            }
            LegacyPackType::Sandbox => {
                let mut near = [0_u8; 12];
                near[..4].copy_from_slice(&3_u32.to_le_bytes());
                near[8..12].copy_from_slice(&1024_i32.to_le_bytes());
                payload.extend_from_slice(&subrecord(b"PLDT", &near));
                let mut far = [0_u8; 12];
                far[..4].copy_from_slice(&7_u32.to_le_bytes());
                far[8..12].copy_from_slice(&256_i32.to_le_bytes());
                payload.extend_from_slice(&subrecord(b"PLD2", &far));
            }
            LegacyPackType::UseWeapon => {
                let mut target = [0_u8; 16];
                target[..4].copy_from_slice(&2_u32.to_le_bytes());
                target[4..8].copy_from_slice(&23_u32.to_le_bytes());
                target[12..16].copy_from_slice(&1_i32.to_le_bytes());
                payload.extend_from_slice(&subrecord(b"PTDT", &target));
                payload.extend_from_slice(&subrecord(b"PKW3", &[0_u8; 24]));
                let mut secondary = [0_u8; 16];
                secondary[4..8].copy_from_slice(&0x0012_3456_u32.to_le_bytes());
                payload.extend_from_slice(&subrecord(b"PTD2", &secondary));
            }
            _ => unreachable!("fixture covers the five audited PACK archetypes"),
        }
        payload
    }

    fn merged_legacy_pack_fixture() -> (Vec<u8>, Vec<(u32, LegacyPackSourceFamily)>) {
        let archetypes = [
            LegacyPackType::Travel,
            LegacyPackType::Patrol,
            LegacyPackType::Follow,
            LegacyPackType::Sandbox,
            LegacyPackType::UseWeapon,
        ];
        let mut records = Vec::new();
        let mut origins = Vec::new();
        for (family, family_name, base_id) in [
            (LegacyPackSourceFamily::Fnv, "FNV", 0x2000_u32),
            (LegacyPackSourceFamily::Fo3, "FO3", 0x3000_u32),
        ] {
            for (offset, package_type) in archetypes.into_iter().enumerate() {
                let form_id = base_id + offset as u32;
                let editor_id = format!("{family_name}{package_type:?}");
                let mut payload = legacy_pack_payload(&editor_id, package_type);
                if family == LegacyPackSourceFamily::Fnv && package_type == LegacyPackType::Travel {
                    payload.extend_from_slice(&subrecord(b"CTDA", &[0_u8; 28]));
                }
                if family == LegacyPackSourceFamily::Fo3 && package_type == LegacyPackType::Travel {
                    payload.extend_from_slice(&subrecord(b"POBA", &[]));
                    payload.extend_from_slice(&subrecord(b"SCHR", &[0_u8; 20]));
                }
                records.push(record(b"PACK", form_id, 0, &payload));
                origins.push((form_id, family));
            }
        }
        let malformed_id = 0x2005;
        let mut malformed = legacy_pack_payload("FNVMalformed", LegacyPackType::Travel);
        malformed.extend_from_slice(&subrecord(b"CTDA", &[0_u8; 19]));
        records.push(record(b"PACK", malformed_id, 0, &malformed));
        origins.push((malformed_id, LegacyPackSourceFamily::Fnv));
        (plugin(&[], &[group(b"PACK", 0, &records)]), origins)
    }

    fn explicit_pack_origins(
        merged_plugin: &str,
        origins: &[(u32, LegacyPackSourceFamily)],
    ) -> Vec<LegacyPackOriginRow> {
        origins
            .iter()
            .map(|&(form_id, family)| {
                let (source_game, source_plugin) = match family {
                    LegacyPackSourceFamily::Fnv => ("fnv", "FalloutNV.esm"),
                    LegacyPackSourceFamily::Fo3 => ("fo3", "Fallout3.esm"),
                    _ => unreachable!(),
                };
                LegacyPackOriginRow {
                    merged_form_key: format!("{form_id:06X}@{merged_plugin}"),
                    source_game: source_game.to_string(),
                    source_plugin: source_plugin.to_string(),
                    source_form_key: format!("{form_id:08X}@{source_plugin}"),
                }
            })
            .collect()
    }

    fn assert_scene_is_nested_under_quest(handle: u64) {
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&handle).unwrap();
        let quest_group = slot
            .parsed
            .root_items
            .iter()
            .find_map(|item| match item {
                ParsedItem::Group(group) if group.group_type == 0 && group.label == *b"QUST" => {
                    Some(group)
                }
                _ => None,
            })
            .expect("QUST top group");
        assert!(
            !slot.parsed.root_items.iter().any(
                |item| matches!(item, ParsedItem::Group(group) if group.group_type == 0 && group.label == *b"SCEN")
            ),
            "SCEN must not be emitted as a top-level group"
        );

        let quest = quest_group
            .children
            .iter()
            .find_map(|item| match item {
                ParsedItem::Record(record) if record.signature.as_str() == "QUST" => Some(record),
                _ => None,
            })
            .expect("QUST record");
        let quest_children = quest_group
            .children
            .iter()
            .find_map(|item| match item {
                ParsedItem::Group(group)
                    if group.group_type == 10
                        && u32::from_le_bytes(group.label) == quest.form_id =>
                {
                    Some(group)
                }
                _ => None,
            })
            .expect("quest-child group");
        let dialogue_index = quest_children
            .children
            .iter()
            .position(
                |item| matches!(item, ParsedItem::Record(record) if record.signature.as_str() == "DIAL"),
            )
            .expect("DIAL record");
        let scene_index = quest_children
            .children
            .iter()
            .position(
                |item| matches!(item, ParsedItem::Record(record) if record.signature.as_str() == "SCEN"),
            )
            .expect("SCEN record");
        assert!(
            dialogue_index < scene_index,
            "FO4 quest-child groups require DIAL/INFO blocks before SCEN records"
        );
    }

    fn write_temp_plugin(bytes: &[u8]) -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut f = tempfile::Builder::new().suffix(".esm").tempfile().unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
        f
    }

    /// Load a real eager handle from a file WITHOUT a Python interpreter
    /// (cargo tests can't start one): empty handle, then overwrite `parsed`.
    fn load_source_handle(path: &std::path::Path, game: &str) -> u64 {
        let parsed = parse_plugin_file(&path.to_string_lossy(), Some(game.to_string()), true)
            .expect("parse source");
        let name = parsed.plugin_name.clone();
        let handle = plugin_handle_new_native(&name, Some(game)).unwrap();
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&handle).unwrap();
        slot.parsed = parsed;
        slot.invalidate_sections();
        handle
    }

    fn fresh_target_handle() -> u64 {
        plugin_handle_new_native("Out.esm", Some("fo4")).unwrap()
    }

    fn run_one(
        use_v2: bool,
        source_path: &std::path::Path,
        records_limit: Option<usize>,
    ) -> (u64, TranslateStats) {
        let source_handle = load_source_handle(source_path, "fo76");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: true,
                preserve_source_ids: false,
                generated_object_id_floor: 0x800,
                records_limit,
                ..Default::default()
            },
        })
        .unwrap();
        let stats = with_run(id, |run| {
            if use_v2 {
                run.translate_all_v2(source_path)
            } else {
                run.translate_all()
            }
        })
        .unwrap();
        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        (target_handle, stats)
    }

    fn run_legacy_magic_one(
        use_v2: bool,
        source_path: &std::path::Path,
    ) -> (u64, TranslateStats, Vec<String>) {
        run_legacy_magic_with_output(use_v2, source_path, "Out.esm", false, 0x800)
    }

    fn run_legacy_magic_with_output(
        use_v2: bool,
        source_path: &std::path::Path,
        output_plugin_name: &str,
        preserve_source_ids: bool,
        generated_object_id_floor: u32,
    ) -> (u64, TranslateStats, Vec<String>) {
        let source_handle = load_source_handle(source_path, "fnv");
        let target_handle = plugin_handle_new_native(output_plugin_name, Some("fo4")).unwrap();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: output_plugin_name.into(),
                is_whole_plugin: false,
                strict_mapper: true,
                preserve_source_ids,
                generated_object_id_floor,
                legacy_pack_expected_counts: Some(LegacyPackExpectedCounts { fnv: 0, fo3: 0 }),
                ..Default::default()
            },
        })
        .unwrap();
        let stats = with_run(id, |run| {
            if use_v2 {
                run.translate_all_v2(source_path)
            } else {
                run.translate_all()
            }
        })
        .unwrap();
        let legacy_magic_events = with_run(id, |run| {
            Ok::<_, RunError>(
                run.event_rx
                    .try_iter()
                    .filter_map(|event| match event {
                        PhaseEvent::Log { message, .. }
                            if message.starts_with("legacy_serial:") =>
                        {
                            Some(message)
                        }
                        _ => None,
                    })
                    .collect(),
            )
        })
        .unwrap();
        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        (target_handle, stats, legacy_magic_events)
    }

    fn run_legacy_unsupported_creature(
        use_v2: bool,
    ) -> (Result<TranslateStats, RunError>, Vec<String>, usize) {
        run_legacy_unsupported_creature_fixture(use_v2, &legacy_unsupported_creature_fixture())
    }

    fn run_legacy_unsupported_creature_fixture(
        use_v2: bool,
        fixture_bytes: &[u8],
    ) -> (Result<TranslateStats, RunError>, Vec<String>, usize) {
        run_legacy_single_record_fixture(use_v2, fixture_bytes)
    }

    fn run_legacy_single_record_fixture(
        use_v2: bool,
        fixture_bytes: &[u8],
    ) -> (Result<TranslateStats, RunError>, Vec<String>, usize) {
        run_legacy_single_record_fixture_with_skips(use_v2, fixture_bytes, Vec::new())
    }

    fn run_legacy_single_record_fixture_with_skips(
        use_v2: bool,
        fixture_bytes: &[u8],
        skip_record_signatures: Vec<String>,
    ) -> (Result<TranslateStats, RunError>, Vec<String>, usize) {
        let fixture = write_temp_plugin(fixture_bytes);
        let source_handle = load_source_handle(fixture.path(), "fnv");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: false,
                records_limit: Some(1),
                skip_record_signatures,
                ..Default::default()
            },
        })
        .unwrap();
        let result = with_run(id, |run| {
            if use_v2 {
                run.translate_all_v2(fixture.path())
            } else {
                run.translate_all()
            }
        });
        let (decision_kinds, output_records) = with_run(id, |run| {
            Ok::<_, RunError>((
                run.decisions
                    .iter()
                    .filter_map(|decision| run.interner.resolve(decision.kind))
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
                handle_records(target_handle).len(),
            ))
        })
        .unwrap();
        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
        (result, decision_kinds, output_records)
    }

    fn run_legacy_pack_preflight_case(
        use_v2: bool,
        source_path: &std::path::Path,
        origins: &[(u32, LegacyPackSourceFamily)],
        expected: LegacyPackExpectedCounts,
    ) -> (
        Result<TranslateStats, RunError>,
        LegacyPackPreflightReport,
        Vec<String>,
        usize,
    ) {
        let source_handle = load_source_handle(source_path, "fnv");
        let source_plugin = parse_plugin_file(
            &source_path.to_string_lossy(),
            Some("fnv".to_string()),
            true,
        )
        .unwrap()
        .plugin_name;
        let target_handle = fresh_target_handle();
        assert!(handle_records(target_handle).is_empty());
        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: true,
                records_limit: Some(0),
                legacy_pack_origins: explicit_pack_origins(&source_plugin, origins),
                legacy_pack_expected_counts: Some(expected),
                legacy_pack_provenance_required: true,
                ..Default::default()
            },
        })
        .unwrap();
        let result = with_run(id, |run| {
            if use_v2 {
                run.translate_all_v2(source_path)
            } else {
                run.translate_all()
            }
        });
        let (report, events) = with_run(id, |run| {
            Ok::<_, RunError>((
                run.legacy_pack_preflight_report
                    .clone()
                    .expect("fatal PACK preflight report"),
                run.event_rx
                    .try_iter()
                    .filter_map(|event| match event {
                        PhaseEvent::Log { message, .. }
                            if message.starts_with("legacy_pack_preflight:") =>
                        {
                            Some(message)
                        }
                        _ => None,
                    })
                    .collect(),
            ))
        })
        .unwrap();
        let target_record_count = handle_records(target_handle).len();
        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
        (result, report, events, target_record_count)
    }

    fn record_with_edid<'a>(records: &'a [ParsedRecord], editor_id: &str) -> &'a ParsedRecord {
        records
            .iter()
            .find(|record| {
                record.subrecords.iter().any(|subrecord| {
                    subrecord.signature.as_str() == "EDID"
                        && subrecord.data.strip_suffix(&[0]).unwrap_or(&subrecord.data)
                            == editor_id.as_bytes()
                })
            })
            .unwrap_or_else(|| panic!("missing target record {editor_id}"))
    }

    fn subrecord_data<'a>(record: &'a ParsedRecord, signature: &str) -> &'a [u8] {
        record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == signature)
            .map(|subrecord| subrecord.data.as_ref())
            .unwrap_or_else(|| panic!("{} missing {signature}", record.signature.as_str()))
    }

    fn raw_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn v2_target_handle_state_is_byte_identical_to_legacy() {
        let f = write_temp_plugin(&gate_fixture());
        let (h_old, stats_old) = run_one(false, f.path(), None);
        let (h_new, stats_new) = run_one(true, f.path(), None);

        assert_eq!(stats_old.records_translated, stats_new.records_translated);
        assert_eq!(stats_old.records_dropped, stats_new.records_dropped);
        assert_eq!(stats_old.records_deferred, stats_new.records_deferred);
        assert_eq!(stats_old.records_failed, stats_new.records_failed);
        assert_eq!(
            stats_old.records_vanilla_remapped,
            stats_new.records_vanilla_remapped
        );

        assert_handles_equal(h_old, h_new);

        plugin_handle_close_native(h_old);
        plugin_handle_close_native(h_new);
    }

    #[test]
    fn merged_legacy_pack_preflight_blocks_before_mutation_with_legacy_v2_parity() {
        let (fixture, origins) = merged_legacy_pack_fixture();
        let fixture = write_temp_plugin(&fixture);
        let expected = LegacyPackExpectedCounts { fnv: 6, fo3: 5 };
        let (legacy_result, legacy_report, legacy_events, legacy_target_records) =
            run_legacy_pack_preflight_case(false, fixture.path(), &origins, expected);
        let (v2_result, v2_report, v2_events, v2_target_records) =
            run_legacy_pack_preflight_case(true, fixture.path(), &origins, expected);

        for (result, report) in [(legacy_result, &legacy_report), (v2_result, &v2_report)] {
            match result {
                Err(RunError::LegacyPackPreflight(error_report)) => {
                    assert_eq!(*error_report, *report)
                }
                _ => panic!("expected fatal legacy PACK preflight"),
            }
        }
        assert_eq!(legacy_report, v2_report);
        assert_eq!(legacy_events, v2_events);
        assert_eq!(legacy_events.len(), 2);
        assert!(legacy_events[0].starts_with("legacy_pack_preflight:start:"));
        assert!(legacy_events[1].starts_with("legacy_pack_preflight:blocked:"));
        assert_eq!(legacy_target_records, 0);
        assert_eq!(v2_target_records, 0);

        assert_eq!(legacy_report.expected, expected);
        assert_eq!(legacy_report.observed_pack_records, 11);
        assert!(legacy_report.exact_expected_coverage);
        assert_eq!(legacy_report.classified.fnv_records, 6);
        assert_eq!(legacy_report.classified.fo3_records, 5);
        assert_eq!(legacy_report.classified.accepted_records, 10);
        assert_eq!(legacy_report.classified.rejected_records, 1);
        assert_eq!(legacy_report.blocked_records.len(), 1);
        assert_eq!(
            legacy_report.blocker_counts.get("malformed_subrecord"),
            Some(&1)
        );
        for type_code in [
            LegacyPackType::Travel.code(),
            LegacyPackType::Patrol.code(),
            LegacyPackType::Follow.code(),
            LegacyPackType::Sandbox.code(),
            LegacyPackType::UseWeapon.code(),
        ] {
            assert_eq!(legacy_report.classified.by_type.get(&type_code), Some(&2));
        }

        let malformed = legacy_report
            .blocked_records
            .iter()
            .find(|record| record.editor_id.as_deref() == Some("FNVMalformed"))
            .unwrap();
        assert!(
            malformed
                .blockers
                .iter()
                .any(|blocker| blocker == "malformed_subrecord")
        );
    }

    #[test]
    fn merged_legacy_pack_preflight_reports_explicit_family_count_drift() {
        let (fixture, origins) = merged_legacy_pack_fixture();
        let fixture = write_temp_plugin(&fixture);
        let (_, report, _, target_records) = run_legacy_pack_preflight_case(
            true,
            fixture.path(),
            &origins,
            LegacyPackExpectedCounts { fnv: 5, fo3: 5 },
        );

        assert!(!report.exact_expected_coverage);
        assert_eq!(report.classified.fnv_records, 6);
        assert_eq!(report.classified.fo3_records, 5);
        assert_eq!(
            report.blocker_counts.get("expected_fnv_count_mismatch"),
            Some(&1)
        );
        assert_eq!(
            report.blocker_counts.get("expected_total_count_mismatch"),
            Some(&1)
        );
        assert_eq!(target_records, 0);
    }

    #[test]
    fn legacy_pack_preflight_is_inactive_for_fo76_partial_and_non_fo4_runs() {
        for (source, target, is_whole_plugin) in [
            (Game::Fo76, Game::Fo4, true),
            (Game::Fnv, Game::Fo4, false),
            (Game::Fnv, Game::Fo76, true),
        ] {
            let source_handle = plugin_handle_new_native(
                "FalloutNV.esm",
                Some(match source {
                    Game::Fo76 => "fo76",
                    Game::Fnv => "fnv",
                    _ => unreachable!(),
                }),
            )
            .unwrap();
            let target_handle = plugin_handle_new_native(
                "Out.esm",
                Some(if target == Game::Fo4 { "fo4" } else { "fo76" }),
            )
            .unwrap();
            let id = create_run(RunParams {
                source,
                target,
                source_handle_id: source_handle,
                target_handle_id: target_handle,
                master_handle_ids: vec![],
                config: RunConfig {
                    output_plugin_name: "Out.esm".into(),
                    is_whole_plugin,
                    legacy_pack_provenance_required: true,
                    legacy_pack_expected_counts: Some(LegacyPackExpectedCounts {
                        fnv: 4_888,
                        fo3: 4_567,
                    }),
                    ..Default::default()
                },
            })
            .unwrap();

            with_run(id, |run| {
                assert!(!run.legacy_pack_gate_active());
                assert!(run.begin_legacy_pack_preflight("FalloutNV.esm").is_none());
                assert!(run.legacy_pack_preflight_report.is_none());
                Ok::<_, RunError>(())
            })
            .unwrap();
            drop_run(id).unwrap();
            plugin_handle_close_native(source_handle);
            plugin_handle_close_native(target_handle);
        }
    }

    #[test]
    fn legacy_serial_records_are_order_independent_and_paths_match() {
        let fixture = write_temp_plugin(&legacy_magic_fixture());
        let (legacy_handle, legacy_stats, legacy_events) =
            run_legacy_magic_one(false, fixture.path());
        let (v2_handle, v2_stats, v2_events) = run_legacy_magic_one(true, fixture.path());

        assert_eq!(legacy_stats.records_failed, 0);
        assert_eq!(v2_stats.records_failed, 0);
        assert_eq!(legacy_stats.records_translated, 9);
        assert_eq!(v2_stats.records_translated, 9);
        assert_handles_equal(legacy_handle, v2_handle);
        for events in [&legacy_events, &v2_events] {
            for decision in [
                "reference:assoc_item:",
                "reference:casting_light:",
                "reference:hit_shader:",
                "PreservedHardcoded",
                "legacy_serial:ALCH:",
                "legacy_serial:ENCH:",
                "legacy_serial:SPEL:",
                ":perk_summary:",
                ":perk_reference:",
                ":wrld_data:",
            ] {
                assert!(
                    events.iter().any(|event| event.contains(decision)),
                    "missing serial-normalizer event containing {decision}"
                );
            }
        }

        let records = handle_records(v2_handle);
        let primary = record_with_edid(&records, "ForwardPrimaryEffect");
        let assoc = record_with_edid(&records, "ForwardAssocEffect");
        let light = record_with_edid(&records, "ForwardLight");
        let shader = record_with_edid(&records, "ForwardShader");
        let data = subrecord_data(primary, "DATA");
        assert_eq!(data.len(), 152);
        assert_eq!(raw_u32(data, 8) & 0x00FF_FFFF, assoc.form_id & 0x00FF_FFFF);
        assert_eq!(raw_u32(data, 24) & 0x00FF_FFFF, light.form_id & 0x00FF_FFFF);
        assert_eq!(
            raw_u32(data, 32) & 0x00FF_FFFF,
            shader.form_id & 0x00FF_FFFF
        );
        assert_eq!(
            raw_u32(data, 36) & 0x00FF_FFFF,
            shader.form_id & 0x00FF_FFFF
        );
        assert_eq!(raw_u32(data, 68), 0xF400_02BC);

        for (editor_id, metadata_sig, metadata_len) in [
            ("ForwardPotion", "ENIT", 20),
            ("ForwardEnchant", "ENIT", 36),
            ("ForwardSpell", "SPIT", 36),
        ] {
            let record = record_with_edid(&records, editor_id);
            assert_eq!(subrecord_data(record, metadata_sig).len(), metadata_len);
            assert_eq!(
                record
                    .subrecords
                    .iter()
                    .filter(|subrecord| subrecord.signature.as_str() == "EFID")
                    .count(),
                1
            );
            assert_eq!(
                record
                    .subrecords
                    .iter()
                    .filter(|subrecord| subrecord.signature.as_str() == "EFIT")
                    .count(),
                1
            );
            assert_eq!(subrecord_data(record, "EFIT").len(), 12);
            assert_eq!(
                raw_u32(subrecord_data(record, "EFID"), 0) & 0x00FF_FFFF,
                primary.form_id & 0x00FF_FFFF
            );
        }

        let spell = record_with_edid(&records, "ForwardSpell");
        let perk = record_with_edid(&records, "ForwardPerk");
        assert_eq!(
            perk.subrecords
                .iter()
                .filter(|subrecord| subrecord.signature.as_str() == "PRKE")
                .count(),
            2,
            "PERK entry conversion must run exactly once"
        );
        assert_eq!(
            perk.subrecords
                .iter()
                .filter(|subrecord| subrecord.signature.as_str() == "PRKF")
                .count(),
            2
        );
        let perk_data = perk
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "DATA")
            .map(|subrecord| subrecord.data.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(
            perk_data
                .iter()
                .find(|data| data.starts_with(&[100, 3, 1]))
                .map(|data| data.len()),
            Some(3),
            "entry-point PERK DATA must use the three-byte FO4 union variant: {perk_data:?}"
        );
        assert!(perk_data.iter().any(|data| {
            data.len() >= 4 && raw_u32(data, 0) & 0x00FF_FFFF == spell.form_id & 0x00FF_FFFF
        }));
        let perk_conditions = perk
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "CTDA")
            .map(|subrecord| subrecord.data.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(perk_conditions.len(), 2);
        assert!(
            perk_conditions
                .iter()
                .all(|condition| condition.len() == 32)
        );
        assert_eq!(
            u16::from_le_bytes(perk_conditions[0][8..10].try_into().unwrap()),
            494
        );
        assert_eq!(
            raw_u32(perk_conditions[1], 12) & 0x00FF_FFFF,
            spell.form_id & 0x00FF_FFFF
        );
        assert_eq!(
            perk.subrecords
                .iter()
                .filter(|subrecord| subrecord.signature.as_str() == "CIS1")
                .count(),
            2
        );
        assert_eq!(
            perk.subrecords
                .iter()
                .map(|subrecord| subrecord.signature.as_str())
                .collect::<Vec<_>>(),
            vec![
                "EDID", "CTDA", "CIS1", "DATA", "PRKE", "DATA", "PRKC", "CTDA", "CIS1", "EPFT",
                "EPFD", "PRKF", "PRKE", "DATA", "PRKF",
            ]
        );

        let world = record_with_edid(&records, "WastelandNV");
        assert_eq!(subrecord_data(world, "DATA"), &[0]);
        assert!(
            !world
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature.as_str() == "INAM")
        );

        plugin_handle_close_native(legacy_handle);
        plugin_handle_close_native(v2_handle);
    }

    #[test]
    fn unsupported_legacy_creatures_are_fail_closed() {
        for use_v2 in [false, true] {
            let (blocked, blocked_decisions, blocked_records) =
                run_legacy_unsupported_creature(use_v2);
            let error = blocked.expect_err("creature policy must fail closed");
            assert!(
                error.to_string().contains("legacy_creature_race_gate:"),
                "{error}"
            );
            assert!(blocked_decisions.is_empty());
            assert_eq!(blocked_records, 0);
        }
    }

    #[test]
    fn explicitly_skipped_legacy_creatures_bypass_the_strict_race_gate() {
        let fixture = legacy_unsupported_creature_fixture();
        for use_v2 in [false, true] {
            let (result, decision_kinds, output_records) =
                run_legacy_single_record_fixture_with_skips(
                    use_v2,
                    &fixture,
                    vec!["CREA".to_string()],
                );
            let stats = result.expect("excluded CREA records must not enter the race gate");
            assert_eq!(stats.records_translated, 0);
            assert_eq!(stats.records_dropped, 1);
            assert_eq!(decision_kinds, vec!["skip_records".to_string()]);
            assert_eq!(output_records, 0);
        }
    }

    #[test]
    fn humanoid_creature_failure_is_identical_in_legacy_and_v2_paths() {
        let fixture = legacy_humanoid_creature_fixture();
        for use_v2 in [false, true] {
            let (blocked, blocked_decisions, blocked_records) =
                run_legacy_unsupported_creature_fixture(use_v2, &fixture);
            let error = blocked.expect_err("creature policy must fail closed");
            let diagnostic = error.to_string();
            assert!(diagnostic.contains("LegionCreature"), "{diagnostic}");
            assert!(
                diagnostic.contains("humanoid_creature_has_no_audited_fo4_race_donor"),
                "{diagnostic}"
            );
            assert!(blocked_decisions.is_empty());
            assert_eq!(blocked_records, 0);
        }
    }

    #[test]
    fn unused_ingredient_sentinel_drop_is_identical_in_legacy_and_v2_paths() {
        let fixture = legacy_unused_ingredient_sentinel_fixture();
        for use_v2 in [false, true] {
            let (result, decision_kinds, output_records) =
                run_legacy_single_record_fixture(use_v2, &fixture);
            let stats = result.expect("unused ingredient sentinel must drop safely");
            assert_eq!(stats.records_translated, 0);
            assert_eq!(stats.records_dropped, 1);
            assert_eq!(output_records, 0);
            assert_eq!(
                decision_kinds,
                vec!["unused_legacy_ingredient_sentinel".to_string()]
            );
        }
    }

    #[test]
    fn duplicate_legacy_wrld_drops_atomically_with_legacy_v2_parity() {
        let fixture = write_temp_plugin(&legacy_duplicate_wrld_fixture());
        let (legacy_handle, legacy_stats, legacy_events) =
            run_legacy_magic_one(false, fixture.path());
        let (v2_handle, v2_stats, v2_events) = run_legacy_magic_one(true, fixture.path());

        assert_eq!(legacy_stats.records_translated, 0);
        assert_eq!(v2_stats.records_translated, 0);
        assert_eq!(legacy_stats.records_dropped, 1);
        assert_eq!(v2_stats.records_dropped, 1);
        assert_handles_equal(legacy_handle, v2_handle);
        assert!(handle_records(legacy_handle).is_empty());
        for events in [&legacy_events, &v2_events] {
            assert!(events.iter().any(|event| {
                event.contains("wrld_drop") && event.contains("DuplicateDataFields")
            }));
        }

        plugin_handle_close_native(legacy_handle);
        plugin_handle_close_native(v2_handle);
    }

    #[test]
    fn same_name_legacy_output_uses_disjoint_ids_with_path_parity() {
        let fixture = write_temp_plugin(&legacy_same_name_identity_collision_fixture());
        let source_name = parse_plugin_file(
            &fixture.path().to_string_lossy(),
            Some("fnv".to_string()),
            true,
        )
        .unwrap()
        .plugin_name;
        let (legacy_handle, legacy_stats, _) =
            run_legacy_magic_with_output(false, fixture.path(), &source_name, true, 0);
        let (v2_handle, v2_stats, _) =
            run_legacy_magic_with_output(true, fixture.path(), &source_name, true, 0);

        assert_eq!(legacy_stats.records_translated, 3);
        assert_eq!(v2_stats.records_translated, 3);
        assert_eq!(legacy_stats.records_failed, 0);
        assert_eq!(v2_stats.records_failed, 0);
        assert_handles_equal(legacy_handle, v2_handle);

        let records = handle_records(v2_handle);
        let generated = record_with_edid(&records, "LowGeneratedEffect");
        let preserved = record_with_edid(&records, "PreservedFloorEffect");
        let potion = record_with_edid(&records, "LowEffectPotion");
        assert_eq!(preserved.form_id & 0x00FF_FFFF, 0x800);
        assert_eq!(generated.form_id & 0x00FF_FFFF, 0x901);
        assert_eq!(
            raw_u32(subrecord_data(potion, "EFID"), 0) & 0x00FF_FFFF,
            generated.form_id & 0x00FF_FFFF
        );

        plugin_handle_close_native(legacy_handle);
        plugin_handle_close_native(v2_handle);
    }

    #[test]
    fn same_name_mapped_climate_is_not_remapped_again_by_late_sweep() {
        let fixture = write_temp_plugin(&skyrim_same_name_reference_collision_fixture());
        let source_name = parse_plugin_file(
            &fixture.path().to_string_lossy(),
            Some("skyrimse".to_string()),
            true,
        )
        .unwrap()
        .plugin_name;
        let source_handle = load_source_handle(fixture.path(), "skyrimse");
        let target_handle = plugin_handle_new_native(&source_name, Some("fo4")).unwrap();
        let id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: source_name.clone(),
                is_whole_plugin: true,
                preserve_source_ids: false,
                generated_object_id_floor: SAME_NAME_COLLIDING_PATH_SOURCE_ID,
                ..Default::default()
            },
        })
        .unwrap();

        let (stats, climate_target, colliding_path_target, unresolved_count) =
            with_run(id, |run| {
                let stats = run.translate_all_v2(fixture.path())?;
                let same_name_plugin = run.interner.intern(&source_name);
                let climate_source = FormKey {
                    local: SAME_NAME_CLIMATE_SOURCE_ID,
                    plugin: same_name_plugin,
                };
                let colliding_path_source = FormKey {
                    local: SAME_NAME_COLLIDING_PATH_SOURCE_ID,
                    plugin: same_name_plugin,
                };
                let mapper_state = run.mapper_state.as_ref().expect("mapper state");
                let climate_target = mapper_state.source_to_target[&climate_source];
                let colliding_path_target = mapper_state.source_to_target[&colliding_path_source];
                run.apply_fixups_v2().map_err(RunError::from)?;
                Ok::<_, RunError>((
                    stats,
                    climate_target,
                    colliding_path_target,
                    run.full_plugin_state.unresolved_ref_count(),
                ))
            })
            .unwrap();

        assert_eq!(stats.records_translated, 3);
        assert_eq!(stats.records_failed, 0);
        assert_eq!(
            climate_target.local, SAME_NAME_COLLIDING_PATH_SOURCE_ID,
            "fixture must make mapper.lookup(A) collide with source B"
        );
        assert_ne!(climate_target, colliding_path_target);
        assert_eq!(unresolved_count, 0);

        let records = handle_records(target_handle);
        let world = record_with_edid(&records, "SameNameWorld");
        let climate_ref = raw_u32(subrecord_data(world, "CNAM"), 0) & 0x00FF_FFFF;
        assert_eq!(climate_ref, climate_target.local);
        assert_ne!(climate_ref, colliding_path_target.local);
        let resolved = records
            .iter()
            .find(|record| record.form_id & 0x00FF_FFFF == climate_ref)
            .expect("WRLD CNAM target record");
        assert_eq!(resolved.signature.as_str(), "CLMT");

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn skyrim_managed_skip_record_bypasses_runtime_component_gate() {
        let fixture = write_temp_plugin(&plugin(
            &[],
            &[group(
                b"PACK",
                0,
                &[record(
                    b"PACK",
                    0x0000_0800,
                    0,
                    &subrecord(b"EDID", b"SkippedPackage\0"),
                )],
            )],
        ));
        let source_handle = load_source_handle(fixture.path(), "skyrimse");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: true,
                ..Default::default()
            },
        })
        .unwrap();

        let stats = with_run(id, |run| run.translate_all_v2(fixture.path())).unwrap();

        assert_eq!(stats.records_translated, 0);
        assert_eq!(stats.records_dropped, 1);
        assert!(handle_records(target_handle).is_empty());

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn skyrim_planned_shout_survives_lowered_spell_mvp_skip() {
        const SHOUT_FORM_ID: u32 = 0x0002_F7BA;
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("Skyrim.esm");
        std::fs::write(
            &source_path,
            plugin(
                &[],
                &[group(
                    b"SHOU",
                    0,
                    &[record(
                        b"SHOU",
                        SHOUT_FORM_ID,
                        0,
                        &subrecord(b"EDID", b"WhirlwindSprintShout\0"),
                    )],
                )],
            ),
        )
        .unwrap();
        let source_handle = load_source_handle(&source_path, "skyrimse");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::SkyrimSe,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Skyrim.esm".into(),
                is_whole_plugin: true,
                preserve_source_ids: true,
                skip_record_signatures: vec!["SPEL".to_string()],
                ..Default::default()
            },
        })
        .unwrap();

        let stats = with_run(id, |run| {
            let stats = run.translate_all_v2(&source_path)?;
            run.finalize_skyrim_runtime_receipt()?;
            Ok::<_, RunError>(stats)
        })
        .unwrap();

        assert_eq!(stats.records_translated, 1);
        let records = handle_records(target_handle);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].signature.as_str(), "SPEL");

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn fo76_perk_fragment_vmad_rewrites_forward_quest_reference() {
        let (fixture, quest_form_id_offset) = perk_fragment_forward_quest_fixture();
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("SeventySix.esm");
        std::fs::write(&source_path, fixture).unwrap();
        let source_handle = load_source_handle(&source_path, "fo76");
        let target_handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "SeventySix.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                is_whole_plugin: false,
                preserve_source_ids: true,
                generated_object_id_floor: 0x800,
                ..Default::default()
            },
        })
        .unwrap();

        let stats = with_run(id, |run| run.translate_all_v2(&source_path)).unwrap();
        let records = handle_records(target_handle);
        let perk = record_with_edid(&records, "W05_MQS_201P_EyebotPartPerk");
        let quest = record_with_edid(&records, "W05_MQS_201P");
        let vmad = subrecord_data(perk, "VMAD");

        assert_eq!(stats.records_translated, 2);
        assert_eq!(stats.records_failed, 0);
        assert_eq!(raw_u32(vmad, quest_form_id_offset), quest.form_id);

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn fo76_frog_collecting_perk_preserves_root_vmad_and_activate_choice_entry() {
        let (fixture, frog_item_form_id_offset) = frog_collecting_perk_fixture();
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp.path().join("SeventySix.esm");
        std::fs::write(&source_path, fixture).unwrap();
        let source_handle = load_source_handle(&source_path, "fo76");
        let target_handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "SeventySix.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                is_whole_plugin: true,
                preserve_source_ids: true,
                generated_object_id_floor: 0x800,
                ..Default::default()
            },
        })
        .unwrap();

        let stats = with_run(id, |run| run.translate_all_v2(&source_path)).unwrap();
        let records = handle_records(target_handle);
        let perk = record_with_edid(&records, "FrogCollectingPerk");
        let frog_item = record_with_edid(&records, "CapturedFrog");
        let vmad = subrecord_data(perk, "VMAD");

        assert_eq!(stats.records_translated, 2);
        assert_eq!(stats.records_failed, 0);
        assert_eq!(raw_u32(vmad, frog_item_form_id_offset), frog_item.form_id);
        assert_eq!(
            perk.subrecords
                .iter()
                .map(|subrecord| subrecord.signature.as_str())
                .collect::<Vec<_>>(),
            vec![
                "EDID", "VMAD", "DATA", "PRKE", "DATA", "PRKC", "CTDA", "PRKC", "CTDA", "CTDA",
                "EPFT", "EPFB", "EPF2", "EPF3", "PRKF",
            ]
        );
        assert_eq!(subrecord_data(perk, "PRKE"), &[2, 0, 0]);
        assert_eq!(
            perk.subrecords
                .iter()
                .filter(|subrecord| subrecord.signature.as_str() == "DATA")
                .nth(1)
                .expect("ActivateChoice entry DATA")
                .data
                .as_ref(),
            &[0x00, 0x09, 0x02]
        );

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn scenes_are_emitted_under_their_parent_quests() {
        let f = write_temp_plugin(&quest_scene_fixture());
        let (h_old, stats_old) = run_one(false, f.path(), None);
        let (h_new, stats_new) = run_one(true, f.path(), None);

        assert_eq!(stats_old.records_translated, 3);
        assert_eq!(stats_new.records_translated, 3);
        assert_handles_equal(h_old, h_new);
        assert_scene_is_nested_under_quest(h_old);
        assert_scene_is_nested_under_quest(h_new);

        plugin_handle_close_native(h_old);
        plugin_handle_close_native(h_new);
    }

    #[test]
    fn records_limit_subset_matches_legacy() {
        let f = write_temp_plugin(&gate_fixture());
        let (h_old, stats_old) = run_one(false, f.path(), Some(3));
        let (h_new, stats_new) = run_one(true, f.path(), Some(3));
        assert_eq!(stats_old.records_translated, stats_new.records_translated);
        assert_handles_equal(h_old, h_new);
        plugin_handle_close_native(h_old);
        plugin_handle_close_native(h_new);
    }

    #[test]
    fn story_manager_subset_emits_selected_records_after_generic_skip() {
        let f = write_temp_plugin(&story_manager_fixture());
        let source_handle = load_source_handle(f.path(), "fo76");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: true,
                preserve_source_ids: false,
                generated_object_id_floor: 0x800,
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| run.translate_all_v2(f.path())).unwrap();
        let after_generic: Vec<_> = handle_records(target_handle)
            .into_iter()
            .map(|record| record.signature)
            .collect();
        assert!(!after_generic.iter().any(|sig| sig == "SMEN"));
        assert!(!after_generic.iter().any(|sig| sig == "SMBN"));
        assert!(!after_generic.iter().any(|sig| sig == "SMQN"));

        let emit_stats = with_run(id, |run| run.emit_story_manager_subset()).unwrap();
        assert_eq!(emit_stats.selected_nodes, 3);
        assert_eq!(emit_stats.translate.records_translated, 3);
        assert_eq!(emit_stats.translate.records_failed, 0);

        let after_emit: Vec<_> = handle_records(target_handle)
            .into_iter()
            .map(|record| record.signature)
            .collect();
        assert!(after_emit.iter().any(|sig| sig == "SMEN"));
        assert!(after_emit.iter().any(|sig| sig == "SMBN"));
        assert!(after_emit.iter().any(|sig| sig == "SMQN"));

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn story_manager_subset_reuses_fo4_event_root() {
        let f = write_temp_plugin(&story_manager_fixture());
        let source_handle = load_source_handle(f.path(), "fo76");
        let target_handle = fresh_target_handle();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                is_whole_plugin: true,
                preserve_source_ids: false,
                generated_object_id_floor: 0x800,
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| run.translate_all_v2(f.path())).unwrap();
        let emit_stats = with_run(id, |run| run.emit_story_manager_subset()).unwrap();

        assert_eq!(emit_stats.selected_nodes, 3);
        assert_eq!(emit_stats.translate.records_translated, 2);
        assert_eq!(emit_stats.translate.records_vanilla_remapped, 1);
        let after_emit: Vec<_> = handle_records(target_handle)
            .into_iter()
            .map(|record| record.signature)
            .collect();
        assert!(!after_emit.iter().any(|sig| sig == "SMEN"));
        assert!(after_emit.iter().any(|sig| sig == "SMBN"));
        assert!(after_emit.iter().any(|sig| sig == "SMQN"));

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn story_manager_subset_lowers_incompatible_event_to_isolated_script_branch() {
        let f = write_temp_plugin(&story_manager_fixture_with_event(b"PCON"));
        let source_handle = load_source_handle(f.path(), "fo76");
        let target_handle = fresh_target_handle();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                is_whole_plugin: true,
                preserve_source_ids: false,
                generated_object_id_floor: 0x800,
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| run.translate_all_v2(f.path())).unwrap();
        let emit_stats = with_run(id, |run| run.emit_story_manager_subset()).unwrap();

        assert_eq!(emit_stats.selected_nodes, 3);
        assert_eq!(
            emit_stats.translate.records_translated, 4,
            "{emit_stats:#?}"
        );
        assert_eq!(emit_stats.translate.records_vanilla_remapped, 0);
        let records = handle_records(target_handle);
        assert!(!records.iter().any(|record| record.signature == "SMEN"));
        let keyword = record_with_edid(&records, "B21_SMEvent_PCON");
        assert_eq!(keyword.signature.as_str(), "KYWD");
        assert_eq!(keyword.form_id & 0x00FF_FFFF, 0x000200);

        let bridge = record_with_edid(&records, "SyntheticEventRoot");
        assert_eq!(bridge.signature.as_str(), "SMBN");
        assert_eq!(
            raw_u32(subrecord_data(bridge, "PNAM"), 0) & 0x00FF_FFFF,
            crate::phase::story_manager::FO4_SCRIPT_EVENT_ROOT_LOCAL
        );
        let gate = subrecord_data(bridge, "CTDA");
        assert_eq!(
            raw_u32(gate, 16) & 0x00FF_FFFF,
            keyword.form_id & 0x00FF_FFFF
        );
        assert_eq!(raw_u32(subrecord_data(bridge, "CITC"), 0), 1);

        let child = record_with_edid(&records, "SyntheticRadioBranch");
        assert_eq!(
            raw_u32(subrecord_data(child, "PNAM"), 0) & 0x00FF_FFFF,
            bridge.form_id & 0x00FF_FFFF
        );

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn story_manager_same_name_bridge_parent_survives_fixup_sweep() {
        let f = write_temp_plugin(&story_manager_fixture_with_event(b"PCON"));
        let source_handle = load_source_handle(f.path(), "fo76");
        let output_plugin_name = plugin_name_for_handle(source_handle).unwrap();
        let target_handle = plugin_handle_new_native(&output_plugin_name, Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name,
                target_master_names: vec!["Fallout4.esm".into()],
                is_whole_plugin: true,
                preserve_source_ids: true,
                generated_object_id_floor: 0x800,
                ..Default::default()
            },
        })
        .unwrap();

        with_run(id, |run| run.translate_all_v2(f.path())).unwrap();
        with_run(id, |run| run.emit_story_manager_subset()).unwrap();

        let records = handle_records(target_handle);
        let bridge_local = record_with_edid(&records, "SyntheticEventRoot").form_id & 0x00FF_FFFF;
        let child = record_with_edid(&records, "SyntheticRadioBranch");
        assert_eq!(
            raw_u32(subrecord_data(child, "PNAM"), 0) & 0x00FF_FFFF,
            bridge_local
        );

        with_run(id, |run| {
            run.apply_fixups_v2().map_err(crate::run::RunError::from)
        })
        .unwrap();

        let records = handle_records(target_handle);
        let child = record_with_edid(&records, "SyntheticRadioBranch");
        assert_eq!(
            raw_u32(subrecord_data(child, "PNAM"), 0) & 0x00FF_FFFF,
            bridge_local
        );

        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn cancel_between_chunks_returns_cancelled() {
        let f = write_temp_plugin(&gate_fixture());
        let source_handle = load_source_handle(f.path(), "fo76");
        let target_handle = fresh_target_handle();
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Out.esm".into(),
                is_whole_plugin: true,
                generated_object_id_floor: 0x800,
                ..Default::default()
            },
        })
        .unwrap();
        let result = with_run(id, |run| {
            run.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            run.translate_all_v2(f.path())
        });
        assert!(matches!(result, Err(RunError::Cancelled)));
        // No records written.
        assert!(handle_records(target_handle).is_empty());
        drop_run(id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }
}
