//! convert_materials_v2 — the canonical native materials phase. It accepts the
//! shared materials-phase param surface and routes it into
//! texture_engine::materials::run_materials_engine. The engine keeps the shared
//! converter, relocation union, and bCastShadows fold-in in one path. No Python
//! in phase code.

use crate::phase::materials::{material_game, phase_log_level};
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::texture_engine::materials::{MaterialsEngineParams, run_materials_engine};
use materials_native::convert::ConvertMaterialsRequest;
use materials_native::fo76_envmap;

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

fn bgsm_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                stack.extend(entries.flatten().map(|entry| entry.path()));
            }
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bgsm"))
        {
            out.push(path);
        }
    }
    out
}

/// FO4's neutral metal cube, the source every generated tinted cube is rescaled from.
fn stock_metal_cube(ctx: &PhaseCtx<'_>) -> Option<Vec<u8>> {
    let rel = format!("textures/{}", fo76_envmap::STOCK_METAL_CUBEMAP).to_ascii_lowercase();
    if let Some(store) = ctx.run.target_assets.as_ref() {
        if let Ok(Some(path)) = store.materialize(&rel) {
            return std::fs::read(path).ok();
        }
    }
    [ctx.target_extracted_dir, ctx.target_data_dir]
        .into_iter()
        .flatten()
        .find_map(|root| std::fs::read(root.join(&rel)).ok())
}

/// Writes every tinted metal cube a converted BGSM names (`Shared/Cubemaps/BACUP/Metal_<rgb>_e.dds`)
/// into `data/textures`, where the archive pass packs it. Returns (written, failures).
fn write_generated_metal_cubemaps(ctx: &PhaseCtx<'_>) -> (u32, u32) {
    let data_root = ctx.mod_path.join("data");
    let mut slots = std::collections::BTreeMap::new();
    for path in bgsm_files(&data_root.join("Materials")) {
        let Some(slot) = std::fs::read(&path)
            .ok()
            .and_then(|bytes| materials_native::bgsm::parse(&bytes).ok())
            .and_then(|bgsm| bgsm.EnvmapTexture)
        else {
            continue;
        };
        let slot = slot.replace('\0', "").replace('\\', "/");
        if let Some(tint) = fo76_envmap::generated_cubemap_tint(&slot) {
            let rel = slot.trim_start_matches('/').to_ascii_lowercase();
            let rel = rel.strip_prefix("textures/").unwrap_or(&rel).to_owned();
            slots.insert(format!("textures/{rel}"), tint);
        }
    }
    if slots.is_empty() {
        return (0, 0);
    }
    let log = |level, message: String| {
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "convert_materials_v2",
            level,
            message,
        });
    };
    let Some(stock) = stock_metal_cube(ctx) else {
        log(
            LogLevel::Warn,
            format!(
                "metal cubemaps: {} not found; {} generated cubes not written",
                fo76_envmap::STOCK_METAL_CUBEMAP,
                slots.len()
            ),
        );
        return (0, slots.len() as u32);
    };
    let (mut written, mut failures) = (0, 0);
    for (rel, tint) in &slots {
        let out = data_root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        let result = fo76_envmap::tinted_cube_bytes(&stock, *tint)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                std::fs::write(&out, bytes).map_err(|e| e.to_string())
            })
            .and_then(|()| match ctx.run.output_sink.as_ref() {
                Some(sink) => sink.add_existing_file(rel, &out).map(|_| ()),
                None => Ok(()),
            });
        match result {
            Ok(()) => written += 1,
            Err(error) => {
                failures += 1;
                log(LogLevel::Warn, format!("metal cubemap {rel}: {error}"));
            }
        }
    }
    log(LogLevel::Info, format!("metal cubemaps: {written} tinted cubes written"));
    (written, failures)
}

impl Phase for ConvertMaterialsV2Phase {
    fn name(&self) -> &'static str {
        "convert_materials_v2"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        if crate::native_starfield::active(ctx.run) {
            return crate::native_starfield::copy_materials(ctx);
        }
        // Shared param surface: entries list, convert_all, overwrite_existing,
        // and source_materialsdb.
        let request =
            ConvertMaterialsRequest::from_json(ctx.params).map_err(PhaseError::BadParams)?;

