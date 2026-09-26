//! Dev harness: run the precombine stage on a plugin exactly as BACUP does
//! and print a summary (for timing and peak-memory measurement).
//!
//! The CSG is written beside the plugin, so point it at a scratch copy.
//! `--stamp <work dir>` runs the whole stage (generate, stamp the plugin in
//! place, write the CDX) and writes the report and timing JSON there.
//!
//! Args: <plugin> <output data dir> <master dir>... [--loose <dir>...] [--archive-dir <FO4 Data>] [--workers <n>]
//! [--stamp <work dir>]
use std::path::PathBuf;

use previs_native::precombine_stage::{PrecombineRequest, generate};

fn main() {
    let mut args = std::env::args().skip(1);
    let plugin = PathBuf::from(args.next().unwrap());
    let output_dir = PathBuf::from(args.next().unwrap());
    let (mut master_dirs, mut loose_roots, mut archives) = (Vec::new(), Vec::new(), Vec::new());
    let mut workers = previs_native::default_workers();
    let mut stamp: Option<PathBuf> = None;
    let mut mode = "master";
    for arg in args {
        match arg.as_str() {
            "--loose" => mode = "loose",
            "--archive-dir" => mode = "archive",
            "--workers" => mode = "workers",
            "--stamp" => mode = "stamp",
            _ if mode == "workers" => workers = arg.parse().unwrap(),
            _ if mode == "stamp" => stamp = Some(PathBuf::from(arg)),
            _ if mode == "loose" => loose_roots.push(PathBuf::from(arg)),
            _ if mode == "archive" => {
                let mut found: Vec<PathBuf> = std::fs::read_dir(&arg)
                    .unwrap()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        let name = p.file_name().unwrap().to_string_lossy().to_lowercase();
                        name.ends_with(".ba2") && (name.contains(" - meshes") || name.contains(" - materials"))
                    })
                    .collect();
                found.sort();
                archives.extend(found);
            }
            _ => master_dirs.push(PathBuf::from(arg)),
        }
    }
    let started = std::time::Instant::now();
    let pool = previs_native::worker_pool(Some(workers));
    let request = PrecombineRequest { plugin, master_dirs, loose_roots, archives, output_dir, interior_cdx_only: false };
    if let Some(work) = stamp {
        let summary = pool
            .install(|| {
                let prepared = previs_native::precombine_generation::prepare(
                    &request,
                    &work.join("precombine_report.json"),
                    &work.join("precombine_timing.json"),
                    &|_, _| {},
                )?;
                prepared.finalize(previs_native::metadata::pack_generation_date(2026, 9, 25)?)
            })
            .unwrap();
        println!("{:.0}s {}", started.elapsed().as_secs_f64(), serde_json::to_string(&summary).unwrap());
        return;
    }
    let report = pool.install(|| generate(&request)).unwrap();
    let groups: usize = report.cells.iter().map(|c| c.groups.len()).sum();
    let refs: usize = report.cells.iter().map(|c| c.xcri.reference_mesh_pairs.len()).sum();
    println!(
        "{:.0}s cells={} groups={} refs={} skipped={}",
        started.elapsed().as_secs_f64(),
        report.cells.len(),
        groups,
        refs,
        report.skipped.len()
    );
}
