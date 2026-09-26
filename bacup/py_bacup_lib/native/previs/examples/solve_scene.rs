//! Dev harness: decode planner `.scene` files, run the clean-room visibility
//! solve and print cell/portal/object statistics.
//!
//! Args: [--voxel <size>] <scene file or dir>...
use std::path::{Path, PathBuf};
use std::time::Instant;

use rayon::prelude::*;

use previs_native::scene::Scene;
use previs_native::umbra_solve::{PortalTarget, SolveParams, Solved, solve_bounded};

fn scene_files(path: &Path) -> Vec<PathBuf> {
    if path.is_dir() {
        let mut files: Vec<PathBuf> = std::fs::read_dir(path)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "scene"))
            .collect();
        files.sort();
        files
    } else {
        vec![path.to_path_buf()]
    }
}

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    rayon::ThreadPoolBuilder::new().num_threads(previs_native::default_workers()).build_global().unwrap();
    let mut params = SolveParams::default();
    let mut files = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--voxel" {
            params.voxel_size = args.next().unwrap().parse().unwrap();
        } else {
            files.extend(scene_files(Path::new(&arg)));
        }
    }
    let started = Instant::now();
    files.par_iter().for_each(|file| {
        let scene = Scene::decode(&std::fs::read(file).unwrap()).unwrap();
        let t = Instant::now();
        let model = match solve_bounded(&scene, &params) {
            Solved::Model(model) => model,
            Solved::NoViewVolume => return println!("{}: no view volume", file.display()),
        };
        let cells: usize = model.tiles.iter().map(|t| t.cells.len()).sum();
        let portals: Vec<_> = model.tiles.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.portals)).collect();
        let outside = portals.iter().filter(|p| p.target == PortalTarget::Outside).count();
        let listed: usize = model.tiles.iter().flat_map(|t| &t.cells).map(|c| c.objects.len()).sum();
        let occluders = scene.instances.iter().filter(|i| i.flags & 1 != 0).count();
        println!(
            "{}: {:.2}s tiles={} cells={} portals={} (outside {}) objects={} listings={} occluders={}",
            file.file_name().unwrap().to_string_lossy(),
            t.elapsed().as_secs_f64(),
            model.tiles.len(),
            cells,
            portals.len(),
            outside,
            model.objects.len(),
            listed,
            occluders,
        );
    });
    println!("{} scene(s) in {:.1}s", files.len(), started.elapsed().as_secs_f64());
}
