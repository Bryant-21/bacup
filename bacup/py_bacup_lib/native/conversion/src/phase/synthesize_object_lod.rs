//! `synthesize_object_lod` phase — give converted FO4 base records a Distant-LOD
//! (`MNAM`) so the native lodgen produces object LOD (`.bto`) for them.
//!
//! FO76 often ships LOD meshes by folder
//! convention while FO4 expects base-record `MNAM` paths. Converted FO4
//! STAT records otherwise carry no `MNAM`; lodgen drops any ref whose base
//! has no DistantLOD model, so object LOD is skipped entirely. This phase reads
//! each LOD-capable base's `MODL`, preserves source `MNAM`, derives FO76
//! `_lod[_N].nif` candidates, and generates per-level proxy meshes for
//! source-visible-distant / already-distant-flagged bases that have no real LOD.
//! FO4 only proves this 1040-byte layout for STAT. Other source base types are
//! asset-discovered but never receive a direct MNAM; a future implementation
//! must synthesize a real STAT proxy and repoint placed references instead.
//!
//! Runs in the record track AFTER translate+fixups (records carry `MODL`) and
//! BEFORE `build_esp` (so the `MNAM` serializes into the output ESP).

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::mem;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bytes::Bytes;
use indexmap::IndexMap;
use nif_core_native::model::{NifBlock, NifFile, NifValue};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value as JsonValue;
use smol_str::SmolStr;

use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_store_ref,
};

use crate::phase::lod_assets::{
    FO4_DIRECT_MNAM_BASE_SIGS, LOD_BASE_SIGS, lod_abs_path, should_skip_object_lod_model,
};
use crate::phase::lod_paths::derive_lod_candidates;
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::translator::Game;

/// FO4 record flag bit 15 — "Has Distant LOD".
const FLAG_HAS_DISTANT_LOD: u32 = 0x0000_8000;
const FLAG_DELETED: u32 = 0x0000_0020;

const XALG_NEVER_VISIBLE_DISTANT: u64 = 0x0000_0008;
const XALG_VISIBLE_DISTANT: u64 = 0x0000_0200;
const MULTIREF_LOD_KEYWORD_OBJECT_ID: u32 = 0x0019_5411;

/// FO4 MNAM "Distant LOD" layout: 4 fixed CP-1252 slots of 260 bytes each.
const MNAM_SLOT: usize = 260;
const FO76_GENERATED_PROXY_PREFIX: &str = "LOD\\Generated\\FO76";
const SKYRIM_GENERATED_PROXY_PREFIX: &str = "LOD\\Generated\\SkyrimSE";
const PROXY_RATIOS: [f32; 4] = [0.35, 0.18, 0.08, 0.035];
const PROXY_MAX_TRIANGLES_PER_SHAPE: [usize; 4] = [2_000, 800, 192, 48];
const PROXY_TARGET_ERRORS: [f32; 4] = [0.02, 0.06, 0.14, 0.28];
const STRUCTURE_PROXY_RATIOS: [f32; 4] = [0.35, 0.18, 0.70, 0.35];
const STRUCTURE_PROXY_MAX_TRIANGLES_PER_SHAPE: [usize; 4] = [2_000, 800, usize::MAX, usize::MAX];
const STRUCTURE_PROXY_TARGET_ERRORS: [f32; 4] = [0.02, 0.06, 0.02, 0.02];
const STRUCTURE_PROXY_LOCK_BORDER: [bool; 4] = [true, true, true, true];
const STRUCTURE_PROXY_ALLOW_SLOPPY: [bool; 4] = [false, false, false, false];
pub(crate) const OBJECT_LOD_OVERLAY_RELATIVE_PATH: &str = ".modkit/object_lod_overlay.v1.json";
const OBJECT_LOD_OVERLAY_VERSION: u32 = 1;

pub struct SynthesizeObjectLodPhase;

impl Phase for SynthesizeObjectLodPhase {
    fn name(&self) -> &'static str {
        "synthesize_object_lod"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let target_handle_id = ctx.run.target_handle_id;
        let source_dir = ctx.source_extracted_dir.to_path_buf();
        let mod_path = ctx.mod_path.to_path_buf();
        let conversion_workers =
            parse_conversion_workers(ctx.params, ctx.run.config.conversion_workers);
        let source_game = ctx.run.source;
        let overlay_path = mod_path.join(OBJECT_LOD_OVERLAY_RELATIVE_PATH);

        remove_stale_object_lod_overlay(&overlay_path)?;

        ctx.check_cancel()?;

        let event_tx = ctx.run.event_tx.clone();
        let log_objlod = |message: String| {
            eprintln!("{message}");
            let _ = event_tx.try_send(crate::phase::PhaseEvent::Log {
                phase: "synthesize_object_lod",
                level: crate::phase::LogLevel::Info,
                message,
            });
        };

        let mut changed: u32 = 0;
        let mut pending = Vec::new();
        let mut virtual_bases = Vec::new();
        let source_hints_for_overlay;
        {
            let mut store = plugin_handle_store_ref()
                .lock()
                .map_err(|e| PhaseError::Internal(format!("plugin handle store poisoned: {e}")))?;
            let started = std::time::Instant::now();
            let source_hints = store
                .get(&ctx.run.source_handle_id)
                .map(|slot| collect_source_lod_hints(&slot.parsed.root_items, &source_dir))
                .unwrap_or_default();
            log_objlod(format!(
                "[objlod_timing] collect_source_lod_hints elapsed_ms={}",
                started.elapsed().as_millis()
            ));
            source_hints_for_overlay = source_hints.clone();
            let slot = store.get_mut(&target_handle_id).ok_or_else(|| {
                PhaseError::Internal(format!("unknown target handle {target_handle_id}"))
            })?;

            let started = std::time::Instant::now();
            let visible_scol_components =
                collect_visible_scol_component_ids(&slot.parsed.root_items, &source_hints);
            log_objlod(format!(
                "[objlod_timing] collect_visible_scol_components elapsed_ms={}",
                started.elapsed().as_millis()
            ));
            let started = std::time::Instant::now();
            synthesize_in_items(
                &mut slot.parsed.root_items,
                &mod_path,
                &source_dir,
                &source_hints,
                &visible_scol_components,
                source_game,
                &mut changed,
                &mut pending,
            );
            log_objlod(format!(
                "[objlod_timing] synthesize_in_items changed={changed} elapsed_ms={}",
                started.elapsed().as_millis()
            ));
            let started = std::time::Instant::now();
            collect_virtual_lod_bases(
                &slot.parsed.root_items,
                &mod_path,
                &source_dir,
                &source_hints,
                &visible_scol_components,
                source_game,
                &mut virtual_bases,
                &mut pending,
            );
            log_objlod(format!(
                "[objlod_timing] collect_virtual_lod_bases virtual_bases={} elapsed_ms={}",
                virtual_bases.len(),
                started.elapsed().as_millis()
            ));
            if changed > 0 {
                slot.clear_record_count_cache();
                slot.invalidate_sections();
            }
        }

        ctx.check_cancel()?;

        let unique_source_models = pending
            .iter()
            .map(|candidate| candidate.source_mnam.to_ascii_lowercase())
            .collect::<HashSet<_>>()
            .len();
        let started = std::time::Instant::now();
        let generated =
            generate_proxy_candidates(&pending, &mod_path, &source_dir, conversion_workers)?;
        let mut proxy_timing = ProxyGenerationTiming::default();
        for result in &generated {
            proxy_timing.merge(result.timing);
        }
        log_objlod(format!(
            "[objlod_timing] generate_proxy_candidates pending={} elapsed_ms={}",
            pending.len(),
            started.elapsed().as_millis()
        ));
        log_objlod(format!(
            "[objlod_timing] generate_proxy_candidate_workers unique_source_models={} source_resolution_ms={} nif_probe_ms={} emission_ms={} source_path_probes={} nif_loads={} outputs_attempted={} directory_preparations={}",
            unique_source_models,
            proxy_timing.source_resolution.as_millis(),
            proxy_timing.nif_probe.as_millis(),
            proxy_timing.emission.as_millis(),
            proxy_timing.source_path_probes,
            proxy_timing.nif_loads,
            proxy_timing.outputs_attempted,
            proxy_timing.directory_preparations,
        ));
        let assets_written: u32 = generated
            .iter()
            .map(|r| r.assets_written)
            .sum::<usize>()
            .try_into()
            .unwrap_or(u32::MAX);
        let warning_messages: Vec<String> = generated
            .iter()
            .flat_map(|r| r.warnings.iter().cloned())
            .collect();
        let warnings: u32 = warning_messages.len().try_into().unwrap_or(u32::MAX);
        for warning in &warning_messages {
            let sym = ctx
                .run
                .interner
                .intern(&format!("synthesize_object_lod:{warning}"));
            ctx.run.warnings.push(sym);
        }
        let successful: Vec<GeneratedProxyMnam> =
            generated.into_iter().filter_map(|r| r.generated).collect();
        virtual_bases.extend(
            successful
                .iter()
                .filter(|generated| {
                    !FO4_DIRECT_MNAM_BASE_SIGS.contains(&generated.signature.as_str())
                })
                .cloned(),
        );

        if !successful.is_empty() || !virtual_bases.is_empty() {
            let started = std::time::Instant::now();
            let mut store = plugin_handle_store_ref()
                .lock()
                .map_err(|e| PhaseError::Internal(format!("plugin handle store poisoned: {e}")))?;
            let slot = store.get_mut(&target_handle_id).ok_or_else(|| {
                PhaseError::Internal(format!("unknown target handle {target_handle_id}"))
            })?;
            let proxy_changed =
                apply_generated_proxy_mnams(&mut slot.parsed.root_items, &successful);
            if proxy_changed > 0 {
                changed = changed.saturating_add(proxy_changed);
                slot.clear_record_count_cache();
                slot.invalidate_sections();
            }
            if !virtual_bases.is_empty() {
                let overlay = build_object_lod_overlay(
                    &slot.parsed.root_items,
                    &virtual_bases,
                    &source_hints_for_overlay,
                    &ctx.run.config.output_plugin_name,
                )?;
                if !overlay.entries.is_empty() {
                    write_object_lod_overlay(&overlay_path, &overlay)?;
                }
            }
            log_objlod(format!(
                "[objlod_timing] apply_mnams_and_overlay proxy_changed={proxy_changed} elapsed_ms={}",
                started.elapsed().as_millis()
            ));
        }

        Ok(PhaseReport {
            records_changed: changed,
            assets_written,
            warnings,
            ..Default::default()
        })
    }
}

type SourceMnamMap = HashMap<(String, u32), [Option<String>; 4]>;
type SourceObjectIdSet = HashSet<u32>;

#[derive(Clone, Default)]
struct SourceLodHints {
    mnams: SourceMnamMap,
    base_visible_distant: SourceObjectIdSet,
    ref_visible_distant: SourceObjectIdSet,
    ref_visible_reference_ids: SourceObjectIdSet,
    never_visible_distant: SourceObjectIdSet,
    has_source: bool,
}

#[derive(Clone, Debug)]
struct ProxyCandidate {
    signature: String,
    form_id: u32,
    source_mnam: String,
    slots: [Option<String>; 4],
    outputs: Vec<ProxyOutput>,
    set_base_flag: bool,
}

#[derive(Clone, Debug)]
struct ProxyOutput {
    level: usize,
    mnam: String,
}

#[derive(Debug)]
struct ProxyGenerationResult {
    generated: Option<GeneratedProxyMnam>,
    assets_written: usize,
    warnings: Vec<String>,
    timing: ProxyGenerationTiming,
}

#[derive(Clone, Copy, Debug, Default)]
struct ProxyGenerationTiming {
    source_resolution: Duration,
    nif_probe: Duration,
    emission: Duration,
    source_path_probes: usize,
    nif_loads: usize,
    outputs_attempted: usize,
    directory_preparations: usize,
}

impl ProxyGenerationTiming {
    fn merge(&mut self, other: Self) {
        self.source_resolution += other.source_resolution;
        self.nif_probe += other.nif_probe;
        self.emission += other.emission;
        self.source_path_probes += other.source_path_probes;
        self.nif_loads += other.nif_loads;
        self.outputs_attempted += other.outputs_attempted;
        self.directory_preparations += other.directory_preparations;
    }
}

#[derive(Clone, Copy)]
struct ProxyDecimationProfile {
    ratios: [f32; 4],
    max_triangles_per_shape: [usize; 4],
    target_errors: [f32; 4],
    lock_border: [bool; 4],
    allow_sloppy: [bool; 4],
}

const DEFAULT_PROXY_DECIMATION_PROFILE: ProxyDecimationProfile = ProxyDecimationProfile {
    ratios: PROXY_RATIOS,
    max_triangles_per_shape: PROXY_MAX_TRIANGLES_PER_SHAPE,
    target_errors: PROXY_TARGET_ERRORS,
    lock_border: [false, false, false, false],
    allow_sloppy: [false, false, true, true],
};

const STRUCTURE_PROXY_DECIMATION_PROFILE: ProxyDecimationProfile = ProxyDecimationProfile {
    ratios: STRUCTURE_PROXY_RATIOS,
    max_triangles_per_shape: STRUCTURE_PROXY_MAX_TRIANGLES_PER_SHAPE,
    target_errors: STRUCTURE_PROXY_TARGET_ERRORS,
    lock_border: STRUCTURE_PROXY_LOCK_BORDER,
    allow_sloppy: STRUCTURE_PROXY_ALLOW_SLOPPY,
};

#[derive(Clone, Debug)]
struct GeneratedProxyMnam {
    signature: String,
    form_id: u32,
    slots: [Option<String>; 4],
    set_base_flag: bool,
}

#[derive(Debug, Serialize)]
struct ObjectLodOverlayDocument {
    schema_version: u32,
    plugin_name: String,
    plugin_size: u64,
    plugin_mtime_ns: u64,
    entries: Vec<ObjectLodOverlayEntry>,
}

#[derive(Clone, Debug, Serialize)]
struct ObjectLodOverlayEntry {
    reference_form_id: u32,
    placed_base_form_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    component_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    component_base_form_id: Option<u32>,
    base_signature: String,
    lod_models: [Option<String>; 4],
    force_visible: bool,
}

#[derive(Clone, Copy)]
struct OverlayPlacedRef<'a> {
    record: &'a ParsedRecord,
    in_visible_distant_group: bool,
}

fn synthesize_in_items(
    items: &mut [ParsedItem],
    mod_path: &Path,
    source_dir: &Path,
    source_hints: &SourceLodHints,
    visible_scol_components: &HashSet<u32>,
    source_game: Game,
    changed: &mut u32,
    pending: &mut Vec<ProxyCandidate>,
) {
    for item in items {
        match item {
            ParsedItem::Record(record) => {
                if synthesize_record_with_visible_components(
                    record,
                    mod_path,
                    source_dir,
                    source_hints,
                    visible_scol_components,
                    source_game,
                    pending,
                ) {
                    *changed += 1;
                }
            }
            ParsedItem::Group(group) => {
                synthesize_in_items(
                    &mut group.children,
                    mod_path,
                    source_dir,
                    source_hints,
                    visible_scol_components,
                    source_game,
                    changed,
                    pending,
                );
            }
        }
    }
}

fn synthesize_record(
    record: &mut ParsedRecord,
    mod_path: &Path,
    source_dir: &Path,
    source_hints: &SourceLodHints,
    source_game: Game,
    pending: &mut Vec<ProxyCandidate>,
) -> bool {
    synthesize_record_with_visible_components(
        record,
        mod_path,
        source_dir,
        source_hints,
        &HashSet::new(),
        source_game,
        pending,
    )
}

