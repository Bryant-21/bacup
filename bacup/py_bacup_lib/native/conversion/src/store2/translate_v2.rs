//! translate_all_v2 — the parallel translate pipeline.
//!
//! Passes per chunk of `CHUNK` source FormKeys (enumeration = legacy order):
//!   P (parallel): mmap read → decode → pre_translate → translate → EDID rename
//!   A (serial):   allocate_or_resolve → NVNM rewrite → rewrite_record   [mapper]
//!   F (parallel): post hooks → class-A → normalizer → namespacing → snapshot
//!   E (serial):   add_record_in_slot (encode + insert; lstring ids in order)
//! Serial passes preserve legacy determinism (FormKey + lstring allocation
//! order and partial-map forward-ref semantics). Tail (structured dialogue +
//! worldspace rebuild) reuses the legacy methods. No Python contact.

use std::path::Path;

use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

use esp_authoring_core::plugin_runtime::{LocalizedStringsState, plugin_handle_store_ref};

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::phase::progress::ProgressReporter;
use crate::phase::{LogLevel, PhaseEvent};
use crate::record::Record;
use crate::run::{
    ConversionRun, FnvScriLink, RunError, TranslateStats, base_asset_namespace,
    capture_target_master_context, is_target_master_remap, namespace_base_asset_model_paths,
    normalized_eid_opt, rename_fo76_target_editor_id_collision,
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
    },
    /// translate() returned Dropped — count dropped against source sig.
    Dropped(Decision),
    /// translate() returned Deferred — count deferred against source sig.
    Deferred(DeferredKind),
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
}

struct SkyrimNavmeshSet {
    converted: FxHashMap<FormKey, Vec<u8>>,
    failures: FxHashMap<FormKey, String>,
}

