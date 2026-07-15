// Phase: build_esp
//
// Params shape (JSON):
// {
//   "output_path":            "/path/to/mods/B21_GaussPistol/GaussPistol.esp",
//   "output_plugin_extension": ".esp",
//   "emit_authoring_yaml":    true,
//   "target_master_names":    ["Fallout4.esm", "DLCRobot.esm"]
// }
//
// The phase:
//  1. Reads ctx.run.target_handle_id (populated by the translate phase).
//  2. Saves the target plugin handle to output_path using the GIL-free
//     esp_authoring_core API.
//  3. Optionally exports the authoring YAML directory (yaml/ next to the
//     plugin) using the GIL-free export API.
//
// PhaseReport:
//   assets_written = 1 on success
//   warnings       = 1 on failure (esp save error, etc.)

use std::path::Path;

use serde_json::Value as JsonValue;

use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

pub struct BuildEspPhase;

impl Phase for BuildEspPhase {
    fn name(&self) -> &'static str {
        "build_esp"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let p = ctx.params;

        // Prefer explicit output_path param; fall back to deriving from mod_path.
        let output_path: String = if let Some(s) = p.get("output_path").and_then(|v| v.as_str()) {
            s.to_string()
        } else {
            let ext = p
                .get("output_plugin_extension")
                .and_then(|v| v.as_str())
                .unwrap_or(".esp");
            let plugin_name = ctx.run.config.output_plugin_name.clone();
            let esp_name = if plugin_name.is_empty() {
                format!(
                    "{}{}",
                    ctx.mod_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Output"),
                    ext
                )
            } else {
                plugin_name.clone()
            };
            ctx.mod_path.join(&esp_name).to_string_lossy().into_owned()
        };

        let emit_authoring_yaml = p
            .get("emit_authoring_yaml")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // The NVNM validator clones the full in-memory plugin. On whole-plugin
        // runs build_esp executes at the memory high-water mark, so callers pass
        // run_nvnm_validator=false to skip that transient clone.
        let run_nvnm_validator = p
            .get("run_nvnm_validator")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let target_handle_id = ctx.run.target_handle_id;

        ctx.check_cancel()?;

        crate::plugin_header::normalize_target_plugin_header(
            target_handle_id,
            ctx.run.target.as_str(),
        )
        .map_err(|e| {
            PhaseError::Internal(format!("build_esp: header normalization failed: {e}"))
        })?;

        if let Some(parent) = Path::new(&output_path).parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Error,
                    message: format!("build_esp: mkdir failed for '{}': {e}", parent.display()),
                });
                return Ok(PhaseReport {
                    warnings: 1,
                    ..Default::default()
                });
            }
        }

        if let Err(e) = esp_authoring_core::plugin_runtime::plugin_handle_save_no_py(
            target_handle_id,
            &output_path,
        ) {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: self.name(),
                level: LogLevel::Error,
                message: format!("build_esp: ESP save failed: {e}"),
            });
            return Ok(PhaseReport {
                warnings: 1,
                ..Default::default()
            });
        }

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: self.name(),
            level: LogLevel::Info,
            message: format!("build_esp: saved ESP to '{output_path}'"),
        });

        ctx.check_cancel()?;

        if emit_authoring_yaml {
            let yaml_dir = ctx.mod_path.join("yaml");
            if let Err(e) =
                esp_authoring_core::plugin_runtime::export_authoring_dir_from_handle_no_py(
                    target_handle_id,
                    &yaml_dir,
                    "yaml",
                )
            {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Warn,
                    message: format!("build_esp: authoring YAML export failed (non-fatal): {e}"),
                });
            } else {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Info,
                    message: format!("build_esp: YAML exported to '{}'", yaml_dir.display()),
                });
            }
        }

        // Run the NVNM structural validator against the in-memory target
        // plugin. Findings are surfaced as Warn
        // PhaseEvent::Log entries rather than failing the build — the conversion
        // run still succeeds, but downstream tooling and humans see the issues.
        let validator_warnings = if run_nvnm_validator {
            match emit_navmesh_validation_warnings(target_handle_id, self.name(), &ctx.run.event_tx)
            {
                Ok(n) => n,
                Err(e) => {
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: self.name(),
                        level: LogLevel::Warn,
                        message: format!("build_esp: NVNM validator failed to run: {e}"),
                    });
                    1
                }
            }
        } else {
            0
        };

        Ok(PhaseReport {
            assets_written: 1,
            warnings: validator_warnings,
            ..Default::default()
        })
    }
}