fn synthesize_record_with_visible_components(
    record: &mut ParsedRecord,
    mod_path: &Path,
    source_dir: &Path,
    source_hints: &SourceLodHints,
    visible_scol_components: &HashSet<u32>,
    source_game: Game,
    pending: &mut Vec<ProxyCandidate>,
) -> bool {
    if !LOD_BASE_SIGS.contains(&record.signature.as_str()) {
        return false;
    }
    if record.flags & FLAG_DELETED != 0 {
        return suppress_target_distant_lod(record);
    }
    if !FO4_DIRECT_MNAM_BASE_SIGS.contains(&record.signature.as_str()) {
        return suppress_target_distant_lod(record);
    }
    let record_object_id = local_object_id(record.form_id);
    if source_hints
        .never_visible_distant
        .contains(&record_object_id)
    {
        return suppress_target_distant_lod(record);
    }
    let modl = record_modl(record);
    if modl.as_deref().is_some_and(should_skip_object_lod_model) {
        return suppress_target_distant_lod(record);
    }
    let base_visible = source_hints
        .base_visible_distant
        .contains(&record_object_id);
    let ref_visible = source_hints.ref_visible_distant.contains(&record_object_id);
    let is_tree = record.signature.as_str() == "TREE";
    let is_scol = record.signature.as_str() == "SCOL";
    let tree_uses_tree_policy = is_tree;
    let can_emit_lod = !source_hints.has_source
        || tree_uses_tree_policy
        || base_visible
        || ref_visible
        || visible_scol_components.contains(&record_object_id);
    let can_set_base_flag = !source_hints.has_source || tree_uses_tree_policy || base_visible;

    if record_has_non_empty_mnam(record) {
        if record_has_generated_proxy_mnam(record) {
            if is_scol {
                return suppress_target_distant_lod(record);
            }
            if !can_emit_lod {
                return suppress_target_distant_lod(record);
            }
            let mut changed = false;
            if !can_set_base_flag && record.flags & FLAG_HAS_DISTANT_LOD != 0 {
                record.flags &= !FLAG_HAS_DISTANT_LOD;
                record.raw_payload = None;
                changed = true;
            }
            if let Some(candidate) = missing_generated_proxy_candidate(
                record,
                mod_path,
                can_set_base_flag,
                generated_proxy_prefix_for_source(source_game),
            ) {
                pending.push(candidate);
            }
            return changed;
        }
        if let Some(candidate) = missing_generated_proxy_candidate(
            record,
            mod_path,
            can_set_base_flag,
            generated_proxy_prefix_for_source(source_game),
        ) {
            pending.push(candidate);
        }
        return false;
    }
    let key = (record.signature.to_string(), record_object_id);
    if let Some(slots) = source_hints.mnams.get(&key) {
        write_mnam(record, slots);
        record.flags |= FLAG_HAS_DISTANT_LOD;
        record.raw_payload = None;
        return true;
    }
    let Some(modl) = modl else {
        return false;
    };

    let Some(slots) = resolve_lod_slots(&modl, source_dir, source_game) else {
        if !can_emit_lod {
            return false;
        }
        if is_scol {
            return false;
        }
        if let Some(candidate) = generated_proxy_candidate_with_prefix(
            record,
            &modl,
            can_set_base_flag,
            generated_proxy_prefix_for_source(source_game),
        ) {
            pending.push(candidate);
        }
        return false;
    };

    if !can_emit_lod {
        return false;
    }
    write_mnam(record, &slots);
    if can_set_base_flag {
        record.flags |= FLAG_HAS_DISTANT_LOD;
    } else {
        record.flags &= !FLAG_HAS_DISTANT_LOD;
    }
    record.raw_payload = None;
    true
}

fn collect_virtual_lod_bases(
    items: &[ParsedItem],
    mod_path: &Path,
    source_dir: &Path,
    source_hints: &SourceLodHints,
    visible_scol_components: &HashSet<u32>,
    source_game: Game,
    ready: &mut Vec<GeneratedProxyMnam>,
    pending: &mut Vec<ProxyCandidate>,
) {
    for item in items {
        match item {
            ParsedItem::Record(record)
                if LOD_BASE_SIGS.contains(&record.signature.as_str())
                    && !FO4_DIRECT_MNAM_BASE_SIGS.contains(&record.signature.as_str()) =>
            {
                collect_virtual_lod_base(
                    record,
                    mod_path,
                    source_dir,
                    source_hints,
                    visible_scol_components,
                    source_game,
                    ready,
                    pending,
                );
            }
            ParsedItem::Group(group) => collect_virtual_lod_bases(
                &group.children,
                mod_path,
                source_dir,
                source_hints,
                visible_scol_components,
                source_game,
                ready,
                pending,
            ),
            _ => {}
        }
    }
}

fn collect_virtual_lod_base(
    record: &ParsedRecord,
    _mod_path: &Path,
    source_dir: &Path,
    source_hints: &SourceLodHints,
    visible_scol_components: &HashSet<u32>,
    source_game: Game,
    ready: &mut Vec<GeneratedProxyMnam>,
    pending: &mut Vec<ProxyCandidate>,
) {
    if record.flags & FLAG_DELETED != 0 {
        return;
    }
    let object_id = local_object_id(record.form_id);
    if source_hints.never_visible_distant.contains(&object_id)
        || record_modl(record)
            .as_deref()
            .is_some_and(should_skip_object_lod_model)
    {
        return;
    }
    let base_visible = source_hints.base_visible_distant.contains(&object_id);
    let ref_visible = source_hints.ref_visible_distant.contains(&object_id);
    let is_tree = record.signature.as_str() == "TREE";
    let is_scol = record.signature.as_str() == "SCOL";
    let tree_uses_tree_policy = is_tree;
    let can_emit_lod = !source_hints.has_source
        || tree_uses_tree_policy
        || base_visible
        || ref_visible
        || visible_scol_components.contains(&object_id);
    if !can_emit_lod {
        return;
    }
    let force_base_visible = !source_hints.has_source || tree_uses_tree_policy || base_visible;
    let key = (record.signature.to_string(), object_id);
    if let Some(slots) = source_hints.mnams.get(&key) {
        ready.push(GeneratedProxyMnam {
            signature: record.signature.to_string(),
            form_id: object_id,
            slots: slots.clone(),
            set_base_flag: force_base_visible,
        });
        return;
    }
    let Some(modl) = record_modl(record) else {
        return;
    };
    if let Some(slots) = resolve_lod_slots(&modl, source_dir, source_game) {
        ready.push(GeneratedProxyMnam {
            signature: record.signature.to_string(),
            form_id: object_id,
            slots,
            set_base_flag: force_base_visible,
        });
        return;
    }
    if !is_scol {
        if let Some(candidate) = generated_proxy_candidate_with_prefix(
            record,
            &modl,
            force_base_visible,
            generated_proxy_prefix_for_source(source_game),
        ) {
            pending.push(candidate);
        }
    }
}

fn build_object_lod_overlay(
    items: &[ParsedItem],
    virtual_bases: &[GeneratedProxyMnam],
    source_hints: &SourceLodHints,
    plugin_name: &str,
) -> Result<ObjectLodOverlayDocument, PhaseError> {
    let virtual_by_key: HashMap<(String, u32), &GeneratedProxyMnam> = virtual_bases
        .iter()
        .map(|base| ((base.signature.clone(), base.form_id), base))
        .collect();
    let mut records_by_form_id = HashMap::new();
    collect_lod_base_records(items, &mut records_by_form_id);
    let mut placed_refs = Vec::new();
    collect_overlay_placed_refs(items, false, &mut placed_refs);
    let mut entries = Vec::new();
    for placed in placed_refs {
        let Some(placed_base_form_id) = record_formid_subrecord(placed.record, "NAME") else {
            continue;
        };
        let Some(placed_base) = records_by_form_id.get(&placed_base_form_id).copied() else {
            continue;
        };
        let force_ref_visible = placed.in_visible_distant_group
            || source_ref_requests_object_lod(placed.record)
            || source_hints
                .ref_visible_reference_ids
                .contains(&local_object_id(placed.record.form_id));
        if placed_base.signature.as_str() == "SCOL" {
            if let Some(virtual_base) =
                virtual_by_key.get(&("SCOL".to_string(), local_object_id(placed_base.form_id)))
            {
                entries.push(object_lod_overlay_entry(
                    placed.record.form_id,
                    placed_base.form_id,
                    None,
                    None,
                    virtual_base,
                    force_ref_visible,
                )?);
                continue;
            }
            for (component_index, component_base_form_id) in
                scol_component_base_form_ids(placed_base)
                    .into_iter()
                    .enumerate()
            {
                let Some(component_base) = records_by_form_id.get(&component_base_form_id).copied()
                else {
                    continue;
                };
                let Some(virtual_base) = virtual_by_key.get(&(
                    component_base.signature.to_string(),
                    local_object_id(component_base.form_id),
                )) else {
                    continue;
                };
                entries.push(object_lod_overlay_entry(
                    placed.record.form_id,
                    placed_base.form_id,
                    Some(component_index as u32),
                    Some(component_base.form_id),
                    virtual_base,
                    force_ref_visible,
                )?);
            }
            continue;
        }
        let Some(virtual_base) = virtual_by_key.get(&(
            placed_base.signature.to_string(),
            local_object_id(placed_base.form_id),
        )) else {
            continue;
        };
        entries.push(object_lod_overlay_entry(
            placed.record.form_id,
            placed_base.form_id,
            None,
            None,
            virtual_base,
            force_ref_visible,
        )?);
    }
    entries.sort_by_key(|entry| {
        (
            entry.reference_form_id,
            entry.placed_base_form_id,
            entry.component_index,
            entry.component_base_form_id,
        )
    });
    entries.dedup_by(|left, right| {
        left.reference_form_id == right.reference_form_id
            && left.placed_base_form_id == right.placed_base_form_id
            && left.component_index == right.component_index
            && left.component_base_form_id == right.component_base_form_id
    });
    Ok(ObjectLodOverlayDocument {
        schema_version: OBJECT_LOD_OVERLAY_VERSION,
        plugin_name: plugin_name.to_string(),
        plugin_size: 0,
        plugin_mtime_ns: 0,
        entries,
    })
}

fn object_lod_overlay_entry(
    reference_form_id: u32,
    placed_base_form_id: u32,
    component_index: Option<u32>,
    component_base_form_id: Option<u32>,
    virtual_base: &GeneratedProxyMnam,
    force_ref_visible: bool,
) -> Result<ObjectLodOverlayEntry, PhaseError> {
    let mut lod_models = [None, None, None, None];
    for (target, source) in lod_models.iter_mut().zip(&virtual_base.slots) {
        if let Some(path) = source {
            *target = Some(normalize_overlay_model_path(path)?);
        }
    }
    if lod_models.iter().all(Option::is_none) {
        return Err(PhaseError::Internal(format!(
            "object LOD overlay {:08X} has no model slots",
            reference_form_id
        )));
    }
    Ok(ObjectLodOverlayEntry {
        reference_form_id,
        placed_base_form_id,
        component_index,
        component_base_form_id,
        base_signature: virtual_base.signature.clone(),
        lod_models,
        force_visible: virtual_base.set_base_flag || force_ref_visible,
    })
}

fn normalize_overlay_model_path(path: &str) -> Result<String, PhaseError> {
    let normalized = path.trim().replace('/', "\\");
    let portable = normalized.replace('\\', "/");
    let candidate = Path::new(&portable);
    if normalized.is_empty()
        || normalized.contains('\0')
        || candidate.is_absolute()
        || candidate.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
        || !matches!(
            candidate.extension().and_then(|ext| ext.to_str()),
            Some(ext) if ext.eq_ignore_ascii_case("nif") || ext.eq_ignore_ascii_case("dds")
        )
    {
        return Err(PhaseError::Internal(format!(
            "unsafe object LOD overlay model path {path:?}"
        )));
    }
    Ok(normalized)
}

fn collect_lod_base_records<'a>(items: &'a [ParsedItem], out: &mut HashMap<u32, &'a ParsedRecord>) {
    for item in items {
        match item {
            ParsedItem::Record(record)
                if LOD_BASE_SIGS.contains(&record.signature.as_str())
                    && record.flags & FLAG_DELETED == 0 =>
            {
                out.insert(record.form_id, record);
            }
            ParsedItem::Group(group) => collect_lod_base_records(&group.children, out),
            _ => {}
        }
    }
}

fn collect_visible_scol_component_ids(
    items: &[ParsedItem],
    source_hints: &SourceLodHints,
) -> HashSet<u32> {
    let mut records_by_form_id = HashMap::new();
    collect_lod_base_records(items, &mut records_by_form_id);
    let mut placed_refs = Vec::new();
    collect_overlay_placed_refs(items, false, &mut placed_refs);
    let mut components = HashSet::new();
    for placed in placed_refs {
        let Some(base_form_id) = record_formid_subrecord(placed.record, "NAME") else {
            continue;
        };
        let Some(base) = records_by_form_id.get(&base_form_id).copied() else {
            continue;
        };
        if base.signature.as_str() != "SCOL"
            || !(placed.in_visible_distant_group
                || source_ref_requests_object_lod(placed.record)
                || source_hints
                    .ref_visible_reference_ids
                    .contains(&local_object_id(placed.record.form_id))
                || source_hints
                    .ref_visible_distant
                    .contains(&local_object_id(base.form_id)))
        {
            continue;
        }
        for component_form_id in scol_component_base_form_ids(base) {
            if records_by_form_id.contains_key(&component_form_id) {
                components.insert(local_object_id(component_form_id));
            }
        }
    }
    components
}

fn collect_overlay_placed_refs<'a>(
    items: &'a [ParsedItem],
    in_visible_distant_group: bool,
    out: &mut Vec<OverlayPlacedRef<'a>>,
) {
    for item in items {
        match item {
            ParsedItem::Record(record)
                if record.signature.as_str() == "REFR" && record.flags & FLAG_DELETED == 0 =>
            {
                out.push(OverlayPlacedRef {
                    record,
                    in_visible_distant_group,
                });
            }
            ParsedItem::Group(group) => collect_overlay_placed_refs(
                &group.children,
                in_visible_distant_group || group.group_type == 10,
                out,
            ),
            _ => {}
        }
    }
}

fn scol_component_base_form_ids(record: &ParsedRecord) -> Vec<u32> {
    let mut current_base = None;
    let mut components = Vec::new();
    for subrecord in &record.subrecords {
        match subrecord.signature.as_str() {
            "ONAM" => current_base = read_u32_at(&subrecord.data, 0),
            "DATA" => {
                if let Some(base) = current_base {
                    components.extend(std::iter::repeat_n(base, subrecord.data.len() / 28));
                }
            }
            _ => {}
        }
    }
    components
}

fn remove_stale_object_lod_overlay(path: &Path) -> Result<(), PhaseError> {
    if path.exists() {
        std::fs::remove_file(path).map_err(|error| {
            PhaseError::Internal(format!(
                "remove stale object LOD overlay {}: {error}",
                path.display()
            ))
        })?;
    }
    Ok(())
}

fn write_object_lod_overlay(
    path: &Path,
    overlay: &ObjectLodOverlayDocument,
) -> Result<(), PhaseError> {
    let parent = path.parent().ok_or_else(|| {
        PhaseError::Internal(format!(
            "object LOD overlay has no parent: {}",
            path.display()
        ))
    })?;
    std::fs::create_dir_all(parent).map_err(|error| {
        PhaseError::Internal(format!(
            "create object LOD overlay directory {}: {error}",
            parent.display()
        ))
    })?;
    let bytes = serde_json::to_vec_pretty(overlay)
        .map_err(|error| PhaseError::Internal(format!("serialize object LOD overlay: {error}")))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
        PhaseError::Internal(format!("create object LOD overlay temp file: {error}"))
    })?;
    temporary.write_all(&bytes).map_err(|error| {
        PhaseError::Internal(format!("write object LOD overlay temp file: {error}"))
    })?;
    temporary.flush().map_err(|error| {
        PhaseError::Internal(format!("flush object LOD overlay temp file: {error}"))
    })?;
    temporary.persist(path).map_err(|error| {
        PhaseError::Internal(format!(
            "persist object LOD overlay {}: {}",
            path.display(),
            error.error
        ))
    })?;
    Ok(())
}

fn record_has_non_empty_mnam(record: &ParsedRecord) -> bool {
    record
        .subrecords
        .iter()
        .any(|s| s.signature.as_str() == "MNAM" && s.data.iter().any(|&b| b != 0))
}

fn missing_generated_proxy_candidate(
    record: &ParsedRecord,
    mod_path: &Path,
    set_base_flag: bool,
    generated_proxy_prefix: &str,
) -> Option<ProxyCandidate> {
    let mnam = record
        .subrecords
        .iter()
        .find(|s| s.signature.as_str() == "MNAM")?;
    let slots = raw_mnam_slots(&mnam.data);
    let mut has_generated_proxy = false;
    let mut missing_generated_proxy = false;
    for path in slots.iter().flatten() {
        if !is_generated_proxy_mnam(path) {
            continue;
        }
        has_generated_proxy = true;
        if !mnam_abs_path_from_mod_root(mod_path, path).is_file() {
            missing_generated_proxy = true;
        }
    }
    if !has_generated_proxy || !missing_generated_proxy {
        return None;
    }
    let modl = record
        .subrecords
        .iter()
        .find(|s| s.signature.as_str() == "MODL")
        .map(|s| read_zstring(&s.data))
        .filter(|m| !m.is_empty())?;
    generated_proxy_candidate_with_prefix(record, &modl, set_base_flag, generated_proxy_prefix)
}

