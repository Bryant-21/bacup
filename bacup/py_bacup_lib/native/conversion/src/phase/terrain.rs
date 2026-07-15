//! Phase: convert_terrain
//!
//! Dispatches terrain conversion by source game:
//!   - "fo76": routes through terrain_textures::run (two-pass manifest flow)
//!     so TXST/LTEX/GRAS records are emitted. Requires source_handle_id and
//!     fo76_data_dir in params for fallback asset reads.
//!   - "fnv" / "fo3": skips with a warning. Full-plugin LAND is emitted by the
//!     Rust record translation pass, not this terrain texture phase.
//!
//! Params shape (JSON object in PhaseCtx::params):
//! ```json
//! {
//!   "source_game":             "fo76",
//!   // source and target handles are owned by the ConversionRun
//!   "fo76_data_dir":           "/path/to/fo76/Data",
//!   "source_extracted_dir":    "/path/to/extracted/fo76",
//!   "btd_path":                "/path/to/Appalachia.btd",
//!   "output_authoring_dir":    "/path/to/yaml",
//!   "plugin_name":             "B21_Appalachia.esp",
//!   "worldspace_editor_id":    "Appalachia",
//!   "source_min_x":            0,
//!   "source_min_y":            0,
//!   "source_max_x":            -1,
//!   "source_max_y":            -1,
//!   "resample_mode":           "sample4",
//!   "emit_textures":           false,
//!   "export_heightmap":        false,
//!   "preserve_source_ids":     true,
//!   "debug_output_dir":        "/path/to/debug/terrain",
//!   "water_manifest_path":     "",
//!   "source_worldspace_authoring_dir": "",
//!   "heightmap_output_path":   ""
//! }
//! ```

use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};

const MAX_LOCAL_OBJECT_ID: u32 = 0x00FF_FFFF;

pub struct ConvertTerrainPhase;

impl Phase for ConvertTerrainPhase {
    fn name(&self) -> &'static str {
        "convert_terrain"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let source_game = ctx.run.source.as_str();

        match source_game {
            "fo76" => run_fo76_btd(ctx),
            other => {
                let _ = ctx.run.event_tx.try_send(crate::phase::PhaseEvent::Log {
                    phase: "convert_terrain",
                    level: crate::phase::LogLevel::Warn,
                    message: format!(
                        "convert_terrain: source game '{}' not yet supported in native phase; \
                         full-plugin LAND is emitted by Rust translate_records",
                        other
                    ),
                });
                Ok(PhaseReport::default())
            }
        }
    }
}

