//! PyO3 boundary for the conversion module.
//!
//! Exposes:
//! - `conversion_run_create`         — create a `ConversionRun`, return its run ID.
//! - `conversion_run_drop`           — drop a `ConversionRun` by ID.
//! - `conversion_run_drain_decisions` — drain and return decisions as compact rows.
//! - `conversion_run_drain_warnings`  — drain and return warnings as Python list of str.
//! - `conversion_run_translate_all`   — translate every record in the source plugin.

use crate::full_plugin::{AssetPhaseFlags, WarningPolicy};
use crate::ids::SigCode;
use crate::record::{FieldValue, Record};
use crate::run::{
    OwnedPluginHandle, OwnedRunHandles, RunConfig, RunError, RunParams, TargetMode,
    TargetRecordPreflightRow, TranslateStats, create_owned_run, create_run, drop_run, with_run,
};
use crate::source_read::{form_key_to_read_str, iter_form_keys_of_sig, read_record};
use crate::sym::StringInterner;
use crate::target_write::NavmeshFinalizeStats;
use crate::target_write::diagnose_navmesh_links_in_slot_native;
use crate::translator::Game;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyList};
use rayon::prelude::*;
use serde_json::Value;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Panic barrier — converts native panics to PyRuntimeError instead of aborting
// ---------------------------------------------------------------------------

fn run_with_panic_catch<R>(label: &'static str, f: impl FnOnce() -> PyResult<R>) -> PyResult<R> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(panic_payload) => {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&'static str>() {
                format!("Native panic in {label}: {s}")
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                format!("Native panic in {label}: {s}")
            } else {
                format!("Native panic in {label}: <unknown payload>")
            };
            Err(PyRuntimeError::new_err(msg))
        }
    }
}

fn conversion_worker_count(run_id: u64) -> Result<Option<usize>, RunError> {
    with_run(run_id, |run| {
        Ok::<Option<usize>, RunError>(run.config.conversion_workers.filter(|workers| *workers > 0))
    })
}

fn with_conversion_worker_pool<R: Send>(
    run_id: u64,
    label: &'static str,
    operation: impl FnOnce() -> Result<R, RunError> + Send,
) -> Result<R, RunError> {
    let Some(workers) = conversion_worker_count(run_id)? else {
        return operation();
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(|err| RunError::InvalidConfig(format!("{label}: rayon pool error: {err}")))?;
    pool.install(operation)
}

#[pyfunction(name = "conversion_diagnose_navmesh_links")]
pub fn diagnose_navmesh_links_py<'py>(
    py: Python<'py>,
    plugin_path: &str,
    game: &str,
) -> PyResult<Bound<'py, PyAny>> {
    run_with_panic_catch("conversion_diagnose_navmesh_links", || {
        use pyo3::IntoPyObjectExt;
        let plugin_path = PathBuf::from(plugin_path);
        let game = game.to_string();
        let stats = py
            .detach(move || diagnose_navmesh_links_for_path(&plugin_path, &game))
            .map_err(PyValueError::new_err)?;
        let out = PyList::empty(py);
        out.append(stats.navmeshes_seen)?;
        out.append(stats.navmeshes_touched)?;
        out.append(stats.bad_internal_links)?;
        out.append(stats.linked_edge_vertex_mismatches)?;
        out.append(stats.opposite_normal_linked_pairs)?;
        out.append(stats.missing_internal_links)?;
        out.append(stats.same_direction_internal_edges)?;
        out.append(stats.ambiguous_local_edges)?;
        out.append(stats.external_links_added)?;
        out.append(stats.missing_external_links)?;
        out.append(stats.ambiguous_external_edges)?;
        out.append(stats.external_link_caps_hit)?;
        out.append(stats.winding_conflicts)?;
        out.append(stats.residual_warning_count())?;
        out.into_bound_py_any(py)
    })
}

fn diagnose_navmesh_links_for_path(
    plugin_path: &Path,
    game: &str,
) -> Result<NavmeshFinalizeStats, String> {
    let handle =
        OwnedPluginHandle::load(plugin_path, game, None).map_err(|error| error.to_string())?;
    diagnose_navmesh_links_in_slot_native(handle.id()).map_err(|error| error.to_string())
}

/// Parse a "PluginName:XXXXXX" form_key string into a `conversion::ids::FormKey`.
fn parse_form_key_str(s: &str, interner: &StringInterner) -> PyResult<crate::ids::FormKey> {
    if let Some((plugin, hex)) = s.rsplit_once(':') {
        if let Ok(local) = u32::from_str_radix(hex.trim(), 16) {
            let sym = interner.intern(plugin.trim());
            return Ok(crate::ids::FormKey { local, plugin: sym });
        }
    }
    Err(PyValueError::new_err(format!(
        "cannot parse form_key: {s:?} (expected 'PluginName:XXXXXX')"
    )))
}

fn is_hex_form_id(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty() && text.len() <= 8 && text.chars().all(|c| c.is_ascii_hexdigit())
}

fn normalize_translate_form_key(s: &str) -> Option<String> {
    if s.contains('@') {
        return Some(s.to_string());
    }
    let (left, right) = s.rsplit_once(':')?;
    if is_hex_form_id(right) {
        return Some(format!("{}@{}", right.trim(), left.trim()));
    }
    if is_hex_form_id(left) {
        return Some(format!("{}@{}", left.trim(), right.trim()));
    }
    None
}

fn field_value_string(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(sym) => interner.resolve(*sym).map(str::to_string),
        FieldValue::Struct(fields) => fields.iter().find_map(|(key, value)| {
            let key = interner.resolve(*key).unwrap_or_default();
            if key.eq_ignore_ascii_case("filename") || key.to_ascii_lowercase().contains("filename")
            {
                field_value_string(value, interner)
            } else {
                None
            }
        }),
        _ => None,
    }
}

fn field_value_scalar_text(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(sym) => interner.resolve(*sym).map(str::to_string),
        FieldValue::Int(value) => Some(value.to_string()),
        FieldValue::Uint(value) => Some(value.to_string()),
        FieldValue::Float(value) => Some(value.to_string()),
        FieldValue::Bool(value) => Some(value.to_string()),
        FieldValue::Struct(fields) => fields.iter().find_map(|(key, value)| {
            let key = interner.resolve(*key).unwrap_or_default();
            let normalized = key.to_ascii_lowercase();
            if normalized.contains("animationtype") || normalized == "animtype" {
                field_value_scalar_text(value, interner)
            } else {
                None
            }
        }),
        _ => None,
    }
}

fn classify_weapon_role(anim_type: &str) -> String {
    if anim_type.trim() == "1" {
        return "melee".to_string();
    }
    let text = anim_type.trim().to_ascii_lowercase();
    if text.is_empty() {
        return String::new();
    }
    if text.contains("melee") || text == "unarmed" || text == "handtohand" {
        "melee".to_string()
    } else {
        "gun".to_string()
    }
}