fn record_has_generated_proxy_mnam(record: &ParsedRecord) -> bool {
    record
        .subrecords
        .iter()
        .find(|s| s.signature.as_str() == "MNAM")
        .map(|mnam| {
            raw_mnam_slots(&mnam.data)
                .iter()
                .flatten()
                .any(|path| is_generated_proxy_mnam(path))
        })
        .unwrap_or(false)
}

fn raw_mnam_slots(data: &[u8]) -> [Option<String>; 4] {
    let mut slots: [Option<String>; 4] = [None, None, None, None];
    for (level, slot) in slots.iter_mut().enumerate() {
        let off = level * MNAM_SLOT;
        if off >= data.len() {
            break;
        }
        let end = (off + MNAM_SLOT).min(data.len());
        let path = normalize_mnam_slot_path(&data[off..end]);
        if !path.is_empty() {
            *slot = Some(path);
        }
    }
    slots
}

fn is_generated_proxy_mnam(path: &str) -> bool {
    let normalized = path.replace('/', "\\");
    [FO76_GENERATED_PROXY_PREFIX, SKYRIM_GENERATED_PROXY_PREFIX]
        .iter()
        .any(|prefix| {
            let prefix = format!("{prefix}\\");
            normalized
                .get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(&prefix))
        })
}

fn collect_source_lod_hints(items: &[ParsedItem], source_dir: &Path) -> SourceLodHints {
    let mut out = SourceLodHints::default();
    out.has_source = true;
    collect_source_lod_hints_in_items(items, source_dir, &mut out);
    out
}

fn collect_source_lod_hints_in_items(
    items: &[ParsedItem],
    source_dir: &Path,
    out: &mut SourceLodHints,
) {
    for item in items {
        match item {
            ParsedItem::Record(record)
                if LOD_BASE_SIGS.contains(&record.signature.as_str())
                    && record.flags & FLAG_DELETED == 0 =>
            {
                let object_id = local_object_id(record.form_id);
                if record_modl(record)
                    .as_deref()
                    .is_some_and(should_skip_object_lod_model)
                {
                    out.never_visible_distant.insert(object_id);
                    continue;
                }
                if source_record_is_never_visible_distant(record) {
                    out.never_visible_distant.insert(object_id);
                    continue;
                }
                if let Some(slots) = source_mnam_slots(record, source_dir) {
                    out.mnams
                        .insert((record.signature.to_string(), object_id), slots);
                }
                if source_record_is_visible_distant(record) {
                    out.base_visible_distant.insert(object_id);
                }
            }
            ParsedItem::Record(record)
                if record.signature.as_str() == "REFR" && record.flags & FLAG_DELETED == 0 =>
            {
                if source_ref_requests_object_lod(record) {
                    out.ref_visible_reference_ids
                        .insert(local_object_id(record.form_id));
                    if let Some(base_form_id) = record_formid_subrecord(record, "NAME") {
                        out.ref_visible_distant.insert(base_form_id);
                    }
                }
            }
            ParsedItem::Group(group) => {
                collect_source_lod_hints_in_items(&group.children, source_dir, out)
            }
            _ => {}
        }
    }
}

fn source_mnam_slots(record: &ParsedRecord, source_dir: &Path) -> Option<[Option<String>; 4]> {
    let mnam = record
        .subrecords
        .iter()
        .find(|s| s.signature.as_str() == "MNAM")?;
    slots_from_mnam_bytes(&mnam.data, source_dir)
}

fn record_modl(record: &ParsedRecord) -> Option<String> {
    record
        .subrecords
        .iter()
        .find(|s| s.signature.as_str() == "MODL")
        .map(|s| read_zstring(&s.data))
        .filter(|m| !m.is_empty())
}

fn source_record_is_visible_distant(record: &ParsedRecord) -> bool {
    let flags = source_xalg_flags(record);
    if flags & XALG_NEVER_VISIBLE_DISTANT != 0 {
        return false;
    }
    record.flags & FLAG_HAS_DISTANT_LOD != 0 || flags & XALG_VISIBLE_DISTANT != 0
}

fn source_ref_requests_object_lod(record: &ParsedRecord) -> bool {
    source_record_is_visible_distant(record)
        || (!source_record_is_never_visible_distant(record) && record_has_multiref_lod_link(record))
}

fn record_has_multiref_lod_link(record: &ParsedRecord) -> bool {
    record
        .subrecords
        .iter()
        .filter(|s| s.signature.as_str() == "XLKR")
        .any(|s| {
            read_u32_at(&s.data, 0)
                .map(local_object_id)
                .is_some_and(|object_id| object_id == MULTIREF_LOD_KEYWORD_OBJECT_ID)
        })
}

fn source_record_is_never_visible_distant(record: &ParsedRecord) -> bool {
    source_xalg_flags(record) & XALG_NEVER_VISIBLE_DISTANT != 0
}

fn source_xalg_flags(record: &ParsedRecord) -> u64 {
    record
        .subrecords
        .iter()
        .filter(|s| s.signature.as_str() == "XALG")
        .fold(0u64, |acc, s| acc | read_xalg_flags(&s.data))
}

fn suppress_target_distant_lod(record: &mut ParsedRecord) -> bool {
    let original_subrecord_count = record.subrecords.len();
    record.subrecords.retain(|s| s.signature.as_str() != "MNAM");
    let had_distant_lod_flag = record.flags & FLAG_HAS_DISTANT_LOD != 0;
    record.flags &= !FLAG_HAS_DISTANT_LOD;
    let changed = original_subrecord_count != record.subrecords.len() || had_distant_lod_flag;
    if changed {
        record.raw_payload = None;
    }
    changed
}

fn record_formid_subrecord(record: &ParsedRecord, sig: &str) -> Option<u32> {
    let data = &record
        .subrecords
        .iter()
        .find(|s| s.signature.as_str() == sig)?
        .data;
    if data.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]) & 0x00ff_ffff)
}

fn read_u32_at(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn local_object_id(form_id: u32) -> u32 {
    form_id & 0x00ff_ffff
}

fn read_xalg_flags(data: &[u8]) -> u64 {
    let mut bytes = [0u8; 8];
    let len = data.len().min(bytes.len());
    bytes[..len].copy_from_slice(&data[..len]);
    u64::from_le_bytes(bytes)
}

fn slots_from_mnam_bytes(data: &[u8], source_dir: &Path) -> Option<[Option<String>; 4]> {
    let mut slots: [Option<String>; 4] = [None, None, None, None];
    for (level, slot) in slots.iter_mut().enumerate() {
        let off = level * MNAM_SLOT;
        if off >= data.len() {
            break;
        }
        let end = (off + MNAM_SLOT).min(data.len());
        let path = normalize_mnam_slot_path(&data[off..end]);
        if path.is_empty() || !mnam_source_exists(source_dir, &path) {
            continue;
        }
        *slot = Some(path);
    }
    recover_misaligned_mnam_slots(data, source_dir, &mut slots);
    fill_far_lod_sibling_slots(source_dir, &mut slots);
    fill_missing_lod_slots(&mut slots);
    if slots.iter().any(Option::is_some) {
        Some(slots)
    } else {
        None
    }
}

fn normalize_mnam_slot_path(data: &[u8]) -> String {
    let mut path = read_zstring(data).trim().replace('/', "\\");
    if path.len() >= 7 && path[..7].eq_ignore_ascii_case("meshes\\") {
        path = path[7..].to_string();
    }
    path
}

fn mnam_abs_path(source_dir: &Path, mnam: &str) -> PathBuf {
    let mut p = source_dir.join("meshes");
    let lower = mnam.to_ascii_lowercase().replace('\\', "/");
    for component in lower.split('/') {
        if !component.is_empty() {
            p.push(component);
        }
    }
    p
}

fn mnam_source_exists(source_dir: &Path, mnam: &str) -> bool {
    mnam_abs_path(source_dir, mnam).is_file()
}

fn recover_misaligned_mnam_slots(data: &[u8], source_dir: &Path, slots: &mut [Option<String>; 4]) {
    for (offset, path) in scan_mnam_lod_paths(data, source_dir) {
        let level = ((offset + MNAM_SLOT / 2) / MNAM_SLOT).min(slots.len() - 1);
        if slots[level].is_none() {
            slots[level] = Some(path);
        }
    }
}

fn scan_mnam_lod_paths(data: &[u8], source_dir: &Path) -> Vec<(usize, String)> {
    let mut paths = Vec::new();
    for start in 0..data.len() {
        if !is_lod_path_start(data, start) {
            continue;
        }
        let Some(end) = find_nif_path_end(data, start) else {
            continue;
        };
        let path = String::from_utf8_lossy(&data[start..end])
            .trim()
            .replace('/', "\\");
        if mnam_source_exists(source_dir, &path) {
            paths.push((start, path));
        }
    }
    paths
}

fn is_lod_path_start(data: &[u8], start: usize) -> bool {
    [
        b"LOD\\".as_slice(),
        b"LOD/".as_slice(),
        b"DLC01\\LOD\\".as_slice(),
        b"DLC01/LOD/".as_slice(),
        b"DLC02\\LOD\\".as_slice(),
        b"DLC02/LOD/".as_slice(),
        b"DLC03\\LOD\\".as_slice(),
        b"DLC03/LOD/".as_slice(),
        b"DLC04\\LOD\\".as_slice(),
        b"DLC04/LOD/".as_slice(),
        b"BYOH\\LOD\\".as_slice(),
        b"BYOH/LOD/".as_slice(),
        b"_BYOH\\LOD\\".as_slice(),
        b"_BYOH/LOD/".as_slice(),
    ]
    .iter()
    .any(|prefix| starts_with_ascii_ci(data, start, prefix))
}

fn starts_with_ascii_ci(data: &[u8], start: usize, needle: &[u8]) -> bool {
    data.get(start..start + needle.len())
        .is_some_and(|hay| hay.eq_ignore_ascii_case(needle))
}

fn find_nif_path_end(data: &[u8], start: usize) -> Option<usize> {
    let mut idx = start;
    while idx + 4 <= data.len() {
        if data[idx..idx + 4].eq_ignore_ascii_case(b".nif") {
            return Some(idx + 4);
        }
        let b = data[idx];
        if !(b == b'\\'
            || b == b'/'
            || b == b'_'
            || b == b'-'
            || b == b'.'
            || b.is_ascii_alphanumeric())
        {
            return None;
        }
        idx += 1;
    }
    None
}

fn fill_far_lod_sibling_slots(source_dir: &Path, slots: &mut [Option<String>; 4]) {
    let existing: Vec<String> = slots.iter().flatten().cloned().collect();
    for path in existing {
        let Some(source_level) = lod_suffix_level(&path) else {
            continue;
        };
        for level in source_level + 1..slots.len() {
            if slots[level].is_some() {
                continue;
            }
            let Some(candidate) = replace_lod_suffix_level(&path, level) else {
                continue;
            };
            if mnam_source_exists(source_dir, &candidate) {
                slots[level] = Some(candidate);
            }
        }
    }
}

fn lod_suffix_level(path: &str) -> Option<usize> {
    let lower = path.to_ascii_lowercase();
    let marker = "_lod_";
    let marker_idx = lower.rfind(marker)?;
    let digit_idx = marker_idx + marker.len();
    let bytes = lower.as_bytes();
    let digit = *bytes.get(digit_idx)?;
    if !bytes.get(digit_idx + 1..)?.eq(b".nif") || !(b'0'..=b'3').contains(&digit) {
        return None;
    }
    Some((digit - b'0') as usize)
}

fn replace_lod_suffix_level(path: &str, level: usize) -> Option<String> {
    if level > 3 {
        return None;
    }
    let lower = path.to_ascii_lowercase();
    let marker_idx = lower.rfind("_lod_")?;
    let digit_idx = marker_idx + "_lod_".len();
    if !lower.as_bytes().get(digit_idx + 1..)?.eq(b".nif") {
        return None;
    }
    let mut out = String::with_capacity(path.len());
    out.push_str(&path[..digit_idx]);
    out.push(char::from(b'0' + level as u8));
    out.push_str(&path[digit_idx + 1..]);
    Some(out)
}

/// Resolve the 4 FO4 MNAM slot strings for a MODL by existence-checking the
/// source game's `_lod` convention under `source_dir`.
/// Multi-level (`_lod_0..3`) wins over single (`_lod`) when present.
fn resolve_lod_slots(
    modl: &str,
    source_dir: &Path,
    source_game: Game,
) -> Option<[Option<String>; 4]> {
    let candidates = derive_lod_candidates(source_game, modl);
    if candidates.is_empty() {
        return None;
    }
    let mut slots: [Option<String>; 4] = [None, None, None, None];
    let mut any = false;
    for c in candidates.iter().filter(|c| c.multi) {
        if c.level < 4 && lod_source_exists(source_dir, &c.source_rel) {
            slots[c.level] = Some(c.mnam.clone());
            any = true;
        }
    }
    if any {
        fill_missing_lod_slots(&mut slots);
    }
    if !any {
        if let Some(c) = candidates.iter().find(|c| !c.multi) {
            if lod_source_exists(source_dir, &c.source_rel) {
                for slot in &mut slots {
                    *slot = Some(c.mnam.clone());
                }
                any = true;
            }
        }
    }
    if any { Some(slots) } else { None }
}

fn fill_missing_lod_slots(slots: &mut [Option<String>; 4]) {
    for level in 0..slots.len() {
        if slots[level].is_some() {
            continue;
        }
        let replacement = (level + 1..slots.len())
            .find_map(|idx| slots[idx].clone())
            .or_else(|| (0..level).rev().find_map(|idx| slots[idx].clone()));
        slots[level] = replacement;
    }
}

fn generated_proxy_candidate(
    record: &ParsedRecord,
    modl: &str,
    set_base_flag: bool,
) -> Option<ProxyCandidate> {
    generated_proxy_candidate_with_prefix(record, modl, set_base_flag, FO76_GENERATED_PROXY_PREFIX)
}

fn generated_proxy_candidate_with_prefix(
    record: &ParsedRecord,
    modl: &str,
    set_base_flag: bool,
    generated_proxy_prefix: &str,
) -> Option<ProxyCandidate> {
    if record.signature.as_str() == "SCOL" {
        return None;
    }
    let source_mnam = normalize_full_model_mnam_path(modl)?;
    let record_object_id = local_object_id(record.form_id);
    let mut slots: [Option<String>; 4] = [None, None, None, None];
    let mut outputs = Vec::with_capacity(4);
    for level in 0..4 {
        let mnam = generated_proxy_mnam_with_prefix(
            generated_proxy_prefix,
            &record.signature,
            record_object_id,
            level,
        );
        slots[level] = Some(mnam.clone());
        outputs.push(ProxyOutput { level, mnam });
    }
    Some(ProxyCandidate {
        signature: record.signature.to_string(),
        form_id: record_object_id,
        source_mnam,
        slots,
        outputs,
        set_base_flag,
    })
}

fn generated_proxy_mnam_with_prefix(
    generated_proxy_prefix: &str,
    signature: &str,
    form_id: u32,
    level: usize,
) -> String {
    format!(
        "{generated_proxy_prefix}\\{}\\{:06X}_LOD_{level}.nif",
        signature,
        form_id & 0x00ff_ffff
    )
}

fn generated_proxy_prefix_for_source(source: Game) -> &'static str {
    match source {
        Game::SkyrimSe => SKYRIM_GENERATED_PROXY_PREFIX,
        _ => FO76_GENERATED_PROXY_PREFIX,
    }
}

fn normalize_full_model_mnam_path(modl: &str) -> Option<String> {
    let mut path = modl.trim().trim_end_matches('\0').trim().replace('/', "\\");
    while let Some(stripped) = path.strip_prefix('\\') {
        path = stripped.to_string();
    }
    if path.len() >= 7 && path[..7].eq_ignore_ascii_case("meshes\\") {
        path = path[7..].to_string();
    }
    if path.len() < 4 || !path[path.len() - 4..].eq_ignore_ascii_case(".nif") {
        return None;
    }
    Some(path)
}

fn mnam_abs_path_from_mod_root(mod_path: &Path, mnam: &str) -> PathBuf {
    let mut p = mod_path.join("data").join("Meshes");
    for component in mnam.replace('\\', "/").split('/') {
        if !component.is_empty() {
            p.push(component);
        }
    }
    p
}

