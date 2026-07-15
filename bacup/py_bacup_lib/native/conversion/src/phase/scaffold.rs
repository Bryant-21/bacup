// Phase: scaffold
//
// Params shape (JSON):
// {
//   "mod_prefix":         "B21",           // EditorID prefix for the mod
//   "output_plugin_name": "GaussPistol.esp", // target ESP filename
//   "emit_authoring_yaml": true             // whether yaml/ dir is needed
// }
//
// Phase output: creates mod directory skeleton:
//   - data/Meshes/, data/Textures/, data/Materials/, data/Sound/, data/Scripts/
//   - yaml/ directory (when emit_authoring_yaml = true)
//   - writes .modprefix sentinel file with the mod_prefix value
//
// PhaseReport:
//   assets_written = number of directories created
//   warnings       = 0 (errors are hard failures)

use std::path::Path;

use serde_json::Value as JsonValue;

use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

pub struct ScaffoldPhase;

impl Phase for ScaffoldPhase {
    fn name(&self) -> &'static str {
        "scaffold"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let p = ctx.params;

        let mod_prefix = p
            .get("mod_prefix")
            .and_then(|v| v.as_str())
            .unwrap_or("B21")
            .to_string();

        let emit_authoring_yaml = p
            .get("emit_authoring_yaml")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        ctx.check_cancel()?;

        let mod_path = ctx.mod_path;
        let mut dirs_created: u32 = 0;

        let data_dir = mod_path.join("data");
        for subdir in &["Meshes", "Textures", "Materials", "Sound", "Scripts"] {
            let dir = data_dir.join(subdir);
            if let Err(e) = std::fs::create_dir_all(&dir) {
                return Err(PhaseError::Internal(format!(
                    "scaffold: mkdir '{}' failed: {e}",
                    dir.display()
                )));
            }
            dirs_created += 1;
        }

        if emit_authoring_yaml {
            let yaml_dir = mod_path.join("yaml");
            if let Err(e) = std::fs::create_dir_all(&yaml_dir) {
                return Err(PhaseError::Internal(format!(
                    "scaffold: mkdir yaml/ failed: {e}",
                )));
            }
            dirs_created += 1;
        }

        // Write .modprefix sentinel so downstream tools know the prefix.
        if !mod_prefix.is_empty() {
            let sentinel = mod_path.join(".modprefix");
            if let Err(e) = std::fs::write(&sentinel, mod_prefix.as_bytes()) {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Warn,
                    message: format!("scaffold: failed to write .modprefix: {e}"),
                });
            }
        }

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: self.name(),
            level: LogLevel::Info,
            message: format!(
                "scaffold: created {dirs_created} directories under '{}'",
                mod_path.display()
            ),
        });

        Ok(PhaseReport {
            assets_written: dirs_created,
            ..Default::default()
        })
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

    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;

    fn make_run() -> u64 {
        create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap()
    }

    #[test]
    fn scaffold_creates_data_subdirs() {
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        let id = make_run();

        let result = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "mod_prefix": "B21",
                "output_plugin_name": "Test.esp",
                "emit_authoring_yaml": true
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
            ScaffoldPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert!(
            result.assets_written >= 6,
            "expected >=6 dirs (5 data + yaml)"
        );
        assert!(mod_path.join("data").join("Meshes").is_dir());
        assert!(mod_path.join("data").join("Textures").is_dir());
        assert!(mod_path.join("data").join("Materials").is_dir());
        assert!(mod_path.join("data").join("Sound").is_dir());
        assert!(mod_path.join("data").join("Scripts").is_dir());
        assert!(mod_path.join("yaml").is_dir());

        drop_run(id).unwrap();
    }

    #[test]
    fn scaffold_writes_modprefix_sentinel() {
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        let id = make_run();

        with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "mod_prefix": "MyMod",
                "output_plugin_name": "Test.esp",
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
            ScaffoldPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        let sentinel = mod_path.join(".modprefix");
        assert!(sentinel.exists(), ".modprefix sentinel missing");
        let content = std::fs::read_to_string(&sentinel).unwrap();
        assert_eq!(content, "MyMod");

        drop_run(id).unwrap();
    }

    #[test]
    fn scaffold_no_yaml_dir_when_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        let id = make_run();

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "mod_prefix": "B21",
                "output_plugin_name": "Test.esp",
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
            ScaffoldPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 5, "5 dirs when yaml disabled");
        assert!(!mod_path.join("yaml").exists(), "yaml/ must not exist");
        drop_run(id).unwrap();
    }
}