fn weapon_metadata_from_record(
    source_form_key: String,
    record: &Record,
    interner: &StringInterner,
) -> (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
) {
    let editor_id = record
        .eid
        .and_then(|eid| interner.resolve(eid))
        .unwrap_or_default()
        .to_string();
    let mut base_model = String::new();
    let mut model_mod1 = String::new();
    let mut model_mod2 = String::new();
    let mut model_mod3 = String::new();
    let mut anim_type = String::new();
    let mut has_ammo = false;

    for field in &record.fields {
        match field.sig.as_str() {
            "MODL" => {
                base_model = field_value_string(&field.value, interner).unwrap_or_default();
            }
            "MOD2" => {
                model_mod1 = field_value_string(&field.value, interner).unwrap_or_default();
            }
            "MOD3" => {
                model_mod2 = field_value_string(&field.value, interner).unwrap_or_default();
            }
            "MOD4" => {
                model_mod3 = field_value_string(&field.value, interner).unwrap_or_default();
            }
            "AMMO" => has_ammo = true,
            "DATA" | "DNAM" => {
                if anim_type.is_empty() {
                    anim_type = field_value_scalar_text(&field.value, interner).unwrap_or_default();
                }
            }
            _ => {}
        }
    }

    let role = classify_weapon_role(&anim_type);
    let ammo_decision = if has_ammo { "unknown" } else { "no_ammo_field" };
    (
        source_form_key,
        editor_id,
        base_model,
        model_mod1,
        model_mod2,
        model_mod3,
        role,
        ammo_decision.to_string(),
        anim_type,
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn run_error_to_py(e: RunError) -> PyErr {
    PyRuntimeError::new_err(format!("{e}"))
}

fn config_object_from_json(config_json: &str) -> PyResult<Value> {
    if config_json.trim().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    let value: Value = serde_json::from_str(config_json)
        .map_err(|e| PyValueError::new_err(format!("invalid config json: {e}")))?;
    match value {
        Value::Object(_) => Ok(value),
        _ => Err(PyValueError::new_err("config json must be an object")),
    }
}

fn optional_path(config: &Value, key: &str) -> Option<PathBuf> {
    config
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
}

fn optional_usize(config: &Value, key: &str) -> Option<usize> {
    config
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn optional_u32(config: &Value, key: &str) -> Option<u32> {
    config
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn bool_from_value(config: &Value, key: &str, default: bool) -> bool {
    config.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn asset_phase_flags_from_value(config: &Value) -> AssetPhaseFlags {
    let Some(phases) = config.get("asset_phases").filter(|value| value.is_object()) else {
        return AssetPhaseFlags::default();
    };
    AssetPhaseFlags {
        terrain: bool_from_value(phases, "terrain", false),
        nifs: bool_from_value(phases, "nifs", false),
        textures: bool_from_value(phases, "textures", false),
        materials: bool_from_value(phases, "materials", false),
        havok: bool_from_value(phases, "havok", false),
        animations: bool_from_value(phases, "animations", false),
        sounds: bool_from_value(phases, "sounds", false),
    }
}

fn target_record_preflight_from_value(config: &Value) -> Vec<TargetRecordPreflightRow> {
    let Some(items) = config
        .get("target_record_preflight")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for item in items {
        if let Some(row) = target_record_preflight_row_from_value(item) {
            rows.push(row);
        }
    }
    rows
}

fn target_record_preflight_row_from_value(item: &Value) -> Option<TargetRecordPreflightRow> {
    if let Some(items) = item.as_array() {
        if items.len() != 3 {
            return None;
        }
        return Some(TargetRecordPreflightRow {
            editor_id: items[0].as_str()?.to_string(),
            signature: items[1].as_str()?.to_string(),
            form_key: items[2].as_str()?.to_string(),
        });
    }
    let object = item.as_object()?;
    Some(TargetRecordPreflightRow {
        editor_id: object.get("editor_id")?.as_str()?.to_string(),
        signature: object.get("signature")?.as_str()?.to_string(),
        form_key: object.get("form_key")?.as_str()?.to_string(),
    })
}

fn string_list_from_value(config: &Value, key: &str) -> Vec<String> {
    config
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect()
}

fn projected_navmesh_offset_from_value(config: &Value) -> [f32; 3] {
    let Some(items) = config
        .get("projected_navmesh_offset")
        .and_then(Value::as_array)
    else {
        return [0.0, 0.0, 0.0];
    };
    if items.len() != 3 {
        return [0.0, 0.0, 0.0];
    }
    [
        items[0].as_f64().unwrap_or(0.0) as f32,
        items[1].as_f64().unwrap_or(0.0) as f32,
        items[2].as_f64().unwrap_or(0.0) as f32,
    ]
}

fn config_from_json(config_json: &str) -> PyResult<RunConfig> {
    let config = config_object_from_json(config_json)?;
    let strict_mapper = bool_from_value(&config, "strict_mapper", false);
    let use_base_game_assets = bool_from_value(&config, "use_base_game_assets", false);
    let preserve_source_ids = bool_from_value(&config, "preserve_source_ids", false);
    let output_plugin_name = config
        .get("output_plugin_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let is_whole_plugin = bool_from_value(&config, "is_whole_plugin", false);
    let root_sig = config
        .get("root_sig")
        .and_then(Value::as_str)
        .and_then(|s| SigCode::from_str(s).ok());
    let mod_path = optional_path(&config, "mod_path");
    let source_extracted_dir = optional_path(&config, "source_extracted_dir");
    let projected_navmesh_offset = projected_navmesh_offset_from_value(&config);
    Ok(RunConfig {
        strict_mapper,
        use_base_game_assets,
        preserve_source_ids,
        generated_object_id_floor: optional_u32(&config, "generated_object_id_floor").unwrap_or(0),
        output_plugin_name,
        is_whole_plugin,
        root_sig,
        mod_path,
        source_extracted_dir,
        target_extracted_dir: optional_path(&config, "target_extracted_dir"),
        target_data_dir: optional_path(&config, "target_data_dir"),
        target_asset_catalog_path: optional_path(&config, "target_asset_catalog_path"),
        target_asset_cache_dir: optional_path(&config, "target_asset_cache_dir"),
        conversion_workers: optional_usize(&config, "conversion_workers"),
        records_limit: optional_usize(&config, "records_limit"),
        warning_policy: WarningPolicy::WarnPlayable,
        asset_phases: asset_phase_flags_from_value(&config),
        target_record_preflight: target_record_preflight_from_value(&config),
        target_master_names: string_list_from_value(&config, "target_master_names"),
        base_asset_namespace: config
            .get("base_asset_namespace")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        base_asset_relocation_mesh_roots: string_list_from_value(
            &config,
            "base_asset_relocation_mesh_roots",
        ),
        projected_navmesh_offset,
        skip_record_signatures: string_list_from_value(&config, "skip_record_signatures"),
        defer_placed_child_ref_class: bool_from_value(
            &config,
            "defer_placed_child_ref_class",
            false,
        ),
    })
}

fn translate_stats_to_pyraw<'py>(
    py: Python<'py>,
    stats: &TranslateStats,
) -> PyResult<Bound<'py, PyAny>> {
    use pyo3::IntoPyObjectExt;
    let by_signature = PyList::empty(py);
    for (sig, sig_stats) in &stats.by_signature {
        by_signature.append((
            sig.as_str(),
            sig_stats.seen,
            sig_stats.translated,
            sig_stats.vanilla_remapped,
            sig_stats.dropped,
            sig_stats.deferred,
            sig_stats.failed,
        ))?;
    }
    (
        stats.records_translated,
        stats.records_vanilla_remapped,
        stats.records_dropped,
        stats.records_deferred,
        stats.records_failed,
        by_signature,
    )
        .into_bound_py_any(py)
}

#[pyfunction(name = "conversion_merge_sources")]
pub fn merge_sources_py(py: Python<'_>, opts_json: &str) -> PyResult<String> {
    run_with_panic_catch("conversion_merge_sources", || {
        let opts: crate::merge_sources::MergeOptions = serde_json::from_str(opts_json)
            .map_err(|error| PyValueError::new_err(format!("invalid merge options: {error}")))?;
        let report = py
            .detach(move || crate::merge_sources::run(&opts))
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        serde_json::to_string(&report)
            .map_err(|error| PyRuntimeError::new_err(format!("serialize merge report: {error}")))
    })
}

/// Enumerate the FO76 LOD-by-convention closures for every LOD-capable base
/// record in `handle_id` (STAT/SCOL/MSTT/TREE/FLOR/ACTI). Returns the
/// `(asset_type, source_path, resolved_path)` rows for each existing `_lod[_N].nif`
/// plus its material/texture closure under `source_extracted_dir`. Consumed by
/// `unified.py::_collect_assets_native` so the regen whole-plugin asset waves ship
/// the LOD meshes that `synthesize_object_lod` writes MNAM for.
#[pyfunction(name = "conversion_run_collect_lod_closures")]
pub fn collect_lod_closures_py<'py>(
    py: Python<'py>,
    run_id: u64,
    root_form_keys: Vec<String>,
) -> PyResult<Bound<'py, PyList>> {
    run_with_panic_catch("conversion_run_collect_lod_closures", || {
        let rows: Vec<(String, String, String)> = py.detach(move || {
            with_run(run_id, |run| {
                let source_handle = run.require_source_handle()?;
                let source_dir = run.config.source_extracted_dir.as_deref().ok_or_else(|| {
                    RunError::InvalidConfig(
                        "LOD closure collection requires source_extracted_dir".to_string(),
                    )
                })?;
                crate::phase::lod_assets::enumerate_lod_closures(
                    source_handle,
                    source_dir,
                    &root_form_keys,
                )
                .map_err(RunError::InvalidConfig)
            })
            .map_err(run_error_to_py)
        })?;
        let out = PyList::empty(py);
        for (asset_type, source_path, resolved_path) in rows {
            out.append((asset_type, source_path, resolved_path))?;
        }
        Ok(out)
    })
}

// ---------------------------------------------------------------------------
// ConversionRun lifecycle PyO3 functions
// ---------------------------------------------------------------------------

/// Private Rust-only constructor retained for focused native tests.
pub(crate) fn create_run_from_handles(
    source: Game,
    target: Game,
    source_handle_id: u64,
    target_handle_id: u64,
    master_handle_ids: Vec<u64>,
    config: RunConfig,
) -> Result<u64, RunError> {
    create_run(RunParams {
        source,
        target,
        source_handle_id,
        target_handle_id,
        master_handle_ids,
        config,
    })
}

fn parse_game(value: &str, role: &str) -> PyResult<Game> {
    Game::from_str(value)
        .ok_or_else(|| PyValueError::new_err(format!("unknown {role} game: {value:?}")))
}

fn normalize_fo76_font_aliases_for_fo4(
    strings: &mut esp_authoring_core::plugin_runtime::LocalizedStringsState,
) {
    for table in strings.by_language.values_mut() {
        for text in table.values_mut() {
            if let Some(rewritten) =
                crate::translator::pair_hooks::fo76_fo4::rewritten_fo76_font_aliases_for_fo4(text)
            {
                *text = rewritten;
            }
        }
    }
}

fn seed_target_localization(
    source_handle_id: u64,
    target_handle_id: u64,
    source: Game,
    target: Game,
) -> Result<(), RunError> {
    let mut store = esp_authoring_core::plugin_runtime::plugin_handle_store_ref()
        .lock()
        .map_err(|_| RunError::LockPoisoned)?;
    let mut strings = store
        .get(&source_handle_id)
        .ok_or_else(|| RunError::InvalidConfig("source plugin handle disappeared".to_string()))?
        .strings_ref()
        .clone();
    if source == Game::Fo76 && target == Game::Fo4 {
        normalize_fo76_font_aliases_for_fo4(&mut strings);
    }
    let target = store
        .get_mut(&target_handle_id)
        .ok_or_else(|| RunError::InvalidConfig("target plugin handle disappeared".to_string()))?;
    if !strings.by_language.is_empty() {
        target.parsed.header.flags |= 0x0000_0080;
        target.parsed.header.raw_subrecords.clear();
    }
    *target.strings_mut() = strings;
    Ok(())
}

fn mark_master_target(target_handle_id: u64) -> Result<(), RunError> {
    let mut store = esp_authoring_core::plugin_runtime::plugin_handle_store_ref()
        .lock()
        .map_err(|_| RunError::LockPoisoned)?;
    let target = store
        .get_mut(&target_handle_id)
        .ok_or_else(|| RunError::InvalidConfig("target plugin handle disappeared".to_string()))?;
    target.parsed.header.flags |= 1;
    target.parsed.header.raw_subrecords.clear();
    target.invalidate_sections();
    Ok(())
}

#[pyfunction(name = "conversion_run_create_from_paths")]
#[pyo3(signature = (source_game, target_game, source_plugin_path=None, target_plugin_name=None, target_plugin_path=None, master_plugin_paths=Vec::new(), source_strings_dir=None, config_json="{}"))]
#[allow(clippy::too_many_arguments)]
pub fn create_run_from_paths_py(
    py: Python<'_>,
    source_game: &str,
    target_game: &str,
    source_plugin_path: Option<String>,
    target_plugin_name: Option<String>,
    target_plugin_path: Option<String>,
    master_plugin_paths: Vec<String>,
    source_strings_dir: Option<String>,
    config_json: &str,
) -> PyResult<u64> {
    run_with_panic_catch("conversion_run_create_from_paths", || {
        if target_plugin_name.is_some() == target_plugin_path.is_some() {
            return Err(PyValueError::new_err(
                "exactly one of target_plugin_name or target_plugin_path is required",
            ));
        }
        let source = parse_game(source_game, "source")?;
        let target = parse_game(target_game, "target")?;
        let is_new = target_plugin_name.is_some();
        let mut config = config_from_json(config_json)?;
        py.detach(move || {
            let source_handle = source_plugin_path
                .as_deref()
                .map(|path| {
                    OwnedPluginHandle::load(
                        Path::new(path),
                        source.as_str(),
                        source_strings_dir.as_deref().map(Path::new),
                    )
                })
                .transpose()?;
            if is_new && source == Game::Fo76 && config.generated_object_id_floor == 0 {
                if let Some(source_handle) = source_handle.as_ref() {
                    let max =
                        esp_authoring_core::plugin_runtime::plugin_handle_max_object_id_no_py(
                            source_handle.id(),
                        )
                        .map_err(RunError::InvalidConfig)?;
                    if max != 0 {
                        let requested = max.saturating_add(0x0010_0000);
                        let aligned = requested.saturating_add(0x000F_FFFF) & !0x000F_FFFF;
                        config.generated_object_id_floor = if aligned <= 0x00FF_FFFF {
                            aligned
                        } else if max < 0x00FF_FFFF {
                            max + 1
                        } else {
                            return Err(RunError::InvalidConfig(
                                "FO76 source plugin has no remaining local FormID space"
                                    .to_string(),
                            ));
                        };
                    }
                }
            }

            let (target_handle, default_target_path, is_new) = if let Some(plugin_name) =
                target_plugin_name.as_deref()
            {
                let default_path = config
                    .mod_path
                    .as_deref()
                    .unwrap_or_else(|| Path::new(""))
                    .join(plugin_name);
                (
                    OwnedPluginHandle::new(plugin_name, target.as_str()),
                    default_path,
                    true,
                )
            } else {
                let path = PathBuf::from(target_plugin_path.as_deref().expect("validated mode"));
                (
                    OwnedPluginHandle::load(&path, target.as_str(), None)?,
                    path,
                    false,
                )
            };

            if is_new
                && target_plugin_name
                    .as_deref()
                    .is_some_and(|name| name.to_ascii_lowercase().ends_with(".esm"))
            {
                mark_master_target(target_handle.id())?;
            }
            if is_new && let Some(source_handle) = source_handle.as_ref() {
                seed_target_localization(source_handle.id(), target_handle.id(), source, target)?;
            }

            let mut masters = Vec::with_capacity(master_plugin_paths.len());
            let mut master_names = Vec::with_capacity(master_plugin_paths.len());
            for master_path in &master_plugin_paths {
                let path = Path::new(master_path);
                let master_name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| {
                        RunError::InvalidConfig(format!(
                            "master path has no filename: {}",
                            path.display()
                        ))
                    })?
                    .to_string();
                let size = std::fs::metadata(path).ok().map(|metadata| metadata.len());
                if is_new {
                    esp_authoring_core::plugin_runtime::plugin_handle_add_master_no_py(
                        target_handle.id(),
                        &master_name,
                        size,
                    )
                    .map_err(RunError::InvalidConfig)?;
                }
                master_names.push(master_name);
                masters.push(OwnedPluginHandle::load_index(path, target.as_str())?);
            }
            config.target_master_names = master_names;
            if config.output_plugin_name.is_empty() {
                config.output_plugin_name = default_target_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Output.esp")
                    .to_string();
            }
            create_owned_run(
                source,
                target,
                config,
                OwnedRunHandles {
                    source: source_handle,
                    target: target_handle,
                    masters,
                },
                default_target_path,
                if is_new {
                    TargetMode::CreateNew
                } else {
                    TargetMode::OpenExisting
                },
            )
        })
        .map_err(run_error_to_py)
    })
}

#[pyfunction(name = "conversion_run_set_target_description")]
pub fn set_target_description_py(py: Python<'_>, run_id: u64, text: &str) -> PyResult<()> {
    let text = text.to_string();
    py.detach(move || {
        with_run(run_id, |run| {
            crate::plugin_header::set_tes4_snam(run.target_handle_id, run.target.as_str(), &text)
        })
        .map_err(run_error_to_py)
    })
}

#[pyfunction(name = "conversion_run_script_reference_records")]
pub fn script_reference_records_py<'py>(
    py: Python<'py>,
    run_id: u64,
    subrecord_signatures: Vec<String>,
) -> PyResult<Bound<'py, PyAny>> {
    use pyo3::IntoPyObjectExt;

    let (target_handle, plugin_name) = py.detach(move || {
        with_run(run_id, |run| {
            Ok::<_, RunError>((run.target_handle_id, run.config.output_plugin_name.clone()))
        })
        .map_err(run_error_to_py)
    })?;
    let form_ids =
        esp_authoring_core::plugin_runtime::plugin_handle_record_form_ids_with_subrecords_native(
            py,
            target_handle,
            subrecord_signatures,
        )?;
    let rows = PyList::empty(py);
    for form_id in form_ids {
        let Some((form_id, signature, editor_id)) =
            esp_authoring_core::plugin_runtime::plugin_handle_record_summary_native(
                py,
                target_handle,
                form_id,
            )?
        else {
            continue;
        };
        let Some(subrecords) =
            esp_authoring_core::plugin_runtime::plugin_handle_record_subrecords_native(
                py,
                target_handle,
                form_id,
            )?
        else {
            continue;
        };
        let authoring =
            esp_authoring_core::plugin_runtime::plugin_handle_read_authoring_record_native(
                target_handle,
                form_id,
            )?;
        rows.append((form_id, signature, editor_id, subrecords, authoring))?;
    }
    (plugin_name, rows).into_bound_py_any(py)
}

