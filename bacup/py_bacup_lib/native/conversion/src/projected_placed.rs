//! Native orchestration of the projected placed-children copy and the
//! worldspace persistent-cell synthesis.
//!
//! Previously Python (`workflows/unified.py`) drove these phases by pulling
//! the full worldspace topology (3.2M+ placed-ref form keys for Appalachia)
//! across the PyO3 boundary as JSON, remapping it in Python, and pushing it
//! back — ~400s of marshalling around ~13s of native mutation, and the
//! persistent phase repeated the identical source scan. Here the topology
//! never leaves Rust: one collection feeds both phases (seed keys are cached
//! on the run), the grid routing is native, and the mapper lookup uses the
//! run's own `mapper_state`. The esp-crate mutation kernels
//! (`copy_cell_slice_children_payload` / `synthesize_worldspace_persistent_cell_payload`)
//! are the same production copy path as before — only the transport moved.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use esp_authoring_core::plugin_runtime::{
    CellChildrenPayload, CellSliceInsertPayload, CellSliceRootsPayload,
    SynthesizePersistentCellPayload, collect_cell_grid_form_keys, collect_cell_slice_roots_payload,
    collect_worldspace_persistent_base_keys, copy_cell_slice_children_payload,
    synthesize_worldspace_persistent_cell_payload,
};
use pyo3::PyResult;
use pyo3::exceptions::PyValueError;

use crate::ids::FormKey;
use crate::run::{RunError, with_run};
use crate::source_read::form_key_to_read_str;

/// Seed form keys cached by the copy phase for the persistent-cell phase
/// (same worldspace, same bounds), so the second full-worldspace source scan
/// is skipped. Consumed (removed) by the persistent phase.
pub struct ProjectedSeedCacheEntry {
    pub bounds: (i32, i32, i32, i32),
    pub seed_keys: Vec<String>,
}

#[derive(serde::Serialize)]
struct CopyProjectedReport {
    /// None when the worldspace had no non-empty grid-cell children (the
    /// Python caller logs "no placed children found" for this case).
    copy: Option<CellSliceInsertPayload>,
    placed: usize,
    source_cells: usize,
    matched_cells: usize,
    collect_warnings: Vec<String>,
    timing: BTreeMap<String, u128>,
}

#[derive(serde::Serialize)]
struct SynthesizeProjectedReport {
    #[serde(flatten)]
    synth: SynthesizePersistentCellPayload,
    seed_cache_hit: bool,
    orchestration_timing: BTreeMap<String, u128>,
}

/// Rust port of unified.py's `_target_form_key_for_preserved_source`: keep the
/// object id, rewrite the plugin to the output plugin (preserve-by-default).
fn preserved_target_form_key(source_form_key: &str, target_plugin_name: &str) -> String {
    let Some((_plugin, object_id)) = source_form_key.rsplit_once(':') else {
        return source_form_key.to_string();
    };
    match u32::from_str_radix(object_id, 16) {
        Ok(raw) => format!("{}:{:06X}", target_plugin_name, raw & 0x00FF_FFFF),
        Err(_) => source_form_key.to_string(),
    }
}

/// Dedup-preserving concatenation of the seed lists, in the same order the
/// Python orchestration built `map_sources` (static, leveled, keyword, layer,
/// region, placed).
fn build_map_sources(roots: &CellSliceRootsPayload) -> Vec<String> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut out = Vec::new();
    for list in [
        &roots.static_base_form_keys,
        &roots.leveled_base_entry_form_keys,
        &roots.linked_ref_keyword_form_keys,
        &roots.layer_form_keys,
        &roots.region_form_keys,
        &roots.placed_form_keys,
    ] {
        for key in list {
            if seen.insert(key.as_str()) {
                out.push(key.clone());
            }
        }
    }
    out
}

fn run_handles(run_id: u64) -> PyResult<(u64, u64)> {
    with_run(run_id, |run| {
        Ok::<_, RunError>((run.source_handle_id, run.target_handle_id))
    })
    .map_err(|err| PyValueError::new_err(format!("projected placed: run lookup failed: {err:?}")))
}

fn cell_slice_form_key_map_key(plugin: &str, local: u32) -> String {
    format!(
        "{}:{:06X}",
        plugin.trim().to_ascii_lowercase(),
        local & 0x00FF_FFFF
    )
}

