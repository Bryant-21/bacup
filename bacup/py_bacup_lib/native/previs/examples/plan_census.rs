//! Runs the previs planner and tallies skip reasons.
//!
//! Args: <plugin> <scene dir> <report json> <master dir> <loose root>...
//! Writes scenes into `<scene dir>` and the full skip list into `<report json>`.
//! With `SOLVE_INTO=<data dir>` each scene is solved at once and only its tome
//! is written (under `<data dir>\Vis\<plugin>\`).
use std::collections::BTreeMap;
use std::path::PathBuf;

use previs_native::stage::{PlanRequest, plan};

fn category(reason: &str) -> String {
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.is_empty() {
            return;
        }
        let hexish = run.len() >= 4 && run.chars().all(|c| c.is_ascii_hexdigit());
        let numeric = run.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '+');
        out.push_str(if hexish || numeric { "#" } else { run });
        run.clear();
    };
    for c in reason.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '+' {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    // Model-specific prefixes (`meshes\a\b.nif: reason`) collapse onto the reason.
    if let Some(index) = out.to_ascii_lowercase().find(".nif: ") {
        out = format!("<model>: {}", &out[index + 6..]);
    }
    if let Some(index) = out.to_ascii_lowercase().find("not found: ") {
        out.truncate(index + 10);
    }
    out
}

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    rayon::ThreadPoolBuilder::new().num_threads(previs_native::default_workers()).build_global().unwrap();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let request = PlanRequest {
        plugin: PathBuf::from(&args[0]),
        master_dirs: vec![PathBuf::from(&args[3])],
        loose_roots: args[4..].iter().map(PathBuf::from).collect(),
        archives: Vec::new(),
        scene_dir: PathBuf::from(&args[1]),
        worlds: Vec::new(),
        interiors: true,
        accept_reference_layer: false,
        solve_into: std::env::var_os("SOLVE_INTO").map(PathBuf::from),
        keep_scenes: false,
        interior_cdx_only: false,
    };
    let started = std::time::Instant::now();
    let report = plan(&request).expect("plan failed");
    let interior_jobs = report.jobs.iter().filter(|j| j.world.is_none()).count();
    println!(
        "{} in {:.0}s: jobs {} (interior {interior_jobs}, exterior {}), stamps {}, skipped {}",
        report.plugin_name,
        started.elapsed().as_secs_f64(),
        report.jobs.len(),
        report.jobs.len() - interior_jobs,
        report.stamps.len(),
        report.skipped.len()
    );
    let mut tally: BTreeMap<(bool, String), (usize, Option<String>)> = BTreeMap::new();
    for skip in &report.skipped {
        let entry = tally
            .entry((skip.world.is_none() && skip.grid.is_none(), category(&skip.reason)))
            .or_insert((0, None));
        entry.0 += 1;
        entry.1.get_or_insert_with(|| skip.reason.clone());
    }
    let mut rows: Vec<_> = tally.into_iter().collect();
    rows.sort_by(|a, b| b.1.0.cmp(&a.1.0));
    for ((interior, reason), (count, example)) in rows {
        println!(
            "{count:>6}  {}  {reason}\n          e.g. {}",
            if interior { "INT" } else { "EXT" },
            example.unwrap()
        );
    }
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for job in &report.jobs {
        for note in &job.non_parity {
            *kinds.entry(note.split(" (x").next().unwrap().to_string()).or_default() += 1;
        }
    }
    println!(
        "non-parity jobs {} of {}; jobs per note kind:",
        report.jobs.iter().filter(|j| !j.non_parity.is_empty()).count(),
        report.jobs.len()
    );
    let mut kinds: Vec<_> = kinds.into_iter().collect();
    kinds.sort_by(|a, b| b.1.cmp(&a.1));
    for (kind, count) in kinds {
        println!("{count:>6}  {kind}");
    }
    let skips: Vec<_> = report
        .skipped
        .iter()
        .map(|s| serde_json::json!({"world": s.world, "grid": s.grid, "cell": s.cell, "reason": s.reason}))
        .collect();
    std::fs::write(&args[2], serde_json::to_string_pretty(&skips).unwrap()).unwrap();
    let jobs: Vec<_> = report
        .jobs
        .iter()
        .map(|j| serde_json::json!({"root_cell": j.root_cell, "world": j.world, "scene": j.scene_path, "non_parity": j.non_parity}))
        .collect();
    std::fs::write(format!("{}.jobs.json", args[2]), serde_json::to_string_pretty(&jobs).unwrap()).unwrap();
}