#[pyfunction(name = "conversion_run_set_record_subrecords")]
pub fn set_record_subrecords_py(
    py: Python<'_>,
    run_id: u64,
    form_id: u32,
    subrecords: Vec<(String, Vec<u8>, Option<String>)>,
) -> PyResult<bool> {
    let target_handle = py.detach(move || {
        with_run(run_id, |run| Ok::<_, RunError>(run.target_handle_id)).map_err(run_error_to_py)
    })?;
    esp_authoring_core::plugin_runtime::plugin_handle_set_record_subrecords_native(
        py,
        target_handle,
        form_id,
        subrecords,
    )
}

#[pyfunction(name = "conversion_run_apply_placed_record_position_offset")]
pub fn apply_placed_record_position_offset_py(
    py: Python<'_>,
    run_id: u64,
    x: f64,
    y: f64,
    z: f64,
) -> PyResult<usize> {
    py.detach(move || {
        let target = with_run(run_id, |run| Ok::<_, RunError>(run.target_handle_id))
            .map_err(run_error_to_py)?;
        esp_authoring_core::plugin_runtime::plugin_handle_apply_placed_record_position_offset_native(
            target, x, y, z,
        )
    })
}

#[pyfunction(name = "conversion_run_sync_cell_regions_from_source")]
pub fn sync_cell_regions_from_source_py(
    py: Python<'_>,
    run_id: u64,
    source_worldspace_editor_id: &str,
    target_worldspace_editor_id: &str,
) -> PyResult<String> {
    let source_worldspace_editor_id = source_worldspace_editor_id.to_string();
    let target_worldspace_editor_id = target_worldspace_editor_id.to_string();
    py.detach(move || {
        let (source, target) = with_run(run_id, |run| {
            Ok::<_, RunError>((run.require_source_handle()?, run.target_handle_id))
        })
        .map_err(run_error_to_py)?;
        esp_authoring_core::plugin_runtime::plugin_handle_sync_cell_regions_from_source_json(
            source,
            target,
            &source_worldspace_editor_id,
            &target_worldspace_editor_id,
        )
    })
}

#[pyfunction(name = "conversion_run_sync_cell_locations_from_lctn")]
pub fn sync_cell_locations_from_lctn_py(py: Python<'_>, run_id: u64) -> PyResult<String> {
    py.detach(move || {
        let target = with_run(run_id, |run| Ok::<_, RunError>(run.target_handle_id))
            .map_err(run_error_to_py)?;
        esp_authoring_core::plugin_runtime::plugin_handle_sync_cell_locations_from_lctn_json(target)
    })
}

#[pyfunction(name = "conversion_run_release_source_handle")]
pub fn release_source_handle_py(py: Python<'_>, run_id: u64) -> PyResult<bool> {
    py.detach(move || {
        with_run(run_id, |run| Ok::<_, RunError>(run.release_source_handle()))
            .map_err(run_error_to_py)
    })
}

#[pyfunction(name = "conversion_run_release_master_handles")]
pub fn release_master_handles_py(py: Python<'_>, run_id: u64) -> PyResult<usize> {
    py.detach(move || {
        with_run(run_id, |run| {
            Ok::<_, RunError>(run.release_master_handles())
        })
        .map_err(run_error_to_py)
    })
}

#[pyfunction(name = "conversion_run_save_target")]
#[pyo3(signature = (run_id, output_path=None, emit_authoring_yaml=false, run_nvnm_validator=true))]
pub fn save_target_py(
    py: Python<'_>,
    run_id: u64,
    output_path: Option<String>,
    emit_authoring_yaml: bool,
    run_nvnm_validator: bool,
) -> PyResult<()> {
    run_with_panic_catch("conversion_run_save_target", || {
        py.detach(move || {
            with_run(run_id, |run| {
                let output_path = output_path
                    .map(PathBuf::from)
                    .or_else(|| run.default_target_path.clone())
                    .ok_or_else(|| {
                        RunError::InvalidConfig("run has no default target path".into())
                    })?;
                if run.target_mode == TargetMode::CreateNew {
                    crate::plugin_header::normalize_target_plugin_header(
                        run.target_handle_id,
                        run.target.as_str(),
                    )?;
                }
                if let Some(parent) = output_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| {
                        RunError::InvalidConfig(format!("create output directory: {error}"))
                    })?;
                }
                esp_authoring_core::plugin_runtime::plugin_handle_save_no_py(
                    run.target_handle_id,
                    &output_path.to_string_lossy(),
                )
                .map_err(RunError::InvalidConfig)?;
                if emit_authoring_yaml {
                    let yaml_dir = run
                        .config
                        .mod_path
                        .clone()
                        .or_else(|| output_path.parent().map(Path::to_path_buf))
                        .unwrap_or_default()
                        .join("yaml");
                    esp_authoring_core::plugin_runtime::export_authoring_dir_from_handle_no_py(
                        run.target_handle_id,
                        &yaml_dir,
                        "yaml",
                    )
                    .map_err(RunError::InvalidConfig)?;
                }
                if run_nvnm_validator {
                    crate::phase::build_esp::emit_navmesh_validation_warnings(
                        run.target_handle_id,
                        "save_target",
                        &run.event_tx,
                    )
                    .map_err(RunError::InvalidConfig)?;
                }
                Ok::<_, RunError>(())
            })
            .map_err(run_error_to_py)
        })
    })
}

/// Drop a `ConversionRun` by ID, releasing all its resources.
///
/// Raises RuntimeError if the run ID is unknown.
#[pyfunction(name = "conversion_run_drop")]
pub fn drop_run_py(py: Python<'_>, run_id: u64) -> PyResult<()> {
    run_with_panic_catch("conversion_run_drop", || {
        py.detach(move || drop_run(run_id).map_err(run_error_to_py))
    })
}

/// Drain all accumulated decisions from the run and return them as a Python list.
///
/// Each entry is `(kind, message)`.
/// After this call the run's decision buffer is empty.
///
/// Raises RuntimeError if the run ID is unknown.
#[pyfunction(name = "conversion_run_drain_decisions")]
pub fn drain_decisions_py<'py>(py: Python<'py>, run_id: u64) -> PyResult<Bound<'py, PyList>> {
    run_with_panic_catch("conversion_run_drain_decisions", || {
        let decisions: Vec<(String, String)> = py.detach(move || {
            with_run(run_id, |run| {
                Ok::<_, RunError>(
                    std::mem::take(&mut run.decisions)
                        .into_iter()
                        .map(|decision| {
                            let kind = run
                                .interner
                                .resolve(decision.kind)
                                .unwrap_or("unknown")
                                .to_string();
                            (kind, decision.message)
                        })
                        .collect(),
                )
            })
            .map_err(run_error_to_py)
        })?;

        let list = PyList::empty(py);
        for (kind, message) in decisions {
            list.append((kind, message))?;
        }
        Ok(list)
    })
}

/// Drain all accumulated warnings from the run and return them as a Python list of str.
///
/// After this call the run's warning buffer is empty.
///
/// Raises RuntimeError if the run ID is unknown.
#[pyfunction(name = "conversion_run_drain_warnings")]
pub fn drain_warnings_py<'py>(py: Python<'py>, run_id: u64) -> PyResult<Bound<'py, PyList>> {
    run_with_panic_catch("conversion_run_drain_warnings", || {
        let resolved: Vec<String> = py.detach(move || {
            with_run(run_id, |run| {
                Ok::<_, RunError>(
                    std::mem::take(&mut run.warnings)
                        .into_iter()
                        .map(|s| run.interner.resolve(s).unwrap_or("?").to_string())
                        .collect(),
                )
            })
            .map_err(run_error_to_py)
        })?;

        let list = PyList::empty(py);
        for s in resolved {
            list.append(s)?;
        }
        Ok(list)
    })
}

// ---------------------------------------------------------------------------
// Memory-reduction helpers
// ---------------------------------------------------------------------------

/// Release the FK remap state (MapperState) from a `ConversionRun`.
///
/// Safe to call after fixups complete — the mapper is not needed
/// by asset phases or ESP serialisation.  Frees the source→target FormID
/// map and EID index, reducing RSS by ~1–3 GB for a full FO76 plugin.
///
/// Raises RuntimeError if the run ID is unknown.
#[pyfunction(name = "conversion_run_release_remap_state")]
pub fn release_remap_state_py(py: Python<'_>, run_id: u64) -> PyResult<()> {
    run_with_panic_catch("conversion_run_release_remap_state", || {
        py.detach(move || {
            with_run(run_id, |run| {
                run.release_remap_state();
                Ok::<_, RunError>(())
            })
            .map_err(run_error_to_py)
        })
    })
}

// ---------------------------------------------------------------------------
// Progress callback and record-translation entry points
// ---------------------------------------------------------------------------

