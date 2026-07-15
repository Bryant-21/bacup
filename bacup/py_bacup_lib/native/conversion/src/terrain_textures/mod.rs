//! Terrain-texture manifest builder + grass asset converter.
//!
//! Replaces the deleted `ltex_resolver.py`. Called from
//! `phase::terrain::ConvertTerrainPhase` and from the direct
//! `convert_fo76_btd_to_fo4_land` Python entry.

pub mod options;

pub mod ba2_resolver;

pub mod bgsm_terrain;
pub mod grass_assets;
pub mod grass_walk;
pub mod ltex_walk;
pub mod manifest;
pub mod nif_refs;

pub use options::{Options, RecordOutputMode, Report, TimingEntry};

const PROJECTED_CELL_IMPORT_BATCH_SIZE: usize = 1024;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;

use crate::terrain_textures::ba2_resolver::Ba2Resolver;
use crate::terrain_textures::grass_walk::{
    gcvr_land_texture_form_keys, grass_entries_for_gcvr, grass_entries_for_ltex,
};
use crate::terrain_textures::ltex_walk::{build_bundle, normalize_esp_form_key};
use crate::terrain_textures::manifest::TextureManifest;

use pyo3::prelude::*;
use std::time::{Duration, Instant};

const PROGRESS_LOG_INTERVAL: Duration = Duration::from_secs(2);