/// Same lookup semantics as `conversion_run_form_key_map`: unparseable keys
/// and unmapped keys are skipped, not errors.
fn lookup_form_key_map(run_id: u64, sources: &[String]) -> PyResult<BTreeMap<String, String>> {
    with_run(run_id, |run| {
        let Some(state) = run.mapper_state.as_ref() else {
            return Ok::<_, RunError>(BTreeMap::new());
        };
        let mut out = BTreeMap::new();
        for text in sources {
            let Some((plugin, hex)) = text.rsplit_once(':') else {
                continue;
            };
            let Ok(local) = u32::from_str_radix(hex.trim(), 16) else {
                continue;
            };
            let source_fk = FormKey {
                local,
                plugin: run.interner.intern(plugin.trim()),
            };
            let Some(target_fk) = state.source_to_target.get(&source_fk) else {
                continue;
            };
            let source_key = form_key_to_read_str(&source_fk, &run.interner);
            let target_key = form_key_to_read_str(target_fk, &run.interner);
            if !source_key.is_empty() && !target_key.is_empty() {
                out.insert(cell_slice_form_key_map_key(plugin, local), target_key);
            }
        }
        Ok(out)
    })
    .map_err(|err| PyValueError::new_err(format!("projected placed: form key map failed: {err:?}")))
}

pub fn copy_projected_placed_children(
    run_id: u64,
    source_worldspace: &str,
    bounds: (i32, i32, i32, i32),
    offset: (f32, f32, f32),
    worker_count: Option<usize>,
) -> PyResult<String> {
    let (source_handle_id, target_handle_id) = run_handles(run_id)?;
    let mut timing: BTreeMap<String, u128> = BTreeMap::new();

    // include_worldspace_persistent_cell=true (the legacy copy collected with
    // false) so the cached seeds also cover the persistent-cell phase; the
    // persistent entry is excluded from the grid routing below, so copy
    // behavior is unchanged.
    let t = Instant::now();
    let roots = collect_cell_slice_roots_payload(
        source_handle_id,
        source_worldspace,
        bounds.0,
        bounds.1,
        bounds.2,
        bounds.3,
        true,
        worker_count,
    )?;
    timing.insert(
        "collect_source_roots_ms".to_string(),
        t.elapsed().as_millis(),
    );

    let t = Instant::now();
    let (target_plugin_name, target_by_grid) = collect_cell_grid_form_keys(target_handle_id)?;
    timing.insert(
        "collect_target_grids_ms".to_string(),
        t.elapsed().as_millis(),
    );

    let seed_keys = build_map_sources(&roots);
    with_run(run_id, |run| {
        run.projected_seed_cache.insert(
            source_worldspace.to_string(),
            ProjectedSeedCacheEntry {
                bounds,
                seed_keys: seed_keys.clone(),
            },
        );
        Ok::<_, RunError>(())
    })
    .map_err(|err| {
        PyValueError::new_err(format!("projected placed: seed cache failed: {err:?}"))
    })?;

    let t = Instant::now();
    let form_key_map = lookup_form_key_map(run_id, &seed_keys)?;
    timing.insert("form_key_map_ms".to_string(), t.elapsed().as_millis());

    let mut children_by_target_cell: BTreeMap<String, CellChildrenPayload> = BTreeMap::new();
    let mut source_cells = 0usize;
    let mut matched_cells = 0usize;
    let mut placed = 0usize;
    for (cell_key, sections) in &roots.cell_children {
        if roots.worldspace_persistent_children_key.as_deref() == Some(cell_key.as_str()) {
            continue;
        }
        if sections.persistent.is_empty() && sections.temporary.is_empty() {
            continue;
        }
        source_cells += 1;
        placed += sections.persistent.len() + sections.temporary.len();
        let grid_match = roots
            .cell_grids
            .get(cell_key)
            .and_then(|grid| target_by_grid.get(&(grid.x, grid.y)))
            .cloned();
        if grid_match.is_some() {
            matched_cells += 1;
        }
        let target_cell_key =
            grid_match.unwrap_or_else(|| preserved_target_form_key(cell_key, &target_plugin_name));
        let entry = children_by_target_cell.entry(target_cell_key).or_default();
        entry.persistent.extend(
            sections
                .persistent
                .iter()
                .map(|key| preserved_target_form_key(key, &target_plugin_name)),
        );
        entry.temporary.extend(
            sections
                .temporary
                .iter()
                .map(|key| preserved_target_form_key(key, &target_plugin_name)),
        );
    }

    let copy = if children_by_target_cell.is_empty() {
        None
    } else {
        let t = Instant::now();
        let payload = copy_cell_slice_children_payload(
            source_handle_id,
            target_handle_id,
            children_by_target_cell,
            offset,
            form_key_map,
        )?;
        timing.insert("native_copy_ms".to_string(), t.elapsed().as_millis());
        Some(payload)
    };

    let report = CopyProjectedReport {
        copy,
        placed,
        source_cells,
        matched_cells,
        collect_warnings: roots.warnings,
        timing,
    };
    serde_json::to_string(&report)
        .map_err(|err| PyValueError::new_err(format!("encode projected placed copy report: {err}")))
}

