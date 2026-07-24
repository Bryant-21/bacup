//! convert_textures_v2 — the canonical native texture phase. It accepts the
//! shared texture-phase param surface and routes it into
//! texture_engine::run_texture_engine. No Python in phase code.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use materials_native::texture_convert::TextureConversionParamsPayload;

use crate::phase::copy_textures::discover_nif_texture_dependencies_with_progress;
use crate::phase::progress::ProgressReporter;
use crate::phase::textures::{extract_f32, parse_conversion_workers, parse_texture_entries};
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::terrain_textures::manifest::TerrainTextureJob;
use crate::texture_engine::{TextureEngineParams, TextureEntryIn, run_texture_engine};

pub struct ConvertTexturesV2Phase;

fn parse_pbr_carry(params: &serde_json::Value) -> bool {
    params
        .get("pbr_carry")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn parse_landscape_mip_flooding(params: &serde_json::Value) -> bool {
    params
        .get("landscape_mip_flooding")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn dedup_key(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

/// Append NIF-discovered textures the explicit list does not already cover.
///
/// The same file reached through two roots would otherwise land in the engine
/// twice and race on its own output.
fn extend_with_nif_dependencies(
    textures: &mut Vec<TextureEntryIn>,
    discovered: Vec<(String, String)>,
) {
    let mut seen: std::collections::HashSet<String> =
        textures.iter().map(|e| dedup_key(&e.source_path)).collect();
    for (_source_path, resolved_path) in discovered {
        if resolved_path.is_empty() || !seen.insert(dedup_key(&resolved_path)) {
            continue;
        }
        textures.push(TextureEntryIn {
            source_path: resolved_path,
            output_subpath: None,
        });
    }
}

fn parse_terrain_jobs(params: &serde_json::Value) -> Result<Vec<TerrainTextureJob>, PhaseError> {
    let Some(value) = params.get("terrain_jobs") else {
        return Ok(Vec::new());
    };
    serde_json::from_value(value.clone())
        .map_err(|err| PhaseError::BadParams(format!("convert_textures_v2 terrain_jobs: {err}")))
}

impl Phase for ConvertTexturesV2Phase {
    fn name(&self) -> &'static str {
        "convert_textures_v2"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        let p = ctx.params;
        let source_extracted: PathBuf = p
            .get("source_extracted")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| ctx.source_extracted_dir.to_path_buf());
        let mut textures: Vec<TextureEntryIn> = parse_texture_entries(p.get("textures"))
            .into_iter()
            .map(|e| TextureEntryIn {
                source_path: e.source_path,
                output_subpath: e.output_subpath,
            })
            .collect();
        let nif_count = p
            .get("nif_paths")
            .and_then(|value| value.as_array())
            .map(|entries| entries.len().min(u32::MAX as usize) as u32)
            .unwrap_or(0);
        let discovery_started = Instant::now();
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "convert_textures_v2",
            level: LogLevel::Info,
            message: format!(
                "textures_v2: scanning {nif_count} NIFs for embedded texture dependencies"
            ),
        });
        let discovery_reporter =
            ProgressReporter::new("convert_textures_v2", nif_count, ctx.run.event_tx.clone());
        let discovery = discover_nif_texture_dependencies_with_progress(p, &discovery_reporter)?;
        discovery_reporter.finish();
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "convert_textures_v2",
            level: LogLevel::Info,
            message: format!(
                "textures_v2: NIF texture dependency scan completed; nifs={nif_count} discovered={} failed={} elapsed_ms={}",
                discovery.textures.len(),
                discovery.failures,
                discovery_started.elapsed().as_millis(),
            ),
        });
        let discovery_failures = discovery.failures;
        extend_with_nif_dependencies(&mut textures, discovery.textures);
        for warning in &discovery.warnings {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "convert_textures_v2",
                level: LogLevel::Warn,
                message: warning.clone(),
            });
        }
        let format_overrides: HashMap<String, String> = p
            .get("target_format_overrides")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        let mut target_dirs: Vec<PathBuf> = Vec::new();
        if ctx.run.target_assets.is_none() {
            if let Some(t) = ctx.target_extracted_dir {
                target_dirs.push(t.to_path_buf());
            }
            if let Some(t) = ctx.target_data_dir {
                target_dirs.push(t.to_path_buf());
            }
        }

        let engine_params = TextureEngineParams {
            source_extracted,
            data_root: ctx.mod_path.join("data"),
            source_game: ctx.run.source.as_str().to_string(),
            target_game: ctx.run.target.as_str().to_string(),
            textures,
            terrain_jobs: parse_terrain_jobs(p)?,
            convert_all: p
                .get("convert_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            pbr_carry: parse_pbr_carry(p),
            landscape_mip_flooding: parse_landscape_mip_flooding(p),
            relocation_members: ctx.run.relocation_members.clone(),
            namespace: crate::run::base_asset_namespace_for_run(ctx.run),
            target_dirs,
            target_assets: ctx.run.target_assets.clone(),
            skip_existing: p
                .get("skip_existing")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            use_gpu: p.get("use_gpu").and_then(|v| v.as_bool()).unwrap_or(true),
            gpu_min_pixels: p
                .get("gpu_min_pixels")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(512 * 512),
            gpu_queue_cap: 8,
            workers: parse_conversion_workers(p, ctx.run.config.conversion_workers),
            conv_params: TextureConversionParamsPayload {
                ao_multiplier: extract_f32(p, "ao_multiplier", 0.5),
                specular_multiplier: extract_f32(p, "specular_multiplier", 1.0),
                gloss_multiplier: extract_f32(p, "gloss_multiplier", 1.0),
                spec_offset: extract_f32(p, "spec_offset", 0.8),
            },
            format_overrides,
            // Full-run texture archives direct-pack from the loose DDS tree at
            // the unified join, not through the incremental DX10 spill.
            sink: None,
        };

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "convert_textures_v2",
            level: LogLevel::Info,
            message: format!(
                "textures_v2: starting; terrain_bundles={} landscape_mip_flooding={}",
                engine_params.terrain_jobs.len(),
                engine_params.landscape_mip_flooding,
            ),
        });

        let event_tx = ctx.run.event_tx.clone();
        let last_sent = AtomicU64::new(0);
        let progress_cb = move |done: u64, total: u64| {
            let interval = if total == 0 { 1 } else { (total / 1000).max(1) };
            let should_send = loop {
                let previous = last_sent.load(Ordering::Relaxed);
                if done <= previous || (done != total && done.saturating_sub(previous) < interval) {
                    break false;
                }
                if last_sent
                    .compare_exchange_weak(previous, done, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    break true;
                }
            };
            if should_send {
                let _ = event_tx.try_send(PhaseEvent::Progress {
                    phase: "convert_textures_v2",
                    current: done.min(u64::from(u32::MAX)) as u32,
                    total: total.min(u64::from(u32::MAX)) as u32,
                    item: None,
                });
            }
        };

        let report = run_texture_engine(&engine_params, ctx.cancel, Some(&progress_cb))
            .map_err(PhaseError::Internal)?;

        for error in &report.errors {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "convert_textures_v2",
                level: LogLevel::Error,
                message: error.clone(),
            });
        }

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "convert_textures_v2",
            level: LogLevel::Info,
            message: format!(
                "textures_v2: groups={} written={} failed={} skipped_existing={} base_owned_groups={} terrain_replaced_groups={} no_request={} mip_flooded={} classes[pass_through={} per_texel={} per_texel_demoted={} bundle={} residue={}] class_worker_ms[pass_through={} per_texel={} bundle={} residue={}] gpu[submissions={} dispatches={} cpu={} overflow={} failures={}] elapsed_ms={}",
                report.groups,
                report.outputs_written,
                report.failed,
                report.skipped_existing,
                report.skipped_base_owned_groups,
                report.skipped_terrain_groups,
                report.no_request_groups,
                report.mip_flooded_outputs,
                report.class.pass_through,
                report.class.per_texel,
                report.class.per_texel_demoted,
                report.class.bundle,
                report.class.legacy_residue,
                report.class.pass_through_ms,
                report.class.per_texel_ms,
                report.class.bundle_ms,
                report.class.legacy_residue_ms,
                report.gpu.gpu_submissions,
                report.gpu.gpu_dispatch_batches,
                report.gpu.cpu_encodes,
                report.gpu.overflow_to_cpu,
                report.gpu.gpu_failures,
                report.elapsed_ms
            ),
        });

        // Keep the public PhaseReport counters stable for Python summaries.
        let failed = report.failed.min(u64::from(u32::MAX)) as u32 + discovery_failures;
        Ok(PhaseReport {
            assets_written: report.outputs_written.min(u64::from(u32::MAX)) as u32,
            records_dropped: report.skipped_base_owned_outputs.min(u64::from(u32::MAX)) as u32,
            warnings: failed,
            items_failed: failed,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nif_dependencies_extend_the_list_without_duplicating_it() {
        let mut textures = vec![TextureEntryIn {
            source_path: r"C:\src\Textures\Clutter\crate_d.dds".to_owned(),
            output_subpath: Some("Textures/Clutter/crate_d.dds".to_owned()),
        }];

        extend_with_nif_dependencies(
            &mut textures,
            vec![
                // Same file the record graph already supplied, reached with the
                // other separator and casing.
                (
                    "Textures/Clutter/crate_d.dds".to_owned(),
                    "C:/SRC/textures/clutter/crate_d.dds".to_owned(),
                ),
                (
                    "Textures/Clutter/crate_n.dds".to_owned(),
                    r"C:\src\Textures\Clutter\crate_n.dds".to_owned(),
                ),
                // Two NIFs naming the same normal must still yield one entry.
                (
                    "Textures/Clutter/crate_n.dds".to_owned(),
                    r"C:\src\Textures\Clutter\crate_n.dds".to_owned(),
                ),
                ("Textures/Clutter/gone.dds".to_owned(), String::new()),
            ],
        );

        assert_eq!(textures.len(), 2);
        assert!(textures[1].source_path.ends_with("crate_n.dds"));
        assert!(textures[1].output_subpath.is_none());
    }

    #[test]
    fn pbr_carry_param_defaults_off_and_parses_true() {
        assert!(!parse_pbr_carry(&serde_json::json!({})));
        assert!(parse_pbr_carry(&serde_json::json!({ "pbr_carry": true })));
    }

    #[test]
    fn landscape_mip_flooding_defaults_off_and_parses_true() {
        assert!(!parse_landscape_mip_flooding(&serde_json::json!({})));
        assert!(parse_landscape_mip_flooding(
            &serde_json::json!({ "landscape_mip_flooding": true })
        ));
    }

    #[test]
    fn texture_v2_phase_relocation_member_absent_from_params() {
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        fn any_dds_under(dir: &std::path::Path) -> bool {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return false;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if any_dds_under(&p) {
                        return true;
                    }
                } else if p
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("dds"))
                    .unwrap_or(false)
                {
                    return true;
                }
            }
            false
        }

        let tmp = std::env::temp_dir().join("texture_v2_phase_relocation_member_absent");
        let source = tmp.join("source");
        let output = tmp.join("mod");
        let tex_dir = source.join("Textures").join("Landscape").join("Rocks");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tex_dir).unwrap();
        directxtex_native::write_dds_rgba_image(
            &tex_dir.join("rock_d.dds"),
            8,
            8,
            &vec![128u8; 8 * 8 * 4],
            "R8G8B8A8_UNORM",
            false,
        )
        .unwrap();

        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                base_asset_namespace: "FO76".into(),
                ..Default::default()
            },
        })
        .unwrap();

        // Inject a texture member directly (bypassing the compare) to isolate the phase.
        with_run(id, |run| -> Result<(), RunError> {
            run.relocation_members
                .insert("textures/landscape/rocks/rock_d.dds".to_string());
            Ok(())
        })
        .unwrap();

        let _report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            // params.textures intentionally EMPTY — the member must still convert.
            let params = serde_json::json!({
                "textures": [],
                "source_extracted": source.to_string_lossy(),
                "skip_existing": false,
                "use_gpu": false
            });
            let mut ctx = PhaseCtx {
                run,
                mod_path: &output,
                source_extracted_dir: &source,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertTexturesV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        let fo76_dir = output.join("data").join("Textures").join("FO76");
        assert!(
            any_dds_under(&fo76_dir),
            "expected a relocated texture under {}",
            fo76_dir.display()
        );

        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn texture_v2_phase_writes_loose_without_streaming_to_attached_sink() {
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::sinks::{Ba2ShardWriter, LooseSink, SinkSet, TerrainSidecarSink};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let tmp = std::env::temp_dir().join("texture_v2_phase_sink_bypassed");
        let source = tmp.join("source");
        let output = tmp.join("mod");
        let tex_dir = source.join("Textures").join("Props");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tex_dir).unwrap();
        directxtex_native::write_dds_rgba_image(
            &tex_dir.join("crate_d.dds"),
            8,
            8,
            &vec![64u8; 8 * 8 * 4],
            "R8G8B8A8_UNORM",
            false,
        )
        .unwrap();

        let id = create_run(RunParams {
            source: Game::Fo76,
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

        let sink = std::sync::Arc::new(SinkSet {
            ba2: Some(Ba2ShardWriter::new(tmp.join("spill")).unwrap()),
            loose: LooseSink {
                enabled: true,
                mod_root: output.clone(),
            },
            terrain: TerrainSidecarSink::default(),
        });
        with_run(id, |run| -> Result<(), RunError> {
            run.output_sink = Some(std::sync::Arc::clone(&sink));
            Ok(())
        })
        .unwrap();

        let src_file = tex_dir.join("crate_d.dds");
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            let params = serde_json::json!({
                "textures": [{"source_path": src_file.to_string_lossy(), "output_subpath": null}],
                "source_extracted": source.to_string_lossy(),
                "skip_existing": false,
                "use_gpu": false
            });
            let mut ctx = PhaseCtx {
                run,
                mod_path: &output,
                source_extracted_dir: &source,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertTexturesV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.items_failed, 0);
        // (a) the loose DDS exists exactly as before…
        let loose = output
            .join("data")
            .join("Textures")
            .join("Props")
            .join("crate_d.dds");
        assert!(loose.is_file(), "loose output missing: {}", loose.display());
        // …and (b) the BA2 sink is left for the unified direct texture pack.
        let streamed = sink.ba2.as_ref().unwrap().streamed_rel_paths();
        assert!(
            streamed.is_empty(),
            "textures should not stream through the DX10 spill"
        );

        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn texture_v2_phase_logs_item_failures() {
        use crate::phase::{LogLevel, PhaseEvent};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let tmp = std::env::temp_dir().join("texture_v2_phase_logs_item_failures");
        let source = tmp.join("source");
        let output = tmp.join("mod");
        let bad = source.join("Textures").join("Bad").join("bad_d.dds");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(bad.parent().unwrap()).unwrap();
        std::fs::write(&bad, b"not a dds").unwrap();

        let id = create_run(RunParams {
            source: Game::Fo76,
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

        let (report, events) = with_run(id, |run| -> Result<_, RunError> {
            let cancel = AtomicBool::new(false);
            let params = serde_json::json!({
                "textures": [{"source_path": bad.to_string_lossy(), "output_subpath": null}],
                "source_extracted": source.to_string_lossy(),
                "skip_existing": false,
                "use_gpu": false
            });
            let mut ctx = PhaseCtx {
                run,
                mod_path: &output,
                source_extracted_dir: &source,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            let report = ConvertTexturesV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))?;
            let events = run.event_rx.try_iter().collect::<Vec<_>>();
            Ok((report, events))
        })
        .unwrap();

        assert_eq!(report.items_failed, 1);
        assert!(events.iter().any(|event| matches!(
            event,
            PhaseEvent::Log {
                phase: "convert_textures_v2",
                level: LogLevel::Error,
                message,
            } if message.contains("texture task failed") && message.contains("bad_d.dds")
        )));

        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn terrain_jobs_parse_from_cross_run_params() {
        let params = serde_json::json!({
            "terrain_jobs": [{
                "diffuse_path": "source/soil_d.dds",
                "normal_path": "source/soil_n.dds",
                "reflectivity_path": "source/soil_r.dds",
                "lighting_path": "source/soil_l.dds",
                "output_prefix": "textures/terrain/appalachia/Soil"
            }]
        });

        let jobs = parse_terrain_jobs(&params).unwrap();

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].output_prefix, "textures/terrain/appalachia/Soil");
    }
}