fn apply_generated_proxy_mnams(items: &mut [ParsedItem], generated: &[GeneratedProxyMnam]) -> u32 {
    let by_key: HashMap<(String, u32), &GeneratedProxyMnam> = generated
        .iter()
        .map(|g| ((g.signature.clone(), g.form_id), g))
        .collect();
    apply_generated_proxy_mnams_in_items(items, &by_key)
}

fn apply_generated_proxy_mnams_in_items(
    items: &mut [ParsedItem],
    generated: &HashMap<(String, u32), &GeneratedProxyMnam>,
) -> u32 {
    let mut changed = 0u32;
    for item in items {
        match item {
            ParsedItem::Record(record) => {
                let key = (
                    record.signature.to_string(),
                    local_object_id(record.form_id),
                );
                if let Some(proxy) = generated.get(&key) {
                    if !FO4_DIRECT_MNAM_BASE_SIGS.contains(&record.signature.as_str()) {
                        continue;
                    }
                    if record
                        .subrecords
                        .iter()
                        .any(|s| s.signature.as_str() == "MNAM" && s.data.iter().any(|&b| b != 0))
                    {
                        continue;
                    }
                    write_mnam(record, &proxy.slots);
                    if proxy.set_base_flag {
                        record.flags |= FLAG_HAS_DISTANT_LOD;
                    } else {
                        record.flags &= !FLAG_HAS_DISTANT_LOD;
                    }
                    record.raw_payload = None;
                    changed = changed.saturating_add(1);
                }
            }
            ParsedItem::Group(group) => {
                changed = changed.saturating_add(apply_generated_proxy_mnams_in_items(
                    &mut group.children,
                    generated,
                ));
            }
        }
    }
    changed
}

fn generate_proxy_candidates(
    candidates: &[ProxyCandidate],
    mod_path: &Path,
    source_dir: &Path,
    conversion_workers: Option<usize>,
) -> Result<Vec<ProxyGenerationResult>, PhaseError> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    if let Some(workers) = conversion_workers {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .map_err(|e| PhaseError::Internal(format!("synthesize_object_lod workers: {e}")))?;
        Ok(pool.install(|| {
            candidates
                .par_iter()
                .map(|candidate| generate_proxy_candidate_assets(candidate, mod_path, source_dir))
                .collect()
        }))
    } else {
        Ok(candidates
            .par_iter()
            .map(|candidate| generate_proxy_candidate_assets(candidate, mod_path, source_dir))
            .collect())
    }
}

fn generate_proxy_candidate_assets(
    candidate: &ProxyCandidate,
    mod_path: &Path,
    source_dir: &Path,
) -> ProxyGenerationResult {
    let mut warnings = Vec::new();
    let mut timing = ProxyGenerationTiming::default();
    let started = Instant::now();
    let output_source_path = mnam_abs_path_from_mod_root(mod_path, &candidate.source_mnam);
    let extracted_source_path = mnam_abs_path(source_dir, &candidate.source_mnam);
    timing.source_path_probes += 1;
    let source_path = if output_source_path.is_file() {
        Some(output_source_path)
    } else {
        timing.source_path_probes += 1;
        extracted_source_path
            .is_file()
            .then_some(extracted_source_path)
    };
    timing.source_resolution = started.elapsed();
    let Some(source_path) = source_path else {
        warnings.push(format!(
            "{:06X}: source proxy model missing {}",
            candidate.form_id & 0x00ff_ffff,
            candidate.source_mnam
        ));
        return ProxyGenerationResult {
            generated: None,
            assets_written: 0,
            warnings,
            timing,
        };
    };

    let started = Instant::now();
    timing.nif_loads = 1;
    let source_nif = match NifFile::load(source_path.clone()) {
        Ok(nif) => nif,
        Err(err) => {
            timing.nif_probe = started.elapsed();
            warnings.push(format!(
                "{:06X}: source proxy model unreadable {}: {err}",
                candidate.form_id & 0x00ff_ffff,
                source_path.display()
            ));
            return ProxyGenerationResult {
                generated: None,
                assets_written: 0,
                warnings,
                timing,
            };
        }
    };
    timing.nif_probe = started.elapsed();

    let started = Instant::now();
    let mut assets_written = 0usize;
    let mut prepared_parent = None;
    let profile = proxy_decimation_profile(&candidate.source_mnam);
    for output in &candidate.outputs {
        timing.outputs_attempted += 1;
        let mut nif = source_nif.clone();
        let stats = decimate_nif_inline_geometry(
            &mut nif,
            profile.ratios[output.level],
            profile.max_triangles_per_shape[output.level],
            profile.target_errors[output.level],
            profile.lock_border[output.level],
            profile.allow_sloppy[output.level],
        );
        if stats.inline_shapes == 0 {
            timing.emission = started.elapsed();
            warnings.push(format!(
                "{:06X}: no inline geometry for generated LOD proxy {}",
                candidate.form_id & 0x00ff_ffff,
                source_path.display()
            ));
            return ProxyGenerationResult {
                generated: None,
                assets_written,
                warnings,
                timing,
            };
        }
        let output_path = mnam_abs_path_from_mod_root(mod_path, &output.mnam);
        if let Some(parent) = output_path.parent() {
            if prepared_parent.as_deref() != Some(parent) {
                timing.directory_preparations += 1;
                if let Err(err) = std::fs::create_dir_all(parent) {
                    timing.emission = started.elapsed();
                    warnings.push(format!(
                        "{:06X}: create proxy dir failed {}: {err}",
                        candidate.form_id & 0x00ff_ffff,
                        parent.display()
                    ));
                    return ProxyGenerationResult {
                        generated: None,
                        assets_written,
                        warnings,
                        timing,
                    };
                }
                prepared_parent = Some(parent.to_path_buf());
            }
        }
        if let Err(err) = nif.save(Some(output_path.clone())) {
            timing.emission = started.elapsed();
            warnings.push(format!(
                "{:06X}: write generated proxy failed {}: {err}",
                candidate.form_id & 0x00ff_ffff,
                output_path.display()
            ));
            return ProxyGenerationResult {
                generated: None,
                assets_written,
                warnings,
                timing,
            };
        }
        assets_written += 1;
    }
    timing.emission = started.elapsed();

    ProxyGenerationResult {
        generated: Some(GeneratedProxyMnam {
            signature: candidate.signature.clone(),
            form_id: candidate.form_id,
            slots: candidate.slots.clone(),
            set_base_flag: candidate.set_base_flag,
        }),
        assets_written,
        warnings,
        timing,
    }
}

fn proxy_decimation_profile(source_mnam: &str) -> ProxyDecimationProfile {
    if is_structure_proxy_model_path(source_mnam) {
        STRUCTURE_PROXY_DECIMATION_PROFILE
    } else {
        DEFAULT_PROXY_DECIMATION_PROFILE
    }
}

fn is_structure_proxy_model_path(source_mnam: &str) -> bool {
    let model = normalize_proxy_model_path(source_mnam);
    if model.starts_with("landscape/trees/")
        || model.starts_with("landscape/plants/")
        || model.starts_with("landscape/grass/")
        || model.starts_with("landscape/rocks/")
        || model.starts_with("flora/")
    {
        return false;
    }

    model.starts_with("architecture/")
        || model.starts_with("dungeons/")
        || model.starts_with("interiors/")
        || model.starts_with("workshop/")
        || model.contains("/architecture/")
}

fn normalize_proxy_model_path(source_mnam: &str) -> String {
    let mut model = source_mnam
        .trim()
        .trim_end_matches('\0')
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('\\', "/")
        .to_ascii_lowercase();
    if let Some(stripped) = model.strip_prefix("meshes/") {
        model = stripped.to_string();
    }
    if let Some(stripped) = model.strip_prefix("fo76/") {
        model = stripped.to_string();
    }
    model
}

#[derive(Default)]
struct ProxyDecimationStats {
    inline_shapes: usize,
    shapes_simplified: usize,
    triangles_before: usize,
    triangles_after: usize,
}

fn decimate_nif_inline_geometry(
    nif: &mut NifFile,
    ratio: f32,
    max_triangles_per_shape: usize,
    target_error: f32,
    lock_border: bool,
    allow_sloppy: bool,
) -> ProxyDecimationStats {
    let mut stats = ProxyDecimationStats::default();
    for block in &mut nif.blocks {
        if !is_inline_geometry_block(block) {
            continue;
        }
        let Some(geometry) = extract_inline_geometry(block) else {
            continue;
        };
        stats.inline_shapes += 1;
        stats.triangles_before += geometry.triangles.len();
        let before = geometry.triangles.len();
        let simplified = simplify_indices(
            &geometry,
            ratio,
            max_triangles_per_shape,
            target_error,
            lock_border,
            allow_sloppy,
        );
        if simplified.len() >= 3 && simplified.len() < before.saturating_mul(3) {
            if rewrite_inline_geometry(block, &simplified) {
                stats.shapes_simplified += 1;
            }
        }
        stats.triangles_after += count_inline_triangles(block);
    }
    stats
}

fn is_inline_geometry_block(block: &NifBlock) -> bool {
    matches!(
        block.type_name.as_str(),
        "BSTriShape" | "BSDynamicTriShape" | "BSSubIndexTriShape"
    ) && matches!(block.get_field("Vertex Data"), Some(NifValue::Array(a)) if !a.is_empty())
        && matches!(block.get_field("Triangles"), Some(NifValue::Array(a)) if !a.is_empty())
}

struct InlineGeometry {
    positions: Vec<[f32; 3]>,
    uvcoords: Vec<[f32; 2]>,
    normals: Vec<[f32; 3]>,
    vertex_colors: Vec<[f32; 4]>,
    triangles: Vec<[u32; 3]>,
}

fn extract_inline_geometry(block: &NifBlock) -> Option<InlineGeometry> {
    let vertex_data = match block.get_field("Vertex Data")? {
        NifValue::Array(items) => items,
        _ => return None,
    };
    let mut positions = Vec::with_capacity(vertex_data.len());
    let mut uvcoords = Vec::new();
    let mut normals = Vec::new();
    let mut vertex_colors = Vec::new();
    for value in vertex_data {
        let NifValue::Struct(fields) = value else {
            return None;
        };
        positions.push(vec3(fields.get("Vertex"))?);
        if let Some(uv) = uv2(fields.get("UV")) {
            uvcoords.push(uv);
        }
        if let Some(normal) = vec3(fields.get("Normal")) {
            normals.push(normal);
        }
        if let Some(color) = color4(fields.get("Vertex Colors"))
            .or_else(|| color4(fields.get("Vertex Color")))
            .or_else(|| color4(fields.get("Color")))
        {
            vertex_colors.push(color);
        }
    }
    if uvcoords.len() != positions.len() {
        uvcoords.clear();
    }
    if normals.len() != positions.len() {
        normals.clear();
    }
    if vertex_colors.len() != positions.len() {
        vertex_colors.clear();
    }
    let triangles = read_triangles(block.get_field("Triangles"));
    if positions.len() < 4 || triangles.is_empty() {
        return None;
    }
    Some(InlineGeometry {
        positions,
        uvcoords,
        normals,
        vertex_colors,
        triangles,
    })
}

fn simplify_indices(
    geometry: &InlineGeometry,
    ratio: f32,
    max_triangles_per_shape: usize,
    target_error: f32,
    lock_border: bool,
    allow_sloppy: bool,
) -> Vec<u32> {
    let indices = flatten_indices(&geometry.triangles);
    if ratio <= 0.0 || ratio >= 1.0 || indices.len() < 12 {
        return indices;
    }
    let target_triangles = (((geometry.triangles.len() as f32) * ratio).ceil() as usize)
        .min(max_triangles_per_shape)
        .max(4);
    let target_indices = target_triangles.saturating_mul(3);
    if target_indices >= indices.len() {
        return indices;
    }

    let adapter = meshopt::VertexDataAdapter::new(
        meshopt::typed_to_bytes(&geometry.positions),
        mem::size_of::<[f32; 3]>(),
        0,
    )
    .expect("inline proxy positions are tightly packed [f32; 3]");
    let locks = vec![false; geometry.positions.len()];
    let (attributes, weights, attribute_count) = vertex_attributes(geometry);
    let mut result_error = 0.0f32;
    let options = if lock_border {
        meshopt::SimplifyOptions::LockBorder
    } else {
        meshopt::SimplifyOptions::None
    };
    let simplified = if attribute_count > 0 {
        meshopt::simplify_with_attributes_and_locks(
            &indices,
            &adapter,
            &attributes,
            &weights,
            attribute_count * mem::size_of::<f32>(),
            &locks,
            target_indices,
            target_error,
            options,
            Some(&mut result_error),
        )
    } else {
        meshopt::simplify_with_locks(
            &indices,
            &adapter,
            &locks,
            target_indices,
            target_error,
            options,
            Some(&mut result_error),
        )
    };
    let mut out = if simplified.len() >= 3 && simplified.len() < indices.len() {
        simplified
    } else {
        indices
    };
    if allow_sloppy && out.len() > target_indices {
        let sloppy = meshopt::simplify_sloppy(
            &out,
            &adapter,
            target_indices.min(out.len()),
            target_error,
            Some(&mut result_error),
        );
        if sloppy.len() >= 3 && sloppy.len() < out.len() {
            out = sloppy;
        }
    }
    out
}

fn vertex_attributes(geometry: &InlineGeometry) -> (Vec<f32>, Vec<f32>, usize) {
    let vertex_count = geometry.positions.len();
    let has_uv = geometry.uvcoords.len() == vertex_count;
    let has_normals = geometry.normals.len() == vertex_count;
    let has_colors = geometry.vertex_colors.len() == vertex_count;
    let mut attribute_count = 0usize;
    if has_uv {
        attribute_count += 2;
    }
    if has_normals {
        attribute_count += 3;
    }
    if has_colors {
        attribute_count += 4;
    }
    if attribute_count == 0 {
        return (Vec::new(), Vec::new(), 0);
    }

    let mut attributes = Vec::with_capacity(vertex_count * attribute_count);
    for i in 0..vertex_count {
        if has_uv {
            attributes.extend_from_slice(&geometry.uvcoords[i]);
        }
        if has_normals {
            attributes.extend_from_slice(&geometry.normals[i]);
        }
        if has_colors {
            attributes.extend_from_slice(&geometry.vertex_colors[i]);
        }
    }

    let mut weights = Vec::with_capacity(attribute_count);
    if has_uv {
        weights.extend_from_slice(&[1.0, 1.0]);
    }
    if has_normals {
        weights.extend_from_slice(&[0.25, 0.25, 0.25]);
    }
    if has_colors {
        weights.extend_from_slice(&[0.10, 0.10, 0.10, 0.10]);
    }
    (attributes, weights, attribute_count)
}

fn rewrite_inline_geometry(block: &mut NifBlock, indices: &[u32]) -> bool {
    let original_vertices = match block.get_field("Vertex Data") {
        Some(NifValue::Array(items)) => items.clone(),
        _ => return false,
    };
    let vertex_count = original_vertices.len() as u32;
    let mut old_to_new: HashMap<u32, u32> = HashMap::new();
    let mut new_vertices = Vec::new();
    let mut new_triangles = Vec::new();

    for chunk in indices.chunks_exact(3) {
        let tri = [chunk[0], chunk[1], chunk[2]];
        if tri[0] >= vertex_count
            || tri[1] >= vertex_count
            || tri[2] >= vertex_count
            || tri[0] == tri[1]
            || tri[1] == tri[2]
            || tri[0] == tri[2]
        {
            continue;
        }
        let mut remapped = [0u32; 3];
        for (i, old) in tri.into_iter().enumerate() {
            let new = if let Some(new) = old_to_new.get(&old) {
                *new
            } else {
                let new = new_vertices.len() as u32;
                old_to_new.insert(old, new);
                new_vertices.push(original_vertices[old as usize].clone());
                new
            };
            remapped[i] = new;
        }
        new_triangles.push(triangle_value(remapped));
    }

    if new_triangles.is_empty() || new_triangles.len() >= count_inline_triangles(block) {
        return false;
    }

    block.set_field("Vertex Data", NifValue::Array(new_vertices.clone()));
    block.set_field("Num Vertices", NifValue::UInt(new_vertices.len() as u64));
    block.set_field("Triangles", NifValue::Array(new_triangles.clone()));
    block.set_field("Num Triangles", NifValue::UInt(new_triangles.len() as u64));
    let data_size = inline_data_size(block, new_vertices.len(), new_triangles.len());
    block.set_field("Data Size", NifValue::UInt(data_size as u64));
    update_inline_bounds(block, &new_vertices);
    true
}

