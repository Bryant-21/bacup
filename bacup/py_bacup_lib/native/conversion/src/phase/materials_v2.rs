//! convert_materials_v2 — the canonical native materials phase. It accepts the
//! shared materials-phase param surface and routes it into
//! texture_engine::materials::run_materials_engine. The engine keeps the shared
//! converter, relocation union, and bCastShadows fold-in in one path. No Python
//! in phase code.

use crate::phase::materials::{material_game, phase_log_level};
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::texture_engine::materials::{MaterialsEngineParams, run_materials_engine};
use materials_native::convert::ConvertMaterialsRequest;

pub struct ConvertMaterialsV2Phase;

fn register_material_outputs_with_sink(ctx: &PhaseCtx<'_>) -> u32 {
    let Some(sink) = ctx.run.output_sink.clone() else {
        return 0;
    };
    if sink.ba2.is_none() {
        return 0;
    }

    let data_root = ctx.mod_path.join("data");
    let materials_root = data_root.join("Materials");
    if !materials_root.is_dir() {
        return 0;
    }

    let mut failures = 0;
    let mut stack = vec![materials_root];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                stack.extend(entries.flatten().map(|entry| entry.path()));
            }
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let is_material = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                let ext = ext.to_ascii_lowercase();
                ext == "bgsm" || ext == "bgem"
            })
            .unwrap_or(false);
        if !is_material {
            continue;
        }
        let Ok(rel) = path.strip_prefix(&data_root) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if sink.add_existing_file(&rel_str, &path).is_err() {
            failures += 1;
        }
    }
    failures
}

impl Phase for ConvertMaterialsV2Phase {
    fn name(&self) -> &'static str {
        "convert_materials_v2"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        // Shared param surface: entries list, convert_all, overwrite_existing,
        // and source_materialsdb.
        let request =
            ConvertMaterialsRequest::from_json(ctx.params).map_err(PhaseError::BadParams)?;

        let engine_params = MaterialsEngineParams {
            mod_path: ctx.mod_path.to_path_buf(),
            source_extracted: ctx.source_extracted_dir.to_path_buf(),
            target_extracted: ctx
                .run
                .target_assets
                .is_none()
                .then(|| ctx.target_extracted_dir.map(|p| p.to_path_buf()))
                .flatten(),
            target_data_dir: ctx
                .run
                .target_assets
                .is_none()
                .then(|| ctx.target_data_dir.map(|p| p.to_path_buf()))
                .flatten(),
            source_game: material_game(ctx.run.source),
            target_game: material_game(ctx.run.target),
            materials: request.materials,
            convert_all: request.convert_all,
            pbr_carry: request.pbr_carry,
            relocation_members: ctx.run.relocation_members.clone(),
            namespace: crate::run::base_asset_namespace_for_run(ctx.run),
            source_materialsdb: request.source_materialsdb,
            overwrite_existing: request.overwrite_existing,
            target_asset_paths: ctx
                .run
                .target_assets
                .as_ref()
                .map(|store| store.list_assets("materials/", "").into_iter().collect())
                .unwrap_or_default(),
        };
        let report = run_materials_engine(engine_params);
        let sink_failures = register_material_outputs_with_sink(ctx);