pub fn synthesize_projected_persistent_cell(
    run_id: u64,
    source_worldspace: &str,
    full_bounds: (i32, i32, i32, i32),
    offset: (f32, f32, f32),
    worker_count: Option<usize>,
) -> PyResult<String> {
    let (source_handle_id, target_handle_id) = run_handles(run_id)?;
    let mut timing: BTreeMap<String, u128> = BTreeMap::new();

    let t = Instant::now();
    let cached = with_run(run_id, |run| {
        Ok::<_, RunError>(run.projected_seed_cache.remove(source_worldspace))
    })
    .map_err(|err| {
        PyValueError::new_err(format!("projected placed: run lookup failed: {err:?}"))
    })?;
    let (mut map_sources, seed_cache_hit) = match cached {
        Some(entry) if entry.bounds == full_bounds => (entry.seed_keys, true),
        _ => {
            let roots = collect_cell_slice_roots_payload(
                source_handle_id,
                source_worldspace,
                full_bounds.0,
                full_bounds.1,
                full_bounds.2,
                full_bounds.3,
                true,
                worker_count,
            )?;
            (build_map_sources(&roots), false)
        }
    };
    timing.insert(
        "collect_source_roots_ms".to_string(),
        t.elapsed().as_millis(),
    );

    // Persistent refs whose base is a non-placement type (IDLM/TERM/BOOK/…)
    // are outside the seed collector's signature set — seed EVERY persistent
    // base so each maps to its translated objid (see the persistent-base-keys
    // collector doc).
    let t = Instant::now();
    let all_base_keys =
        collect_worldspace_persistent_base_keys(source_handle_id, source_worldspace, None)?;
    timing.insert("collect_base_keys_ms".to_string(), t.elapsed().as_millis());
    // map_sources is already deduped (build_map_sources); append only the
    // base keys it doesn't cover.
    let mut seen: BTreeSet<String> = map_sources.iter().cloned().collect();
    for key in all_base_keys {
        if seen.insert(key.clone()) {
            map_sources.push(key);
        }
    }

    let t = Instant::now();
    let form_key_map = lookup_form_key_map(run_id, &map_sources)?;
    timing.insert("form_key_map_ms".to_string(), t.elapsed().as_millis());

    let t = Instant::now();
    let synth = synthesize_worldspace_persistent_cell_payload(
        source_handle_id,
        target_handle_id,
        source_worldspace,
        offset,
        form_key_map,
    )?;
    timing.insert("native_synth_ms".to_string(), t.elapsed().as_millis());

    let report = SynthesizeProjectedReport {
        synth,
        seed_cache_hit,
        orchestration_timing: timing,
    };
    serde_json::to_string(&report)
        .map_err(|err| PyValueError::new_err(format!("encode projected persistent report: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserved_target_form_key_masks_objid_and_rewrites_plugin() {
        assert_eq!(
            preserved_target_form_key("SeventySix.esm:03ABCD", "Out.esm"),
            "Out.esm:03ABCD"
        );
        // Master-index prefix beyond 0x00FFFFFF is masked off.
        assert_eq!(
            preserved_target_form_key("SeventySix.esm:FF03ABCD", "Out.esm"),
            "Out.esm:03ABCD"
        );
        // Unparseable inputs pass through unchanged (parity with the removed
        // Python _target_form_key_for_preserved_source).
        assert_eq!(
            preserved_target_form_key("no-colon-here", "Out.esm"),
            "no-colon-here"
        );
        assert_eq!(
            preserved_target_form_key("Plugin.esm:notahex", "Out.esm"),
            "Plugin.esm:notahex"
        );
    }

    #[test]
    fn build_map_sources_dedups_in_seed_list_order() {
        let mut roots = CellSliceRootsPayload::default();
        roots.static_base_form_keys = vec!["A:000001".into(), "A:000002".into()];
        roots.leveled_base_entry_form_keys = vec!["A:000002".into(), "A:000003".into()];
        roots.region_form_keys = vec!["A:000001".into()];
        roots.placed_form_keys = vec!["A:000004".into()];
        assert_eq!(
            build_map_sources(&roots),
            vec![
                "A:000001".to_string(),
                "A:000002".to_string(),
                "A:000003".to_string(),
                "A:000004".to_string(),
            ]
        );
    }

    #[test]
    fn form_key_map_keys_match_cell_slice_normalization() {
        assert_eq!(
            cell_slice_form_key_map_key("SeventySix.esm", 0x0100_0034),
            "seventysix.esm:000034"
        );
    }
}