/// PyO3 entry. Takes options as a JSON string, returns a JSON string report.
/// Two-pass FO76 BTD: pass 1 collects required_ltex_form_ids (emit_textures=false);
/// build_manifest populates texture_manifest.json; pass 2 re-runs with the manifest
/// so terrain_native emits TXST/LTEX/GRAS YAML records.
#[pyfunction(name = "conversion_terrain_with_textures")]
pub fn py_terrain_with_textures(py: Python<'_>, options_json: &str) -> PyResult<String> {
    let options_json = options_json.to_owned();
    let report = py
        .detach(move || -> Result<Report, String> {
            let value: serde_json::Value =
                serde_json::from_str(&options_json).map_err(|e| format!("bad options: {e}"))?;
            for legacy_key in ["source_handle_id", "target_handle_id", "record_output_mode"] {
                if value.get(legacy_key).is_some() {
                    return Err(format!(
                        "legacy terrain option is not supported: {legacy_key}"
                    ));
                }
            }
            let mut opts: Options =
                serde_json::from_value(value).map_err(|e| format!("bad options: {e}"))?;
            if opts.source_plugin_path.trim().is_empty() {
                return Err("source_plugin_path is required".to_string());
            }
            let source_game = normalize_source_game(&opts.source_game)?;
            let source_handle = crate::run::OwnedPluginHandle::load(
                Path::new(&opts.source_plugin_path),
                source_game.as_str(),
                None,
            )
            .map_err(|error| error.to_string())?;
            opts.source_handle_id = source_handle.id();
            run(opts)
        })
        .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
    serde_json::to_string(&report)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

pub fn run(opts: Options) -> Result<Report, String> {
    run_with_progress(opts, |message| eprintln!("{message}"))
}

pub fn run_with_progress(
    opts: Options,
    mut progress: impl FnMut(String),
) -> Result<Report, String> {
    use std::path::PathBuf;
    use terrain_native::authoring_emit::{
        AuthoringEmitError, AuthoringRecordPayload, ConvertOptions, TerrainRecordOutput,
        collect_required_texture_usages_lightweight_for_options, convert_btd,
        convert_btd_with_record_sink,
    };

    let total_started = Instant::now();
    let mut timings = Vec::new();
    let source_game = normalize_source_game(&opts.source_game)?;

    let to_convert_options = |emit_textures: bool, manifest_path: &str| ConvertOptions {
        btd_path: opts.btd_path.clone(),
        output_authoring_dir: opts.output_authoring_dir.clone(),
        plugin_name: opts.plugin_name.clone(),
        worldspace_editor_id: opts.worldspace_editor_id.clone(),
        source_min_x: opts.source_min_x,
        source_min_y: opts.source_min_y,
        source_max_x: opts.source_max_x,
        source_max_y: opts.source_max_y,
        first_form_id: 0x000800,
        world_form_id: opts.world_form_id,
        first_cell_form_id: opts.first_cell_form_id,
        resample_mode: opts.resample_mode.clone(),
        debug_output_dir: opts.debug_output_dir.clone(),
        texture_manifest_path: manifest_path.to_owned(),
        water_manifest_path: opts.water_manifest_path.clone(),
        emit_textures,
        export_heightmap: opts.export_heightmap,
        debug_flat_land: opts.debug_flat_land,
        preserve_source_ids: opts.preserve_source_ids,
        reserved_object_ids: opts.reserved_object_ids.clone(),
        source_worldspace_authoring_dir: opts.source_worldspace_authoring_dir.clone(),
        source_worldspace_terrain_ids_json: opts.source_worldspace_terrain_ids_json.clone(),
        heightmap_output_path: opts.heightmap_output_path.clone(),
        // Pass .btd4 emission only on pass 2 (emit_textures=true = the record-emitting pass).
        // Pass 1 (emit_textures=false) only collects required LTEX form IDs; writing the
        // sidecar there would produce an incomplete file and run the emitter twice.
        btd4_output_path: if emit_textures {
            opts.btd4_output_path.clone()
        } else {
            String::new()
        },
        conversion_workers: opts.conversion_workers,
        land_skip_ground_cover_variants: opts.land_skip_ground_cover_variants,
        // Reuse only matters on the record-emitting pass (pass 2). Pass 1
        // (emit_textures=false) returns early before any encode anyway.
        reuse_existing_textures: opts.reuse_existing_textures && emit_textures,
    };

    // Resolve debug dir for manifest output.
    let debug_dir = if opts.debug_output_dir.is_empty() {
        PathBuf::from(&opts.output_authoring_dir)
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("debug")
            .join("terrain")
    } else {
        PathBuf::from(&opts.debug_output_dir)
    };
    let manifest_path = debug_dir.join("texture_manifest.json");
    let extraction_root = debug_dir.join("source_textures");
    let output_prefix_root = format!(
        "textures/terrain/{}",
        opts.worldspace_editor_id.to_ascii_lowercase()
    );
    let source_extracted_dir = if opts.source_extracted_dir.trim().is_empty() {
        None
    } else {
        Some(Path::new(&opts.source_extracted_dir))
    };

    let water_manifest_started = Instant::now();
    write_source_water_manifest(&opts, &source_game)?;
    push_timing(
        &mut timings,
        "write_source_water_manifest",
        water_manifest_started,
    );

    // Pass 1 only needs the BTD texture index. Do not build the full height grid here.
    progress("[terrain_tex] scanning BTD texture usage".to_owned());
    let required_ltex_started = Instant::now();
    let required_usages =
        collect_required_texture_usages_lightweight_for_options(to_convert_options(false, ""))
            .map_err(|e| format!("first-pass terrain texture scan: {e}"))?;
    let required_ltex: BTreeSet<&str> = required_usages
        .iter()
        .map(|usage| usage.ltex_form_key.as_str())
        .collect();
    push_timing(&mut timings, "collect_required_ltex", required_ltex_started);

    // Build manifest only if we have textures to emit and LTEXs to resolve.
    progress(format!(
        "[terrain_tex] found {} required LTEX record(s)",
        required_ltex.len()
    ));
    let manifest_started = Instant::now();
    let (textures_resolved, mut opt_manifest) = if opts.emit_textures && !required_ltex.is_empty() {
        progress("[terrain_tex] resolving texture manifest".to_owned());
        let manifest = build_manifest(
            opts.source_handle_id,
            Path::new(&opts.fo76_data_dir),
            source_extracted_dir,
            &required_usages,
            &extraction_root,
            &output_prefix_root,
            &source_game,
            &manifest_path,
        )?;
        let count = manifest.textures.len() as u32;
        (count, Some(manifest))
    } else {
        (0, None)
    };
    push_timing(&mut timings, "build_texture_manifest", manifest_started);

    if let Some(manifest) = opt_manifest.as_ref() {
        progress(format!(
            "[terrain_tex] queued {} LTEX texture set(s) for textures_v2",
            manifest.textures.len()
        ));
    }

    // Count grass entries resolved (LTEX + GCVR → GRAS lookups) — this is the
    // pre-asset-conversion count.
    let grass_resolved = opt_manifest
        .as_ref()
        .map(|m| m.textures.iter().map(|b| b.grass.len() as u32).sum())
        .unwrap_or(0);
    let grass_ltex_variants = opt_manifest
        .as_ref()
        .map(|m| {
            m.textures
                .iter()
                .filter(|bundle| bundle.source_gcvr_form_key.is_some())
                .count() as u32
        })
        .unwrap_or(0);
    let grass_position_range_normalized = opt_manifest
        .as_ref()
        .map(|m| {
            m.textures
                .iter()
                .flat_map(|bundle| bundle.grass.iter())
                .filter(|grass| grass.position_range_normalized)
                .count() as u32
        })
        .unwrap_or(0);
    let gcvr_records_resolved = required_usages
        .iter()
        .filter_map(|usage| usage.ground_cover_form_key.as_deref())
        .collect::<HashSet<_>>()
        .len() as u32;

    // Pass 2: with manifest path.
    let manifest_path_str = if opts.emit_textures && textures_resolved > 0 {
        manifest_path.to_string_lossy().into_owned()
    } else {
        String::new()
    };
    let mut records_imported = 0u32;
    progress("[terrain] emitting and importing LAND/CELL records".to_owned());
    let pass2_started = Instant::now();
    let pass2 = match opts.record_output_mode {
        RecordOutputMode::AuthoringDir => {
            let pass2_output = convert_btd(
                to_convert_options(opts.emit_textures, &manifest_path_str),
                TerrainRecordOutput::WriteAuthoringFiles,
            )
            .map_err(|e| format!("second-pass terrain: {e}"))?;
            serde_json::to_value(&pass2_output.report)
                .map_err(|e| format!("second-pass report value: {e}"))?
        }
        RecordOutputMode::TargetHandle => {
            let target_handle_id = opts.target_handle_id.ok_or_else(|| {
                "target_handle_id is required when record_output_mode=target_handle".to_owned()
            })?;
            let mut target_cell_editor_ids =
                target_cell_editor_id_collision_set(&opts, &source_game);
            let mut terrain_record_remaps = TerrainRecordRemaps::new(&opts);
            let mut projected_cell_batch: Vec<AuthoringRecordPayload> =
                Vec::with_capacity(PROJECTED_CELL_IMPORT_BATCH_SIZE);
            let report = {
                let mut record_sink =
                    |record: AuthoringRecordPayload| -> Result<(), AuthoringEmitError> {
                        if is_projected_cell_record(&record) {
                            projected_cell_batch.push(record);
                            if projected_cell_batch.len() >= PROJECTED_CELL_IMPORT_BATCH_SIZE {
                                let imported = import_projected_cell_batch(
                                    target_handle_id,
                                    &mut projected_cell_batch,
                                    &mut target_cell_editor_ids,
                                )
                                .map_err(AuthoringEmitError::Message)?;
                                records_imported = records_imported.saturating_add(imported);
                                progress(format!(
                                    "[terrain] imported {records_imported} terrain record(s) so far"
                                ));
                            }
                        } else {
                            let imported = import_projected_cell_batch(
                                target_handle_id,
                                &mut projected_cell_batch,
                                &mut target_cell_editor_ids,
                            )
                            .map_err(AuthoringEmitError::Message)?;
                            records_imported = records_imported.saturating_add(imported);
                            let imported = import_terrain_record(
                                target_handle_id,
                                &record,
                                &mut target_cell_editor_ids,
                                &mut terrain_record_remaps,
                            )
                            .map_err(AuthoringEmitError::Message)?;
                            records_imported = records_imported.saturating_add(imported);
                        }
                        Ok(())
                    };
                convert_btd_with_record_sink(
                    to_convert_options(opts.emit_textures, &manifest_path_str),
                    &mut record_sink,
                )
            }
            .map_err(|e| format!("second-pass terrain: {e}"))?;
            let imported = import_projected_cell_batch(
                target_handle_id,
                &mut projected_cell_batch,
                &mut target_cell_editor_ids,
            )?;
            records_imported = records_imported.saturating_add(imported);
            serde_json::to_value(&report).map_err(|e| format!("second-pass report value: {e}"))?
        }
    };
    progress(format!(
        "[terrain] emitted {} cell(s); imported {records_imported} terrain record(s)",
        pass2
            .get("cells_written")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
    ));
    append_terrain_timings(&mut timings, &pass2);
    push_timing(&mut timings, "emit_and_import_land", pass2_started);

    // Job 2: write downgraded BGSM files for LTEX-material entries.
    let materials_started = Instant::now();
    let materials_written = if opts.write_materials {
        if let Some(ref manifest) = opt_manifest {
            if manifest
                .textures
                .iter()
                .any(|t| !t.source_material_file.is_empty())
            {
                progress("[terrain] writing converted terrain materials".to_owned());
                let mod_path = PathBuf::from(&opts.output_authoring_dir)
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                bgsm_terrain::write_converted_materials(manifest, &mod_path, &source_game)?
            } else {
                0
            }
        } else {
            0
        }
    } else {
        0
    };
    if materials_written > 0 {
        progress(format!(
            "[terrain] wrote {materials_written} converted terrain material(s)"
        ));
    }
    push_timing(&mut timings, "write_terrain_materials", materials_started);

    let mut report_grass_nifs: u32 = 0;
    let mut report_grass_textures: u32 = 0;
    let mut report_grass_materials: u32 = 0;
    let mut report_grass_nifs_collected: u32 = 0;
    let mut report_grass_textures_collected: u32 = 0;
    let mut report_grass_materials_collected: u32 = 0;

    // Job 3: populate grass NIF/material/texture asset graphs.
    let grass_assets_started = Instant::now();
    if opts.emit_textures && opts.populate_grass_assets {
        let resolver =
            match crate::terrain_textures::ba2_resolver::Ba2Resolver::open_with_extracted_dir(
                Path::new(&opts.fo76_data_dir),
                source_extracted_dir,
            ) {
                Ok(r) => Some(r),
                Err(e) => {
                    eprintln!("[grass_assets] BA2 resolver open failed (Job 3 skipped): {e}");
                    None
                }
            };
        if let (Some(resolver), Some(manifest_ref)) = (resolver, opt_manifest.as_mut()) {
            let mod_path = PathBuf::from(&opts.output_authoring_dir)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            let mut grass_asset_cache =
                crate::terrain_textures::grass_assets::GrassAssetCache::default();
            progress(format!(
                "[grass_assets] processing {grass_resolved} resolved grass record(s)"
            ));
            let mut grass_processed = 0u32;
            let mut last_grass_progress: Option<Instant> = None;
            for bundle in manifest_ref.textures.iter_mut() {
                for grass in bundle.grass.iter_mut() {
                    if last_grass_progress
                        .map(|last| last.elapsed() >= PROGRESS_LOG_INTERVAL)
                        .unwrap_or(true)
                    {
                        progress(format!(
                            "[grass_assets] processing {}/{}: {}",
                            grass_processed.saturating_add(1),
                            grass_resolved,
                            grass.source_editor_id
                        ));
                        last_grass_progress = Some(Instant::now());
                    }
                    if opts.convert_grass_assets {
                        crate::terrain_textures::grass_assets::populate_assets_cached(
                            grass,
                            &mut grass_asset_cache,
                            &resolver,
                            &extraction_root,
                            &mod_path,
                            &source_game,
                        );
                    } else {
                        crate::terrain_textures::grass_assets::collect_asset_refs_cached(
                            grass,
                            &mut grass_asset_cache,
                            &resolver,
                            &extraction_root,
                            &source_game,
                        );
                    }
                    grass_processed = grass_processed.saturating_add(1);
                }
            }
            progress(format!(
                "[grass_assets] processed {grass_processed} grass record(s)"
            ));
            // Re-serialize the manifest now that grass.assets is populated.
            let _ = std::fs::write(
                &manifest_path,
                serde_json::to_string_pretty(&*manifest_ref).unwrap_or_default(),
            );

            // Aggregate counts.
            let mut nifs = 0u32;
            let mut texs = 0u32;
            let mut mats = 0u32;
            for bundle in &manifest_ref.textures {
                for g in &bundle.grass {
                    for a in &g.assets {
                        match a.asset_type.as_str() {
                            "nif" => nifs += 1,
                            "texture" => texs += 1,
                            "material" => mats += 1,
                            _ => {}
                        }
                    }
                }
            }
            if opts.convert_grass_assets {
                report_grass_nifs = nifs;
                report_grass_textures = texs;
                report_grass_materials = mats;
            }
            report_grass_nifs_collected = nifs;
            report_grass_textures_collected = texs;
            report_grass_materials_collected = mats;
        }
    }
    push_timing(&mut timings, "populate_grass_assets", grass_assets_started);
    push_timing(&mut timings, "total", total_started);

    let mut report = Report::default();
    report.cells_written = pass2
        .get("cells_written")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    report.dropped_texture_layers = pass2
        .get("dropped_texture_layers")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    report.textures_resolved = textures_resolved;
    report.grass_resolved = grass_resolved;
    report.ground_cover_layers = pass2
        .get("ground_cover_layers")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    report.no_ground_cover_layers = pass2
        .get("no_ground_cover_layers")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    report.grass_ltex_variants = grass_ltex_variants;
    report.gcvr_records_resolved = gcvr_records_resolved;
    report.gcvr_records_missing = 0;
    report.grass_position_range_normalized = grass_position_range_normalized;
    report.materials_written = materials_written;
    report.grass_nifs_collected = report_grass_nifs_collected;
    report.grass_textures_collected = report_grass_textures_collected;
    report.grass_materials_collected = report_grass_materials_collected;
    report.grass_nifs_converted = report_grass_nifs;
    report.grass_textures_converted = report_grass_textures;
    report.grass_materials_converted = report_grass_materials;
    report.records_imported = records_imported;
    report.btd4_output_path = pass2
        .get("btd4_output_path")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_owned());
    report.layers_recovered = pass2
        .get("layers_recovered")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    report.timings = timings;
    if let Some(ref manifest) = opt_manifest {
        report.terrain_texture_jobs = manifest
            .textures
            .iter()
            .map(crate::terrain_textures::manifest::TerrainTextureJob::from)
            .collect();
    }
    write_timing_report(&debug_dir, &opts.worldspace_editor_id, &report)?;
    Ok(report)
}

