// Phase: convert_btos
//
// Params shape (JSON):
// {
//   "source_game": "fo76",
//   "target_game": "fo4",
//   "bto_paths": [
//     { "source_path": "Meshes/Terrain/Appalachia/Tile.bto", "resolved_path": "/abs/Tile.bto" }
//   ],
//   "skip_existing": true,
//   "conversion_workers": 16
// }

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use std::sync::Arc;

use rayon::prelude::*;
use serde_json::Value as JsonValue;

use crate::phase::progress::ProgressReporter;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use nif_core_native::convert_file::{ConvertFileOptions, ConvertFileReport, convert_nif_file};

pub struct ConvertBtosPhase;
pub struct ConvertBtosV2Phase;

impl Phase for ConvertBtosPhase {
    fn name(&self) -> &'static str {
        "convert_btos"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        run_convert_btos(ctx, "convert_btos")
    }
}

impl Phase for ConvertBtosV2Phase {
    fn name(&self) -> &'static str {
        "convert_btos_v2"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        run_convert_btos(ctx, "convert_btos_v2")
    }
}

fn run_convert_btos(
    ctx: &mut PhaseCtx<'_>,
    phase_name: &'static str,
) -> Result<PhaseReport, PhaseError> {
    let p = ctx.params;

    let source_game = p["source_game"]
        .as_str()
        .ok_or_else(|| PhaseError::BadParams("missing source_game".into()))?
        .to_string();
    let target_game = p["target_game"]
        .as_str()
        .ok_or_else(|| PhaseError::BadParams("missing target_game".into()))?
        .to_string();
    let bto_entries = parse_bto_entries(p)?;
    let skip_existing = p
        .get("skip_existing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let conversion_workers = parse_conversion_workers(p, ctx.run.config.conversion_workers);

    let mod_path = ctx.mod_path;
    // Sink registration of the loose artifact (see phase/nifs.rs).
    let sink = ctx.run.output_sink.clone();
    let data_root = mod_path.join("data");
    let register_with_sink = |dst: &Path| -> bool {
        let Some(s) = &sink else { return true };
        let Ok(rel) = dst.strip_prefix(&data_root) else {
            return true;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        s.add_existing_file(&rel_str, dst).is_ok()
    };
    let total = bto_entries.len() as u32;
    let mut skipped_existing: u32 = 0;
    let mut sink_failures: u32 = 0;
    let mut work_entries = Vec::with_capacity(bto_entries.len());
    for entry in bto_entries {
        let dst = bto_output_path(mod_path, &entry.source_path);
        if skip_existing && dst.exists() {
            skipped_existing += 1;
            if !register_with_sink(&dst) {
                sink_failures += 1;
            }
        } else {
            work_entries.push(entry);
        }
    }

    let work_count = work_entries.len();
    let worker_label = conversion_workers
        .map(|workers| workers.to_string())
        .unwrap_or_else(|| "rayon-default".to_string());
    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Info,
            message: format!(
                "{phase_name}: {work_count} to convert, {skipped_existing} existing-output skipped, workers={worker_label}"
            ),
        });

    let reporter = Arc::new(ProgressReporter::new(
        phase_name,
        work_count as u32,
        ctx.run.event_tx.clone(),
    ));
    let cancel = ctx.cancel;
    let convert_work = || {
        work_entries
            .into_par_iter()
            .filter_map(|entry| {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return None;
                }
                let options = ConvertFileOptions {
                    asset_prefix: None,
                    material_namespace: None,
                    asset_namespace_paths: Default::default(),
                    material_namespace_paths: Default::default(),
                    addon_index_map: Default::default(),
                    translation_maps_dir: None,
                    auto_skin_reference_body: None,
                    emit_first_person: false,
                    first_person_reference: None,
                    morph_weight_cap: 0.5,
                    weapon_role: None,
                    source_material_dir: None,
                    material_source_overrides: Default::default(),
                };
                let dst = bto_output_path(mod_path, &entry.source_path);

                let result = (|| -> Result<ConvertFileReport, String> {
                    let src = Path::new(&entry.resolved_path);
                    if !src.exists() {
                        return Err(format!("BTO not found: {}", entry.resolved_path));
                    }
                    convert_nif_file(src, &dst, &source_game, &target_game, None, &options)
                        .map_err(|e| e.to_string())
                })();

                let sink_failed = match &result {
                    Ok(report) if report.supported && report.errors.is_empty() => {
                        !register_with_sink(&dst)
                    }
                    _ => false,
                };
                reporter.inc(1);
                Some(BtoResult {
                    source_path: entry.source_path,
                    result,
                    sink_failed,
                })
            })
            .collect()
    };
    let results: Vec<BtoResult> = if let Some(workers) = conversion_workers {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .map_err(|err| PhaseError::Internal(format!("rayon pool error: {err}")))?;
        pool.install(convert_work)
    } else {
        convert_work()
    };
    reporter.finish();

    let mut assets_written = skipped_existing;
    let mut warnings = 0;
    let mut report_warnings = 0;
    let mut timing_summary = TimingSummary::default();
    for r in &results {
        if r.sink_failed {
            sink_failures += 1;
        }
        match &r.result {
            Ok(report) if report.supported && report.errors.is_empty() => {
                assets_written += 1;
                timing_summary.add(&r.source_path, report);
                report_warnings +=
                    emit_bto_report_warnings(ctx, phase_name, &r.source_path, report);
            }
            Ok(report) => {
                let message = bto_report_failure_message(&r.source_path, report);
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: phase_name,
                    level: LogLevel::Error,
                    message,
                });
                warnings += 1;
            }
            Err(msg) => {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: phase_name,
                    level: LogLevel::Error,
                    message: format!("{phase_name} failed {}: {}", r.source_path, msg),
                });
                warnings += 1;
            }
        }
    }
    if report_warnings > 0 {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Warn,
            message: format!("{phase_name}: emitted {report_warnings} BTO report warning(s)"),
        });
    }
    for message in timing_summary.log_messages() {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Info,
            message,
        });
    }

    let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
        phase: phase_name,
        current: total,
        total,
        item: None,
    });

    Ok(PhaseReport {
        assets_written,
        warnings,
        items_failed: warnings + sink_failures,
        ..Default::default()
    })
}

