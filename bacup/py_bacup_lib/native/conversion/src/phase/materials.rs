use crate::phase::LogLevel;
use crate::translator;
use materials_native::convert::{ConvertLogLevel, Game as MaterialGame};

pub(crate) fn material_game(game: translator::Game) -> MaterialGame {
    match game {
        translator::Game::Fo3 => MaterialGame::Fo3,
        translator::Game::Fnv => MaterialGame::Fnv,
        translator::Game::Fo4 => MaterialGame::Fo4,
        translator::Game::Fo76 => MaterialGame::Fo76,
        translator::Game::Skyrim => MaterialGame::Skyrim,
        translator::Game::SkyrimSe => MaterialGame::SkyrimSe,
        translator::Game::Starfield => MaterialGame::Starfield,
        translator::Game::Oblivion => MaterialGame::Oblivion,
    }
}

pub(crate) fn phase_log_level(level: ConvertLogLevel) -> LogLevel {
    match level {
        ConvertLogLevel::Info => LogLevel::Info,
        ConvertLogLevel::Warn => LogLevel::Warn,
        ConvertLogLevel::Error => LogLevel::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::{Phase, PhaseCtx, PhaseReport};
    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;
    use std::sync::atomic::AtomicBool;

    fn any_file_with_ext_under(dir: &std::path::Path, ext: &str) -> bool {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return false;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if any_file_with_ext_under(&p, ext) {
                    return true;
                }
            } else if p
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case(ext))
                .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }

    #[test]
    fn material_phase_converts_relocation_member_absent_from_params() {
        use materials_native::bgsm;

        let tmp = std::env::temp_dir().join("material_phase_relocation_member_absent");
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

        // Inject a material member directly (bypassing the compare) to isolate the phase.
        with_run(id, |run| -> Result<(), RunError> {
            run.relocation_members
                .insert("materials/landscape/rock01.bgsm".to_string());
            Ok(())
        })
        .unwrap();

        let _report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            // materials params intentionally EMPTY — the member must still convert.
            let params = serde_json::json!({ "materials": [] });
            let mut ctx = PhaseCtx {
                run,
                mod_path: &output,
                source_extracted_dir: &source,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            crate::phase::materials_v2::ConvertMaterialsV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        let fo76_dir = output.join("data").join("Materials").join("FO76");
        assert!(
            any_file_with_ext_under(&fo76_dir, "bgsm"),
            "expected a relocated material under {}",
            fo76_dir.display()
        );

        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