fn write_source_water_manifest(opts: &Options, source_game: &str) -> Result<(), String> {
    if source_game != "fo76" || opts.water_manifest_path.trim().is_empty() {
        return Ok(());
    }
    let manifest_text =
        esp_authoring_core::plugin_runtime::plugin_handle_collect_water_manifest_json(
            opts.source_handle_id,
            &opts.worldspace_editor_id,
            opts.source_min_x,
            opts.source_min_y,
            opts.source_max_x,
            opts.source_max_y,
        )
        .map_err(|err| format!("collect FO76 water manifest: {err}"))?;
    let path = Path::new(&opts.water_manifest_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create water manifest dir {}: {err}", parent.display()))?;
    }
    fs::write(path, manifest_text)
        .map_err(|err| format!("write water manifest {}: {err}", path.display()))
}

fn push_timing(timings: &mut Vec<TimingEntry>, name: &str, started: Instant) {
    let elapsed = started.elapsed().as_secs_f64();
    timings.push(TimingEntry {
        name: name.to_owned(),
        elapsed_seconds: (elapsed * 1_000_000.0).round() / 1_000_000.0,
    });
}

fn append_terrain_timings(timings: &mut Vec<TimingEntry>, pass2: &serde_json::Value) {
    let Some(entries) = pass2.get("timings").and_then(|value| value.as_array()) else {
        return;
    };
    for entry in entries {
        let Some(name) = entry.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(elapsed_seconds) = entry
            .get("elapsed_seconds")
            .and_then(|value| value.as_f64())
        else {
            continue;
        };
        timings.push(TimingEntry {
            name: format!("terrain_native.{name}"),
            elapsed_seconds,
        });
    }
}

