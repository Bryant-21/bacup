//! Runs the previs stage the way the pipeline does (`stage::plan` with
//! `solve_into`) and writes its report as `<solve_into>\previs_plan.json`.
//!
//! `cargo run --release --example previs_stage -- <plugin> <game data> <extracted root> <solve_into> [workers] [world,...|-] [no-interiors]`
//! `SCENE_DIR=<dir>` also keeps every scene file there.
use std::path::PathBuf;

use previs_native::stage::{PlanRequest, plan};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let plugin = PathBuf::from(&args[0]);
    let data = PathBuf::from(&args[1]);
    let extracted = PathBuf::from(&args[2]);
    let solve_into = PathBuf::from(&args[3]);
    let workers = args.get(4).and_then(|w| w.parse().ok());
    let worlds = match args.get(5).map(String::as_str) {
        None | Some("-") => Vec::new(),
        Some(list) => list.split(',').map(str::to_string).collect(),
    };
    let interiors = args.get(6).map(String::as_str) != Some("no-interiors");
    let mod_root = plugin.parent().unwrap().to_path_buf();
    let mut archives: Vec<PathBuf> = std::fs::read_dir(&data)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy().to_lowercase();
            name.ends_with(".ba2") && (name.contains(" - meshes") || name.contains(" - materials"))
        })
        .collect();
    archives.sort();
    let scene_dir = std::env::var_os("SCENE_DIR").map(PathBuf::from);
    let request = PlanRequest {
        plugin,
        master_dirs: vec![mod_root.clone(), data],
        loose_roots: vec![mod_root.join("data"), extracted],
        archives,
        scene_dir: scene_dir.clone().unwrap_or_else(|| solve_into.join("scenes")),
        worlds,
        interiors,
        accept_reference_layer: false,
        solve_into: Some(solve_into.clone()),
        keep_scenes: scene_dir.is_some(),
        interior_cdx_only: false,
    };
    let started = std::time::Instant::now();
    let report = previs_native::worker_pool(workers).install(|| plan(&request)).unwrap();
    let elapsed = started.elapsed();
    std::fs::write(solve_into.join("previs_plan.json"), serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!(
        "{} jobs, {} stamps, {} skipped in {elapsed:.1?}",
        report.jobs.len(),
        report.stamps.len(),
        report.skipped.len()
    );
}