/// Set (or clear) a Python progress callback on an existing `ConversionRun`.
///
/// The callback signature: `def callback(records_processed: int) -> bool`.
/// Return `False` from the callback to cancel translation.
///
/// Pass `None` to clear a previously-set callback.
///
/// Raises RuntimeError if the run ID is unknown.
#[pyfunction(name = "conversion_run_set_progress_callback")]
#[pyo3(signature = (run_id, cb=None))]
pub fn set_progress_callback_py(
    py: Python<'_>,
    run_id: u64,
    cb: Option<Py<PyAny>>,
) -> PyResult<()> {
    run_with_panic_catch("conversion_run_set_progress_callback", || {
        py.detach(move || {
            with_run(run_id, |run| {
                run.progress_callback = cb;
                Ok::<_, RunError>(())
            })
            .map_err(run_error_to_py)
        })
    })
}

/// Translate all records from the source plugin into the target plugin.
///
/// Arguments:
///   run_id (int) — run ID returned by `conversion_run_create`.
///   progress_callback (callable | None) — optional `(records_processed: int) -> bool`.
///     Called every 1000 records. Return `False` to cancel.
///     If not supplied, any callback previously set via
///     `conversion_run_set_progress_callback` is used instead.
///
/// Returns a dict with keys:
///   "records_translated" (int)
///   "records_dropped"    (int)
///   "records_deferred"   (int)
///   "records_failed"     (int)
///   "by_signature"       (dict[str, dict[str, int]])
///
/// Raises RuntimeError if the run ID is unknown, a fatal error occurs, or
/// translation is cancelled (message: "translation cancelled").
#[pyfunction(name = "conversion_run_translate_all")]
#[pyo3(signature = (run_id, progress_callback=None))]
pub fn translate_all_py<'py>(
    py: Python<'py>,
    run_id: u64,
    progress_callback: Option<Py<PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    run_with_panic_catch("conversion_run_translate_all", || {
        if let Some(cb) = progress_callback {
            py.detach(move || {
                with_run(run_id, |run| {
                    run.progress_callback = Some(cb);
                    Ok::<_, RunError>(())
                })
                .map_err(run_error_to_py)
            })?;
        }

        let stats = py.detach(move || {
            with_conversion_worker_pool(run_id, "conversion_run_translate_all", || {
                with_run(run_id, |run| run.translate_all().map_err(|e: RunError| e))
            })
            .map_err(run_error_to_py)
        })?;

        translate_stats_to_pyraw(py, &stats)
    })
}

/// Translate a bounded list of source FormKeys into the target plugin.
///
/// Caller passes the FormKeys reached by the dependency-graph walker so the
/// Rust translator stays scoped to that subset instead of iterating the whole
/// source plugin.
///
/// Arguments:
///   run_id (int) — run ID returned by `conversion_run_create`.
///   form_keys (list[str]) — list of FormKey strings ("XXXXXX@Plugin.esm"
///     or "Plugin.esm:XXXXXX"; both formats accepted).
///   progress_callback (callable | None) — same semantics as translate_all.
///
/// Returns the same dict shape as `conversion_run_translate_all`.
///
/// FormKeys that can't be parsed are counted as `records_failed` with a
/// "bad_form_key:" warning; FormKeys not present in the source plugin
/// generate a "read_error:" warning per the existing per-record path.
#[pyfunction(name = "conversion_run_translate_records")]
#[pyo3(signature = (run_id, form_keys, progress_callback=None))]
pub fn translate_records_py<'py>(
    py: Python<'py>,
    run_id: u64,
    form_keys: Vec<String>,
    progress_callback: Option<Py<PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    run_with_panic_catch("conversion_run_translate_records", || {
        if let Some(cb) = progress_callback {
            py.detach(move || {
                with_run(run_id, |run| {
                    run.progress_callback = Some(cb);
                    Ok::<_, RunError>(())
                })
                .map_err(run_error_to_py)
            })?;
        }

        let stats = py.detach(move || {
            with_conversion_worker_pool(run_id, "conversion_run_translate_records", || {
                with_run(run_id, |run| {
                    // Parse FK strings against the run's interner so plugin Syms
                    // line up with the rest of the run.
                    let mut parsed: Vec<crate::ids::FormKey> = Vec::with_capacity(form_keys.len());
                    let mut bad_count: u32 = 0;
                    for s in &form_keys {
                        // Accept "XXXXXX@Plugin", "Plugin:XXXXXX", and the older
                        // "XXXXXX:Plugin" legacy spelling.
                        let Some(normalized) = normalize_translate_form_key(s) else {
                            let w = run.interner.intern(&format!("bad_form_key:{s}"));
                            run.warnings.push(w);
                            bad_count += 1;
                            continue;
                        };
                        match crate::ids::FormKey::parse(&normalized, &mut run.interner) {
                            Ok(fk) => parsed.push(fk),
                            Err(_) => {
                                let w = run.interner.intern(&format!("bad_form_key:{s}"));
                                run.warnings.push(w);
                                bad_count += 1;
                            }
                        }
                    }
                    let mut stats = run.translate_records(&parsed).map_err(|e: RunError| e)?;
                    stats.records_failed = stats.records_failed.saturating_add(bad_count);
                    Ok::<_, RunError>(stats)
                })
            })
            .map_err(run_error_to_py)
        })?;

        translate_stats_to_pyraw(py, &stats)
    })
}

/// Return scalar WEAP metadata needed by Python asset orchestration.
///
/// This intentionally exposes only scalar metadata. Rust reads the source WEAP
/// records and returns model path strings, role/ammo summaries, and identity
/// fields.
#[pyfunction(name = "conversion_run_weapon_metadata")]
pub fn weapon_metadata_py<'py>(
    py: Python<'py>,
    run_id: u64,
    source_form_keys: &Bound<'_, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    run_with_panic_catch("conversion_run_weapon_metadata", || {
        let mut input_fks: Vec<String> = Vec::with_capacity(source_form_keys.len());
        for item in source_form_keys.iter() {
            input_fks.push(item.extract()?);
        }

        let rows: Vec<(
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        )> = py.detach(move || {
            with_run(run_id, |run| {
                let weap_sig = SigCode::from_str("WEAP")
                    .map_err(|e| RunError::InvalidConfig(format!("WEAP signature: {e}")))?;
                let form_keys = if input_fks.is_empty() {
                    iter_form_keys_of_sig(run.source_handle_id, weap_sig, &run.interner)
                        .map_err(RunError::from)?
                } else {
                    let mut parsed = Vec::with_capacity(input_fks.len());
                    for source_text in input_fks {
                        let Some(normalized) = normalize_translate_form_key(&source_text) else {
                            continue;
                        };
                        if let Ok(fk) = crate::ids::FormKey::parse(&normalized, &mut run.interner) {
                            parsed.push(fk);
                        }
                    }
                    parsed
                };

                let mut out = Vec::with_capacity(form_keys.len());
                for fk in form_keys {
                    let source_key = form_key_to_read_str(&fk, &run.interner);
                    if source_key.is_empty() {
                        continue;
                    }
                    let Ok(record) = read_record(
                        run.source_handle_id,
                        &source_key,
                        &run.schema_source,
                        &run.interner,
                    ) else {
                        continue;
                    };
                    if record.sig != weap_sig {
                        continue;
                    }
                    out.push(weapon_metadata_from_record(
                        source_key,
                        &record,
                        &run.interner,
                    ));
                }
                Ok::<_, RunError>(out)
            })
            .map_err(run_error_to_py)
        })?;

        let out = PyList::empty(py);
        for row in rows {
            out.append(row)?;
        }
        Ok(out)
    })
}

/// Convert the native FNV legacy-scripting result to a compact Python list.
fn fnv_legacy_result_to_pyraw<'py>(
    py: Python<'py>,
    result: &crate::fnv_legacy_scripting::FnvLegacyScriptingResult,
) -> PyResult<Bound<'py, PyAny>> {
    use pyo3::IntoPyObjectExt;
    let skipped_list = PyList::empty(py);
    for (sig, eid, reason) in &result.skipped_records {
        skipped_list.append((sig, eid, reason))?;
    }

    let lip_list = PyList::empty(py);
    for path in &result.lip_regeneration_needed {
        lip_list.append(path)?;
    }

    let vmad_list = PyList::empty(py);
    for intent in &result.vmad_intents {
        vmad_list.append((&intent.target_form_key, &intent.script_class_name))?;
    }
    let out = PyList::empty(py);
    out.append(result.translated_scripts.len())?;
    out.append(result.translated_quests.len())?;
    out.append(result.translated_scenes.len())?;
    out.append(result.translated_infos.len())?;
    out.append(result.dialogue_groups.len())?;
    out.append(result.records_written)?;
    out.append(result.records_failed)?;
    out.append(result.psc_files_written)?;
    out.append(result.psc_files_skipped)?;
    out.append(skipped_list)?;
    out.append(lip_list)?;
    out.append(vmad_list)?;
    out.append(result.vmad_attached_in_rust)?;
    out.into_bound_py_any(py)
}

#[pyfunction(name = "conversion_run_fnv_legacy_scripting_from_run")]
pub fn fnv_legacy_scripting_from_run_py<'py>(
    py: Python<'py>,
    run_id: u64,
    mod_prefix: &str,
    source_plugin: &str,
    mod_path: &str,
) -> PyResult<Bound<'py, PyAny>> {
    run_with_panic_catch("conversion_run_fnv_legacy_scripting_from_run", || {
        let mod_prefix = mod_prefix.to_string();
        let source_plugin = source_plugin.to_string();
        let mod_path = mod_path.to_string();
        let result = py.detach(move || {
            with_run(run_id, |run| {
                run.run_fnv_legacy_scripting_from_deferred(
                    mod_prefix.as_str(),
                    source_plugin.as_str(),
                    mod_path.as_str(),
                )
                .map_err(crate::run::RunError::from)
            })
            .map_err(run_error_to_py)
        })?;
        fnv_legacy_result_to_pyraw(py, &result)
    })
}

/// Return mapped target FormKeys for the given source FormKeys.
///
/// For each input
/// `source_form_key` return the corresponding target form key.
///
/// Returns a list of `Option[str]` parallel to `source_form_keys`. `None`
/// indicates the source FK was not mapped by the Rust translate phase (the
/// caller decides what to do with the missing mapping).
///
/// Reads the mapper state from the run; the run must still be alive (use
/// before `conversion_run_drop`).
///
/// Form-key strings use the Python "OBJID:Plugin" rendering convention
/// (matches `RecordNode.form_key` and `FormKeyMapper.mappings` keys).
#[pyfunction(name = "conversion_run_target_form_keys")]
pub fn target_form_keys_py<'py>(
    py: Python<'py>,
    run_id: u64,
    source_form_keys: &Bound<'_, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    run_with_panic_catch("conversion_run_target_form_keys", || {
        let mut input_fks: Vec<String> = Vec::with_capacity(source_form_keys.len());
        for item in source_form_keys.iter() {
            let fk: String = item.extract()?;
            input_fks.push(fk);
        }

        // Snapshot the run's source→target mapping table as a string→string
        // HashMap keyed in Python's "OBJID:Plugin" form.
        let mapping_table: std::collections::HashMap<String, String> = py.detach(move || {
            with_run(run_id, |run| {
                let state = match run.mapper_state.as_ref() {
                    Some(s) => s,
                    None => return Ok::<_, RunError>(std::collections::HashMap::new()),
                };
                let mut out: std::collections::HashMap<String, String> =
                    std::collections::HashMap::with_capacity(state.source_to_target.len());
                for (&src, &tgt) in state.source_to_target.iter() {
                    let src_plugin = run.interner.resolve(src.plugin).unwrap_or("");
                    let tgt_plugin = run.interner.resolve(tgt.plugin).unwrap_or("");
                    if src_plugin.is_empty() || tgt_plugin.is_empty() {
                        continue;
                    }
                    let src_key = format!("{:06X}:{}", src.local, src_plugin);
                    let tgt_key = format!("{:06X}:{}", tgt.local, tgt_plugin);
                    out.insert(src_key, tgt_key);
                }
                Ok(out)
            })
            .map_err(run_error_to_py)
        })?;

        let out = PyList::empty(py);
        for src_fk in input_fks {
            let normalized = src_fk.trim().to_string();
            let target_fk = match mapping_table.get(&normalized) {
                Some(tfk) => tfk,
                None => {
                    out.append(py.None())?;
                    continue;
                }
            };
            out.append(target_fk)?;
        }
        Ok(out)
    })
}