impl Candidate {
    fn from_pass_p(source_fk: FormKey, source_sig: SigCode, outcome: PassPOutcome) -> Self {
        let mut c = Candidate {
            source_fk,
            source_sig,
            seen: false,
            record: None,
            fnv_scri_target: None,
            disposition: Disposition::Failed,
            warnings: Vec::new(),
            decisions: Vec::new(),
            deferred: Vec::new(),
            fnv_scri_links: Vec::new(),
            full_plugin_snapshot: None,
        };
        match outcome {
            PassPOutcome::Translated {
                record,
                fnv_scri_target,
            } => {
                c.seen = true;
                c.record = Some(record);
                c.fnv_scri_target = fnv_scri_target;
                c.disposition = Disposition::Translated;
            }
            PassPOutcome::Dropped(decision) => {
                c.seen = true;
                c.decisions.push(decision);
                c.disposition = Disposition::Dropped;
            }
            PassPOutcome::Deferred(kind) => {
                c.seen = true;
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
    log_translate_v2(run, "translate_v2: init mapper state start");
    run.init_mapper_state()?;
    log_translate_v2(run, "translate_v2: init mapper state done");

    if run.config.records_limit == Some(0) {
        // Nothing to translate; still run the tail exactly like translate_all.
        let mut stats = TranslateStats::default();
        if run.should_emit_fo76_quest_scenes() {
            stats.absorb(run.emit_quest_child_scenes()?);
        }
        if run.should_emit_fo76_quest_dialogue() {
            stats.absorb(run.emit_quest_child_dialogue()?);
            stats.absorb(run.emit_topic_child_infos()?);
        }
        run.rebuild_full_plugin_worldspace_groups()?;
        return Ok(stats);
    }

    log_translate_v2(run, "translate_v2: opening source esm");
    let esm = SourceEsm::open(source_esm_path).map_err(|e| {
        RunError::InvalidConfig(format!("store2 source open {source_esm_path:?}: {e}"))
    })?;
    log_translate_v2(run, "translate_v2: source esm opened");
    log_translate_v2(run, "translate_v2: capture source context start");
    let source_ctx = capture_source_ctx(run.source_handle_id)?;
    log_translate_v2(run, "translate_v2: capture source context done");
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

    // Target master context (skip set + first master sym), built once.
    log_translate_v2(run, "translate_v2: capture target master context start");
    let (target_master_syms, first_target_master_sym) = capture_target_master_context(run);
    log_translate_v2(run, "translate_v2: capture target master context done");

    // The structured quest-child tail owns SCEN and, when dialogue is enabled,
    // DIAL/INFO. Exclude those signatures from the main enumeration.
    let emit_structured_dialogue = run.should_emit_fo76_quest_dialogue();
    let emit_structured_scenes = run.should_emit_fo76_quest_scenes();
    let dial_sig = SigCode::from_str("DIAL").map(|s| s.0).unwrap_or([0; 4]);
    let info_sig = SigCode::from_str("INFO").map(|s| s.0).unwrap_or([0; 4]);
    let scen_sig = SigCode::from_str("SCEN").map(|s| s.0).unwrap_or([0; 4]);

    // Enumerate (sig, position) in legacy order with the same per-sig-batch cap.
    let enumeration: Vec<(SigCode, usize)> = esm
        .enumerate_positions_sorted_sig(run.config.records_limit)
        .into_iter()
        .filter(|(sig, _)| {
            !((emit_structured_dialogue && (*sig == dial_sig || *sig == info_sig))
                || (emit_structured_scenes && *sig == scen_sig))
        })
        .map(|(sig_bytes, pos)| (SigCode(sig_bytes), pos))
        .collect();
    let total = enumeration.len() as u32;
    log_translate_v2(run, format!("translate_v2: enumerated records={total}"));
    let excluded_source_formkeys = collect_excluded_source_formkeys(
        &esm,
        &enumeration,
        &source_ctx,
        &run.translator.maps.skip_records,
        &run.interner,
    );

    let reporter = ProgressReporter::new("translate_v2", total, run.event_tx.clone());
    let mut stats = TranslateStats::default();

    for chunk in enumeration.chunks(CHUNK) {
        if run.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(RunError::Cancelled);
        }

        let mut candidates = run_pass_p(run, &esm, &source_ctx, &excluded_source_formkeys, chunk);
        run_pass_a(
            run,
            &target_master_syms,
            skyrim_navmeshes.as_ref(),
            &mut candidates,
        );
        run_pass_f(run, &mut candidates);
        run_pass_e(
            run,
            &target_master_syms,
            first_target_master_sym,
            &mut candidates,
        );

        merge_chunk(run, &mut stats, candidates);
        reporter.inc(chunk.len() as u32);
    }
    log_translate_v2(run, "translate_v2: per-record chunks done");
    log_translate_v2(run, "translate_v2: reporter finish start");
    reporter.finish();
    log_translate_v2(run, "translate_v2: reporter finish done");

    // Tail — mirror translate_all exactly.
    if emit_structured_scenes {
        log_translate_v2(run, "translate_v2: emit quest child scenes start");
        stats.absorb(run.emit_quest_child_scenes()?);
        log_translate_v2(run, "translate_v2: emit quest child scenes done");
    }
    if emit_structured_dialogue {
        log_translate_v2(run, "translate_v2: emit quest child dialogue start");
        stats.absorb(run.emit_quest_child_dialogue()?);
        log_translate_v2(run, "translate_v2: emit quest child dialogue done");
        log_translate_v2(run, "translate_v2: emit topic child infos start");
        stats.absorb(run.emit_topic_child_infos()?);
        log_translate_v2(run, "translate_v2: emit topic child infos done");
    }
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

    log_translate_v2(run, "translate_v2: returning stats");
    Ok(stats)
}

fn log_translate_v2(run: &ConversionRun, message: impl Into<String>) {
    let _ = run.event_tx.try_send(PhaseEvent::Log {
        phase: "translate_v2",
        level: LogLevel::Info,
        message: message.into(),
    });
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

    // FO76→FO4 struct relayout context.
    let relayout_target_schema =
        (source == Game::Fo76 && target == Game::Fo4).then(|| run.schema_target.clone());
    let relayout_ctx = relayout_target_schema.as_deref().map(|target_schema| {
        crate::struct_relayout::StructRelayoutCtx {
            target_schema,
            target_form_version:
                crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION,
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
                crate::translator::pair_hooks::fnv_fo4::FnvFo4Hook::capture_scri_target(
                    &src_record,
                    interner,
                )
                .map(str::to_string)
            } else {
                None
            };

            // pre_translate (errors → warning, record continues).
            {
                let mut ctx = crate::translator::pair_hook::PairCtx { interner };
                if let Err(e) = translator.pre_translate(&mut ctx, &mut src_record) {
                    pass_p_warnings.push(format!("pre_translate:{e}"));
                }
            }

            // translate (TopLevel mode — no forced-skip-signature mutation).
            let translated = match translator.translate(&src_record, interner) {
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
                    let mut c =
                        Candidate::from_pass_p(source_fk, source_sig, PassPOutcome::Deferred(kind));
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
            if source == Game::Fo76 && target == Game::Fo4 {
                if let Some(state) = mapper_state {
                    let translated_sig = translated.sig;
                    let renamed = rename_fo76_target_editor_id_collision(
                        &mut translated,
                        &state.target_eid_index,
                        interner,
                        crate::run::is_editor_id_collision_rename_forced(
                            source,
                            target,
                            translated_sig,
                        ),
                    );
                    if let Some((old, new)) = renamed {
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
                },
            );
            c.warnings = pass_p_warnings;
            c
        })
        .collect()
}

fn collect_excluded_source_formkeys(
    esm: &SourceEsm,
    enumeration: &[(SigCode, usize)],
    source_ctx: &SourceCtx,
    skipped_signatures: &FxHashSet<String>,
    interner: &StringInterner,
) -> FxHashSet<FormKey> {
    enumeration
        .iter()
        .filter(|(sig, _)| skipped_signatures.contains(sig.as_str()))
        .filter_map(|(_, pos)| esm.view_at(*pos).map(|view| view.form_id()))
        .map(|raw| form_key_for_raw(raw, &source_ctx.plugin_name, &source_ctx.masters, interner))
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

/// Pass A — serial, position order: allocate FormKeys + NVNM + rewrite refs.
/// The mapper is touched ONLY here and only in position order (deterministic
/// FK pre-allocation; partial-map forward-ref semantics match legacy).
fn run_pass_a(
    run: &mut ConversionRun,
    target_master_syms: &FxHashSet<Sym>,
    skyrim_navmeshes: Option<&SkyrimNavmeshSet>,
    candidates: &mut [Candidate],
) {
    let source = run.source;
    let target = run.target;
    let source_handle_id = run.source_handle_id;
    let target_handle_id = run.target_handle_id;
    let interner = &run.interner;
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
        if let Err(e) = mapper.rewrite_record(record) {
            c.warnings.push(format!("rewrite_record:{e}"));
        }

        if is_target_master_remap(target_fk, target_master_syms) {
            c.disposition = Disposition::VanillaRemapped;
            c.record = None;
            continue;
        }

        // FNV SCRI link — needs target_fk.
        if let Some(source_scpt_form_key) = c.fnv_scri_target.take() {
            if let Some(target_form_key) = crate::run::form_key_to_legacy_str(target_fk, interner) {
                c.fnv_scri_links.push(FnvScriLink {
                    target_form_key,
                    source_scpt_form_key,
                });
            }
        }
    }
}

/// Pass F — parallel: post hooks → class-A → normalizer → namespacing → snapshot.
fn run_pass_f(run: &mut ConversionRun, candidates: &mut [Candidate]) {
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
            let mut ctx = crate::translator::pair_hook::PairCtx { interner };
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
                );
            }
        }
    }
}

