//! Phase: graft_terrain — reuse prior terrain + navmesh (`regen.py --re-use-land`)
//!
//! Replaces `convert_terrain` + `emit_projected_navmeshes` + the navmesh emit on a
//! reuse run: instead of regenerating LAND/NAVM/NAVI from the BTD + source, it
//! structurally clones the exterior CELL shells + LAND + NAVM + terrain-texture
//! records from a prior FO4 output ESM opened conversion-locally from
//! `prior_plugin_path`. NAVI is NOT grafted — the unchanged
//! `rebuild_projected_navi` phase rebuilds it from the grafted exterior NAVM plus
//! the freshly-converted interior NAVM.

use std::path::Path;

use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::run::OwnedPluginHandle;

pub struct GraftTerrainPhase;

impl Phase for GraftTerrainPhase {
    fn name(&self) -> &'static str {
        "graft_terrain"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.params.get("prior_handle_id").is_some() {
            return Err(PhaseError::BadParams(
                "graft_terrain: legacy parameter is not supported: prior_handle_id".into(),
            ));
        }
        let prior_plugin_path = ctx
            .params
            .get("prior_plugin_path")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                PhaseError::BadParams("graft_terrain: prior_plugin_path is required".into())
            })?;
        let prior =
            OwnedPluginHandle::load(Path::new(prior_plugin_path), ctx.run.target.as_str(), None)
                .map_err(|error| PhaseError::BadParams(format!("graft_terrain: {error}")))?;

        let stats = ctx
            .run
            .graft_terrain_navmesh(prior.id())
            .map_err(|e| PhaseError::Internal(e.to_string()))?;

        Ok(PhaseReport {
            records_added: stats.records_translated,
            records_dropped: stats.records_dropped,
            warnings: stats.records_failed,
            ..PhaseReport::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use esp_authoring_core::plugin_runtime::{
        ParsedRecord, insert_parsed_record_in_slot, plugin_handle_close_native,
        plugin_handle_new_native, plugin_handle_save_no_py, plugin_handle_store_ref,
    };
    use serde_json::json;
    use smol_str::SmolStr;

    use crate::run::{RunConfig, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;

    fn write_empty_plugin(dir: &Path, name: &str, game: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let handle = plugin_handle_new_native(name, Some(game)).unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            insert_parsed_record_in_slot(
                store.get_mut(&handle).unwrap(),
                ParsedRecord {
                    signature: SmolStr::new("WRLD"),
                    form_id: 0x800,
                    flags: 0,
                    version_control: 0,
                    form_version: None,
                    version2: None,
                    subrecords: Vec::new(),
                    raw_payload: None,
                    parse_error: None,
                },
            );
        }
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
        plugin_handle_close_native(handle);
        path
    }

    fn run_phase(
        source_handle_id: u64,
        target_handle_id: u64,
        mod_path: &Path,
        params: &serde_json::Value,
    ) -> Result<PhaseReport, PhaseError> {
        let run_id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id,
            target_handle_id,
            master_handle_ids: Vec::new(),
            config: RunConfig::default(),
        })
        .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let result = with_run(run_id, |run| -> Result<_, crate::run::RunError> {
            let mut ctx = PhaseCtx {
                run,
                mod_path,
                source_extracted_dir: mod_path,
                target_extracted_dir: None,
                target_data_dir: None,
                params,
                cancel: &cancel,
            };
            Ok(GraftTerrainPhase.run(&mut ctx))
        })
        .unwrap();
        drop_run(run_id).unwrap();
        assert!(!cancel.load(Ordering::Relaxed));
        result
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
    fn prior_plugin_path_closes_local_handle_after_success_and_error() {
        let tmp = tempfile::tempdir().unwrap();
        let prior_path = write_empty_plugin(tmp.path(), "Prior.esm", "fo4");
        let source = plugin_handle_new_native("Source.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native("Target.esm", Some("fo4")).unwrap();
        let params = json!({"prior_plugin_path": prior_path});

        run_phase(source, target, tmp.path(), &params).unwrap();
        assert!(!store_contains_path(&prior_path));

        assert!(run_phase(source, u64::MAX, tmp.path(), &params).is_err());
        assert!(!store_contains_path(&prior_path));

        plugin_handle_close_native(target);
        plugin_handle_close_native(source);
    }
}