fn write_timing_report(debug_dir: &Path, worldspace: &str, report: &Report) -> Result<(), String> {
    fs::create_dir_all(debug_dir).map_err(|e| format!("mkdir {}: {e}", debug_dir.display()))?;
    let payload = serde_json::json!({
        "cells_written": report.cells_written,
        "textures_resolved": report.textures_resolved,
        "grass_resolved": report.grass_resolved,
        "ground_cover_layers": report.ground_cover_layers,
        "no_ground_cover_layers": report.no_ground_cover_layers,
        "grass_ltex_variants": report.grass_ltex_variants,
        "gcvr_records_resolved": report.gcvr_records_resolved,
        "gcvr_records_missing": report.gcvr_records_missing,
        "grass_position_range_normalized": report.grass_position_range_normalized,
        "materials_written": report.materials_written,
        "grass_nifs_collected": report.grass_nifs_collected,
        "grass_textures_collected": report.grass_textures_collected,
        "grass_materials_collected": report.grass_materials_collected,
        "grass_nifs_converted": report.grass_nifs_converted,
        "grass_textures_converted": report.grass_textures_converted,
        "grass_materials_converted": report.grass_materials_converted,
        "dropped_texture_layers": report.dropped_texture_layers,
        "records_imported": report.records_imported,
        "timings": report.timings,
    });
    let json = serde_json::to_string_pretty(&payload)
        .map_err(|e| format!("serialize terrain timing report: {e}"))?;
    // Canonical name (read by tooling/tests) plus a per-worldspace copy so a
    // later run on a different worldspace can't clobber this one's detail.
    let canonical = debug_dir.join("terrain_timing.json");
    fs::write(&canonical, &json).map_err(|e| format!("write {}: {e}", canonical.display()))?;
    let slug: String = worldspace
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if !slug.is_empty() {
        let keyed = debug_dir.join(format!("terrain_timing_{slug}.json"));
        fs::write(&keyed, &json).map_err(|e| format!("write {}: {e}", keyed.display()))?;
    }
    Ok(())
}

fn import_terrain_record(
    target_handle_id: u64,
    record: &terrain_native::authoring_emit::AuthoringRecordPayload,
    target_cell_editor_ids: &mut HashSet<String>,
    terrain_record_remaps: &mut TerrainRecordRemaps,
) -> Result<u32, String> {
    let mut value = terrain_record_value(record)?;
    terrain_record_remaps.rewrite_references(&mut value);
    if terrain_record_remaps.skip_or_track(record, &value) {
        return Ok(0);
    }
    if record.signature == "CELL" && value.get("Landscape").is_some() {
        rename_projected_cell_editor_id_collision(&mut value, target_cell_editor_ids);
        esp_authoring_core::plugin_runtime::plugin_handle_replace_projected_cell_authoring_record_value(
            target_handle_id,
            &value,
            &record.relative_path,
        )
        .map(|form_keys| form_keys.len() as u32)
        .map_err(|e| format!("terrain record {} import failed: {e}", record.relative_path))
    } else {
        esp_authoring_core::plugin_runtime::plugin_handle_replace_authoring_record_value(
            target_handle_id,
            &value,
        )
        .map(|_| 1)
        .map_err(|e| format!("terrain record {} import failed: {e}", record.relative_path))
    }
}

fn import_projected_cell_batch(
    target_handle_id: u64,
    records: &mut Vec<terrain_native::authoring_emit::AuthoringRecordPayload>,
    target_cell_editor_ids: &mut HashSet<String>,
) -> Result<u32, String> {
    if records.is_empty() {
        return Ok(0);
    }
    let mut values = Vec::with_capacity(records.len());
    for record in records.drain(..) {
        let mut value = terrain_record_value(&record)?;
        rename_projected_cell_editor_id_collision(&mut value, target_cell_editor_ids);
        values.push((value, record.relative_path));
    }
    esp_authoring_core::plugin_runtime::plugin_handle_replace_projected_cell_authoring_record_values_at_locations(
        target_handle_id,
        values,
    )
    .map(|imported| imported as u32)
    .map_err(|e| format!("terrain projected CELL batch import failed: {e}"))
}