struct BtoEntry {
    source_path: String,
    resolved_path: String,
}

struct BtoResult {
    source_path: String,
    result: Result<ConvertFileReport, String>,
    /// Successful convert whose BA2 sink registration failed.
    sink_failed: bool,
}

fn emit_bto_report_warnings(
    ctx: &mut PhaseCtx<'_>,
    phase_name: &'static str,
    source_path: &str,
    report: &ConvertFileReport,
) -> u32 {
    for warning in &report.warnings {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Warn,
            message: bto_report_warning_message(source_path, warning),
        });
    }
    report.warnings.len() as u32
}

fn bto_report_warning_message(source_path: &str, warning: &str) -> String {
    if is_havok_or_collision_warning(warning) {
        format!("BTO Havok/collision warning {source_path}: {warning}")
    } else {
        format!("BTO warning {source_path}: {warning}")
    }
}

fn is_havok_or_collision_warning(warning: &str) -> bool {
    let lower = warning.to_ascii_lowercase();
    lower.contains("havok")
        || lower.contains("bhk")
        || lower.contains("hknp")
        || lower.contains("collision")
}

#[derive(Default)]
struct TimingSummary {
    file_count: u32,
    total_file_ms: u64,
    by_step_ms: HashMap<String, u64>,
    slowest: Vec<SlowBtoTiming>,
}

struct SlowBtoTiming {
    elapsed_ms: u64,
    source_path: String,
}