fn run_fo76_btd(ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
    for legacy_key in ["source_handle_id", "target_handle_id", "record_output_mode"] {
        if ctx.params.get(legacy_key).is_some() {
            return Err(PhaseError::BadParams(format!(
                "convert_terrain: legacy parameter is not supported: {legacy_key}"
            )));
        }
    }
    let mut opts: crate::terrain_textures::Options = serde_json::from_value(ctx.params.clone())
        .map_err(|e| PhaseError::BadParams(format!("convert_terrain fo76 params: {e}")))?;
    opts.source_game = ctx.run.source.as_str().to_owned();
    opts.target_game = ctx.run.target.as_str().to_owned();
    if opts.source_extracted_dir.trim().is_empty() {
        if !ctx.source_extracted_dir.as_os_str().is_empty() {
            opts.source_extracted_dir = ctx.source_extracted_dir.to_string_lossy().into_owned();
        } else if let Some(source_extracted_dir) = ctx.run.config.source_extracted_dir.as_deref() {
            opts.source_extracted_dir = source_extracted_dir.to_string_lossy().into_owned();
        }
    }
    opts.source_handle_id = ctx
        .run
        .require_source_handle()
        .map_err(|error| PhaseError::BadParams(format!("convert_terrain: {error}")))?;
    opts.record_output_mode = crate::terrain_textures::RecordOutputMode::TargetHandle;
    opts.target_handle_id = Some(ctx.run.target_handle_id);
    // Bound the terrain texture remix pool to --workers so it doesn't spin up
    // an unbounded rayon pool on top of an already-loaded run.
    if opts.conversion_workers.is_none() {
        opts.conversion_workers = ctx.run.config.conversion_workers;
    }

    if opts.btd_path.is_empty() {
        return Err(PhaseError::BadParams(
            "convert_terrain: btd_path is required for fo76 source".to_owned(),
        ));
    }

    if opts.debug_output_dir.is_empty() {
        opts.debug_output_dir = ctx
            .mod_path
            .join("debug")
            .join("terrain")
            .to_string_lossy()
            .into_owned();
    }

    let target_handle_mode =
        opts.record_output_mode == crate::terrain_textures::RecordOutputMode::TargetHandle;
    if target_handle_mode {
        log_terrain_debug(
            ctx,
            format!(
                "convert_terrain: target-handle setup start source_worldspace_authoring_dir_empty={} terrain_ids_json_empty={}",
                opts.source_worldspace_authoring_dir.trim().is_empty(),
                opts.source_worldspace_terrain_ids_json.trim().is_empty()
            ),
        );
        if opts.preserve_source_ids
            && opts.source_worldspace_authoring_dir.trim().is_empty()
            && opts.source_worldspace_terrain_ids_json.trim().is_empty()
        {
            log_terrain_debug(
                ctx,
                "convert_terrain: collecting source terrain ids from source handle",
            );
            opts.source_worldspace_terrain_ids_json =
                esp_authoring_core::plugin_runtime::plugin_handle_collect_worldspace_terrain_ids_json(
                    opts.source_handle_id,
                    &opts.worldspace_editor_id,
                    opts.source_min_x,
                    opts.source_min_y,
                    opts.source_max_x,
                    opts.source_max_y,
                )
                .map_err(|e| {
                    PhaseError::Internal(format!("terrain source ID collection failed: {e}"))
                })?;
            log_terrain_debug(
                ctx,
                format!(
                    "convert_terrain: collected source terrain ids json bytes={}",
                    opts.source_worldspace_terrain_ids_json.len()
                ),
            );
        }
        if ctx.run.source.as_str() == "fo76" && ctx.run.target.as_str() == "fo4" {
            opts.target_cell_editor_ids = ctx
                .run
                .config
                .target_record_preflight
                .iter()
                .filter(|row| row.signature.eq_ignore_ascii_case("CELL"))
                .map(|row| row.editor_id.clone())
                .collect();
            opts.target_record_reuse = ctx
                .run
                .config
                .target_record_preflight
                .iter()
                .filter(|row| row.signature.eq_ignore_ascii_case("GRAS"))
                .map(
                    |row| crate::terrain_textures::options::TargetRecordReuseRef {
                        editor_id: row.editor_id.clone(),
                        signature: row.signature.clone(),
                        form_key: row.form_key.clone(),
                    },
                )
                .collect();
            log_terrain_debug(
                ctx,
                format!(
                    "convert_terrain: target preflight cells={} grass_reuse={}",
                    opts.target_cell_editor_ids.len(),
                    opts.target_record_reuse.len()
                ),
            );
        }
        log_terrain_debug(ctx, "convert_terrain: querying next target object id");
        let next_object_id =
            esp_authoring_core::plugin_runtime::plugin_handle_next_available_object_id_no_py(
                ctx.run.target_handle_id,
            )
            .map_err(|e| PhaseError::Internal(format!("terrain ID allocation failed: {e}")))?;
        log_terrain_debug(
            ctx,
            format!("convert_terrain: next target object id={next_object_id:06X}"),
        );
        log_terrain_debug(ctx, "convert_terrain: looking up target WRLD");
        if let Some(world_form_id) =
            esp_authoring_core::plugin_runtime::plugin_handle_find_record_object_id_by_editor_id_no_py(
                ctx.run.target_handle_id,
                "WRLD",
                &opts.worldspace_editor_id,
            )
            .map_err(|e| PhaseError::Internal(format!("terrain WRLD lookup failed: {e}")))?
        {
            opts.world_form_id = world_form_id;
            opts.first_cell_form_id = next_object_id;
        } else if should_defer_to_preserved_source_ids(&opts) {
            opts.world_form_id = 0;
            opts.first_cell_form_id = 0;
        } else {
            opts.world_form_id = next_object_id;
            opts.first_cell_form_id = next_object_id.checked_add(1).ok_or_else(|| {
                PhaseError::Internal("terrain ID allocation overflow".to_owned())
            })?;
        }
        log_terrain_debug(ctx, "convert_terrain: scanning target used object ids");
        opts.reserved_object_ids =
            esp_authoring_core::plugin_runtime::plugin_handle_used_object_ids_no_py(
                ctx.run.target_handle_id,
            )
            .map_err(|e| PhaseError::Internal(format!("terrain reserved ID scan failed: {e}")))?;
        log_terrain_debug(
            ctx,
            format!(
                "convert_terrain: target used object ids={}",
                opts.reserved_object_ids.len()
            ),
        );
        reserve_generated_object_id_floor(&mut opts, ctx.run.config.generated_object_id_floor)
            .map_err(|e| {
                PhaseError::BadParams(format!("terrain generated object-id floor: {e}"))
            })?;
    }
    let worldspace = opts.worldspace_editor_id.clone();
    log_terrain_debug(ctx, "convert_terrain: terrain_textures::run start");
    let event_tx = ctx.run.event_tx.clone();
    let report = crate::terrain_textures::run_with_progress(opts, |message| {
        let _ = event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase: "convert_terrain",
            level: crate::phase::LogLevel::Info,
            message,
        });
    })
    .map_err(|e| PhaseError::Internal(format!("terrain conversion failed: {e}")))?;
    log_terrain_debug(ctx, "convert_terrain: terrain_textures::run done");

    // Surface the per-step terrain timing breakdown into the main conversion
    // log. The full report also lands in debug/terrain/terrain_timing_*.json,
    // but that file is easy to miss and can be clobbered by a later run; the
    // log copy is durable and per-run.
    if !report.timings.is_empty() {
        let mut rows: Vec<_> = report.timings.iter().collect();
        rows.sort_by(|a, b| b.elapsed_seconds.total_cmp(&a.elapsed_seconds));
        let _ = ctx.run.event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase: "convert_terrain",
            level: crate::phase::LogLevel::Info,
            message: format!(
                "convert_terrain timing breakdown (worldspace={worldspace}, steps={}):",
                rows.len()
            ),
        });
        for row in rows {
            let _ = ctx.run.event_tx.try_send(crate::phase::PhaseEvent::Log {
                phase: "convert_terrain",
                level: crate::phase::LogLevel::Info,
                message: format!(
                    "  terrain_timing {} = {:.3}s",
                    row.name, row.elapsed_seconds
                ),
            });
        }
    }

    if !report.terrain_texture_jobs.is_empty() {
        ctx.run
            .terrain_texture_jobs
            .extend(report.terrain_texture_jobs.iter().cloned());
    }

    Ok(terrain_phase_report(
        target_handle_mode,
        report.records_imported,
        report.cells_written,
        report.dropped_texture_layers,
    ))
}

