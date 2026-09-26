//! Repeats a frozen scene as concurrent cluster jobs, including occupancy,
//! solving and tome serialization: `<scene> <report.json> <workers> <jobs> [sky]`.
use std::time::Instant;

use previs_native::scene::Scene;
use previs_native::umbra_solve::{Occupancy, Reach, SolveParams, Solved, scene_volume, solve_occupancy};
use rayon::prelude::*;
use serde::Serialize;

#[path = "../src/memory.rs"]
mod memory;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Serialize)]
struct Output {
    sha256: String,
    bytes: usize,
    objects: usize,
    tiles: usize,
    cells: usize,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(args.len() >= 4, "<scene> <report.json> <workers> <jobs> [sky]");
    let mut memory = memory::MemoryMonitor::start();
    let scene = Scene::decode(&std::fs::read(&args[0]).unwrap()).unwrap();
    let volume = scene_volume(&scene.volumes).unwrap();
    let reach = Reach {
        sky: args.get(4).is_some_and(|v| v == "sky"),
        points: scene.volumes.iter().filter(|v| (0..3).all(|k| v[k] == v[k + 3])).map(|v| [v[0], v[1], v[2]]).collect(),
    };
    let workers: usize = args[2].parse().unwrap();
    let jobs: usize = args[3].parse().unwrap();
    let pool = previs_native::worker_pool(Some(workers));
    let params = SolveParams::default();
    let started = Instant::now();
    let outputs: Vec<Output> = pool.install(|| {
        (0..jobs)
            .into_par_iter()
            .map(|_| {
                let occupancy = Occupancy::from_scene(&scene, &params);
                let Solved::Model(model) = solve_occupancy(occupancy, volume, &params, &reach) else { panic!("no view volume") };
                let tome = previs_native::umbra_build::build_from_model(&model).unwrap();
                drop(model);
                previs_native::umbra_validate::validate(&tome).unwrap();
                let bytes = previs_native::umbra_tome::write(&tome);
                Output {
                    sha256: previs_native::sha256_hex(&bytes),
                    bytes: bytes.len(),
                    objects: tome.num_objects as usize,
                    tiles: tome.tiles.iter().flatten().filter(|t| t.is_leaf()).count(),
                    cells: tome.tiles.iter().flatten().map(|t| t.num_cells()).sum(),
                }
            })
            .collect()
    });
    let seconds = started.elapsed().as_secs_f64();
    let report = serde_json::json!({ "seconds": seconds, "workers": workers, "jobs": jobs, "outputs": outputs, "memory": memory.finish() });
    std::fs::write(&args[1], serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("{jobs} cluster jobs on {workers} workers: {seconds:.3}s");
}