impl TimingSummary {
    fn add(&mut self, source_path: &str, report: &ConvertFileReport) {
        if report.timings_ms.is_empty() {
            return;
        }

        self.file_count += 1;
        let mut file_total_ms = 0;
        for (step, elapsed_ms) in &report.timings_ms {
            if step == "total" {
                file_total_ms = *elapsed_ms;
            } else {
                *self.by_step_ms.entry(step.clone()).or_insert(0) += *elapsed_ms;
            }
        }
        if file_total_ms == 0 {
            file_total_ms = report
                .timings_ms
                .iter()
                .filter(|(step, _)| step != "total")
                .map(|(_, elapsed_ms)| *elapsed_ms)
                .sum();
        }

        self.total_file_ms += file_total_ms;
        self.slowest.push(SlowBtoTiming {
            elapsed_ms: file_total_ms,
            source_path: source_path.to_string(),
        });
        self.slowest
            .sort_by(|left, right| right.elapsed_ms.cmp(&left.elapsed_ms));
        self.slowest.truncate(5);
    }

    fn log_messages(&self) -> Vec<String> {
        if self.file_count == 0 {
            return Vec::new();
        }

        let avg_ms = self.total_file_ms as f64 / f64::from(self.file_count);
        let mut step_totals: Vec<_> = self.by_step_ms.iter().collect();
        step_totals.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
        let step_text = step_totals
            .into_iter()
            .take(12)
            .map(|(step, elapsed_ms)| format!("{step}={elapsed_ms}ms"))
            .collect::<Vec<_>>()
            .join(", ");

        let mut messages = vec![format!(
            "convert_btos timings: files={}, total_file_ms={}, avg_file_ms={avg_ms:.1}, steps=[{step_text}]",
            self.file_count, self.total_file_ms
        )];
        let slowest_text = self
            .slowest
            .iter()
            .map(|timing| format!("{}ms {}", timing.elapsed_ms, timing.source_path))
            .collect::<Vec<_>>()
            .join("; ");
        if !slowest_text.is_empty() {
            messages.push(format!("convert_btos slowest: {slowest_text}"));
        }
        messages
    }
}

fn parse_bto_entries(p: &JsonValue) -> Result<Vec<BtoEntry>, PhaseError> {
    let arr = p
        .get("bto_paths")
        .and_then(|v| v.as_array())
        .ok_or_else(|| PhaseError::BadParams("missing bto_paths array".into()))?;

    arr.iter()
        .enumerate()
        .map(|(i, entry)| {
            let source_path = entry["source_path"]
                .as_str()
                .ok_or_else(|| {
                    PhaseError::BadParams(format!(
                        "bto_paths[{i}].source_path missing or not a string"
                    ))
                })?
                .to_string();
            let resolved_path = entry["resolved_path"]
                .as_str()
                .ok_or_else(|| {
                    PhaseError::BadParams(format!(
                        "bto_paths[{i}].resolved_path missing or not a string"
                    ))
                })?
                .to_string();
            Ok(BtoEntry {
                source_path,
                resolved_path,
            })
        })
        .collect()
}

fn parse_conversion_workers(p: &JsonValue, fallback: Option<usize>) -> Option<usize> {
    p.get("conversion_workers")
        .and_then(|v| v.as_u64())
        .and_then(|v| usize::try_from(v).ok())
        .filter(|workers| *workers > 0)
        .or_else(|| fallback.filter(|workers| *workers > 0))
}

fn bto_report_failure_message(source_path: &str, report: &ConvertFileReport) -> String {
    let mut details: Vec<String> = Vec::new();
    if !report.supported {
        details.push("unsupported conversion report".to_string());
    }
    details.extend(report.errors.iter().cloned());
    if details.is_empty() {
        details.push("conversion did not produce a supported output".to_string());
    }
    format!("convert_btos failed {source_path}: {}", details.join("; "))
}

fn bto_output_path(mod_path: &Path, source_path: &str) -> PathBuf {
    let rel = mesh_relative_bto_path(source_path);

    let mut out = mod_path.to_path_buf();
    out.push("data");
    out.push("Meshes");
    for component in rel.split('/') {
        if !component.is_empty() {
            out.push(component);
        }
    }
    out
}