fn terrain_record_value(
    record: &terrain_native::authoring_emit::AuthoringRecordPayload,
) -> Result<serde_json::Value, String> {
    let mut value: serde_json::Value = serde_saphyr::from_str(&record.yaml)
        .map_err(|e| format!("terrain record {} parse failed: {e}", record.relative_path))?;
    if value.get("signature").is_none() {
        let object = value.as_object_mut().ok_or_else(|| {
            format!(
                "terrain record {} parsed to non-object YAML",
                record.relative_path
            )
        })?;
        object.insert(
            "signature".to_owned(),
            serde_json::Value::String(record.signature.clone()),
        );
    }
    Ok(value)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FormReference {
    plugin: String,
    object_id: String,
}

impl FormReference {
    fn new(plugin: impl Into<String>, object_id: u32) -> Self {
        Self {
            plugin: plugin.into(),
            object_id: form_id_hex(object_id),
        }
    }
}

struct TerrainRecordRemaps {
    output_plugin_name: String,
    target_grass_by_editor_id: HashMap<String, FormReference>,
    output_grass_by_editor_id: HashMap<String, FormReference>,
    remaps_by_object_id: HashMap<u32, FormReference>,
}

impl TerrainRecordRemaps {
    fn new(opts: &Options) -> Self {
        let target_grass_by_editor_id = opts
            .target_record_reuse
            .iter()
            .filter(|row| row.signature.eq_ignore_ascii_case("GRAS"))
            .filter_map(|row| {
                form_reference_from_form_key(&row.form_key)
                    .map(|reference| (normalized_editor_id(&row.editor_id), reference))
            })
            .collect();
        Self {
            output_plugin_name: opts.plugin_name.clone(),
            target_grass_by_editor_id,
            output_grass_by_editor_id: HashMap::new(),
            remaps_by_object_id: HashMap::new(),
        }
    }

    fn rewrite_references(&self, value: &mut serde_json::Value) -> u32 {
        rewrite_form_references(value, &self.output_plugin_name, &self.remaps_by_object_id)
    }

    fn skip_or_track(
        &mut self,
        record: &terrain_native::authoring_emit::AuthoringRecordPayload,
        value: &serde_json::Value,
    ) -> bool {
        if record.signature != "GRAS" {
            return false;
        }
        let Some(editor_id) = record_editor_id(value) else {
            return false;
        };
        let Some(object_id) = record_object_id(value) else {
            return false;
        };
        let key = normalized_editor_id(&editor_id);
        if let Some(reference) = self.target_grass_by_editor_id.get(&key).cloned() {
            self.remaps_by_object_id.insert(object_id, reference);
            return true;
        }
        if let Some(reference) = self.output_grass_by_editor_id.get(&key).cloned() {
            self.remaps_by_object_id.insert(object_id, reference);
            return true;
        }
        self.output_grass_by_editor_id.insert(
            key,
            FormReference::new(self.output_plugin_name.clone(), object_id),
        );
        false
    }
}

fn rewrite_form_references(
    value: &mut serde_json::Value,
    output_plugin_name: &str,
    remaps_by_object_id: &HashMap<u32, FormReference>,
) -> u32 {
    match value {
        serde_json::Value::Object(object) => {
            let mut rewritten =
                rewrite_reference_object(object, output_plugin_name, remaps_by_object_id);
            for child in object.values_mut() {
                rewritten +=
                    rewrite_form_references(child, output_plugin_name, remaps_by_object_id);
            }
            rewritten
        }
        serde_json::Value::Array(items) => items
            .iter_mut()
            .map(|item| rewrite_form_references(item, output_plugin_name, remaps_by_object_id))
            .sum(),
        _ => 0,
    }
}

fn rewrite_reference_object(
    object: &mut serde_json::Map<String, serde_json::Value>,
    output_plugin_name: &str,
    remaps_by_object_id: &HashMap<u32, FormReference>,
) -> u32 {
    let Some(plugin) = object
        .get("plugin")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
    else {
        return 0;
    };
    if !plugin.eq_ignore_ascii_case(output_plugin_name) {
        return 0;
    }
    let Some(object_id) = object.get("object_id").and_then(object_id_from_value) else {
        return 0;
    };
    let Some(reference) = remaps_by_object_id.get(&object_id) else {
        return 0;
    };
    object.insert(
        "plugin".to_owned(),
        serde_json::Value::String(reference.plugin.clone()),
    );
    object.insert(
        "object_id".to_owned(),
        serde_json::Value::String(reference.object_id.clone()),
    );
    1
}

fn record_editor_id(value: &serde_json::Value) -> Option<String> {
    for key in ["eid", "editor_id", "EditorID"] {
        if let Some(editor_id) = value.get(key).and_then(|value| value.as_str()) {
            let trimmed = editor_id.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_owned());
            }
        }
    }
    None
}

fn record_object_id(value: &serde_json::Value) -> Option<u32> {
    value.get("form_id").and_then(object_id_from_value)
}

fn object_id_from_value(value: &serde_json::Value) -> Option<u32> {
    if let Some(number) = value.as_u64() {
        return Some((number as u32) & 0x00FF_FFFF);
    }
    value.as_str().and_then(object_id_from_form_key)
}

fn form_reference_from_form_key(value: &str) -> Option<FormReference> {
    for delimiter in ['@', ':'] {
        let Some((left, right)) = value.split_once(delimiter) else {
            continue;
        };
        if let Some(object_id) = parse_hex_object_id(left) {
            return Some(FormReference::new(right.trim(), object_id));
        }
        if let Some(object_id) = parse_hex_object_id(right) {
            return Some(FormReference::new(left.trim(), object_id));
        }
    }
    None
}

fn object_id_from_form_key(value: &str) -> Option<u32> {
    value
        .split(['@', ':'])
        .find_map(parse_hex_object_id)
        .map(|object_id| object_id & 0x00FF_FFFF)
}