/// Pull the in-memory ParsedPlugin out of the handle store and run the NVNM
/// structural validator across every NAVM. Each finding becomes a Warn-level
/// PhaseEvent::Log; the returned count is the number of warnings emitted.
pub(crate) fn emit_navmesh_validation_warnings(
    target_handle_id: u64,
    phase_name: &'static str,
    event_tx: &crossbeam_channel::Sender<PhaseEvent>,
) -> Result<u32, String> {
    let (parsed, _strings) =
        esp_authoring_core::plugin_runtime::clone_plugin_handle_state_no_py(target_handle_id)?;
    let report = esp_authoring_core::nvnm::validate_plugin_navmeshes(&parsed);
    if report.is_ok() {
        let _ = event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Info,
            message: "build_esp: NVNM validator clean (no structural findings)".to_string(),
        });
        return Ok(0);
    }
    for err in &report.errors {
        let _ = event_tx.try_send(PhaseEvent::Log {
            phase: phase_name,
            level: LogLevel::Warn,
            message: format!(
                "build_esp: NVNM validator [{}] {}: {}",
                err.kind.as_str(),
                err.mesh_form_key,
                err.detail
            ),
        });
    }
    Ok(report.errors.len() as u32)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;

    fn make_run(target_handle_id: u64) -> u64 {
        create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap()
    }

    #[test]
    fn missing_handle_returns_warning() {
        // Handle ID 88887 doesn't exist — save should fail gracefully.
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        let id = make_run(88887);

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "output_path": mod_path.join("Output.esp").to_string_lossy().as_ref(),
                "emit_authoring_yaml": false
            });
            let source_dir = mod_path.clone();
            let mut ctx = crate::phase::PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            BuildEspPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        // Missing handle → warning count incremented, nothing written.
        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 1);

        drop_run(id).unwrap();
    }

    #[test]
    fn nvnm_validator_emits_warn_events_for_downfacing_triangle() {
        // Build a live target plugin handle, inject one NAVM whose NVNM payload
        // contains a clockwise-from-above (downfacing) triangle, then run the
        // BuildEspPhase. The phase must:
        //   - save the ESP (assets_written = 1)
        //   - emit one Warn-level PhaseEvent::Log naming "downfacing_normal"
        //   - report warnings = 1 (one validator finding)
        use bytes::Bytes;
        use esp_authoring_core::nvnm::{
            NvnmGrid, NvnmParent, NvnmPayload, NvnmTriangle, NvnmVertex, write_nvnm,
        };
        use esp_authoring_core::plugin_runtime::{
            ParsedRecord, ParsedSubrecord, insert_parsed_record_in_slot,
            plugin_handle_close_native, plugin_handle_new_native, plugin_handle_store_ref,
        };
        use smol_str::SmolStr;

        let target_handle_id = plugin_handle_new_native("Validator.esp", Some("fo4"))
            .expect("plugin_handle_new_native");

        // Downfacing NVNM: one CW-from-above triangle at z=0.
        let downfacing_payload = NvnmPayload {
            version: 15,
            flags: 0,
            parent: NvnmParent::Interior { cell: 0x123 },
            vertices: vec![
                NvnmVertex {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                NvnmVertex {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
                NvnmVertex {
                    x: 0.0,
                    y: -1.0,
                    z: 0.0,
                }, // CW = downfacing
            ],
            triangles: vec![NvnmTriangle {
                vertices: [0, 1, 2],
                links: [-1; 3],
                cover_marker: [0; 9],
                flags: 0,
            }],
            edge_links: vec![],
            door_refs: vec![],
            cover_array: vec![],
            cover_triangle_mappings: vec![],
            waypoints: vec![],
            grid: NvnmGrid::default(),
        };
        let nvnm_bytes = write_nvnm(&downfacing_payload);

        // Inject a NAVM record carrying that NVNM into the target handle.
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store
                .get_mut(&target_handle_id)
                .expect("target handle present");
            insert_parsed_record_in_slot(
                slot,
                ParsedRecord {
                    signature: SmolStr::from("NAVM"),
                    form_id: 0x0100_0001,
                    flags: 0,
                    version_control: 0,
                    form_version: None,
                    version2: None,
                    subrecords: vec![ParsedSubrecord {
                        signature: SmolStr::from("NVNM"),
                        data: Bytes::from(nvnm_bytes),
                        semantic_type: None,
                    }],
                    raw_payload: None,
                    parse_error: None,
                },
            );
        }

        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        let id = make_run(target_handle_id);

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "output_path": mod_path.join("Validator.esp").to_string_lossy().as_ref(),
                "emit_authoring_yaml": false
            });
            let source_dir = mod_path.clone();
            let mut ctx = crate::phase::PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            BuildEspPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        // ESP saved + validator reported one downfacing finding.
        assert_eq!(report.assets_written, 1, "ESP should have been written");
        assert_eq!(
            report.warnings, 1,
            "expected exactly one validator finding (downfacing triangle)"
        );

        // Drain events and look for the warning naming the downfacing finding.
        let events: Vec<_> = with_run(id, |run| -> Result<Vec<PhaseEvent>, RunError> {
            let mut events = Vec::new();
            while let Ok(ev) = run.event_rx.try_recv() {
                events.push(ev);
            }
            Ok(events)
        })
        .unwrap();
        let downfacing_logs: Vec<_> = events
            .iter()
            .filter_map(|ev| match ev {
                PhaseEvent::Log {
                    level: LogLevel::Warn,
                    message,
                    ..
                } if message.contains("downfacing_normal") => Some(message.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            downfacing_logs.len(),
            1,
            "expected one Warn log mentioning downfacing_normal; got events {events:?}"
        );

        assert!(
            plugin_handle_store_ref()
                .lock()
                .unwrap()
                .contains_key(&target_handle_id)
        );
        drop_run(id).unwrap();
        // Raw RunParams runs do not own handles; path-owned lifecycle is covered in run.rs.
        assert!(plugin_handle_close_native(target_handle_id));
    }

    #[test]
    fn nvnm_validator_skipped_when_disabled() {
        // With run_nvnm_validator=false, build_esp must still save the ESP but
        // emit NO "NVNM validator" log (neither the clean-Info nor any finding)
        // and report zero warnings — the transient clone is skipped entirely.
        use esp_authoring_core::plugin_runtime::plugin_handle_new_native;

        let target_handle_id = plugin_handle_new_native("NoValidate.esp", Some("fo4"))
            .expect("plugin_handle_new_native");

        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        let id = make_run(target_handle_id);

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "output_path": mod_path.join("NoValidate.esp").to_string_lossy().as_ref(),
                "emit_authoring_yaml": false,
                "run_nvnm_validator": false
            });
            let source_dir = mod_path.clone();
            let mut ctx = crate::phase::PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            BuildEspPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 1, "ESP should have been written");
        assert_eq!(report.warnings, 0, "validator disabled → no warnings");

        let events: Vec<_> = with_run(id, |run| -> Result<Vec<PhaseEvent>, RunError> {
            let mut events = Vec::new();
            while let Ok(ev) = run.event_rx.try_recv() {
                events.push(ev);
            }
            Ok(events)
        })
        .unwrap();
        let validator_logs: Vec<_> = events
            .iter()
            .filter_map(|ev| match ev {
                PhaseEvent::Log { message, .. } if message.contains("NVNM validator") => {
                    Some(message.as_str())
                }
                _ => None,
            })
            .collect();
        assert!(
            validator_logs.is_empty(),
            "expected no NVNM validator logs when disabled; got {validator_logs:?}"
        );

        drop_run(id).unwrap();
    }

    #[test]
    fn output_path_derived_from_mod_path_when_not_specified() {
        // Even without a handle, the path derivation logic should produce a
        // sensible path. We just verify the phase doesn't panic and returns a
        // warning (because the handle doesn't exist).
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        let id = make_run(77776);

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            // No output_path → derive from mod_path + output_plugin_name.
            let params = serde_json::json!({
                "output_plugin_extension": ".esp",
                "emit_authoring_yaml": false
            });
            let source_dir = mod_path.clone();
            let mut ctx = crate::phase::PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            BuildEspPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        // Missing handle → warning, not panic.
        assert_eq!(report.warnings, 1);
        drop_run(id).unwrap();
    }
}