#[pyfunction(name = "conversion_run_form_key_map")]
pub fn form_key_map_py<'py>(
    py: Python<'py>,
    run_id: u64,
    source_form_keys: &Bound<'_, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    run_with_panic_catch("conversion_run_form_key_map", || {
        let mut input_fks: Vec<String> = Vec::with_capacity(source_form_keys.len());
        for item in source_form_keys.iter() {
            input_fks.push(item.extract()?);
        }

        let pairs: Vec<(String, String)> = py.detach(move || {
            with_run(run_id, |run| {
                let Some(state) = run.mapper_state.as_ref() else {
                    return Ok::<_, RunError>(Vec::new());
                };
                let mut out = Vec::with_capacity(input_fks.len());
                for source_text in input_fks {
                    let Ok(source_fk) = parse_form_key_str(source_text.as_str(), &run.interner)
                    else {
                        continue;
                    };
                    let Some(target_fk) = state.source_to_target.get(&source_fk) else {
                        continue;
                    };
                    let source_key = form_key_to_read_str(&source_fk, &run.interner);
                    let target_key = form_key_to_read_str(target_fk, &run.interner);
                    if !source_key.is_empty() && !target_key.is_empty() {
                        out.push((source_key, target_key));
                    }
                }
                Ok(out)
            })
            .map_err(run_error_to_py)
        })?;

        let out = PyList::empty(py);
        for (source_key, target_key) in pairs {
            out.append((source_key, target_key))?;
        }
        Ok(out)
    })
}

#[pyfunction(name = "conversion_run_apply_registry_mappings")]
pub fn apply_registry_mappings_py(
    py: Python<'_>,
    run_id: u64,
    mappings: &Bound<'_, PyList>,
) -> PyResult<usize> {
    run_with_panic_catch("conversion_run_apply_registry_mappings", || {
        let mut map: std::collections::HashMap<String, String> =
            std::collections::HashMap::with_capacity(mappings.len());
        for item in mappings.iter() {
            let pair = item.downcast::<PyList>()?;
            if pair.len() != 2 {
                return Err(PyValueError::new_err(
                    "each registry mapping must be [source_form_key, target_form_key]",
                ));
            }
            let k: String = pair.get_item(0)?.extract()?;
            let v: String = pair.get_item(1)?.extract()?;
            map.insert(k, v);
        }
        py.detach(move || {
            with_run(run_id, |run| run.apply_registry_mappings(&map)).map_err(run_error_to_py)
        })
    })
}

// ---------------------------------------------------------------------------
// Phase dispatcher PyO3 surface
// ---------------------------------------------------------------------------

use crate::phase::{self, DispatchParams, LogLevel, PhaseEvent, PhaseReport};

fn log_level_as_str(level: &LogLevel) -> &'static str {
    match level {
        LogLevel::Debug => "DEBUG",
        LogLevel::Info => "INFO",
        LogLevel::Warn => "WARN",
        LogLevel::Error => "ERROR",
    }
}

fn phase_event_to_pyraw<'py>(py: Python<'py>, ev: &PhaseEvent) -> PyResult<Bound<'py, PyAny>> {
    use pyo3::IntoPyObjectExt;
    match ev {
        PhaseEvent::Started { phase } => ("started", *phase).into_bound_py_any(py),
        PhaseEvent::Progress {
            phase,
            current,
            total,
            item,
        } => ("progress", *phase, *current, *total, item.clone()).into_bound_py_any(py),
        PhaseEvent::Log {
            phase,
            level,
            message,
        } => ("log", *phase, log_level_as_str(level), message).into_bound_py_any(py),
        PhaseEvent::Completed { phase, report } => {
            ("completed", *phase, phase_report_to_pyraw(py, report)?).into_bound_py_any(py)
        }
        PhaseEvent::StageStarted { stage } => ("stage_started", *stage).into_bound_py_any(py),
        PhaseEvent::StageCompleted {
            stage,
            items_done,
            items_failed,
            elapsed_ms,
        } => (
            "stage_completed",
            *stage,
            *items_done,
            *items_failed,
            *elapsed_ms,
        )
            .into_bound_py_any(py),
        PhaseEvent::StageFailed { stage, message } => {
            ("stage_failed", *stage, message).into_bound_py_any(py)
        }
    }
}

fn phase_report_to_pyraw<'py>(py: Python<'py>, r: &PhaseReport) -> PyResult<Bound<'py, PyAny>> {
    use pyo3::IntoPyObjectExt;
    (
        r.records_changed,
        r.records_added,
        r.records_dropped,
        r.assets_written,
        r.warnings,
        r.elapsed_ms,
        r.items_failed,
        r.records_vanilla_remapped,
        r.records_deferred,
    )
        .into_bound_py_any(py)
}

fn pipeline_report_to_pyraw<'py>(
    py: Python<'py>,
    report: &crate::pipeline::PipelineReport,
) -> PyResult<Bound<'py, PyAny>> {
    use pyo3::IntoPyObjectExt;
    let stages = PyList::empty(py);
    for (name, r) in &report.stages {
        stages.append((
            *name,
            r.items_done,
            r.items_failed,
            r.warnings,
            r.elapsed_ms,
        ))?;
    }
    let counters = PyList::empty(py);
    for (k, v) in &report.counters {
        counters.append((k, v))?;
    }
    (stages, report.elapsed_ms, counters).into_bound_py_any(py)
}

/// Run a stage-DAG plan of registered phases. The plan json names
/// the runs, phases, params, and `after` edges; independent stages overlap.
/// Cancel via `conversion_run_cancel(events_run_id)`.
#[pyfunction(name = "conversion_pipeline_run")]
pub fn pipeline_run_py<'py>(py: Python<'py>, plan_json: &str) -> PyResult<Bound<'py, PyAny>> {
    run_with_panic_catch("conversion_pipeline_run", || {
        let report = py
            .detach(|| crate::pipeline::plan::run_plan(plan_json))
            .map_err(PyRuntimeError::new_err)?;
        pipeline_report_to_pyraw(py, &report)
    })
}

#[pyfunction(name = "conversion_run_terrain_texture_jobs_json")]
pub fn terrain_texture_jobs_json_py(py: Python<'_>, run_id: u64) -> PyResult<String> {
    run_with_panic_catch("conversion_run_terrain_texture_jobs_json", || {
        let jobs = py.detach(|| {
            with_run(run_id, |run| {
                Ok::<_, RunError>(run.terrain_texture_jobs.clone())
            })
            .map_err(run_error_to_py)
        })?;
        serde_json::to_string(&jobs).map_err(|err| PyRuntimeError::new_err(err.to_string()))
    })
}

pub(crate) fn hash_files_blake3_core(
    paths: &[String],
    workers: Option<usize>,
) -> Result<Vec<String>, String> {
    let hash_one = |path: &String| -> Result<String, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
        let mut reader = std::io::BufReader::with_capacity(1 << 20, file);
        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n =
                std::io::Read::read(&mut reader, &mut buf).map_err(|e| format!("{path}: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hasher.finalize().to_hex().to_string())
    };
    let hash_all = || {
        paths
            .par_iter()
            .map(hash_one)
            .collect::<Result<Vec<_>, _>>()
    };
    match workers.filter(|w| *w > 0) {
        Some(w) => rayon::ThreadPoolBuilder::new()
            .num_threads(w)
            .build()
            .map_err(|e| format!("rayon pool: {e}"))?
            .install(hash_all),
        None => hash_all(),
    }
}

/// Batch blake3 of files (hex digests, input order) — the cache manifest
/// hasher.
#[pyfunction(name = "conversion_hash_files_blake3")]
#[pyo3(signature = (paths, workers=None))]
pub fn hash_files_blake3_py(
    py: Python<'_>,
    paths: Vec<String>,
    workers: Option<usize>,
) -> PyResult<Vec<String>> {
    run_with_panic_catch("conversion_hash_files_blake3", || {
        py.detach(|| hash_files_blake3_core(&paths, workers).map_err(PyRuntimeError::new_err))
    })
}

// ---------------------------------------------------------------------------
// Output sinks
// ---------------------------------------------------------------------------

/// config json: {"mod_root": str, "spill_dir": str, "emit_loose": bool,
/// "enable_ba2": bool}
#[pyfunction(name = "sinks_create")]
pub fn sinks_create_py(py: Python<'_>, config_json: &str) -> PyResult<u64> {
    run_with_panic_catch("sinks_create", || {
        py.detach(|| {
            let config: Value = serde_json::from_str(config_json)
                .map_err(|e| PyValueError::new_err(format!("invalid sink config json: {e}")))?;
            let mod_root = config
                .get("mod_root")
                .and_then(Value::as_str)
                .ok_or_else(|| PyValueError::new_err("missing 'mod_root'"))?;
            let emit_loose = config
                .get("emit_loose")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let enable_ba2 = config
                .get("enable_ba2")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let ba2 = if enable_ba2 {
                let spill_dir = config
                    .get("spill_dir")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PyValueError::new_err("missing 'spill_dir'"))?;
                Some(
                    crate::sinks::Ba2ShardWriter::new(PathBuf::from(spill_dir))
                        .map_err(PyRuntimeError::new_err)?,
                )
            } else {
                None
            };
            let sink = crate::sinks::SinkSet {
                ba2,
                loose: crate::sinks::LooseSink {
                    enabled: emit_loose,
                    mod_root: PathBuf::from(mod_root),
                },
                terrain: crate::sinks::TerrainSidecarSink::default(),
            };
            Ok(crate::sinks::register_sink(sink))
        })
    })
}

#[pyfunction(name = "sinks_attach_run")]
pub fn sinks_attach_run_py(py: Python<'_>, run_id: u64, sink_id: u64) -> PyResult<()> {
    run_with_panic_catch("sinks_attach_run", || {
        py.detach(|| {
            let sink = crate::sinks::sink_handle(sink_id).map_err(PyValueError::new_err)?;
            with_run(run_id, |run| {
                run.output_sink = Some(sink);
                Ok::<(), RunError>(())
            })
            .map_err(run_error_to_py)
        })
    })
}

#[pyfunction(name = "sinks_streamed")]
pub fn sinks_streamed_py(py: Python<'_>, sink_id: u64) -> PyResult<Vec<String>> {
    run_with_panic_catch("sinks_streamed", || {
        py.detach(|| {
            let sink = crate::sinks::sink_handle(sink_id).map_err(PyValueError::new_err)?;
            Ok(match &sink.ba2 {
                Some(ba2) => ba2.streamed_rel_paths(),
                None => Vec::new(),
            })
        })
    })
}