fn parse_hex_object_id(value: &str) -> Option<u32> {
    let trimmed = value.trim();
    let hex = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    if hex.is_empty() || hex.len() > 8 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

fn form_id_hex(value: u32) -> String {
    format!("{:06X}", value & 0x00FF_FFFF)
}

fn target_cell_editor_id_collision_set(opts: &Options, source_game: &str) -> HashSet<String> {
    if source_game != "fo76" || normalized_game_option(&opts.target_game) != "fo4" {
        return HashSet::new();
    }
    opts.target_cell_editor_ids
        .iter()
        .filter(|editor_id| !editor_id.is_empty())
        .map(|editor_id| normalized_editor_id(editor_id))
        .collect()
}

fn rename_projected_cell_editor_id_collision(
    value: &mut serde_json::Value,
    target_cell_editor_ids: &mut HashSet<String>,
) -> Option<(String, String)> {
    let original = projected_cell_editor_id(value)?;
    if original.is_empty() || !target_cell_editor_ids.contains(&normalized_editor_id(&original)) {
        return None;
    }

    let mut candidate = format!("{original}fo76");
    let mut suffix = 1_u32;
    while target_cell_editor_ids.contains(&normalized_editor_id(&candidate)) {
        candidate = format!("{original}fo76{suffix}");
        suffix += 1;
    }
    set_projected_cell_editor_id(value, &candidate);
    target_cell_editor_ids.insert(normalized_editor_id(&candidate));
    Some((original, candidate))
}

fn projected_cell_editor_id(value: &serde_json::Value) -> Option<String> {
    for key in ["eid", "editor_id", "EditorID"] {
        if let Some(editor_id) = value.get(key).and_then(|value| value.as_str()) {
            return Some(editor_id.to_owned());
        }
    }
    None
}

fn set_projected_cell_editor_id(value: &mut serde_json::Value, editor_id: &str) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    for key in ["eid", "editor_id", "EditorID"] {
        if object.contains_key(key) {
            object.insert(
                key.to_owned(),
                serde_json::Value::String(editor_id.to_owned()),
            );
        }
    }
    set_projected_cell_edid_subrecord(object, editor_id);
}

fn set_projected_cell_edid_subrecord(
    object: &mut serde_json::Map<String, serde_json::Value>,
    editor_id: &str,
) {
    let Some(subrecords) = object
        .get_mut("subrecords")
        .and_then(|value| value.as_array_mut())
    else {
        return;
    };
    for subrecord in subrecords {
        let Some(subrecord) = subrecord.as_object_mut() else {
            continue;
        };
        let is_edid = subrecord
            .get("signature")
            .and_then(|value| value.as_str())
            .is_some_and(|signature| signature.eq_ignore_ascii_case("EDID"));
        if is_edid {
            subrecord.insert(
                "data_hex".to_owned(),
                serde_json::Value::String(zstring_hex(editor_id)),
            );
        }
    }
}

fn zstring_hex(value: &str) -> String {
    let mut out = String::with_capacity((value.len() + 1) * 2);
    for byte in value.as_bytes() {
        out.push_str(&format!("{byte:02X}"));
    }
    out.push_str("00");
    out
}

fn normalized_game_option(game: &str) -> String {
    game.trim().to_ascii_lowercase().replace('-', "")
}

fn normalized_editor_id(editor_id: &str) -> String {
    editor_id.to_ascii_lowercase()
}

fn is_projected_cell_record(
    record: &terrain_native::authoring_emit::AuthoringRecordPayload,
) -> bool {
    record.signature == "CELL" && record.yaml.contains("\nLandscape:")
}

fn normalize_source_game(source_game: &str) -> Result<String, String> {
    let normalized = source_game.trim().to_ascii_lowercase().replace('-', "");
    if normalized.is_empty() {
        return Err("terrain conversion requires source_game".to_owned());
    }
    if crate::translator::Game::from_str(&normalized).is_none() {
        return Err(format!(
            "unknown source game for terrain conversion: {source_game}"
        ));
    }
    Ok(normalized)
}

/// Build the FO76→FO4 terrain texture manifest in two steps:
/// 1. For each LTEX form_key, call `build_bundle` (TXST or BGSM branch).
/// 2. Attach any direct LTEX grass and BTD/GCVR grass to the same standard LTEX.
/// Writes the manifest as pretty-printed JSON to `manifest_output_path`.
/// Returns the in-memory manifest so callers can mutate it (e.g. populating
/// grass.assets) before re-serializing.
///
/// The plugin handle is NOT thread-safe — this loop is sequential by design.
pub fn build_manifest(
    handle_id: u64,
    fo76_data_dir: &Path,
    source_extracted_dir: Option<&Path>,
    required_texture_usages: &[terrain_native::authoring_emit::RequiredTextureUsage],
    extraction_root: &Path,
    output_prefix_root: &str,
    source_game: &str,
    manifest_output_path: &Path,
) -> Result<TextureManifest, String> {
    let resolver = Ba2Resolver::open_with_extracted_dir(fo76_data_dir, source_extracted_dir)?;
    let mut usages_by_ltex: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for usage in required_texture_usages {
        let ground_cover_form_keys = usages_by_ltex
            .entry(usage.ltex_form_key.clone())
            .or_default();
        if let Some(gcvr_form_key) = usage.ground_cover_form_key.clone() {
            ground_cover_form_keys.insert(gcvr_form_key);
        }
    }
    let mut textures = Vec::with_capacity(usages_by_ltex.len());
    let mut gcvr_ltex_cache: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (ltex_fk, ground_cover_form_keys) in usages_by_ltex {
        let mut bundle = build_bundle(
            handle_id,
            &ltex_fk,
            &resolver,
            extraction_root,
            output_prefix_root,
        )?;
        bundle.source_gcvr_form_key = None;
        bundle.source_gcvr_editor_id = None;
        bundle.grass = grass_entries_for_ltex(handle_id, &ltex_fk, source_game)?;
        let mut seen_grass = bundle
            .grass
            .iter()
            .map(|grass| normalize_esp_form_key(&grass.source_form_key).into_owned())
            .collect::<BTreeSet<_>>();

        for gcvr_form_key in ground_cover_form_keys {
            if !gcvr_supports_ltex(handle_id, &gcvr_form_key, &ltex_fk, &mut gcvr_ltex_cache)? {
                continue;
            }
            for grass in grass_entries_for_gcvr(handle_id, &gcvr_form_key, source_game)? {
                if seen_grass.insert(normalize_esp_form_key(&grass.source_form_key).into_owned()) {
                    bundle.grass.push(grass);
                }
            }
        }
        textures.push(bundle);
    }

    let manifest = TextureManifest { textures };
    if let Some(parent) = manifest_output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let json =
        serde_json::to_string_pretty(&manifest).map_err(|e| format!("serialize manifest: {e}"))?;
    fs::write(manifest_output_path, json)
        .map_err(|e| format!("write manifest {}: {e}", manifest_output_path.display()))?;
    Ok(manifest)
}

