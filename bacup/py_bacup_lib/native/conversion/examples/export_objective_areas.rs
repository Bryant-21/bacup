use std::path::PathBuf;

use conversion_native::phase::{DispatchParams, run_phase};
use conversion_native::run::{OwnedPluginHandle, RunConfig, RunParams, create_run};
use conversion_native::translator::Game;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err(
            "usage: export_objective_areas SOURCE.esm OUTPUT.esm MOD_DIRECTORY [OUTPUT_MASTER ...]"
                .into(),
        );
    }
    let source = OwnedPluginHandle::load(&PathBuf::from(&args[0]), "fo76", None)?;
    let output = OwnedPluginHandle::load(&PathBuf::from(&args[1]), "fo4", None)?;
    let masters: Vec<_> = args[3..]
        .iter()
        .map(|p| OwnedPluginHandle::load_index(&PathBuf::from(p), "fo4"))
        .collect::<Result<_, _>>()?;
    let run = create_run(RunParams {
        source: Game::Fo76,
        target: Game::Fo4,
        source_handle_id: source.id(),
        target_handle_id: output.id(),
        master_handle_ids: masters.iter().map(OwnedPluginHandle::id).collect(),
        config: RunConfig {
            output_plugin_name: PathBuf::from(&args[1])
                .file_name()
                .ok_or("output filename missing")?
                .to_string_lossy()
                .into_owned(),
            ..RunConfig::default()
        },
    })?;
    let report = run_phase(
        run,
        "emit_objective_areas",
        DispatchParams {
            mod_path: PathBuf::from(&args[2]),
            source_extracted_dir: PathBuf::new(),
            target_extracted_dir: None,
            target_data_dir: None,
            params: serde_json::json!({"match_existing": true}),
        },
    )?;
    println!(
        "{} catalog written; {} skipped entries",
        report.assets_written, report.warnings
    );
    Ok(())
}