/// Bulk-add already-on-disk files to the BA2 spills (the join reconcile).
/// `files` = [(abs_path, data_rel)]. Returns the number actually added
/// (first-wins dedup skips don't count).
#[pyfunction(name = "sinks_add_files")]
#[pyo3(signature = (sink_id, files, workers=None))]
pub fn sinks_add_files_py(
    py: Python<'_>,
    sink_id: u64,
    files: Vec<(String, String)>,
    workers: Option<usize>,
) -> PyResult<usize> {
    run_with_panic_catch("sinks_add_files", || {
        py.detach(|| {
            let sink = crate::sinks::sink_handle(sink_id).map_err(PyValueError::new_err)?;
            let Some(ba2) = &sink.ba2 else {
                return Err(PyRuntimeError::new_err("sink has no BA2 writer"));
            };
            let add_all = || -> Result<usize, String> {
                let added: Result<Vec<bool>, String> = files
                    .par_iter()
                    .map(|(abs, rel)| ba2.add_file(rel, Path::new(abs)))
                    .collect();
                Ok(added?.into_iter().filter(|a| *a).count())
            };
            let result = match workers.filter(|w| *w > 0) {
                Some(w) => rayon::ThreadPoolBuilder::new()
                    .num_threads(w)
                    .build()
                    .map_err(|e| PyRuntimeError::new_err(format!("rayon pool: {e}")))?
                    .install(add_all),
                None => add_all(),
            };
            result.map_err(PyRuntimeError::new_err)
        })
    })
}

/// Finalize one PLANNED archive from the spills. `texture_archive` is the
/// planner's PlannedArchive flag; `ordered_rels_json` = JSON array of the
/// planned entries' relative paths.
#[pyfunction(name = "sinks_finalize_archive")]
pub fn sinks_finalize_archive_py(
    py: Python<'_>,
    sink_id: u64,
    output_path: &str,
    texture_archive: bool,
    ordered_rels_json: &str,
) -> PyResult<()> {
    run_with_panic_catch("sinks_finalize_archive", || {
        let rels: Vec<String> = serde_json::from_str(ordered_rels_json)
            .map_err(|e| PyValueError::new_err(format!("invalid ordered_rels json: {e}")))?;
        let output = PathBuf::from(output_path);
        py.detach(|| {
            let sink = crate::sinks::sink_handle(sink_id).map_err(PyValueError::new_err)?;
            let Some(ba2) = &sink.ba2 else {
                return Err(PyRuntimeError::new_err("sink has no BA2 writer"));
            };
            let rel_refs: Vec<&str> = rels.iter().map(String::as_str).collect();
            ba2.finalize_archive(&output, texture_archive, &rel_refs)
                .map_err(PyRuntimeError::new_err)
        })
    })
}

#[pyfunction(name = "sinks_register_sidecar")]
pub fn sinks_register_sidecar_py(py: Python<'_>, sink_id: u64, rel: &str) -> PyResult<()> {
    run_with_panic_catch("sinks_register_sidecar", || {
        py.detach(|| {
            let sink = crate::sinks::sink_handle(sink_id).map_err(PyValueError::new_err)?;
            sink.terrain.register(rel);
            Ok(())
        })
    })
}

#[pyfunction(name = "sinks_sidecars")]
pub fn sinks_sidecars_py(py: Python<'_>, sink_id: u64) -> PyResult<Vec<String>> {
    run_with_panic_catch("sinks_sidecars", || {
        py.detach(|| {
            let sink = crate::sinks::sink_handle(sink_id).map_err(PyValueError::new_err)?;
            Ok(sink.terrain.list())
        })
    })
}

/// Abort: delete the spill files (partial *.ba2 outputs are the Python
/// side's responsibility — it knows which outputs this call wrote).
#[pyfunction(name = "sinks_abort")]
pub fn sinks_abort_py(py: Python<'_>, sink_id: u64) -> PyResult<()> {
    run_with_panic_catch("sinks_abort", || {
        py.detach(|| {
            let sink = crate::sinks::sink_handle(sink_id).map_err(PyValueError::new_err)?;
            if let Some(ba2) = &sink.ba2 {
                ba2.abort();
            }
            Ok(())
        })
    })
}

/// Delete the spill files after a successful join (the sink stays
/// registered — the sidecar list is still consumed by the manifest step).
#[pyfunction(name = "sinks_cleanup_spills")]
pub fn sinks_cleanup_spills_py(py: Python<'_>, sink_id: u64) -> PyResult<()> {
    run_with_panic_catch("sinks_cleanup_spills", || {
        py.detach(|| {
            let sink = crate::sinks::sink_handle(sink_id).map_err(PyValueError::new_err)?;
            if let Some(ba2) = &sink.ba2 {
                ba2.cleanup();
            }
            Ok(())
        })
    })
}

#[pyfunction(name = "sinks_drop")]
pub fn sinks_drop_py(py: Python<'_>, sink_id: u64) -> PyResult<()> {
    run_with_panic_catch("sinks_drop", || {
        py.detach(|| {
            if let Ok(sink) = crate::sinks::sink_handle(sink_id) {
                if let Some(ba2) = &sink.ba2 {
                    ba2.cleanup();
                }
            }
            crate::sinks::drop_sink(sink_id).map_err(PyValueError::new_err)
        })
    })
}

fn dispatch_params_from_json(params_json: &str) -> PyResult<DispatchParams> {
    let value: Value = serde_json::from_str(params_json)
        .map_err(|e| PyValueError::new_err(format!("invalid phase params json: {e}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| PyValueError::new_err("phase params json must be an object"))?;
    let mod_path = object
        .get("mod_path")
        .and_then(Value::as_str)
        .ok_or_else(|| PyValueError::new_err("missing 'mod_path'"))?
        .to_string();
    let source_extracted_dir = object
        .get("source_extracted_dir")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let target_extracted_dir = object
        .get("target_extracted_dir")
        .and_then(Value::as_str)
        .map(str::to_string);
    let target_data_dir = object
        .get("target_data_dir")
        .and_then(Value::as_str)
        .map(str::to_string);
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    Ok(DispatchParams {
        mod_path: PathBuf::from(mod_path),
        source_extracted_dir: PathBuf::from(source_extracted_dir),
        target_extracted_dir: target_extracted_dir.map(PathBuf::from),
        target_data_dir: target_data_dir.map(PathBuf::from),
        params,
    })
}

#[pyfunction(name = "conversion_run_phase")]
pub fn run_phase_py<'py>(
    py: Python<'py>,
    run_id: u64,
    phase: String,
    params_json: &str,
) -> PyResult<Bound<'py, PyAny>> {
    run_with_panic_catch("conversion_run_phase", || {
        let dispatch = dispatch_params_from_json(params_json)?;
        let report = py
            .detach(move || {
                let worker_count = conversion_worker_count(run_id)
                    .map_err(|e| phase::PhaseError::Internal(format!("{e}")))?;
                let run_phase = || phase::run_phase(run_id, &phase, dispatch);
                let Some(workers) = worker_count else {
                    return run_phase();
                };
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(workers)
                    .build()
                    .map_err(|err| {
                        phase::PhaseError::Internal(format!("rayon pool error: {err}"))
                    })?;
                pool.install(run_phase)
            })
            .map_err(|e| match e {
                phase::PhaseError::Cancelled => PyRuntimeError::new_err("cancelled"),
                phase::PhaseError::BadParams(s) => PyValueError::new_err(s),
                phase::PhaseError::NotImplemented(n) => {
                    PyRuntimeError::new_err(format!("phase {n} not yet implemented"))
                }
                phase::PhaseError::Internal(s) => PyRuntimeError::new_err(s),
            })?;
        phase_report_to_pyraw(py, &report)
    })
}

#[pyfunction(name = "conversion_run_drain_events")]
#[pyo3(signature = (run_id, max=256))]
pub fn drain_events_py<'py>(
    py: Python<'py>,
    run_id: u64,
    max: usize,
) -> PyResult<Bound<'py, PyList>> {
    run_with_panic_catch("conversion_run_drain_events", || {
        let events: Vec<PhaseEvent> = py.detach(|| {
            // Slot receiver — drains WITHOUT the run lock, so mid-phase
            // draining works even while a phase holds the run.
            let slot = crate::run::run_slot(run_id)
                .map_err(|e| PyRuntimeError::new_err(format!("run lookup: {e}")))?;
            let mut out = Vec::with_capacity(max.min(64));
            for _ in 0..max {
                match slot.events.try_recv() {
                    Ok(ev) => out.push(ev),
                    Err(_) => break,
                }
            }
            Ok::<_, PyErr>(out)
        })?;
        let list = PyList::empty(py);
        for ev in &events {
            list.append(phase_event_to_pyraw(py, ev)?)?;
        }
        Ok(list)
    })
}

#[pyfunction(name = "conversion_run_cancel")]
pub fn cancel_py(py: Python<'_>, run_id: u64) -> PyResult<()> {
    run_with_panic_catch("conversion_run_cancel", || {
        py.detach(move || {
            // Slot flag — settable WITHOUT the run lock, so cancel reaches a
            // running phase instead of waiting for it to finish.
            let slot = crate::run::run_slot(run_id)
                .map_err(|e| PyRuntimeError::new_err(format!("run lookup: {e}")))?;
            slot.cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        })
    })
}

#[pyfunction(name = "conversion_run_list_phases")]
pub fn list_phases_py<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
    let list = PyList::empty(py);
    for n in phase::registry().names() {
        list.append(n)?;
    }
    Ok(list)
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// Post-copy repair of the deferred placed-ref-target class.
///
/// Runs the authoritative LCTN LCUN/LCEP/ACEP resolution over the conversion
/// run's in-memory output plugin AFTER the FO76→FO4 phase-6 cell-slice copy +
/// cell-location sync re-insert the exterior placed children. Keeps every ref
/// whose target is now present; nulls (LCUN: drops the row in lockstep) any still
/// absent. Pairs with the pre-copy deferral gated by
/// `RunConfig::defer_placed_child_ref_class`; a no-op when the class was not
/// deferred. Returns `{ "records_changed": int }`.
#[pyfunction(name = "conversion_run_repair_placed_child_refs")]
pub fn repair_placed_child_refs_py<'py>(py: Python<'py>, run_id: u64) -> PyResult<u32> {
    run_with_panic_catch("conversion_run_repair_placed_child_refs", || {
        py.detach(move || {
            with_conversion_worker_pool(run_id, "conversion_run_repair_placed_child_refs", || {
                with_run(run_id, |run| {
                    let report = run
                        .repair_placed_child_refs()
                        .map_err(|e: crate::fixups::FixupError| RunError::from(e))?;
                    Ok::<_, RunError>(report.records_changed)
                })
            })
            .map_err(run_error_to_py)
        })
    })
}

/// Native orchestration of the projected placed-children copy: collect the
/// source worldspace topology, route it to target grid cells, resolve the
/// FO76→FO4 seed FormKey map from the run's mapper, and run the production
/// copy kernel — all without the topology crossing into Python. Returns the
/// copy report as a dict (see `projected_placed::CopyProjectedReport`).
#[pyfunction(name = "conversion_run_copy_projected_placed_children")]
#[pyo3(signature = (run_id, source_worldspace, min_x, min_y, max_x, max_y, offset_x, offset_y, offset_z, worker_count=None))]
#[allow(clippy::too_many_arguments)]
pub fn copy_projected_placed_children_py(
    py: Python<'_>,
    run_id: u64,
    source_worldspace: &str,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
    offset_x: f32,
    offset_y: f32,
    offset_z: f32,
    worker_count: Option<usize>,
) -> PyResult<Py<PyAny>> {
    run_with_panic_catch("conversion_run_copy_projected_placed_children", || {
        let source_worldspace = source_worldspace.to_string();
        let text = py.detach(move || {
            crate::projected_placed::copy_projected_placed_children(
                run_id,
                source_worldspace.as_str(),
                (min_x, min_y, max_x, max_y),
                (offset_x, offset_y, offset_z),
                worker_count,
            )
        })?;
        let json = PyModule::import(py, "json")?;
        Ok(json.call_method1("loads", (text,))?.into_any().unbind())
    })
}