        let engine_params = MaterialsEngineParams {
            mod_path: ctx.mod_path.to_path_buf(),
            source_extracted: ctx.source_extracted_dir.to_path_buf(),
            source_inventory: ctx.run.source_asset_inventory.clone(),
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
            base_overwrite_prefixes: crate::run::base_overwrite_prefixes_for_run(ctx.run),
            base_overwrite_paths: ctx.run.native_overwrite_paths.clone(),
            copy_as_authored: crate::run::native_assets_active(ctx.run),
        };
        let overwrites = crate::native_overwrites::begin_phase(ctx, "convert_materials_v2");
        let report = run_materials_engine(engine_params);
        crate::native_overwrites::finish_phase(ctx, "convert_materials_v2", &overwrites);
        let sink_failures = register_material_outputs_with_sink(ctx);
        let (cubes_written, cube_failures) = write_generated_metal_cubemaps(ctx);

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
            assets_written: report.assets_written + cubes_written,
            warnings: report.warnings + cube_failures,
            items_failed: report.warnings + sink_failures + cube_failures,
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
    fn materials_v2_native_assets_copies_fo76_materials_as_authored() {
        let tmp = std::env::temp_dir().join("materials_v2_phase_native");
        let source = tmp.join("source");
        let output = tmp.join("mod");
        let mat_dir = source.join("Materials").join("Test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&mat_dir).unwrap();

        let mut data = bgsm::BgsmData::default();
        data.header.signature = bgsm::BGSM_SIGNATURE;
        data.header.version = 22;
        data.DiffuseTexture = "Textures\\Test\\Rock01_d.dds".to_string();
        data.CastShadows = false;
        let source_bytes = bgsm::write(&data);
        let source_file = mat_dir.join("rock01.bgsm");
        std::fs::write(&source_file, &source_bytes).unwrap();

        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                native_assets: true,
                ..Default::default()
            },
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
        let out = find_bgsm_under(&output.join("data")).expect("material output");
        assert_eq!(std::fs::read(out).unwrap(), source_bytes);
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

    #[test]
    fn materials_v2_gives_fo76_coloured_metal_a_generated_tinted_cube() {
        use directxtex_native::{
            CP_FLAGS_NONE, DDS_FLAGS_NONE, DXGI_FORMAT_B8G8R8A8_UNORM, ScratchImage,
        };
        let tmp = std::env::temp_dir().join("materials_v2_phase_metal_cube");
        let source = tmp.join("source");
        let target = tmp.join("target");
        let output = tmp.join("mod");
        let _ = std::fs::remove_dir_all(&tmp);
        let mat_dir = source.join("Materials").join("Props");
        let tex_dir = source.join("textures").join("Props");
        let cube_dir = target.join("textures").join("shared").join("cubemaps");
        for dir in [&mat_dir, &tex_dir, &cube_dir] {
            std::fs::create_dir_all(dir).unwrap();
        }
        // A gold `_r`: every texel a coloured metal F0.
        let gold = [150u8, 115, 40, 255].repeat(8 * 8);
        directxtex_native::write_dds_rgba_image(
            &tex_dir.join("brass_r.dds"),
            8,
            8,
            &gold,
            "R8G8B8A8_UNORM",
            false,
        )
        .unwrap();
        let mut cube = ScratchImage::default();
        cube.initialize_cube(DXGI_FORMAT_B8G8R8A8_UNORM, 4, 4, 1, 3, CP_FLAGS_NONE)
            .unwrap();
        cube.pixels_mut().fill(128);
        std::fs::write(
            cube_dir.join("metalbrushed01cube_e.dds"),
            cube.save_dds(DDS_FLAGS_NONE).unwrap().buffer(),
        )
        .unwrap();

        let mut data = bgsm::BgsmData::default();
        data.header.signature = bgsm::BGSM_SIGNATURE;
        data.header.version = 22;
        data.DiffuseTexture = "Props\\brass_d.dds".to_string();
        data.SpecularTexture = Some("Props\\brass_r.dds".to_string());
        let source_file = mat_dir.join("brass.bgsm");
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
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            let params = serde_json::json!({
                "materials": [{
                    "source_path": "Materials/Props/brass.bgsm",
                    "resolved_path": source_file.to_string_lossy(),
                    "is_cdb_ref": false
                }],
                "overwrite_existing": true
            });
            let mut ctx = PhaseCtx {
                run,
                mod_path: &output,
                source_extracted_dir: &source,
                target_extracted_dir: Some(&target),
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertMaterialsV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();
        drop_run(id).unwrap();
        assert_eq!(report.items_failed, 0);

        let out = find_bgsm_under(&output.join("data")).expect("material output");
        let parsed = bgsm::parse(&std::fs::read(out).unwrap()).unwrap();
        let env = parsed.EnvmapTexture.unwrap().replace('\0', "");
        assert!(env.starts_with("Shared/Cubemaps/BACUP/Metal_"), "{env}");
        assert_eq!(parsed.header.env_mapping, Some(true));
        assert_eq!(
            parsed.header.env_mapping_mask_scale,
            Some(fo76_envmap::METAL_ENVMAP_MASK_SCALE)
        );
        let cube_path = output.join("data").join("textures").join(env.to_ascii_lowercase());
        let cube_bytes = std::fs::read(&cube_path).expect("generated cube written");
        let mut md = directxtex_native::TexMetadata::default();
        let img =
            ScratchImage::load_dds(&cube_bytes, DDS_FLAGS_NONE, Some(&mut md), None).unwrap();
        assert!(md.is_cubemap() && md.mip_levels == 3);
        let px = &img.pixels()[..4];
        assert!(px[2] > px[1] && px[1] > px[0], "BGRA gold tint expected, got {px:?}");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