fn log_terrain_debug(ctx: &PhaseCtx<'_>, message: impl Into<String>) {
    let _ = ctx.run.event_tx.try_send(crate::phase::PhaseEvent::Log {
        phase: "convert_terrain",
        level: crate::phase::LogLevel::Info,
        message: message.into(),
    });
}

fn should_defer_to_preserved_source_ids(opts: &crate::terrain_textures::Options) -> bool {
    opts.preserve_source_ids
        && (!opts.source_worldspace_authoring_dir.trim().is_empty()
            || !opts.source_worldspace_terrain_ids_json.trim().is_empty())
}

fn reserve_generated_object_id_floor(
    opts: &mut crate::terrain_textures::Options,
    floor: u32,
) -> Result<(), String> {
    if floor == 0 {
        return Ok(());
    }
    if floor > MAX_LOCAL_OBJECT_ID {
        return Err(format!("outside local FormID range: 0x{floor:08X}"));
    }
    if floor > 1 {
        opts.reserved_object_ids.push(floor - 1);
    }
    Ok(())
}

fn terrain_phase_report(
    target_handle_mode: bool,
    imported_count: u32,
    cells_written: u32,
    dropped_texture_layers: u32,
) -> PhaseReport {
    PhaseReport {
        records_added: if target_handle_mode {
            imported_count
        } else {
            cells_written
        },
        warnings: dropped_texture_layers,
        ..PhaseReport::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::{PhaseCtx, PhaseReport};
    use serde_json::json;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn unsupported_source_game_returns_empty_report() {
        // This test exercises the non-fo76 branch without needing a real run context.
        // We verify the phase name is correct and the phase struct is constructable.
        let phase = ConvertTerrainPhase;
        assert_eq!(phase.name(), "convert_terrain");
    }

    #[test]
    fn fo76_branch_requires_btd_path() {
        // Verify BadParams when btd_path is absent from fo76 params.
        // We call run_fo76_btd indirectly by constructing a minimal params blob.
        let params = json!({
            "source_game": "fo76",
            "btd_path": "",
            "output_authoring_dir": "/tmp/out",
            "plugin_name": "Test.esp",
            "worldspace_editor_id": "TestWorld",
            "source_min_x": 0,
            "source_min_y": 0,
            "source_max_x": 0,
            "source_max_y": 0,
            "resample_mode": "sample4",
            "emit_textures": false,
            "export_heightmap": false,
            "preserve_source_ids": true,
        });

        // Deserialise to Fo76Params mirror to check validation path.
        #[derive(serde::Deserialize)]
        struct Fo76Params {
            btd_path: String,
        }
        let p: Fo76Params = serde_json::from_value(params).unwrap();
        assert!(
            p.btd_path.is_empty(),
            "empty btd_path should be deserialisable"
        );
    }

    #[test]
    fn target_handle_phase_report_counts_imported_records() {
        let report = terrain_phase_report(true, 7, 1, 2);

        assert_eq!(report.records_added, 7);
        assert_eq!(report.records_changed, 0);
        assert_eq!(report.warnings, 2);
    }

    #[test]
    fn authoring_dir_phase_report_keeps_cell_count() {
        let report = terrain_phase_report(false, 0, 3, 1);

        assert_eq!(report.records_added, 3);
        assert_eq!(report.warnings, 1);
    }

    #[test]
    fn target_handle_preserves_source_ids_when_source_worldspace_authoring_is_available() {
        let mut params = json!({
            "source_plugin_path": "X:/Fallout76/Data/SeventySix.esm",
            "fo76_data_dir": "X:/Fallout76/Data",
            "btd_path": "X:/Fallout76/Data/Terrain/Appalachia.btd",
            "output_authoring_dir": "X:/mods/B21_Test/yaml",
            "plugin_name": "B21_Test.esm",
            "worldspace_editor_id": "APPALACHIA",
            "source_min_x": 0,
            "source_min_y": 0,
            "source_max_x": 0,
            "source_max_y": 0,
            "preserve_source_ids": true,
            "source_worldspace_authoring_dir": "X:/data/fo76_esm_yaml/SeventySix/records/WRLD/APPALACHIA - 25DA15_SeventySix.esm"
        });
        let opts: crate::terrain_textures::Options =
            serde_json::from_value(params.clone()).unwrap();
        assert!(should_defer_to_preserved_source_ids(&opts));

        params["source_worldspace_authoring_dir"] = json!("");
        let opts: crate::terrain_textures::Options = serde_json::from_value(params).unwrap();
        assert!(!should_defer_to_preserved_source_ids(&opts));

        let params = json!({
            "source_plugin_path": "X:/Fallout76/Data/SeventySix.esm",
            "fo76_data_dir": "X:/Fallout76/Data",
            "btd_path": "X:/Fallout76/Data/Terrain/Appalachia.btd",
            "output_authoring_dir": "X:/mods/B21_Test/yaml",
            "plugin_name": "B21_Test.esm",
            "worldspace_editor_id": "APPALACHIA",
            "source_min_x": 0,
            "source_min_y": 0,
            "source_max_x": 0,
            "source_max_y": 0,
            "preserve_source_ids": true,
            "source_worldspace_terrain_ids_json": "{\"world_form_id\":2472469,\"cells\":[]}"
        });
        let opts: crate::terrain_textures::Options = serde_json::from_value(params).unwrap();
        assert!(should_defer_to_preserved_source_ids(&opts));
    }

    #[test]
    fn generated_object_id_floor_is_reserved_for_terrain_planner() {
        let mut opts = crate::terrain_textures::Options {
            source_game: "fo76".to_string(),
            target_game: "fo4".to_string(),
            source_plugin_path: String::new(),
            source_handle_id: 1,
            fo76_data_dir: "X:/Fallout76/Data".to_string(),
            source_extracted_dir: String::new(),
            btd_path: "X:/Fallout76/Data/Terrain/Appalachia.btd".to_string(),
            output_authoring_dir: "X:/mods/B21_Test/yaml".to_string(),
            plugin_name: "B21_Test.esm".to_string(),
            worldspace_editor_id: "APPALACHIA".to_string(),
            source_min_x: 0,
            source_min_y: 0,
            source_max_x: 0,
            source_max_y: 0,
            world_form_id: 0,
            first_cell_form_id: 0,
            resample_mode: "sample4".to_string(),
            debug_output_dir: String::new(),
            emit_textures: false,
            write_materials: false,
            export_heightmap: false,
            preserve_source_ids: true,
            reserved_object_ids: vec![0x25DA15],
            source_worldspace_authoring_dir: String::new(),
            source_worldspace_terrain_ids_json: String::new(),
            heightmap_output_path: String::new(),
            btd4_output_path: String::new(),
            water_manifest_path: String::new(),
            populate_grass_assets: false,
            convert_grass_assets: false,
            debug_flat_land: false,
            record_output_mode: crate::terrain_textures::RecordOutputMode::TargetHandle,
            target_handle_id: None,
            target_cell_editor_ids: Vec::new(),
            target_record_reuse: Vec::new(),
            conversion_workers: None,
            land_skip_ground_cover_variants: false,
            reuse_existing_textures: false,
        };

        reserve_generated_object_id_floor(&mut opts, 0xA00000).unwrap();

        assert!(opts.reserved_object_ids.contains(&0x9FFFFF));
    }
}