/// Native orchestration of the worldspace persistent-cell synthesis. Reuses
/// the seed keys cached by `conversion_run_copy_projected_placed_children`
/// when the copy covered the same bounds (skipping the second full-worldspace
/// source scan), appends the all-signature persistent base keys, resolves the
/// FormKey map natively, and runs the synthesis kernel. Returns the synthesis
/// report as a dict (payload fields + `seed_cache_hit` + orchestration timing).
#[pyfunction(name = "conversion_run_synthesize_worldspace_persistent_cell")]
#[pyo3(signature = (run_id, source_worldspace, min_x, min_y, max_x, max_y, offset_x, offset_y, offset_z, worker_count=None))]
#[allow(clippy::too_many_arguments)]
pub fn synthesize_worldspace_persistent_cell_py(
    py: Python<'_>,
    run_id: u64,
    source_worldspace: &str,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
    offset_x: f32,
    offset_y: f32,
    offset_z: f32,
    worker_count: Option<usize>,
) -> PyResult<Py<PyAny>> {
    run_with_panic_catch(
        "conversion_run_synthesize_worldspace_persistent_cell",
        || {
            let source_worldspace = source_worldspace.to_string();
            let text = py.detach(move || {
                crate::projected_placed::synthesize_projected_persistent_cell(
                    run_id,
                    source_worldspace.as_str(),
                    (min_x, min_y, max_x, max_y),
                    (offset_x, offset_y, offset_z),
                    worker_count,
                )
            })?;
            let json = PyModule::import(py, "json")?;
            Ok(json.call_method1("loads", (text,))?.into_any().unbind())
        },
    )
}

#[pyfunction(name = "conversion_run_synthesize_encounter_zones")]
#[pyo3(signature = (run_id, identity_resolve = false))]
pub fn synthesize_encounter_zones_py<'py>(
    py: Python<'py>,
    run_id: u64,
    identity_resolve: bool,
) -> PyResult<u32> {
    run_with_panic_catch("conversion_run_synthesize_encounter_zones", || {
        py.detach(move || {
            with_conversion_worker_pool(run_id, "conversion_run_synthesize_encounter_zones", || {
                with_run(run_id, |run| {
                    let report = run
                        .synthesize_encounter_zones(identity_resolve)
                        .map_err(|e: crate::fixups::FixupError| RunError::from(e))?;
                    Ok::<_, RunError>(report.records_added + report.records_changed)
                })
            })
            .map_err(run_error_to_py)
        })
    })
}