        // Keep the public PhaseReport counters stable for Python summaries.
        for log in &report.logs {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "convert_materials_v2",
                level: phase_log_level(log.level),
                message: log.message.clone(),
            });
        }
        for progress in &report.progress {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
                phase: "convert_materials_v2",
                current: progress.current,
                total: progress.total,
                item: None,
            });
        }

        Ok(PhaseReport {
            assets_written: report.assets_written,
            warnings: report.warnings,
            items_failed: report.warnings + sink_failures,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;
    use materials_native::bgsm;
    use std::sync::atomic::AtomicBool;

    fn find_bgsm_under(dir: &std::path::Path) -> Option<std::path::PathBuf> {
        let rd = std::fs::read_dir(dir).ok()?;
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(found) = find_bgsm_under(&p) {
                    return Some(found);
                }
            } else if p
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("bgsm"))
                .unwrap_or(false)
            {
                return Some(p);
            }
        }
        None
    }

    #[test]
    fn materials_v2_is_registered() {
        assert!(
            crate::phase::build_registry()
                .get("convert_materials_v2")
                .is_some()
        );
    }

    #[test]
    fn materials_v2_registers_outputs_with_attached_sink() {
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::sinks::{Ba2ShardWriter, LooseSink, SinkSet, TerrainSidecarSink};
        use crate::translator::Game;
        use std::sync::Arc;

        let tmp = std::env::temp_dir().join("materials_v2_phase_sink");
        let source = tmp.join("source");
        let output = tmp.join("mod");
        let mat_dir = source.join("Materials").join("Test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&mat_dir).unwrap();

        let mut data = bgsm::BgsmData::default();
        data.header.signature = bgsm::BGSM_SIGNATURE;
        data.header.version = 20;
        data.DiffuseTexture = "Textures\\Test\\Rock01_d.dds".to_string();
        data.NormalTexture = "Textures\\Test\\Rock01_n.dds".to_string();
        let source_file = mat_dir.join("rock01.bgsm");
        std::fs::write(&source_file, bgsm::write(&data)).unwrap();

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

        let sink = Arc::new(SinkSet {
            ba2: Some(Ba2ShardWriter::new(tmp.join("spill")).unwrap()),
            loose: LooseSink {
                enabled: false,
                mod_root: output.clone(),
            },
            terrain: TerrainSidecarSink::default(),
        });
        with_run(id, |run| -> Result<(), RunError> {
            run.output_sink = Some(Arc::clone(&sink));
            Ok(())
        })
        .unwrap();

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            let params = serde_json::json!({
                "materials": [{
                    "source_path": "Materials/Test/rock01.bgsm",
                    "resolved_path": source_file.to_string_lossy(),
                    "is_cdb_ref": false
                }],
                "overwrite_existing": true
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
            ConvertMaterialsV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 1);
        assert_eq!(report.items_failed, 0);
        assert_eq!(
            sink.ba2.as_ref().unwrap().streamed_rel_paths(),
            vec!["materials/test/rock01.bgsm".to_string()]
        );

        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn materials_v2_relocation_member_absent_from_params_and_cast_shadows_fold_in() {
        let tmp = std::env::temp_dir().join("materials_v2_phase_relocation");
        let source = tmp.join("source");
        let output = tmp.join("mod");
        let mat_dir = source.join("Materials").join("Landscape");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&mat_dir).unwrap();

        let mut data = bgsm::BgsmData::default();
        data.header.signature = bgsm::BGSM_SIGNATURE;
        data.header.version = 20;
        data.DiffuseTexture = "Textures\\Landscape\\Rock01_d.dds".to_string();
        data.NormalTexture = "Textures\\Landscape\\Rock01_n.dds".to_string();
        data.CastShadows = false; // the fold-in must force it true
        std::fs::write(mat_dir.join("rock01.bgsm"), bgsm::write(&data)).unwrap();

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

        with_run(id, |run| -> Result<(), RunError> {
            run.relocation_members
                .insert("materials/landscape/rock01.bgsm".to_string());
            Ok(())
        })
        .unwrap();

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            // materials params intentionally EMPTY — the member must still convert.
            let params = serde_json::json!({ "materials": [], "overwrite_existing": true });
            let mut ctx = PhaseCtx {
                run,
                mod_path: &output,
                source_extracted_dir: &source,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertMaterialsV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();
        assert!(report.assets_written >= 1);
        assert_eq!(report.items_failed, 0);

        let fo76_dir = output.join("data").join("Materials").join("FO76");
        let out = find_bgsm_under(&fo76_dir).unwrap_or_else(|| {
            panic!("expected a relocated material under {}", fo76_dir.display())
        });
        let parsed = bgsm::parse(&std::fs::read(&out).unwrap()).expect("output BGSM must parse");
        assert!(
            parsed.CastShadows,
            "bCastShadows fold-in must force true on engine outputs"
        );

        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