fn gcvr_supports_ltex(
    handle_id: u64,
    gcvr_form_key: &str,
    ltex_form_key: &str,
    gcvr_ltex_cache: &mut BTreeMap<String, BTreeSet<String>>,
) -> Result<bool, String> {
    let gcvr_key = normalize_esp_form_key(gcvr_form_key).into_owned();
    if !gcvr_ltex_cache.contains_key(&gcvr_key) {
        let refs = gcvr_land_texture_form_keys(handle_id, &gcvr_key)?;
        let normalized_refs = refs
            .into_iter()
            .map(|value| normalize_esp_form_key(&value).into_owned())
            .collect::<BTreeSet<_>>();
        gcvr_ltex_cache.insert(gcvr_key.clone(), normalized_refs);
    }
    let Some(land_textures) = gcvr_ltex_cache.get(&gcvr_key) else {
        return Ok(false);
    };
    if land_textures.is_empty() {
        return Ok(true);
    }
    let ltex_key = normalize_esp_form_key(ltex_form_key).into_owned();
    Ok(land_textures.contains(&ltex_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fo76_fo4_projected_cell_editor_id_collision_gets_suffix() {
        let opts = Options {
            source_game: "fo76".to_owned(),
            target_game: "fo4".to_owned(),
            target_cell_editor_ids: vec!["RelayTower03Ext".to_owned()],
            ..minimal_options()
        };
        let mut target_cell_editor_ids = target_cell_editor_id_collision_set(&opts, "fo76");
        let mut value = json!({
            "signature": "CELL",
            "eid": "RelayTower03Ext",
            "subrecords": [
                {
                    "signature": "EDID",
                    "data_hex": "52656C6179546F776572303345787400"
                },
                {
                    "signature": "DATA",
                    "data_hex": "0200"
                }
            ],
            "Landscape": {}
        });

        let rename =
            rename_projected_cell_editor_id_collision(&mut value, &mut target_cell_editor_ids);

        assert_eq!(
            rename,
            Some((
                "RelayTower03Ext".to_owned(),
                "RelayTower03Extfo76".to_owned()
            ))
        );
        assert_eq!(
            value.get("eid").and_then(|value| value.as_str()),
            Some("RelayTower03Extfo76")
        );
        assert_eq!(
            value
                .get("subrecords")
                .and_then(|value| value.as_array())
                .and_then(|subrecords| subrecords.first())
                .and_then(|subrecord| subrecord.get("data_hex"))
                .and_then(|value| value.as_str()),
            Some("52656C6179546F7765723033457874666F373600")
        );
        assert!(target_cell_editor_ids.contains("relaytower03extfo76"));
    }

    #[test]
    fn projected_cell_editor_id_collision_suffix_avoids_existing_fo76_name() {
        let opts = Options {
            source_game: "fo76".to_owned(),
            target_game: "fo4".to_owned(),
            target_cell_editor_ids: vec![
                "RelayTower03Ext".to_owned(),
                "RelayTower03Extfo76".to_owned(),
            ],
            ..minimal_options()
        };
        let mut target_cell_editor_ids = target_cell_editor_id_collision_set(&opts, "fo76");
        let mut value = json!({
            "signature": "CELL",
            "editor_id": "RelayTower03Ext",
            "Landscape": {}
        });

        let rename =
            rename_projected_cell_editor_id_collision(&mut value, &mut target_cell_editor_ids);

        assert_eq!(
            rename,
            Some((
                "RelayTower03Ext".to_owned(),
                "RelayTower03Extfo761".to_owned()
            ))
        );
        assert_eq!(
            value.get("editor_id").and_then(|value| value.as_str()),
            Some("RelayTower03Extfo761")
        );
    }

    #[test]
    fn projected_cell_editor_id_is_unchanged_without_target_collision() {
        let opts = Options {
            source_game: "fo76".to_owned(),
            target_game: "fo4".to_owned(),
            target_cell_editor_ids: vec!["RelayTower03Ext".to_owned()],
            ..minimal_options()
        };
        let mut target_cell_editor_ids = target_cell_editor_id_collision_set(&opts, "fo76");
        let mut value = json!({
            "signature": "CELL",
            "eid": "RelayTower05Ext",
            "Landscape": {}
        });

        let rename =
            rename_projected_cell_editor_id_collision(&mut value, &mut target_cell_editor_ids);

        assert_eq!(rename, None);
        assert_eq!(
            value.get("eid").and_then(|value| value.as_str()),
            Some("RelayTower05Ext")
        );
    }

    #[test]
    fn target_cell_collision_set_is_only_enabled_for_fo76_to_fo4() {
        let opts = Options {
            source_game: "fo76".to_owned(),
            target_game: "skyrimse".to_owned(),
            target_cell_editor_ids: vec!["RelayTower03Ext".to_owned()],
            ..minimal_options()
        };

        assert!(target_cell_editor_id_collision_set(&opts, "fo76").is_empty());
        assert!(target_cell_editor_id_collision_set(&opts, "fo4").is_empty());
    }

    #[test]
    fn terrain_record_remaps_vanilla_grass_to_target_master() {
        let opts = Options {
            plugin_name: "SeventySix.esm".to_owned(),
            target_record_reuse: vec![crate::terrain_textures::options::TargetRecordReuseRef {
                editor_id: "GrassphaltObj03".to_owned(),
                signature: "GRAS".to_owned(),
                form_key: "044181:Fallout4.esm".to_owned(),
            }],
            ..minimal_options()
        };
        let mut remaps = TerrainRecordRemaps::new(&opts);
        let grass_record = terrain_native::authoring_emit::AuthoringRecordPayload {
            signature: "GRAS".to_owned(),
            relative_path: "records/GRAS/GrassphaltObj03.yaml".to_owned(),
            yaml: String::new(),
        };
        let grass_value = json!({
            "signature": "GRAS",
            "form_id": "044181",
            "eid": "GrassphaltObj03",
            "fields": [],
        });

        assert!(remaps.skip_or_track(&grass_record, &grass_value));

        let mut ltex = json!({
            "signature": "LTEX",
            "fields": [{
                "Grass": {
                    "reference": {
                        "plugin": "SeventySix.esm",
                        "object_id": "044181"
                    }
                }
            }]
        });
        assert_eq!(remaps.rewrite_references(&mut ltex), 1);
        let reference = &ltex["fields"][0]["Grass"]["reference"];
        assert_eq!(reference["plugin"], "Fallout4.esm");
        assert_eq!(reference["object_id"], "044181");
    }

    #[test]
    fn terrain_record_remaps_duplicate_grass_to_first_output_record() {
        let opts = Options {
            plugin_name: "SeventySix.esm".to_owned(),
            ..minimal_options()
        };
        let mut remaps = TerrainRecordRemaps::new(&opts);
        let grass_record = terrain_native::authoring_emit::AuthoringRecordPayload {
            signature: "GRAS".to_owned(),
            relative_path: "records/GRAS/MtnRemovalRockObj02.yaml".to_owned(),
            yaml: String::new(),
        };
        let first_grass = json!({
            "signature": "GRAS",
            "form_id": "00869F",
            "eid": "MtnRemovalRockObj02",
            "fields": [],
        });
        let duplicate_grass = json!({
            "signature": "GRAS",
            "form_id": "A09E26",
            "eid": "MtnRemovalRockObj02",
            "fields": [],
        });

        assert!(!remaps.skip_or_track(&grass_record, &first_grass));
        assert!(remaps.skip_or_track(&grass_record, &duplicate_grass));

        let mut ltex = json!({
            "signature": "LTEX",
            "fields": [{
                "Grass": {
                    "reference": {
                        "plugin": "SeventySix.esm",
                        "object_id": "A09E26"
                    }
                }
            }]
        });
        assert_eq!(remaps.rewrite_references(&mut ltex), 1);
        let reference = &ltex["fields"][0]["Grass"]["reference"];
        assert_eq!(reference["plugin"], "SeventySix.esm");
        assert_eq!(reference["object_id"], "00869F");
    }

    fn minimal_options() -> Options {
        Options {
            source_game: String::new(),
            target_game: String::new(),
            source_plugin_path: String::new(),
            source_handle_id: 1,
            fo76_data_dir: String::new(),
            source_extracted_dir: String::new(),
            btd_path: String::new(),
            output_authoring_dir: String::new(),
            plugin_name: "Test.esp".to_owned(),
            worldspace_editor_id: "TestWorld".to_owned(),
            source_min_x: 0,
            source_min_y: 0,
            source_max_x: 0,
            source_max_y: 0,
            world_form_id: 0,
            first_cell_form_id: 0,
            resample_mode: "sample4".to_owned(),
            debug_output_dir: String::new(),
            emit_textures: true,
            write_materials: true,
            export_heightmap: false,
            debug_flat_land: false,
            preserve_source_ids: true,
            reserved_object_ids: Vec::new(),
            source_worldspace_authoring_dir: String::new(),
            source_worldspace_terrain_ids_json: String::new(),
            heightmap_output_path: String::new(),
            btd4_output_path: String::new(),
            water_manifest_path: String::new(),
            populate_grass_assets: true,
            convert_grass_assets: true,
            record_output_mode: RecordOutputMode::AuthoringDir,
            target_handle_id: None,
            target_cell_editor_ids: Vec::new(),
            target_record_reuse: Vec::new(),
            conversion_workers: None,
            land_skip_ground_cover_variants: false,
            reuse_existing_textures: false,
        }
    }

    #[test]
    fn terrain_texture_job_keeps_manifest_paths() {
        use crate::terrain_textures::manifest::{TerrainTextureJob, TextureBundle};

        let bundle = TextureBundle {
            diffuse_path: "source/land_d.dds".to_owned(),
            normal_path: "source/land_n.dds".to_owned(),
            reflectivity_path: "source/land_r.dds".to_owned(),
            lighting_path: "source/land_l.dds".to_owned(),
            output_prefix: "textures/terrain/appalachia/Land".to_owned(),
            ..Default::default()
        };

        let job = TerrainTextureJob::from(&bundle);

        assert_eq!(job.diffuse_path, bundle.diffuse_path);
        assert_eq!(job.normal_path, bundle.normal_path);
        assert_eq!(job.reflectivity_path, bundle.reflectivity_path);
        assert_eq!(job.lighting_path, bundle.lighting_path);
        assert_eq!(job.output_prefix, bundle.output_prefix);
    }

    #[test]
    fn gcvr_supports_ltex_uses_normalized_land_texture_refs() {
        let mut cache = BTreeMap::from([(
            "SeventySix.esm:081263".to_owned(),
            BTreeSet::from(["SeventySix.esm:00E559".to_owned()]),
        )]);

        assert!(
            gcvr_supports_ltex(
                0,
                "081263:SeventySix.esm",
                "00E559:SeventySix.esm",
                &mut cache
            )
            .unwrap()
        );
        assert!(
            !gcvr_supports_ltex(
                0,
                "081263:SeventySix.esm",
                "00D677:SeventySix.esm",
                &mut cache
            )
            .unwrap()
        );
    }
}