#[pyfunction(name = "conversion_run_synthesize_sky_regions")]
#[pyo3(signature = (run_id))]
pub fn synthesize_sky_regions_py<'py>(py: Python<'py>, run_id: u64) -> PyResult<u32> {
    run_with_panic_catch("conversion_run_synthesize_sky_regions", || {
        py.detach(move || {
            with_conversion_worker_pool(run_id, "conversion_run_synthesize_sky_regions", || {
                with_run(run_id, |run| {
                    let report = run
                        .synthesize_sky_regions()
                        .map_err(|e: crate::fixups::FixupError| RunError::from(e))?;
                    Ok::<_, RunError>(report.records_added + report.records_changed)
                })
            })
            .map_err(run_error_to_py)
        })
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_anim_text_data_assets_for_paths(
    target_plugin_path: &str,
    game: &str,
    base_race_plugin_paths: &[String],
    src_meshes_root: &Path,
    target_data_dir: &Path,
    target_catalog_path: &Path,
    target_cache_dir: &Path,
    target_overlay_dir: Option<&Path>,
) -> Result<String, String> {
    let target_handle = OwnedPluginHandle::load(Path::new(target_plugin_path), game, None)
        .map_err(|error| error.to_string())?;
    let base_race_handles = base_race_plugin_paths
        .iter()
        .map(|path| {
            OwnedPluginHandle::load(Path::new(path), game, None).map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let base_race_handle_ids = base_race_handles
        .iter()
        .map(OwnedPluginHandle::id)
        .collect::<Vec<_>>();
    let store = crate::target_assets::TargetAssetStore::open_shared(
        target_data_dir,
        target_catalog_path,
        target_cache_dir,
        target_overlay_dir,
    )?;
    crate::target_assets::prepare_anim_text_data_assets(
        &store,
        target_handle.id(),
        &base_race_handle_ids,
        src_meshes_root,
    )
    .map(|path| path.to_string_lossy().into_owned())
}

#[pyfunction(name = "conversion_prepare_anim_text_data_assets")]
#[pyo3(signature = (
    target_plugin_path,
    game,
    base_race_plugin_paths,
    src_meshes_root,
    target_data_dir,
    target_catalog_path,
    target_cache_dir,
    target_overlay_dir=None
))]
#[allow(clippy::too_many_arguments)]
pub fn prepare_anim_text_data_assets_py(
    py: Python<'_>,
    target_plugin_path: &str,
    game: &str,
    base_race_plugin_paths: Vec<String>,
    src_meshes_root: &str,
    target_data_dir: &str,
    target_catalog_path: &str,
    target_cache_dir: &str,
    target_overlay_dir: Option<&str>,
) -> PyResult<String> {
    run_with_panic_catch("conversion_prepare_anim_text_data_assets", || {
        let target_plugin_path = target_plugin_path.to_string();
        let game = game.to_string();
        let src_meshes_root = PathBuf::from(src_meshes_root);
        let target_data_dir = PathBuf::from(target_data_dir);
        let target_catalog_path = PathBuf::from(target_catalog_path);
        let target_cache_dir = PathBuf::from(target_cache_dir);
        let target_overlay_dir = target_overlay_dir.map(PathBuf::from);
        py.detach(move || {
            prepare_anim_text_data_assets_for_paths(
                &target_plugin_path,
                &game,
                &base_race_plugin_paths,
                &src_meshes_root,
                &target_data_dir,
                &target_catalog_path,
                &target_cache_dir,
                target_overlay_dir.as_deref(),
            )
            .map_err(PyRuntimeError::new_err)
        })
    })
}

#[pyfunction(name = "conversion_run_synthesize_vendor_dialogue")]
#[pyo3(signature = (run_id))]
pub fn synthesize_vendor_dialogue_py<'py>(py: Python<'py>, run_id: u64) -> PyResult<u32> {
    run_with_panic_catch("conversion_run_synthesize_vendor_dialogue", || {
        py.detach(move || {
            with_conversion_worker_pool(run_id, "conversion_run_synthesize_vendor_dialogue", || {
                with_run(run_id, |run| {
                    let report = run
                        .synthesize_vendor_dialogue()
                        .map_err(|e: crate::fixups::FixupError| RunError::from(e))?;
                    Ok::<_, RunError>(report.records_added + report.records_changed)
                })
            })
            .map_err(run_error_to_py)
        })
    })
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<crate::target_assets::PyTargetAssetStore>()?;
    m.add_function(wrap_pyfunction!(crate::target_assets::build_catalog_py, m)?)?;
    m.add_function(wrap_pyfunction!(
        crate::target_assets::catalog_schema_version_py,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(diagnose_navmesh_links_py, m)?)?;
    m.add_function(wrap_pyfunction!(merge_sources_py, m)?)?;
    m.add_function(wrap_pyfunction!(collect_lod_closures_py, m)?)?;
    m.add_function(wrap_pyfunction!(create_run_from_paths_py, m)?)?;
    m.add_function(wrap_pyfunction!(set_target_description_py, m)?)?;
    m.add_function(wrap_pyfunction!(script_reference_records_py, m)?)?;
    m.add_function(wrap_pyfunction!(set_record_subrecords_py, m)?)?;
    m.add_function(wrap_pyfunction!(apply_placed_record_position_offset_py, m)?)?;
    m.add_function(wrap_pyfunction!(sync_cell_regions_from_source_py, m)?)?;
    m.add_function(wrap_pyfunction!(sync_cell_locations_from_lctn_py, m)?)?;
    m.add_function(wrap_pyfunction!(release_source_handle_py, m)?)?;
    m.add_function(wrap_pyfunction!(release_master_handles_py, m)?)?;
    m.add_function(wrap_pyfunction!(save_target_py, m)?)?;
    m.add_function(wrap_pyfunction!(drop_run_py, m)?)?;
    m.add_function(wrap_pyfunction!(drain_decisions_py, m)?)?;
    m.add_function(wrap_pyfunction!(drain_warnings_py, m)?)?;
    m.add_function(wrap_pyfunction!(release_remap_state_py, m)?)?;
    m.add_function(wrap_pyfunction!(set_progress_callback_py, m)?)?;
    m.add_function(wrap_pyfunction!(translate_all_py, m)?)?;
    m.add_function(wrap_pyfunction!(translate_records_py, m)?)?;
    m.add_function(wrap_pyfunction!(weapon_metadata_py, m)?)?;
    m.add_function(wrap_pyfunction!(fnv_legacy_scripting_from_run_py, m)?)?;
    m.add_function(wrap_pyfunction!(target_form_keys_py, m)?)?;
    m.add_function(wrap_pyfunction!(form_key_map_py, m)?)?;
    m.add_function(wrap_pyfunction!(apply_registry_mappings_py, m)?)?;
    // Phase dispatcher entry points
    m.add_function(wrap_pyfunction!(run_phase_py, m)?)?;
    m.add_function(wrap_pyfunction!(pipeline_run_py, m)?)?;
    m.add_function(wrap_pyfunction!(drain_events_py, m)?)?;
    m.add_function(wrap_pyfunction!(cancel_py, m)?)?;
    m.add_function(wrap_pyfunction!(list_phases_py, m)?)?;
    m.add_function(wrap_pyfunction!(terrain_texture_jobs_json_py, m)?)?;
    m.add_function(wrap_pyfunction!(hash_files_blake3_py, m)?)?;
    m.add_function(wrap_pyfunction!(sinks_create_py, m)?)?;
    m.add_function(wrap_pyfunction!(sinks_attach_run_py, m)?)?;
    m.add_function(wrap_pyfunction!(sinks_streamed_py, m)?)?;
    m.add_function(wrap_pyfunction!(sinks_add_files_py, m)?)?;
    m.add_function(wrap_pyfunction!(sinks_finalize_archive_py, m)?)?;
    m.add_function(wrap_pyfunction!(sinks_register_sidecar_py, m)?)?;
    m.add_function(wrap_pyfunction!(sinks_sidecars_py, m)?)?;
    m.add_function(wrap_pyfunction!(sinks_abort_py, m)?)?;
    m.add_function(wrap_pyfunction!(sinks_cleanup_spills_py, m)?)?;
    m.add_function(wrap_pyfunction!(sinks_drop_py, m)?)?;
    // Two-pass terrain texture driver
    m.add_function(wrap_pyfunction!(
        crate::terrain_textures::py_terrain_with_textures,
        m
    )?)?;
    // Post-copy LCTN LCUN/LCEP/ACEP repair
    m.add_function(wrap_pyfunction!(repair_placed_child_refs_py, m)?)?;
    // Native projected placed-children copy + persistent-cell synthesis
    m.add_function(wrap_pyfunction!(copy_projected_placed_children_py, m)?)?;
    m.add_function(wrap_pyfunction!(
        synthesize_worldspace_persistent_cell_py,
        m
    )?)?;
    // FO76→FO4 encounter-zone synthesis (post-copy ECZN + CELL.XEZN)
    m.add_function(wrap_pyfunction!(synthesize_encounter_zones_py, m)?)?;
    // FO76→FO4 interior sky-region assignment (post-copy CELL.XCCM)
    m.add_function(wrap_pyfunction!(synthesize_sky_regions_py, m)?)?;
    m.add_function(wrap_pyfunction!(synthesize_vendor_dialogue_py, m)?)?;
    m.add_function(wrap_pyfunction!(prepare_anim_text_data_assets_py, m)?)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedRecord, ParsedSubrecord, insert_parsed_record_in_slot, plugin_handle_store_ref,
    };
    use smol_str::SmolStr;
    use std::sync::Once;

    fn write_empty_plugin(dir: &Path, name: &str, game: &str) -> PathBuf {
        let path = dir.join(name);
        let handle = esp_authoring_core::plugin_runtime::plugin_handle_new_native(name, Some(game))
            .expect("new plugin handle");
        esp_authoring_core::plugin_runtime::plugin_handle_save_no_py(
            handle,
            path.to_str().unwrap(),
        )
        .expect("save plugin");
        assert!(esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle));
        path
    }

    #[test]
    fn fo76_font_aliases_are_rewritten_for_fo4_in_every_language() {
        let mut strings = esp_authoring_core::plugin_runtime::LocalizedStringsState::default();
        strings.by_language.insert(
            "en".to_string(),
            std::collections::HashMap::from([(
                1,
                "<font face='$Typewriter_Font'>typed</font> <font face='$76HandwrittenNeat_Font'>neat</font>"
                    .to_string(),
            )]),
        );
        strings.by_language.insert(
            "de".to_string(),
            std::collections::HashMap::from([(
                1,
                "<font face='$76HandwrittenIlliterate'>gekritzelt</font>".to_string(),
            )]),
        );

        normalize_fo76_font_aliases_for_fo4(&mut strings);

        assert_eq!(
            strings.by_language["en"][&1],
            "<font face='$Terminal_Font'>typed</font> <font face='$HandwrittenFont'>neat</font>"
        );
        assert_eq!(
            strings.by_language["de"][&1],
            "<font face='$HandwrittenFont'>gekritzelt</font>"
        );
    }

    #[test]
    fn owned_plugin_handle_loads_with_explicit_game_and_closes_on_drop() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_empty_plugin(tmp.path(), "Target.esp", "fo4");
        let handle = OwnedPluginHandle::load(&path, "fo4", None).unwrap();
        let handle_id = handle.id();
        {
            let store = esp_authoring_core::plugin_runtime::plugin_handle_store_ref()
                .lock()
                .unwrap();
            assert_eq!(
                store.get(&handle_id).unwrap().parsed.game.as_deref(),
                Some("fo4")
            );
        }
        drop(handle);
        let store = esp_authoring_core::plugin_runtime::plugin_handle_store_ref()
            .lock()
            .unwrap();
        assert!(!store.contains_key(&handle_id));
    }

    fn store_contains_path(path: &Path) -> bool {
        let expected = path.to_string_lossy();
        plugin_handle_store_ref()
            .lock()
            .unwrap()
            .values()
            .any(|slot| slot.parsed.file_path == expected)
    }

    #[test]
    fn diagnose_navmesh_path_closes_handle_after_success() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_empty_plugin(tmp.path(), "Target.esp", "fo4");

        let stats = diagnose_navmesh_links_for_path(&path, "fo4").unwrap();

        assert_eq!(stats, NavmeshFinalizeStats::default());
        assert!(!store_contains_path(&path));
    }

    #[test]
    fn diagnose_navmesh_path_closes_handle_after_diagnostic_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Malformed.esp");
        let handle = esp_authoring_core::plugin_runtime::plugin_handle_new_native(
            "Malformed.esp",
            Some("fo4"),
        )
        .unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            insert_parsed_record_in_slot(
                slot,
                ParsedRecord {
                    signature: SmolStr::new("NAVM"),
                    form_id: 0x800,
                    flags: 0,
                    version_control: 0,
                    form_version: None,
                    version2: None,
                    subrecords: vec![ParsedSubrecord {
                        signature: SmolStr::new("NVNM"),
                        data: Bytes::from_static(b"invalid"),
                        semantic_type: None,
                    }],
                    raw_payload: None,
                    parse_error: None,
                },
            );
        }
        esp_authoring_core::plugin_runtime::plugin_handle_save_no_py(
            handle,
            path.to_str().unwrap(),
        )
        .unwrap();
        assert!(esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle));

        assert!(diagnose_navmesh_links_for_path(&path, "fo4").is_err());
        assert!(!store_contains_path(&path));
    }

    #[test]
    fn prepare_anim_text_data_assets_closes_plugins_when_store_open_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let target = write_empty_plugin(tmp.path(), "Target.esp", "fo4");
        let base = write_empty_plugin(tmp.path(), "Fallout4.esm", "fo4");
        let target_path = target.to_string_lossy().into_owned();
        let base_path = base.to_string_lossy().into_owned();
        let result = prepare_anim_text_data_assets_for_paths(
            &target_path,
            "fo4",
            std::slice::from_ref(&base_path),
            tmp.path(),
            tmp.path(),
            &tmp.path().join("missing-catalog.sqlite3"),
            &tmp.path().join("cache"),
            None,
        );
        assert!(result.is_err());
        let store = esp_authoring_core::plugin_runtime::plugin_handle_store_ref()
            .lock()
            .unwrap();
        assert!(
            store
                .values()
                .all(|slot| slot.parsed.file_path != target_path
                    && slot.parsed.file_path != base_path)
        );
    }

    #[test]
    fn hash_files_blake3_matches_direct_hash() {
        let tmp = std::env::temp_dir().join(format!(
            "conv-blake3-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let a = tmp.join("a.bin");
        let b = tmp.join("b.bin");
        std::fs::write(&a, b"alpha bytes").unwrap();
        std::fs::write(&b, vec![7u8; 3 << 20]).unwrap(); // multi-buffer read
        let paths = vec![
            a.to_string_lossy().to_string(),
            b.to_string_lossy().to_string(),
        ];
        let hashes = hash_files_blake3_core(&paths, Some(2)).unwrap();
        assert_eq!(hashes[0], blake3::hash(b"alpha bytes").to_hex().to_string());
        assert_eq!(
            hashes[1],
            blake3::hash(&vec![7u8; 3 << 20]).to_hex().to_string()
        );
        let err =
            hash_files_blake3_core(&[tmp.join("missing").to_string_lossy().to_string()], None)
                .unwrap_err();
        assert!(err.contains("missing"), "got: {err}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn initialize_python_for_tests() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            if std::env::var_os("PYTHONHOME").is_none() {
                if let Some(home) = std::env::var_os("VIRTUAL_ENV")
                    .and_then(|venv| {
                        std::fs::read_to_string(std::path::Path::new(&venv).join("pyvenv.cfg")).ok()
                    })
                    .and_then(|cfg| {
                        cfg.lines()
                            .find_map(|line| line.strip_prefix("home = ").map(str::to_string))
                    })
                {
                    unsafe {
                        std::env::set_var("PYTHONHOME", home);
                    }
                }
            }
            Python::initialize();
        });
    }

    // NOTE: A full `run_with_panic_catch` test requires `Python::with_gil`, which
    // needs the `auto-initialize` pyo3 feature. That feature is incompatible with
    // `extension-module` (used by this crate), so we cannot call it here without
    // the embedded Python runtime. The test below instead verifies the underlying
    // `catch_unwind` mechanic — the same logic that `run_with_panic_catch` wraps —
    // to confirm that panics are caught and their payloads are extractable.
    #[test]
    fn catch_unwind_captures_str_payload() {
        let result = catch_unwind(AssertUnwindSafe(|| -> i32 { panic!("intentional panic") }));
        assert!(result.is_err());
        let payload = result.unwrap_err();
        let msg = payload
            .downcast_ref::<&'static str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str));
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("intentional panic"));
    }

    #[test]
    fn catch_unwind_captures_string_payload() {
        let result = catch_unwind(AssertUnwindSafe(|| -> i32 {
            panic!("{}", "owned string panic".to_string())
        }));
        assert!(result.is_err());
        let payload = result.unwrap_err();
        let got_str = payload.downcast_ref::<&'static str>().copied();
        let got_owned = payload.downcast_ref::<String>().map(String::as_str);
        let msg = got_str.or(got_owned).unwrap_or("<unknown>");
        assert!(msg.contains("owned string panic"));
    }

    #[test]
    fn normalize_translate_form_key_accepts_plugin_first_native_form() {
        assert_eq!(
            normalize_translate_form_key("SeventySix.esm:000800").as_deref(),
            Some("000800@SeventySix.esm")
        );
    }

    #[test]
    fn normalize_translate_form_key_accepts_legacy_hex_first_form() {
        assert_eq!(
            normalize_translate_form_key("000800:SeventySix.esm").as_deref(),
            Some("000800@SeventySix.esm")
        );
    }

    #[test]
    fn config_from_json_reads_full_plugin_fields() {
        let cfg = config_from_json(
            r#"{
                "output_plugin_name": "SeventySix.esm",
                "preserve_source_ids": true,
                "use_base_game_assets": true,
                "is_whole_plugin": true,
                "mod_path": "X:/mods/SeventySix",
                "source_extracted_dir": "X:/extracted/fo76",
                "target_extracted_dir": "X:/extracted/fo4",
                "target_data_dir": "N:/Steam Games/steamapps/common/Fallout 4/Data",
                "target_record_preflight": [["Ammo10mm", "AMMO", "01F276:Fallout4.esm"]],
                "target_master_names": ["Fallout4.esm"],
                "conversion_workers": 7,
                "records_limit": 123,
                "asset_phases": {
                    "terrain": true,
                    "nifs": true,
                    "textures": true,
                    "materials": true,
                    "havok": true,
                    "animations": true,
                    "sounds": true
                }
            }"#,
        )
        .unwrap();
        assert_eq!(cfg.output_plugin_name, "SeventySix.esm");
        assert!(cfg.preserve_source_ids);
        assert!(cfg.use_base_game_assets);
        assert!(cfg.is_whole_plugin);
        assert_eq!(cfg.conversion_workers, Some(7));
        assert_eq!(cfg.records_limit, Some(123));
        assert_eq!(cfg.warning_policy, WarningPolicy::WarnPlayable);
        assert_eq!(cfg.target_record_preflight.len(), 1);
        assert_eq!(cfg.target_record_preflight[0].editor_id, "Ammo10mm");
        assert_eq!(cfg.target_master_names, vec!["Fallout4.esm".to_string()]);
        assert!(cfg.mod_path.is_some());
        assert!(cfg.source_extracted_dir.is_some());
        assert!(cfg.target_extracted_dir.is_some());
        assert!(cfg.target_data_dir.is_some());
        assert!(cfg.asset_phases.terrain);
        assert!(cfg.asset_phases.nifs);
        assert!(cfg.asset_phases.textures);
        assert!(cfg.asset_phases.materials);
        assert!(cfg.asset_phases.havok);
        assert!(cfg.asset_phases.animations);
        assert!(cfg.asset_phases.sounds);
    }

    #[test]
    fn target_record_preflight_from_json_accepts_list_rows() {
        let cfg = config_from_json(
            r#"{"target_record_preflight": [["Ammo10mm", "AMMO", "01F276:Fallout4.esm"]]}"#,
        )
        .unwrap();
        assert_eq!(cfg.target_record_preflight.len(), 1);
        assert_eq!(cfg.target_record_preflight[0].editor_id, "Ammo10mm");
        assert_eq!(cfg.target_record_preflight[0].signature, "AMMO");
        assert_eq!(
            cfg.target_record_preflight[0].form_key,
            "01F276:Fallout4.esm"
        );
    }

    #[test]
    fn optional_path_treats_none_and_blank_as_absent() {
        let value: Value =
            serde_json::from_str(r#"{"none_path": null, "empty_path": "", "blank_path": "   "}"#)
                .unwrap();
        assert_eq!(optional_path(&value, "missing_path"), None);
        assert_eq!(optional_path(&value, "none_path"), None);
        assert_eq!(optional_path(&value, "empty_path"), None);
        assert_eq!(optional_path(&value, "blank_path"), None);
    }
}