fn inline_data_size(block: &NifBlock, vertices: usize, triangles: usize) -> usize {
    let stride = block
        .get_field("Vertex Desc")
        .map(NifValue::as_i64)
        .unwrap_or(0)
        & 0xF;
    (stride.max(0) as usize)
        .saturating_mul(vertices)
        .saturating_mul(4)
        .saturating_add(triangles.saturating_mul(6))
}

fn update_inline_bounds(block: &mut NifBlock, vertices: &[NifValue]) {
    let mut points = Vec::new();
    for vertex in vertices {
        let NifValue::Struct(fields) = vertex else {
            continue;
        };
        if let Some(v) = vec3(fields.get("Vertex")) {
            points.push(v);
        }
    }
    if points.is_empty() || block.get_field("Bounding Sphere").is_none() {
        return;
    }
    let center = bbox_center(&points);
    let radius = points
        .iter()
        .map(|p| distance(*p, center))
        .fold(0.0f32, f32::max);
    let mut sphere = IndexMap::new();
    sphere.insert("Center".to_string(), NifValue::Vec3(center));
    sphere.insert("Radius".to_string(), NifValue::Float(radius as f64));
    block.set_field("Bounding Sphere", NifValue::Struct(sphere));
}

fn bbox_center(points: &[[f32; 3]]) -> [f32; 3] {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in points {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ]
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn flatten_indices(triangles: &[[u32; 3]]) -> Vec<u32> {
    let mut indices = Vec::with_capacity(triangles.len() * 3);
    for tri in triangles {
        indices.extend_from_slice(tri);
    }
    indices
}

fn count_inline_triangles(block: &NifBlock) -> usize {
    read_triangles(block.get_field("Triangles")).len()
}

fn read_triangles(v: Option<&NifValue>) -> Vec<[u32; 3]> {
    match v {
        Some(NifValue::Array(items)) => items
            .iter()
            .filter_map(|t| match t {
                NifValue::Struct(f) => Some([
                    val_u64(f.get("v1"))? as u32,
                    val_u64(f.get("v2"))? as u32,
                    val_u64(f.get("v3"))? as u32,
                ]),
                NifValue::Array(a) if a.len() == 3 => Some([
                    val_u64(Some(&a[0]))? as u32,
                    val_u64(Some(&a[1]))? as u32,
                    val_u64(Some(&a[2]))? as u32,
                ]),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn triangle_value(triangle: [u32; 3]) -> NifValue {
    let mut fields = IndexMap::new();
    fields.insert("v1".to_string(), NifValue::UInt(triangle[0] as u64));
    fields.insert("v2".to_string(), NifValue::UInt(triangle[1] as u64));
    fields.insert("v3".to_string(), NifValue::UInt(triangle[2] as u64));
    NifValue::Struct(fields)
}

/// `true` iff `<source_dir>/meshes/<source_rel>` is a file. Uses the SAME path
/// join as `lod_assets` so the MNAM-written set matches the shipped-mesh set.
fn lod_source_exists(source_dir: &Path, source_rel: &str) -> bool {
    lod_abs_path(source_dir, source_rel).is_file()
}

/// Write the 1040-byte MNAM (4 × 260, unused slots zero-filled), replacing any
/// existing MNAM in place or appending one (FO4 places MNAM last in STAT).
/// Callers must first prove the record signature is in
/// `FO4_DIRECT_MNAM_BASE_SIGS`.
fn write_mnam(record: &mut ParsedRecord, slots: &[Option<String>; 4]) {
    let mut mnam = vec![0u8; MNAM_SLOT * 4];
    for (level, slot) in slots.iter().enumerate() {
        if let Some(path) = slot {
            let bytes = path.as_bytes();
            let len = bytes.len().min(MNAM_SLOT - 1); // keep a null terminator
            let off = level * MNAM_SLOT;
            mnam[off..off + len].copy_from_slice(&bytes[..len]);
        }
    }
    let data = Bytes::from(mnam);
    if let Some(existing) = record
        .subrecords
        .iter_mut()
        .find(|s| s.signature.as_str() == "MNAM")
    {
        existing.data = data;
    } else {
        record.subrecords.push(ParsedSubrecord {
            signature: SmolStr::new("MNAM"),
            data,
            semantic_type: None,
        });
    }
}

fn read_zstring(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).into_owned()
}

fn parse_conversion_workers(p: &JsonValue, fallback: Option<usize>) -> Option<usize> {
    p.get("conversion_workers")
        .and_then(|v| v.as_u64())
        .and_then(|workers| usize::try_from(workers).ok())
        .filter(|workers| *workers > 0)
        .or_else(|| fallback.filter(|workers| *workers > 0))
}

fn val_u64(v: Option<&NifValue>) -> Option<u64> {
    match v? {
        NifValue::UInt(u) => Some(*u),
        NifValue::Int(i) if *i >= 0 => Some(*i as u64),
        NifValue::Float(f) if *f >= 0.0 => Some(*f as u64),
        NifValue::Ref(r) if *r >= 0 => Some(*r as u64),
        _ => None,
    }
}

fn val_f32(v: Option<&NifValue>) -> Option<f32> {
    match v? {
        NifValue::Float(f) => Some(*f as f32),
        NifValue::Int(i) => Some(*i as f32),
        NifValue::UInt(u) => Some(*u as f32),
        _ => None,
    }
}

fn vec3(v: Option<&NifValue>) -> Option<[f32; 3]> {
    match v? {
        NifValue::Vec3(v) => Some(*v),
        NifValue::Struct(fields) => Some([
            val_f32(fields.get("x")).unwrap_or(0.0),
            val_f32(fields.get("y")).unwrap_or(0.0),
            val_f32(fields.get("z")).unwrap_or(0.0),
        ]),
        _ => None,
    }
}

fn uv2(v: Option<&NifValue>) -> Option<[f32; 2]> {
    match v? {
        NifValue::Struct(fields) => Some([
            val_f32(fields.get("u")).unwrap_or(0.0),
            val_f32(fields.get("v")).unwrap_or(0.0),
        ]),
        NifValue::Array(items) if items.len() >= 2 => Some([
            val_f32(Some(&items[0])).unwrap_or(0.0),
            val_f32(Some(&items[1])).unwrap_or(0.0),
        ]),
        _ => None,
    }
}

fn color4(v: Option<&NifValue>) -> Option<[f32; 4]> {
    match v? {
        NifValue::Color4(c) | NifValue::Vec4(c) => Some(*c),
        NifValue::Struct(fields) => Some([
            val_f32(fields.get("r")).unwrap_or(1.0),
            val_f32(fields.get("g")).unwrap_or(1.0),
            val_f32(fields.get("b")).unwrap_or(1.0),
            val_f32(fields.get("a")).unwrap_or(1.0),
        ]),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_new_native, plugin_handle_store_ref,
    };

    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;

    fn stat_record(form_id: u32, modl: &str) -> ParsedRecord {
        base_record("STAT", form_id, modl, 0)
    }

    fn stat_record_with_flags(form_id: u32, modl: &str, flags: u32) -> ParsedRecord {
        base_record("STAT", form_id, modl, flags)
    }

    fn tree_record(form_id: u32, modl: &str) -> ParsedRecord {
        base_record("TREE", form_id, modl, 0)
    }

    fn base_record(signature: &str, form_id: u32, modl: &str, flags: u32) -> ParsedRecord {
        let mut model = modl.as_bytes().to_vec();
        model.push(0);
        ParsedRecord {
            signature: SmolStr::new(signature),
            form_id,
            flags,
            version_control: 0,
            form_version: None,
            version2: None,
            subrecords: vec![ParsedSubrecord {
                signature: SmolStr::new("MODL"),
                data: Bytes::from(model),
                semantic_type: None,
            }],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn xalg_subrecord(flags: u64) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new("XALG"),
            data: Bytes::from(flags.to_le_bytes().to_vec()),
            semantic_type: None,
        }
    }

    fn name_subrecord(form_id: u32) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new("NAME"),
            data: Bytes::from(form_id.to_le_bytes().to_vec()),
            semantic_type: None,
        }
    }

    fn xlkr_subrecord(keyword_form_id: u32, linked_ref_form_id: u32) -> ParsedSubrecord {
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&keyword_form_id.to_le_bytes());
        data.extend_from_slice(&linked_ref_form_id.to_le_bytes());
        ParsedSubrecord {
            signature: SmolStr::new("XLKR"),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn refr_record(form_id: u32, base_form_id: u32, xalg_flags: u64) -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new("REFR"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: None,
            version2: None,
            subrecords: vec![xalg_subrecord(xalg_flags), name_subrecord(base_form_id)],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn write_lod_fixture(root: &Path, rel: &str) {
        let mut path = root.join("meshes");
        for component in rel.split('/') {
            path.push(component);
        }
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"nif").unwrap();
    }

    fn write_full_model_fixture(root: &Path, rel: &str, grid: usize) -> PathBuf {
        write_full_model_fixture_at(mnam_abs_path_from_mod_root(root, rel), grid)
    }

    fn write_source_full_model_fixture(root: &Path, rel: &str, grid: usize) -> PathBuf {
        write_full_model_fixture_at(mnam_abs_path(root, rel), grid)
    }

    fn write_full_model_fixture_at(path: PathBuf, grid: usize) -> PathBuf {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        let mut nif = NifFile::new("fo4");
        let shape_id = nif.add_block("BSTriShape", None);
        nif.blocks[0].set_field("Num Children", NifValue::UInt(1));
        nif.blocks[0].set_field(
            "Children",
            NifValue::Array(vec![NifValue::Ref(shape_id as i32)]),
        );

        let (vertices, triangles) = grid_geometry(grid);
        {
            let shape = &mut nif.blocks[shape_id];
            shape.set_field("Name", NifValue::String("proxy_source".into()));
            shape.set_field("Skin", NifValue::Ref(-1));
            shape.set_field("Vertex Desc", NifValue::Int(193_514_046_685_700));
            shape.set_field("Num Vertices", NifValue::UInt(vertices.len() as u64));
            shape.set_field("Vertex Data", NifValue::Array(vertices));
            shape.set_field("Num Triangles", NifValue::UInt(triangles.len() as u64));
            shape.set_field("Triangles", NifValue::Array(triangles));
            shape.set_field("Data Size", NifValue::UInt(0));
        }
        nif.save(Some(path.clone()))
            .unwrap_or_else(|err| panic!("write test NIF {}: {err}", path.display()));
        path
    }

    #[test]
    fn skyrim_source_mnam_is_preserved_for_fo4_lodgen() {
        let tmp = tempfile::tempdir().unwrap();
        let lod = r"LOD\Architecture\Whiterun\WRWallGate_LOD.nif";
        write_mnam_source_file(tmp.path(), lod);

        let form_id = 0x0001_2345;
        let mut source = stat_record_with_flags(
            form_id,
            r"Architecture\Whiterun\WRWallGate.nif",
            FLAG_HAS_DISTANT_LOD,
        );
        let expected_slots: [Option<String>; 4] = std::array::from_fn(|_| Some(lod.to_string()));
        write_mnam(&mut source, &expected_slots);
        let source_hints = collect_source_lod_hints(&[ParsedItem::Record(source)], tmp.path());

        let mut target = stat_record(form_id, r"Architecture\Whiterun\WRWallGate.nif");
        let mut pending = Vec::new();
        let changed = synthesize_record(
            &mut target,
            tmp.path(),
            tmp.path(),
            &source_hints,
            Game::SkyrimSe,
            &mut pending,
        );

        assert!(changed);
        assert!(pending.is_empty());
        assert_ne!(target.flags & FLAG_HAS_DISTANT_LOD, 0);
        let mnam = target
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "MNAM")
            .expect("target MNAM");
        assert_eq!(raw_mnam_slots(&mnam.data), expected_slots);
    }

    #[test]
    fn skyrim_visible_distant_fixture_yields_proxy_candidates() {
        let tmp = tempfile::tempdir().unwrap();
        let source_root = tmp.path().join("source");
        let mod_root = tmp.path().join("mod");
        let model = r"Architecture\Whiterun\WRWallGate.nif";
        write_source_full_model_fixture(&source_root, model, 8);

        let form_id = 0x0001_2345;
        let source = stat_record_with_flags(form_id, model, FLAG_HAS_DISTANT_LOD);
        let source_hints = collect_source_lod_hints(&[ParsedItem::Record(source)], &source_root);
        let mut target = stat_record(form_id, model);
        let mut pending = Vec::new();

        let changed = synthesize_record(
            &mut target,
            &mod_root,
            &source_root,
            &source_hints,
            Game::SkyrimSe,
            &mut pending,
        );

        assert!(!changed, "MNAM is applied after proxy generation");
        assert_eq!(
            pending.len(),
            1,
            "visible Skyrim base must emit one candidate"
        );
        assert!(
            pending[0]
                .outputs
                .iter()
                .all(|output| { output.mnam.starts_with(r"LOD\Generated\SkyrimSE\STAT\") })
        );

        let result = generate_proxy_candidate_assets(&pending[0], &mod_root, &source_root);
        assert_eq!(result.assets_written, 4, "{result:?}");
        assert!(result.generated.is_some(), "{result:?}");
        assert!(result.warnings.is_empty(), "{result:?}");
    }

    #[test]
    fn generated_proxy_candidate_writes_decimated_proxy_assets() {
        let tmp = tempfile::tempdir().unwrap();
        write_full_model_fixture(tmp.path(), r"Architecture\SkiResort\SkiResort.nif", 8);
        let record = stat_record(0x0000_bc2a, r"Architecture\SkiResort\SkiResort.nif");
        let candidate =
            generated_proxy_candidate(&record, r"Architecture\SkiResort\SkiResort.nif", true)
                .expect("candidate");

        let result = generate_proxy_candidate_assets(&candidate, tmp.path(), tmp.path());

        assert_eq!(result.assets_written, 4, "{result:?}");
        assert!(result.generated.is_some(), "{result:?}");
        assert!(result.warnings.is_empty(), "{result:?}");
        assert_eq!(result.timing.source_path_probes, 1);
        assert_eq!(result.timing.nif_loads, 1);
        assert_eq!(result.timing.outputs_attempted, 4);
        assert_eq!(result.timing.directory_preparations, 1);
        for output in &candidate.outputs {
            assert!(mnam_abs_path_from_mod_root(tmp.path(), &output.mnam).is_file());
        }
    }

    #[test]
    fn generated_structure_proxy_uses_lockborder_far_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let source =
            write_full_model_fixture(tmp.path(), r"Architecture\SkiResort\SkiResort.nif", 24);
        let record = stat_record(0x0000_bc2a, r"Architecture\SkiResort\SkiResort.nif");
        let candidate =
            generated_proxy_candidate(&record, r"Architecture\SkiResort\SkiResort.nif", true)
                .expect("candidate");

        let result = generate_proxy_candidate_assets(&candidate, tmp.path(), tmp.path());

        assert_eq!(result.assets_written, 4, "{result:?}");
        let profile = proxy_decimation_profile(r"Architecture\SkiResort\SkiResort.nif");
        assert_eq!(profile.lock_border, [true, true, true, true]);
        let source_tris = total_inline_triangles(&source);
        let lod2 = total_inline_triangles(&mnam_abs_path_from_mod_root(
            tmp.path(),
            candidate.outputs[2].mnam.as_str(),
        ));
        let lod3 = total_inline_triangles(&mnam_abs_path_from_mod_root(
            tmp.path(),
            candidate.outputs[3].mnam.as_str(),
        ));

        assert!(lod2 < source_tris, "LOD2 should still simplify structures");
        assert!(lod3 < lod2, "LOD3 should be cheaper than structure LOD2");
        assert!(
            lod2 > PROXY_MAX_TRIANGLES_PER_SHAPE[2],
            "structure LOD2 should not use the shrub/tree cap, got {lod2}"
        );
        assert!(
            lod3 > PROXY_MAX_TRIANGLES_PER_SHAPE[3],
            "structure LOD3 should not use the shrub/tree cap, got {lod3}"
        );
    }

    #[test]
    fn generated_proxy_candidate_falls_back_to_source_extracted_model() {
        let tmp = tempfile::tempdir().unwrap();
        let source_root = tmp.path().join("source");
        let mod_root = tmp.path().join("mod");
        write_source_full_model_fixture(&source_root, r"Architecture\SkiResort\SkiResort.nif", 8);
        let record = stat_record(0x0000_bc2a, r"Architecture\SkiResort\SkiResort.nif");
        let candidate =
            generated_proxy_candidate(&record, r"Architecture\SkiResort\SkiResort.nif", true)
                .expect("candidate");

        let result = generate_proxy_candidate_assets(&candidate, &mod_root, &source_root);

        assert_eq!(result.assets_written, 4, "{result:?}");
        assert!(result.generated.is_some(), "{result:?}");
        assert!(result.warnings.is_empty(), "{result:?}");
        for output in &candidate.outputs {
            assert!(mnam_abs_path_from_mod_root(&mod_root, &output.mnam).is_file());
        }
    }

    #[test]
    fn existing_generated_mnam_regenerates_missing_proxy_assets() {
        let tmp = tempfile::tempdir().unwrap();
        write_full_model_fixture(tmp.path(), r"Architecture\SkiResort\SkiResort.nif", 8);
        let handle_id = plugin_handle_new_native("OutputStaleProxy.esm", Some("fo4"))
            .expect("new target handle");
        {
            let mut record = stat_record(0x0000_bc2a, r"Architecture\SkiResort\SkiResort.nif");
            let slots: [Option<String>; 4] = std::array::from_fn(|level| {
                Some(format!(r"LOD\Generated\FO76\STAT\00BC2A_LOD_{level}.nif"))
            });
            write_mnam(&mut record, &slots);
            record.flags |= FLAG_HAS_DISTANT_LOD;

            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(slot, record);
        }

        let report = run_phase_on(handle_id, tmp.path());

        assert_eq!(report.records_changed, 0);
        assert_eq!(report.assets_written, 4);
        for level in 0..4 {
            let mnam = format!(r"LOD\Generated\FO76\STAT\00BC2A_LOD_{level}.nif");
            assert!(
                mnam_abs_path_from_mod_root(tmp.path(), &mnam).is_file(),
                "missing regenerated proxy {mnam}"
            );
        }

        plugin_handle_close_native(handle_id);
    }

    #[test]
    fn generated_proxy_candidate_decimates_all_slots_and_keeps_lod3_small() {
        let tmp = tempfile::tempdir().unwrap();
        let source = write_full_model_fixture(tmp.path(), r"Landscape\Trees\Stump01.nif", 24);
        let record = stat_record(0x0004_0208, r"Landscape\Trees\Stump01.nif");
        let candidate = generated_proxy_candidate(&record, r"Landscape\Trees\Stump01.nif", true)
            .expect("candidate");

        let result = generate_proxy_candidate_assets(&candidate, tmp.path(), tmp.path());

        assert_eq!(result.assets_written, 4, "{result:?}");
        let source_tris = total_inline_triangles(&source);
        let lod0 = total_inline_triangles(&mnam_abs_path_from_mod_root(
            tmp.path(),
            candidate.outputs[0].mnam.as_str(),
        ));
        let lod1 = total_inline_triangles(&mnam_abs_path_from_mod_root(
            tmp.path(),
            candidate.outputs[1].mnam.as_str(),
        ));
        let lod3 = total_inline_triangles(&mnam_abs_path_from_mod_root(
            tmp.path(),
            candidate.outputs[3].mnam.as_str(),
        ));

        assert!(
            lod0 < source_tris,
            "LOD0 should be a proxy, not the source mesh"
        );
        assert!(lod1 < lod0, "LOD1 should be cheaper than LOD0");
        assert!(lod3 < lod1, "LOD3 should be cheaper than LOD1");
        assert!(
            lod3 <= PROXY_MAX_TRIANGLES_PER_SHAPE[3],
            "LOD3 should stay stump/shrub-scale, got {lod3}"
        );
    }

    #[test]
    fn generated_proxy_candidate_ignores_scol_parent() {
        let record = base_record(
            "SCOL",
            0x0084_274b,
            r"SCOL\SeventySix.esm\CM0084274B.NIF",
            0,
        );

        assert!(
            generated_proxy_candidate(&record, r"SCOL\SeventySix.esm\CM0084274B.NIF", true)
                .is_none()
        );
    }

    #[test]
    fn source_mnam_recovers_misaligned_far_tree_slots() {
        let tmp = tempfile::tempdir().unwrap();
        let lod1 = r"LOD\Landscape\Trees\Chargen\TreeMaplePW03Or_LOD_1.nif";
        let lod2 = r"LOD\Landscape\Trees\Chargen\TreeMaplePW03Or_LOD_2.nif";
        let lod3 = r"LOD\Landscape\Trees\Chargen\TreeMaplePW03Or_LOD_3.nif";
        for path in [lod1, lod2, lod3] {
            write_mnam_source_file(tmp.path(), path);
        }

        let mut data = vec![0u8; MNAM_SLOT * 4];
        write_mnam_test_path(&mut data, 0, lod1);
        write_mnam_test_path(&mut data, MNAM_SLOT, lod1);
        write_mnam_test_path(&mut data, MNAM_SLOT * 2 - 1, lod2);
        write_mnam_test_path(&mut data, MNAM_SLOT * 3 - 1, lod3);

        let slots = slots_from_mnam_bytes(&data, tmp.path()).expect("recovered source MNAM");

        assert_eq!(slots[0].as_deref(), Some(lod1));
        assert_eq!(slots[1].as_deref(), Some(lod1));
        assert_eq!(slots[2].as_deref(), Some(lod2));
        assert_eq!(slots[3].as_deref(), Some(lod3));
    }

    #[test]
    fn skyrim_farmhouse_mnam_recovers_plain_paths_before_slot_boundaries() {
        let tmp = tempfile::tempdir().unwrap();
        let hlod = r"LOD\Farmhouse\Farmhouse01_HLOD.nif";
        let lod = r"LOD\Farmhouse\Farmhouse01_LOD.nif";
        for path in [hlod, lod] {
            write_mnam_source_file(tmp.path(), path);
        }

        let mut data = vec![0u8; MNAM_SLOT * 4];
        write_mnam_test_path(&mut data, 0, hlod);
        write_mnam_test_path(&mut data, MNAM_SLOT, hlod);
        write_mnam_test_path(&mut data, MNAM_SLOT * 2 - 1, hlod);
        write_mnam_test_path(&mut data, MNAM_SLOT * 3 - 1, lod);

        let slots = slots_from_mnam_bytes(&data, tmp.path()).expect("recovered source MNAM");

        assert_eq!(slots[0].as_deref(), Some(hlod));
        assert_eq!(slots[1].as_deref(), Some(hlod));
        assert_eq!(slots[2].as_deref(), Some(hlod));
        assert_eq!(slots[3].as_deref(), Some(lod));
    }

    #[test]
    fn skyrim_sibling_lod_is_used_without_proxy_generation() {
        let tmp = tempfile::tempdir().unwrap();
        let model = r"Architecture\Farmhouse\Farmhouse01.nif";
        let sibling = r"architecture/farmhouse/farmhouse01_lod.nif";
        write_lod_fixture(tmp.path(), sibling);

        let form_id = 0x0000_084a;
        let source = stat_record_with_flags(form_id, model, FLAG_HAS_DISTANT_LOD);
        let source_hints = collect_source_lod_hints(&[ParsedItem::Record(source)], tmp.path());
        let mut target = stat_record(form_id, model);
        let mut pending = Vec::new();

        let changed = synthesize_record(
            &mut target,
            tmp.path(),
            tmp.path(),
            &source_hints,
            Game::SkyrimSe,
            &mut pending,
        );

        assert!(changed);
        assert!(pending.is_empty());
        let mnam = target
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "MNAM")
            .expect("target MNAM");
        assert_eq!(
            raw_mnam_slots(&mnam.data),
            std::array::from_fn(|_| {
                Some(r"Architecture\Farmhouse\Farmhouse01_LOD.nif".to_string())
            })
        );
    }

    #[test]
    fn unflagged_skyrim_tree_generates_object_lod_proxy() {
        let tmp = tempfile::tempdir().unwrap();
        let model = r"Landscape\Trees\TreePineForest01.nif";
        let form_id = 0x0001_3000;
        let source = tree_record(form_id, model);
        let source_hints = collect_source_lod_hints(&[ParsedItem::Record(source)], tmp.path());
        let mut target = tree_record(form_id, model);
        let mut pending = Vec::new();

        let changed = synthesize_record(
            &mut target,
            tmp.path(),
            tmp.path(),
            &source_hints,
            Game::SkyrimSe,
            &mut pending,
        );

        assert!(!changed);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].signature, "TREE");
        assert_eq!(pending[0].form_id, form_id);
        assert!(pending[0].set_base_flag);
    }

    fn write_mnam_source_file(source_dir: &Path, mnam: &str) {
        let path = mnam_abs_path(source_dir, mnam);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, b"nif").unwrap();
    }

    fn write_mnam_test_path(data: &mut [u8], offset: usize, path: &str) {
        let bytes = path.as_bytes();
        data[offset..offset + bytes.len()].copy_from_slice(bytes);
    }

    fn total_inline_triangles(path: &Path) -> usize {
        let nif = NifFile::load(path.to_path_buf())
            .unwrap_or_else(|err| panic!("load generated NIF {}: {err}", path.display()));
        nif.blocks.iter().map(count_inline_triangles).sum()
    }

    fn grid_geometry(grid: usize) -> (Vec<NifValue>, Vec<NifValue>) {
        let mut vertices = Vec::new();
        for y in 0..=grid {
            for x in 0..=grid {
                let fx = x as f32;
                let fy = y as f32;
                vertices.push(vertex_struct(
                    [fx, fy, 0.0],
                    [fx / grid as f32, fy / grid as f32],
                ));
            }
        }
        let mut triangles = Vec::new();
        let stride = grid + 1;
        for y in 0..grid {
            for x in 0..grid {
                let v0 = (y * stride + x) as u32;
                let v1 = v0 + 1;
                let v2 = v0 + stride as u32;
                let v3 = v2 + 1;
                triangles.push(triangle_value([v0, v1, v3]));
                triangles.push(triangle_value([v0, v3, v2]));
            }
        }
        (vertices, triangles)
    }

    fn vertex_struct(pos: [f32; 3], uv: [f32; 2]) -> NifValue {
        let mut fields = IndexMap::new();
        fields.insert("Vertex".into(), NifValue::Vec3(pos));
        fields.insert("Unused W".into(), NifValue::UInt(0));
        fields.insert("UV".into(), uv_value(uv));
        fields.insert("Normal".into(), NifValue::Vec3([0.0, 0.0, 1.0]));
        fields.insert("Bitangent Y".into(), NifValue::Float(0.0));
        NifValue::Struct(fields)
    }

    fn uv_value(uv: [f32; 2]) -> NifValue {
        let mut fields = IndexMap::new();
        fields.insert("u".into(), NifValue::Float(uv[0] as f64));
        fields.insert("v".into(), NifValue::Float(uv[1] as f64));
        NifValue::Struct(fields)
    }

    fn mnam_and_flags(handle_id: u64, form_id: u32) -> (Option<Vec<u8>>, u32) {
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&handle_id).unwrap();
        fn find(items: &[ParsedItem], form_id: u32) -> Option<(Option<Vec<u8>>, u32)> {
            for item in items {
                match item {
                    ParsedItem::Record(r) if r.form_id == form_id => {
                        let mnam = r
                            .subrecords
                            .iter()
                            .find(|s| s.signature.as_str() == "MNAM")
                            .map(|s| s.data.to_vec());
                        return Some((mnam, r.flags));
                    }
                    ParsedItem::Group(g) => {
                        if let Some(found) = find(&g.children, form_id) {
                            return Some(found);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        find(&slot.parsed.root_items, form_id).unwrap_or((None, 0))
    }

    fn run_phase_on_with_source(
        target_handle_id: u64,
        source_handle_id: u64,
        source_dir: &Path,
    ) -> PhaseReport {
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id,
            target_handle_id,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esm".into(),
                ..Default::default()
            },
        })
        .unwrap();
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({});
            let mod_dir = source_dir.to_path_buf();
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            SynthesizeObjectLodPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();
        drop_run(id).unwrap();
        report
    }

    fn run_phase_on(handle_id: u64, source_dir: &Path) -> PhaseReport {
        run_phase_on_with_source(handle_id, 999_999, source_dir)
    }

    #[test]
    fn writes_mnam_and_flag_when_lod_exists() {
        let tmp = tempfile::tempdir().unwrap();
        // STAT 0x801 has a conventional single _lod; STAT 0x802 has none.
        write_lod_fixture(tmp.path(), "lod/architecture/foo/bar01_lod.nif");

        let handle_id = plugin_handle_new_native("Output.esm", Some("fo4")).expect("new handle");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                slot,
                stat_record(0x0000_0801, "Architecture\\Foo\\Bar01.nif"),
            );
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                slot,
                stat_record(0x0000_0802, "Architecture\\Foo\\Missing01.nif"),
            );
        }

        let report = run_phase_on(handle_id, tmp.path());
        assert_eq!(
            report.records_changed, 1,
            "only the STAT with a _lod changes"
        );

        let (mnam, flags) = mnam_and_flags(handle_id, 0x0000_0801);
        let mnam = mnam.expect("STAT 0x801 should have MNAM");
        assert_eq!(mnam.len(), MNAM_SLOT * 4, "MNAM must be 1040 bytes");
        assert_ne!(flags & FLAG_HAS_DISTANT_LOD, 0, "Has Distant LOD flag set");
        let level0 = read_zstring(&mnam[..MNAM_SLOT]);
        assert_eq!(level0, "LOD\\Architecture\\Foo\\Bar01_LOD.nif");
        for level in 1..4 {
            assert_eq!(
                read_zstring(&mnam[MNAM_SLOT * level..MNAM_SLOT * (level + 1)]),
                "LOD\\Architecture\\Foo\\Bar01_LOD.nif",
                "single _lod fallback should cover every LOD slot"
            );
        }

        let (missing_mnam, missing_flags) = mnam_and_flags(handle_id, 0x0000_0802);
        assert!(missing_mnam.is_none(), "no _lod => no MNAM");
        assert_eq!(missing_flags & FLAG_HAS_DISTANT_LOD, 0);

        plugin_handle_close_native(handle_id);
    }

    #[test]
    fn non_stat_lod_bases_never_receive_illegal_mnam() {
        let tmp = tempfile::tempdir().unwrap();
        write_lod_fixture(tmp.path(), "lod/architecture/foo/activator_lod.nif");
        write_lod_fixture(tmp.path(), "lod/architecture/foo/movable_lod.nif");

        let handle_id = plugin_handle_new_native("OutputNonStat.esm", Some("fo4")).expect("new");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle_id).unwrap();
            for (signature, form_id, model) in [
                ("ACTI", 0x0000_bbf9, r"Architecture\Foo\Activator.nif"),
                ("MSTT", 0x0000_bbfa, r"Architecture\Foo\Movable.nif"),
            ] {
                esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                    slot,
                    base_record(signature, form_id, model, 0),
                );
            }
        }

        let report = run_phase_on(handle_id, tmp.path());
        assert_eq!(report.records_changed, 0);
        for form_id in [0x0000_bbf9, 0x0000_bbfa] {
            let (mnam, flags) = mnam_and_flags(handle_id, form_id);
            assert!(mnam.is_none(), "ACTI/MSTT must not receive FO4 STAT MNAM");
            assert_eq!(flags & FLAG_HAS_DISTANT_LOD, 0);
        }

        plugin_handle_close_native(handle_id);
    }

    #[test]
    fn non_stat_lod_overlay_is_exact_to_the_placed_reference() {
        let base_form_id = 0x0000_bbf9;
        let reference_form_id = 0x0001_2345;
        let items = vec![
            ParsedItem::Record(base_record(
                "ACTI",
                base_form_id,
                r"Architecture\Foo\Activator.nif",
                0,
            )),
            ParsedItem::Record(refr_record(reference_form_id, base_form_id, 0)),
        ];
        let slots = [
            Some(r"LOD\Architecture\Foo\Activator_LOD.nif".to_string()),
            None,
            None,
            None,
        ];
        let overlay = build_object_lod_overlay(
            &items,
            &[GeneratedProxyMnam {
                signature: "ACTI".to_string(),
                form_id: local_object_id(base_form_id),
                slots: slots.clone(),
                set_base_flag: false,
            }],
            &SourceLodHints::default(),
            "Output.esm",
        )
        .expect("object LOD overlay");

        assert_eq!(overlay.entries.len(), 1);
        let entry = &overlay.entries[0];
        assert_eq!(entry.reference_form_id, reference_form_id);
        assert_eq!(entry.placed_base_form_id, base_form_id);
        assert_eq!(entry.component_index, None);
        assert_eq!(entry.component_base_form_id, None);
        assert_eq!(entry.base_signature, "ACTI");
        assert_eq!(entry.lod_models, slots);
        assert!(!entry.force_visible);
    }

    #[test]
    fn non_stat_lod_overlay_excludes_deleted_reference_and_base() {
        let base_form_id = 0x0000_bbf9;
        let reference_form_id = 0x0001_2345;
        let virtual_base = GeneratedProxyMnam {
            signature: "ACTI".to_string(),
            form_id: local_object_id(base_form_id),
            slots: [
                Some(r"LOD\Architecture\Foo\Activator_LOD.nif".to_string()),
                None,
                None,
                None,
            ],
            set_base_flag: true,
        };
        let mut deleted_ref = refr_record(reference_form_id, base_form_id, 0);
        deleted_ref.flags = FLAG_DELETED;
        let deleted_reference_overlay = build_object_lod_overlay(
            &[
                ParsedItem::Record(base_record(
                    "ACTI",
                    base_form_id,
                    r"Architecture\Foo\Activator.nif",
                    0,
                )),
                ParsedItem::Record(deleted_ref),
            ],
            std::slice::from_ref(&virtual_base),
            &SourceLodHints::default(),
            "Output.esm",
        )
        .expect("deleted reference overlay");
        assert!(deleted_reference_overlay.entries.is_empty());

        let deleted_base_overlay = build_object_lod_overlay(
            &[
                ParsedItem::Record(base_record(
                    "ACTI",
                    base_form_id,
                    r"Architecture\Foo\Activator.nif",
                    FLAG_DELETED,
                )),
                ParsedItem::Record(refr_record(reference_form_id, base_form_id, 0)),
            ],
            std::slice::from_ref(&virtual_base),
            &SourceLodHints::default(),
            "Output.esm",
        )
        .expect("deleted base overlay");
        assert!(deleted_base_overlay.entries.is_empty());
    }

    #[test]
    fn scol_overlay_targets_only_the_non_stat_component() {
        let scol_form_id = 0x0000_4000_u32;
        let component_form_id = 0x0000_4001_u32;
        let reference_form_id = 0x0000_5000_u32;
        let mut scol = base_record("SCOL", scol_form_id, r"SCOL\Composite.nif", 0);
        scol.subrecords.push(ParsedSubrecord {
            signature: SmolStr::new("ONAM"),
            data: Bytes::from(component_form_id.to_le_bytes().to_vec()),
            semantic_type: None,
        });
        scol.subrecords.push(ParsedSubrecord {
            signature: SmolStr::new("DATA"),
            data: Bytes::from(vec![0; 28]),
            semantic_type: None,
        });
        let items = vec![
            ParsedItem::Record(scol),
            ParsedItem::Record(base_record(
                "MSTT",
                component_form_id,
                r"Architecture\Foo\Movable.nif",
                0,
            )),
            ParsedItem::Record(refr_record(reference_form_id, scol_form_id, 0)),
        ];
        let overlay = build_object_lod_overlay(
            &items,
            &[GeneratedProxyMnam {
                signature: "MSTT".to_string(),
                form_id: local_object_id(component_form_id),
                slots: [
                    Some(r"LOD\Architecture\Foo\Movable_LOD.nif".to_string()),
                    None,
                    None,
                    None,
                ],
                set_base_flag: true,
            }],
            &SourceLodHints::default(),
            "Output.esm",
        )
        .expect("SCOL component overlay");

        assert_eq!(overlay.entries.len(), 1);
        let entry = &overlay.entries[0];
        assert_eq!(entry.reference_form_id, reference_form_id);
        assert_eq!(entry.placed_base_form_id, scol_form_id);
        assert_eq!(entry.component_index, Some(0));
        assert_eq!(entry.component_base_form_id, Some(component_form_id));
        assert_eq!(entry.base_signature, "MSTT");
        assert!(entry.force_visible);
    }

    #[test]
    fn scol_overlay_excludes_deleted_component_base() {
        let scol_form_id = 0x0000_4000_u32;
        let component_form_id = 0x0000_4001_u32;
        let mut scol = base_record("SCOL", scol_form_id, r"SCOL\Composite.nif", 0);
        scol.subrecords.push(ParsedSubrecord {
            signature: SmolStr::new("ONAM"),
            data: Bytes::from(component_form_id.to_le_bytes().to_vec()),
            semantic_type: None,
        });
        scol.subrecords.push(ParsedSubrecord {
            signature: SmolStr::new("DATA"),
            data: Bytes::from(vec![0; 28]),
            semantic_type: None,
        });
        let overlay = build_object_lod_overlay(
            &[
                ParsedItem::Record(scol),
                ParsedItem::Record(base_record(
                    "MSTT",
                    component_form_id,
                    r"Architecture\Foo\Movable.nif",
                    FLAG_DELETED,
                )),
                ParsedItem::Record(refr_record(0x0000_5000, scol_form_id, 0)),
            ],
            &[GeneratedProxyMnam {
                signature: "MSTT".to_string(),
                form_id: local_object_id(component_form_id),
                slots: [
                    Some(r"LOD\Architecture\Foo\Movable_LOD.nif".to_string()),
                    None,
                    None,
                    None,
                ],
                set_base_flag: true,
            }],
            &SourceLodHints::default(),
            "Output.esm",
        )
        .expect("deleted SCOL component overlay");
        assert!(overlay.entries.is_empty());
    }

    #[test]
    fn visible_scol_ref_generates_overlay_for_unflagged_component() {
        let temp = tempfile::tempdir().unwrap();
        write_lod_fixture(temp.path(), "lod/architecture/foo/movable_lod.nif");
        let scol_form_id = 0x0000_4000_u32;
        let component_form_id = 0x0000_4001_u32;
        let reference_form_id = 0x0000_5000_u32;
        let mut scol = base_record("SCOL", scol_form_id, r"SCOL\Composite.nif", 0);
        scol.subrecords.push(ParsedSubrecord {
            signature: SmolStr::new("ONAM"),
            data: Bytes::from(component_form_id.to_le_bytes().to_vec()),
            semantic_type: None,
        });
        scol.subrecords.push(ParsedSubrecord {
            signature: SmolStr::new("DATA"),
            data: Bytes::from(vec![0; 28]),
            semantic_type: None,
        });
        let items = vec![
            ParsedItem::Record(scol),
            ParsedItem::Record(base_record(
                "MSTT",
                component_form_id,
                r"Architecture\Foo\Movable.nif",
                0,
            )),
            ParsedItem::Record(refr_record(
                reference_form_id,
                scol_form_id,
                XALG_VISIBLE_DISTANT,
            )),
        ];
        let source_hints = SourceLodHints {
            has_source: true,
            ..Default::default()
        };
        let visible_components = collect_visible_scol_component_ids(&items, &source_hints);
        assert_eq!(
            visible_components,
            HashSet::from([local_object_id(component_form_id)])
        );
        let mut virtual_bases = Vec::new();
        let mut pending = Vec::new();
        collect_virtual_lod_bases(
            &items,
            temp.path(),
            temp.path(),
            &source_hints,
            &visible_components,
            Game::Fo76,
            &mut virtual_bases,
            &mut pending,
        );
        assert!(pending.is_empty());
        assert_eq!(virtual_bases.len(), 1);

        let overlay = build_object_lod_overlay(&items, &virtual_bases, &source_hints, "Output.esm")
            .expect("visible SCOL component overlay");
        assert_eq!(overlay.entries.len(), 1);
        assert_eq!(overlay.entries[0].component_index, Some(0));
        assert_eq!(
            overlay.entries[0].component_base_form_id,
            Some(component_form_id)
        );
        assert!(overlay.entries[0].force_visible);
    }

    #[test]
    fn visible_scol_ref_synthesizes_unflagged_stat_component_without_global_flag() {
        let temp = tempfile::tempdir().unwrap();
        write_lod_fixture(temp.path(), "lod/architecture/foo/static_lod.nif");
        let scol_form_id = 0x0000_4100_u32;
        let component_form_id = 0x0000_4101_u32;
        let mut scol = base_record("SCOL", scol_form_id, r"SCOL\Composite.nif", 0);
        scol.subrecords.push(ParsedSubrecord {
            signature: SmolStr::new("ONAM"),
            data: Bytes::from(component_form_id.to_le_bytes().to_vec()),
            semantic_type: None,
        });
        scol.subrecords.push(ParsedSubrecord {
            signature: SmolStr::new("DATA"),
            data: Bytes::from(vec![0; 28]),
            semantic_type: None,
        });
        let items = vec![
            ParsedItem::Record(scol),
            ParsedItem::Record(refr_record(0x0000_5100, scol_form_id, XALG_VISIBLE_DISTANT)),
        ];
        let source_hints = SourceLodHints {
            has_source: true,
            ..Default::default()
        };
        let visible_components = collect_visible_scol_component_ids(
            &[
                items[0].clone(),
                ParsedItem::Record(stat_record(
                    component_form_id,
                    r"Architecture\Foo\Static.nif",
                )),
                items[1].clone(),
            ],
            &source_hints,
        );
        let mut component = stat_record(component_form_id, r"Architecture\Foo\Static.nif");
        let mut pending = Vec::new();

        let changed = synthesize_record_with_visible_components(
            &mut component,
            temp.path(),
            temp.path(),
            &source_hints,
            &visible_components,
            Game::Fo76,
            &mut pending,
        );

        assert!(changed);
        assert!(pending.is_empty());
        assert!(record_has_non_empty_mnam(&component));
        assert_eq!(component.flags & FLAG_HAS_DISTANT_LOD, 0);
    }

    #[test]
    fn carried_non_stat_mnam_and_flag_are_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let target_handle_id =
            plugin_handle_new_native("OutputCarriedMnam.esm", Some("fo4")).expect("new target");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let target_slot = store.get_mut(&target_handle_id).unwrap();
            let mut target = base_record(
                "ACTI",
                0x0000_0821,
                r"Architecture\Foo\Activator.nif",
                FLAG_HAS_DISTANT_LOD,
            );
            write_mnam(
                &mut target,
                &[
                    Some(r"LOD\Architecture\Foo\Activator_LOD.nif".to_string()),
                    None,
                    None,
                    None,
                ],
            );
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(target_slot, target);
        }

        let report = run_phase_on(target_handle_id, tmp.path());
        assert_eq!(report.records_changed, 1);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0000_0821);
        assert!(mnam.is_none());
        assert_eq!(flags & FLAG_HAS_DISTANT_LOD, 0);

        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn copies_source_mnam_before_deriving_from_namespaced_modl() {
        let tmp = tempfile::tempdir().unwrap();
        write_lod_fixture(
            tmp.path(),
            "lod/landscape/trees/chargen/treemaplepw01or_lod_1.nif",
        );
        write_lod_fixture(
            tmp.path(),
            "lod/landscape/trees/chargen/treemaplepw01or_lod_2.nif",
        );
        write_lod_fixture(
            tmp.path(),
            "lod/landscape/trees/chargen/treemaplepw01or_lod_3.nif",
        );

        let source_handle_id =
            plugin_handle_new_native("Source.esm", Some("fo76")).expect("new source handle");
        let target_handle_id =
            plugin_handle_new_native("Output3.esm", Some("fo4")).expect("new target handle");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            let mut source = stat_record(
                0x0000_0820,
                "Landscape\\Trees\\Chargen\\TreeMaplePreWar01Orange.nif",
            );
            write_mnam(
                &mut source,
                &[
                    Some(r"LOD\Landscape\Trees\Chargen\TreeMaplePW01Or_LOD_1.nif".to_string()),
                    Some(r"LOD\Landscape\Trees\Chargen\TreeMaplePW01Or_LOD_1.nif".to_string()),
                    Some(r"LOD\Landscape\Trees\Chargen\TreeMaplePW01Or_LOD_2.nif".to_string()),
                    Some(r"LOD\Landscape\Trees\Chargen\TreeMaplePW01Or_LOD_3.nif".to_string()),
                ],
            );
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(source_slot, source);

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                target_slot,
                stat_record(
                    0x0000_0820,
                    "FO76\\Landscape\\Trees\\Chargen\\TreeMaplePreWar01Orange.nif",
                ),
            );
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 1);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0000_0820);
        let mnam = mnam.expect("source MNAM should be copied");
        assert_ne!(flags & FLAG_HAS_DISTANT_LOD, 0);
        assert_eq!(
            read_zstring(&mnam[..MNAM_SLOT]),
            r"LOD\Landscape\Trees\Chargen\TreeMaplePW01Or_LOD_1.nif"
        );
        assert_eq!(
            read_zstring(&mnam[MNAM_SLOT * 3..MNAM_SLOT * 4]),
            r"LOD\Landscape\Trees\Chargen\TreeMaplePW01Or_LOD_3.nif"
        );

        plugin_handle_close_native(source_handle_id);
        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn source_xalg_visible_distant_uses_full_model_proxy_when_no_lod_mesh() {
        const XALG_VISIBLE_DISTANT: u64 = 0x0000_0200;

        let tmp = tempfile::tempdir().unwrap();
        write_full_model_fixture(tmp.path(), r"Architecture\SkiResort\SkiResort.nif", 8);
        let source_handle_id =
            plugin_handle_new_native("SourceXalg.esm", Some("fo76")).expect("new source handle");
        let target_handle_id =
            plugin_handle_new_native("OutputXalg.esm", Some("fo4")).expect("new target handle");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            let mut source = stat_record(0x0000_bc2a, r"Architecture\SkiResort\SkiResort.nif");
            source.subrecords.push(xalg_subrecord(XALG_VISIBLE_DISTANT));
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(source_slot, source);

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                target_slot,
                stat_record(0x0000_bc2a, r"Architecture\SkiResort\SkiResort.nif"),
            );
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 1);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0000_bc2a);
        let mnam = mnam.expect("XALG-visible source record should get proxy MNAM");
        assert_ne!(flags & FLAG_HAS_DISTANT_LOD, 0);
        for level in 0..4 {
            let slot = read_zstring(&mnam[MNAM_SLOT * level..MNAM_SLOT * (level + 1)]);
            assert_eq!(
                slot,
                format!(r"LOD\Generated\FO76\STAT\00BC2A_LOD_{level}.nif")
            );
            assert!(
                mnam_abs_path_from_mod_root(tmp.path(), &slot).is_file(),
                "generated proxy should exist for {slot}"
            );
        }

        plugin_handle_close_native(source_handle_id);
        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn source_xalg_visible_distant_skips_runtime_sky_proxy() {
        const XALG_VISIBLE_DISTANT: u64 = 0x0000_0200;

        let tmp = tempfile::tempdir().unwrap();
        write_full_model_fixture(tmp.path(), r"Sky\Cloud_ValleyFog01_circle.nif", 8);
        let source_handle_id =
            plugin_handle_new_native("SourceSkyXalg.esm", Some("fo76")).expect("new source");
        let target_handle_id =
            plugin_handle_new_native("OutputSkyXalg.esm", Some("fo4")).expect("new target");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            let mut source = stat_record(0x0001_827d, r"Sky\Cloud_ValleyFog01_circle.nif");
            source.subrecords.push(xalg_subrecord(XALG_VISIBLE_DISTANT));
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(source_slot, source);

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                target_slot,
                stat_record(0x0001_827d, r"Sky\Cloud_ValleyFog01_circle.nif"),
            );
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 0);
        assert_eq!(report.assets_written, 0);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0001_827d);
        assert!(mnam.is_none());
        assert_eq!(flags & FLAG_HAS_DISTANT_LOD, 0);

        plugin_handle_close_native(source_handle_id);
        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn source_refr_xalg_visible_distant_skips_runtime_effect_proxy() {
        const XALG_VISIBLE_DISTANT: u64 = 0x0000_0200;

        let tmp = tempfile::tempdir().unwrap();
        write_full_model_fixture(tmp.path(), r"Effects\RadStormDistantCloud.nif", 8);
        let source_handle_id =
            plugin_handle_new_native("SourceEffectRefrXalg.esm", Some("fo76")).expect("new source");
        let target_handle_id =
            plugin_handle_new_native("OutputEffectRefrXalg.esm", Some("fo4")).expect("new target");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                source_slot,
                refr_record(0x003a_308f, 0x0005_2b06, XALG_VISIBLE_DISTANT),
            );

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                target_slot,
                stat_record(0x0005_2b06, r"Effects\RadStormDistantCloud.nif"),
            );
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 0);
        assert_eq!(report.assets_written, 0);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0005_2b06);
        assert!(mnam.is_none());
        assert_eq!(flags & FLAG_HAS_DISTANT_LOD, 0);

        plugin_handle_close_native(source_handle_id);
        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn source_refr_xalg_visible_distant_uses_proxy_without_global_base_flag() {
        const XALG_VISIBLE_DISTANT: u64 = 0x0000_0200;

        let tmp = tempfile::tempdir().unwrap();
        write_full_model_fixture(tmp.path(), r"Architecture\SkiResort\SkiResort.nif", 8);
        let source_handle_id =
            plugin_handle_new_native("SourceRefrXalg.esm", Some("fo76")).expect("new source");
        let target_handle_id =
            plugin_handle_new_native("OutputRefrXalg.esm", Some("fo4")).expect("new target");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                source_slot,
                refr_record(0x003a_308f, 0x0000_bc2a, XALG_VISIBLE_DISTANT),
            );

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                target_slot,
                stat_record(0x0000_bc2a, r"Architecture\SkiResort\SkiResort.nif"),
            );
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 1);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0000_bc2a);
        let mnam = mnam.expect("XALG-visible source placement should get proxy MNAM");
        assert_eq!(flags & FLAG_HAS_DISTANT_LOD, 0);
        assert_eq!(
            read_zstring(&mnam[..MNAM_SLOT]),
            r"LOD\Generated\FO76\STAT\00BC2A_LOD_0.nif"
        );
        assert_eq!(
            read_zstring(&mnam[MNAM_SLOT..MNAM_SLOT * 2]),
            r"LOD\Generated\FO76\STAT\00BC2A_LOD_1.nif"
        );

        plugin_handle_close_native(source_handle_id);
        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn source_refr_multiref_lod_uses_proxy_without_global_base_flag() {
        let tmp = tempfile::tempdir().unwrap();
        write_full_model_fixture(tmp.path(), r"Architecture\HighTech_Mansions\Wall01.nif", 8);
        let source_handle_id = plugin_handle_new_native("SourceRefrMultirefLod.esm", Some("fo76"))
            .expect("new source");
        let target_handle_id =
            plugin_handle_new_native("OutputRefrMultirefLod.esm", Some("fo4")).expect("new target");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            let mut source_ref = refr_record(0x0709_1fae, 0x0700_316d, 0);
            source_ref
                .subrecords
                .push(xlkr_subrecord(0x0719_5411, 0x0733_978c));
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                source_slot,
                source_ref,
            );

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                target_slot,
                stat_record(0x0000_316d, r"Architecture\HighTech_Mansions\Wall01.nif"),
            );
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 1);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0000_316d);
        let mnam = mnam.expect("MultirefLOD source placement should get proxy MNAM");
        assert_eq!(flags & FLAG_HAS_DISTANT_LOD, 0);
        assert_eq!(
            read_zstring(&mnam[..MNAM_SLOT]),
            r"LOD\Generated\FO76\STAT\00316D_LOD_0.nif"
        );
    }

    #[test]
    fn runtime_sky_model_clears_stale_generated_proxy_mnam() {
        const XALG_VISIBLE_DISTANT: u64 = 0x0000_0200;

        let tmp = tempfile::tempdir().unwrap();
        let source_handle_id =
            plugin_handle_new_native("SourceStaleSky.esm", Some("fo76")).expect("new source");
        let target_handle_id =
            plugin_handle_new_native("OutputStaleSky.esm", Some("fo4")).expect("new target");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            let mut source = stat_record(0x0001_827d, r"Sky\Cloud_ValleyFog01_circle.nif");
            source.subrecords.push(xalg_subrecord(XALG_VISIBLE_DISTANT));
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(source_slot, source);

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            let mut target = stat_record_with_flags(
                0x0001_827d,
                r"Sky\Cloud_ValleyFog01_circle.nif",
                FLAG_HAS_DISTANT_LOD,
            );
            write_mnam(
                &mut target,
                &[
                    Some(r"LOD\Generated\FO76\STAT\01827D_LOD_0.nif".to_string()),
                    Some(r"LOD\Generated\FO76\STAT\01827D_LOD_1.nif".to_string()),
                    Some(r"LOD\Generated\FO76\STAT\01827D_LOD_2.nif".to_string()),
                    Some(r"LOD\Generated\FO76\STAT\01827D_LOD_3.nif".to_string()),
                ],
            );
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(target_slot, target);
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 1);
        assert_eq!(report.assets_written, 0);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0001_827d);
        assert!(mnam.is_none());
        assert_eq!(flags & FLAG_HAS_DISTANT_LOD, 0);

        plugin_handle_close_native(source_handle_id);
        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn unmarked_source_clears_stale_generated_proxy_mnam() {
        let tmp = tempfile::tempdir().unwrap();
        let source_handle_id =
            plugin_handle_new_native("SourceUnmarked.esm", Some("fo76")).expect("new source");
        let target_handle_id =
            plugin_handle_new_native("OutputStaleProxy.esm", Some("fo4")).expect("new target");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                source_slot,
                stat_record(0x0001_bc2b, r"Landscape\Rocks\MtnTopStackedBoulder02.nif"),
            );

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            let mut target = stat_record_with_flags(
                0x0001_bc2b,
                r"Landscape\Rocks\MtnTopStackedBoulder02.nif",
                FLAG_HAS_DISTANT_LOD,
            );
            write_mnam(
                &mut target,
                &[
                    Some(r"LOD\Generated\FO76\STAT\01BC2B_LOD_0.nif".to_string()),
                    Some(r"LOD\Generated\FO76\STAT\01BC2B_LOD_1.nif".to_string()),
                    Some(r"LOD\Generated\FO76\STAT\01BC2B_LOD_2.nif".to_string()),
                    Some(r"LOD\Generated\FO76\STAT\01BC2B_LOD_3.nif".to_string()),
                ],
            );
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(target_slot, target);
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 1);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0001_bc2b);
        assert!(mnam.is_none());
        assert_eq!(flags & FLAG_HAS_DISTANT_LOD, 0);

        plugin_handle_close_native(source_handle_id);
        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn stale_generated_scol_mnam_is_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let source_handle_id =
            plugin_handle_new_native("SourceScol.esm", Some("fo76")).expect("new source");
        let target_handle_id =
            plugin_handle_new_native("OutputScol.esm", Some("fo4")).expect("new target");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                source_slot,
                base_record(
                    "SCOL",
                    0x0084_274b,
                    r"SCOL\SeventySix.esm\CM0084274B.NIF",
                    0,
                ),
            );

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            let mut target = base_record(
                "SCOL",
                0x0084_274b,
                r"SCOL\SeventySix.esm\CM0084274B.NIF",
                FLAG_HAS_DISTANT_LOD,
            );
            write_mnam(
                &mut target,
                &[
                    Some(r"LOD\Generated\FO76\SCOL\84274B_LOD_0.nif".to_string()),
                    Some(r"LOD\Generated\FO76\SCOL\84274B_LOD_1.nif".to_string()),
                    Some(r"LOD\Generated\FO76\SCOL\84274B_LOD_2.nif".to_string()),
                    Some(r"LOD\Generated\FO76\SCOL\84274B_LOD_3.nif".to_string()),
                ],
            );
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(target_slot, target);
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 1);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0084_274b);
        assert!(mnam.is_none());
        assert_eq!(flags & FLAG_HAS_DISTANT_LOD, 0);

        plugin_handle_close_native(source_handle_id);
        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn source_header_distant_lod_flag_uses_full_model_proxy_when_target_flag_was_stripped() {
        let tmp = tempfile::tempdir().unwrap();
        write_full_model_fixture(tmp.path(), r"Architecture\SkiResort\SkiResort.nif", 8);
        let source_handle_id =
            plugin_handle_new_native("SourceHeaderFlag.esm", Some("fo76")).expect("new source");
        let target_handle_id =
            plugin_handle_new_native("OutputHeaderFlag.esm", Some("fo4")).expect("new target");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                source_slot,
                stat_record_with_flags(
                    0x0000_bc2a,
                    r"Architecture\SkiResort\SkiResort.nif",
                    FLAG_HAS_DISTANT_LOD,
                ),
            );

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                target_slot,
                stat_record(0x0000_bc2a, r"Architecture\SkiResort\SkiResort.nif"),
            );
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 1);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0000_bc2a);
        let mnam = mnam.expect("source header distant-LOD flag should synthesize proxy MNAM");
        assert_ne!(flags & FLAG_HAS_DISTANT_LOD, 0);
        for level in 0..4 {
            let slot = read_zstring(&mnam[MNAM_SLOT * level..MNAM_SLOT * (level + 1)]);
            assert_eq!(
                slot,
                format!(r"LOD\Generated\FO76\STAT\00BC2A_LOD_{level}.nif")
            );
        }

        plugin_handle_close_native(source_handle_id);
        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn source_local_id_matches_target_full_form_id_for_proxy_lod() {
        let tmp = tempfile::tempdir().unwrap();
        write_full_model_fixture(tmp.path(), r"Architecture\SkiResort\SkiResort.nif", 8);
        let source_handle_id =
            plugin_handle_new_native("SourceLocalId.esm", Some("fo76")).expect("new source");
        let target_handle_id =
            plugin_handle_new_native("OutputFullId.esm", Some("fo4")).expect("new target");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            let mut source = stat_record(0x0000_bc2a, r"Architecture\SkiResort\SkiResort.nif");
            source.subrecords.push(xalg_subrecord(XALG_VISIBLE_DISTANT));
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(source_slot, source);

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                target_slot,
                stat_record(0x0700_bc2a, r"Architecture\SkiResort\SkiResort.nif"),
            );
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 1);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0700_bc2a);
        let mnam = mnam.expect("target full FormID should match source local object id");
        assert_ne!(flags & FLAG_HAS_DISTANT_LOD, 0);
        assert_eq!(
            read_zstring(&mnam[MNAM_SLOT..MNAM_SLOT * 2]),
            r"LOD\Generated\FO76\STAT\00BC2A_LOD_1.nif"
        );

        plugin_handle_close_native(source_handle_id);
        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn has_distant_lod_flag_uses_full_model_proxy_when_no_lod_mesh() {
        let tmp = tempfile::tempdir().unwrap();
        write_full_model_fixture(tmp.path(), r"architecture\bunkers\BunExtMidTower02.nif", 8);
        let handle_id = plugin_handle_new_native("OutputFlag.esm", Some("fo4")).expect("new");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                slot,
                stat_record_with_flags(
                    0x003b_f623,
                    r"architecture\bunkers\BunExtMidTower02.nif",
                    FLAG_HAS_DISTANT_LOD,
                ),
            );
        }

        let report = run_phase_on(handle_id, tmp.path());
        assert_eq!(report.records_changed, 1);

        let (mnam, flags) = mnam_and_flags(handle_id, 0x003b_f623);
        let mnam = mnam.expect("distant flag should keep the record in object LOD");
        assert_ne!(flags & FLAG_HAS_DISTANT_LOD, 0);
        assert_eq!(
            read_zstring(&mnam[..MNAM_SLOT]),
            r"LOD\Generated\FO76\STAT\3BF623_LOD_0.nif"
        );
        assert_eq!(
            read_zstring(&mnam[MNAM_SLOT * 3..MNAM_SLOT * 4]),
            r"LOD\Generated\FO76\STAT\3BF623_LOD_3.nif"
        );

        plugin_handle_close_native(handle_id);
    }

    #[test]
    fn source_xalg_never_visible_distant_suppresses_full_model_proxy() {
        let tmp = tempfile::tempdir().unwrap();
        let source_handle_id =
            plugin_handle_new_native("SourceNeverXalg.esm", Some("fo76")).expect("new source");
        let target_handle_id =
            plugin_handle_new_native("OutputNeverXalg.esm", Some("fo4")).expect("new target");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            let mut source = stat_record(0x0000_bc2a, r"Architecture\SkiResort\SkiResort.nif");
            source.subrecords.push(xalg_subrecord(
                XALG_VISIBLE_DISTANT | XALG_NEVER_VISIBLE_DISTANT,
            ));
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(source_slot, source);

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                target_slot,
                stat_record(0x0000_bc2a, r"Architecture\SkiResort\SkiResort.nif"),
            );
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 0);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0000_bc2a);
        assert!(mnam.is_none());
        assert_eq!(flags & FLAG_HAS_DISTANT_LOD, 0);

        plugin_handle_close_native(source_handle_id);
        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn source_xalg_never_visible_distant_suppresses_source_header_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let source_handle_id =
            plugin_handle_new_native("SourceNeverHeader.esm", Some("fo76")).expect("new source");
        let target_handle_id =
            plugin_handle_new_native("OutputNeverHeader.esm", Some("fo4")).expect("new target");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            let mut source = stat_record_with_flags(
                0x0000_bc2a,
                r"Architecture\SkiResort\SkiResort.nif",
                FLAG_HAS_DISTANT_LOD,
            );
            source
                .subrecords
                .push(xalg_subrecord(XALG_NEVER_VISIBLE_DISTANT));
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(source_slot, source);

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                target_slot,
                stat_record(0x0000_bc2a, r"Architecture\SkiResort\SkiResort.nif"),
            );
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 0);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0000_bc2a);
        assert!(mnam.is_none());
        assert_eq!(flags & FLAG_HAS_DISTANT_LOD, 0);

        plugin_handle_close_native(source_handle_id);
        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn source_xalg_never_visible_distant_clears_target_header_flag() {
        let tmp = tempfile::tempdir().unwrap();
        write_full_model_fixture(tmp.path(), r"Landscape\Trees\MtnTopRedPineStump01.nif", 8);
        let source_handle_id = plugin_handle_new_native("SourceNeverTargetFlag.esm", Some("fo76"))
            .expect("new source");
        let target_handle_id =
            plugin_handle_new_native("OutputNeverTargetFlag.esm", Some("fo4")).expect("new target");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source_slot = store.get_mut(&source_handle_id).unwrap();
            let mut source = stat_record(0x0004_0208, r"Landscape\Trees\MtnTopRedPineStump01.nif");
            source
                .subrecords
                .push(xalg_subrecord(XALG_NEVER_VISIBLE_DISTANT));
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(source_slot, source);

            let target_slot = store.get_mut(&target_handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                target_slot,
                stat_record_with_flags(
                    0x0004_0208,
                    r"Landscape\Trees\MtnTopRedPineStump01.nif",
                    FLAG_HAS_DISTANT_LOD,
                ),
            );
        }

        let report = run_phase_on_with_source(target_handle_id, source_handle_id, tmp.path());
        assert_eq!(report.records_changed, 1);
        assert_eq!(report.assets_written, 0);

        let (mnam, flags) = mnam_and_flags(target_handle_id, 0x0004_0208);
        assert!(mnam.is_none());
        assert_eq!(flags & FLAG_HAS_DISTANT_LOD, 0);

        plugin_handle_close_native(source_handle_id);
        plugin_handle_close_native(target_handle_id);
    }

    #[test]
    fn multi_level_lod_fills_all_slots_from_present_meshes() {
        let tmp = tempfile::tempdir().unwrap();
        write_lod_fixture(tmp.path(), "dlc03/lod/architecture/barn/barn01_lod_0.nif");
        write_lod_fixture(tmp.path(), "dlc03/lod/architecture/barn/barn01_lod_1.nif");

        let handle_id = plugin_handle_new_native("Output2.esm", Some("fo4")).expect("new handle");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle_id).unwrap();
            esp_authoring_core::plugin_runtime::insert_parsed_record_in_slot(
                slot,
                stat_record(0x0000_0810, "DLC03\\Architecture\\Barn\\Barn01.nif"),
            );
        }

        let report = run_phase_on(handle_id, tmp.path());
        assert_eq!(report.records_changed, 1);

        let (mnam, _) = mnam_and_flags(handle_id, 0x0000_0810);
        let mnam = mnam.expect("MNAM present");
        assert_eq!(
            read_zstring(&mnam[..MNAM_SLOT]),
            "DLC03\\LOD\\Architecture\\Barn\\Barn01_LOD_0.nif"
        );
        assert_eq!(
            read_zstring(&mnam[MNAM_SLOT..MNAM_SLOT * 2]),
            "DLC03\\LOD\\Architecture\\Barn\\Barn01_LOD_1.nif"
        );
        assert_eq!(
            read_zstring(&mnam[MNAM_SLOT * 2..MNAM_SLOT * 3]),
            "DLC03\\LOD\\Architecture\\Barn\\Barn01_LOD_1.nif"
        );
        assert_eq!(
            read_zstring(&mnam[MNAM_SLOT * 3..MNAM_SLOT * 4]),
            "DLC03\\LOD\\Architecture\\Barn\\Barn01_LOD_1.nif"
        );

        plugin_handle_close_native(handle_id);
    }
}