/// Merge one chunk's candidates into the run's stats/warnings/decisions/etc.,
/// strictly in position order.
fn merge_chunk(run: &mut ConversionRun, stats: &mut TranslateStats, candidates: Vec<Candidate>) {
    for c in candidates {
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
    use crate::record::{FieldEntry, FieldValue, Record};
    use crate::run::{RunConfig, RunParams, create_run, drop_run, with_run};
    use crate::store2::source::COMPRESSED_RECORD_FLAG;
    use crate::store2::source::test_fixture::*;
    use crate::store2::test_util::{assert_handles_equal, handle_records};
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, parse_plugin_file, plugin_handle_add_master_native, plugin_handle_close_native,
        plugin_handle_new_native, plugin_handle_store_ref,
    };

    #[test]
    fn placed_records_with_excluded_bases_are_identified() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("Skyrim_Merged.esm");
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

    fn story_manager_fixture() -> Vec<u8> {
        let root_id = 0x0000_0200u32;
        let branch_id = 0x0000_0201u32;
        let smqn_id = 0x0000_0202u32;
        let quest_id = 0x0000_0300u32;

        let mut qust_data = vec![0u8; 16];
        qust_data[0..4].copy_from_slice(&1u32.to_le_bytes());
        let mut quest_payload = subrecord(b"EDID", b"SyntheticRadioQuest\0");
        quest_payload.extend_from_slice(&subrecord(b"DATA", &qust_data));
        let quest = record(b"QUST", quest_id, 0, &quest_payload);

        let root = record(b"SMEN", root_id, 0, &subrecord(b"ENAM", b"SCPT"));

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

        let mut quest_data = vec![0u8; 16];
        quest_data[0..4].copy_from_slice(&1u32.to_le_bytes());
        let mut quest_payload = subrecord(b"EDID", b"SyntheticQuest\0");
        quest_payload.extend_from_slice(&subrecord(b"DATA", &quest_data));
        let quest = record(b"QUST", quest_id, 0, &quest_payload);

        let mut scene_payload = subrecord(b"EDID", b"SyntheticScene\0");
        scene_payload.extend_from_slice(&subrecord(b"FNAM", &0u32.to_le_bytes()));
        scene_payload.extend_from_slice(&subrecord(b"PNAM", &quest_id.to_le_bytes()));
        scene_payload.extend_from_slice(&subrecord(b"INAM", &0u32.to_le_bytes()));
        scene_payload.extend_from_slice(&subrecord(b"VNAM", &[0u8; 16]));
        scene_payload.extend_from_slice(&subrecord(b"XNAM", &0u32.to_le_bytes()));
        let scene = record(b"SCEN", scene_id, 0, &scene_payload);
        let quest_children = group(&quest_id.to_le_bytes(), 10, &[scene]);

        plugin(&[], &[group(b"QUST", 0, &[quest, quest_children])])
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
        assert!(quest_children.children.iter().any(
            |item| matches!(item, ParsedItem::Record(record) if record.signature.as_str() == "SCEN")
        ));
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
    fn scenes_are_emitted_under_their_parent_quests() {
        let f = write_temp_plugin(&quest_scene_fixture());
        let (h_old, stats_old) = run_one(false, f.path(), None);
        let (h_new, stats_new) = run_one(true, f.path(), None);

        assert_eq!(stats_old.records_translated, 2);
        assert_eq!(stats_new.records_translated, 2);
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