fn mesh_relative_bto_path(source_path: &str) -> String {
    let mut rel = source_path.replace('\\', "/");
    rel = rel.trim_start_matches('/').to_string();
    if rel.len() >= 5 && rel[..5].eq_ignore_ascii_case("data/") {
        rel = rel[5..].to_string();
    }
    if rel.len() >= 7 && rel[..7].eq_ignore_ascii_case("meshes/") {
        rel = rel[7..].to_string();
    }
    rel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bto_output_path_preserves_bto_extension() {
        let result = bto_output_path(
            Path::new("/mod"),
            "Data/Meshes/Terrain/Appalachia/Tile01.bto",
        );
        assert_eq!(
            result,
            Path::new("/mod/data/Meshes/Terrain/Appalachia/Tile01.bto")
        );
    }

    #[test]
    fn bto_output_path_adds_meshes_root_for_mesh_relative_path() {
        let result = bto_output_path(Path::new("/mod"), "Terrain/Appalachia/Tile01.bto");
        assert_eq!(
            result,
            Path::new("/mod/data/Meshes/Terrain/Appalachia/Tile01.bto")
        );
    }

    #[test]
    fn parse_bto_entries_missing_field_returns_error() {
        let p = serde_json::json!({
            "bto_paths": [{ "source_path": "Meshes/Terrain/Foo.bto" }]
        });
        assert!(parse_bto_entries(&p).is_err());
    }

    #[test]
    fn invalid_bto_counts_as_warning_not_written() {
        use crate::phase::{Phase, PhaseCtx, PhaseEvent, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let temp = tempfile::tempdir().unwrap();
        let src = temp.path().join("bad.bto");
        std::fs::write(&src, b"not a nif").unwrap();
        let mod_dir = temp.path().join("mod");

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

        let (report, events) = with_run(
            id,
            |run| -> Result<(PhaseReport, Vec<PhaseEvent>), RunError> {
                let cancel = std::sync::Arc::new(AtomicBool::new(false));
                let params = serde_json::json!({
                    "source_game": "fo76",
                    "target_game": "fo4",
                    "bto_paths": [
                        {
                            "source_path": "Meshes/Terrain/Bad.bto",
                            "resolved_path": src.to_string_lossy()
                        }
                    ]
                });
                let source_dir = temp.path().join("source");
                let mut ctx = PhaseCtx {
                    run,
                    mod_path: &mod_dir,
                    source_extracted_dir: &source_dir,
                    target_extracted_dir: None,
                    target_data_dir: None,
                    params: &params,
                    cancel: &cancel,
                };
                let report = ConvertBtosPhase
                    .run(&mut ctx)
                    .map_err(|e| RunError::InvalidConfig(e.to_string()))?;
                let events = run.event_rx.try_iter().collect();
                Ok((report, events))
            },
        )
        .unwrap();

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 1);
        assert!(!mod_dir.join("data/Meshes/Terrain/Bad.bto").exists());
        assert!(events.iter().any(|event| matches!(
            event,
            PhaseEvent::Log {
                phase: "convert_btos",
                level: LogLevel::Error,
                message
            } if message.contains("source is not a recognized NIF file")
        )));
        drop_run(id).unwrap();
    }

    #[test]
    fn convert_btos_v2_is_registered_and_runs_empty_input() {
        use crate::phase::{Phase, PhaseCtx, PhaseEvent, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        assert!(
            crate::phase::build_registry()
                .get("convert_btos_v2")
                .is_some()
        );

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

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "source_game": "fo76",
                "target_game": "fo4",
                "bto_paths": []
            });
            let source_dir = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            let report = ConvertBtosV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))?;
            Ok(report)
        })
        .unwrap();

        assert_eq!(report.assets_written, 0);
        drop_run(id).unwrap();
    }
}
