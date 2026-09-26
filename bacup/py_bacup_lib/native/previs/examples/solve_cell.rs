//! Plans and solves one CELL the way the pipeline does and writes its tome,
//! for offline checks with `tome_probe` and `vis_harness`. An exterior CELL
//! plans the 3x3 cluster it belongs to.
//!
//! `cargo run --release --example solve_cell -- <plugin> <game data> <extracted root> <cell hex> <out.uvd>`
//! `cargo run --release --example solve_cell -- <file.scene> <out.uvd>` solves a saved scene.
//! `SCENE_OUT=<file>` also writes the planned scene; a scene's point view volumes
//! seed the camera (`SKY=1` adds the open sky); `MIN_TREE_LEAF=<units>` and
//! `VOXEL`, `TILE`, `MAX_TILE_CELLS`, `BLOCK`, `MERGE_OPEN`, `MERGE_FILL`,
//! and `MAX_MERGED` override those solve parameters.
//! `WORKERS` sets the solve pool size; `TIMING_OUT` writes detailed solve timings.
use std::path::PathBuf;

use previs_native::assets::AssetResolver;
use previs_native::plugin::LoadOrder;
use previs_native::previs::{self, ClusterPlan, Context, ModelCache};
use previs_native::records::{self, ReferencePolicy};
use previs_native::scene::{Scene, SceneBuilder, SceneSink, TeeSink, exterior_volume};
use previs_native::umbra_solve::{Aabb, Occupancy, OccupancySink, Reach, SolveParams, Solved, scene_volume, solve_occupancy_profiled};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn cluster_root(value: i32) -> i32 {
    3 * (value + 1).div_euclid(3)
}

/// Plans the CELL (or its exterior cluster) into an occupancy, with the scene too for `SCENE_OUT`.
fn plan(args: &[String], params: &SolveParams) -> (Occupancy, Aabb, Reach) {
    let plugin = PathBuf::from(&args[0]);
    let data = PathBuf::from(&args[1]);
    let extracted = PathBuf::from(&args[2]);
    let cell = u32::from_str_radix(&args[3], 16).unwrap();
    let mod_root = plugin.parent().unwrap().to_path_buf();
    let load_order = LoadOrder::open(&plugin, &[mod_root.clone(), data.clone()]).unwrap();
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
    let assets = AssetResolver::new(vec![mod_root.join("data"), extracted], &archives).unwrap();
    let models = ModelCache::default();
    let context = Context {
        load_order: &load_order,
        assets: &assets,
        models: &models,
        policy: ReferencePolicy { accept_layer: false },
    };
    let target = load_order.target();
    let form_id = target.records_of(b"CELL").map(|r| r.form_id).find(|id| id & 0x00FF_FFFF == cell).unwrap();
    let record = records::read_cell(target, &target.record(form_id).unwrap()).unwrap();
    let world = target.cells.get(&form_id).and_then(|t| t.world);
    let (footprint, reach) = match (world, record.grid) {
        (Some(_), Some((x, y))) => {
            let v = exterior_volume(cluster_root(x), cluster_root(y), 0.0);
            let footprint = Aabb { min: [v[0], v[1], f32::NEG_INFINITY], max: [v[3], v[4], f32::INFINITY] };
            (Some(footprint), Reach { sky: true, points: Vec::new() })
        }
        _ => {
            let camera = previs::interior_camera(target, form_id).unwrap();
            (camera.region, Reach { sky: false, points: camera.seeds })
        }
    };
    let planner = |sink: &mut dyn SceneSink| -> ClusterPlan {
        match (world, record.grid) {
            (Some(world), Some((x, y))) => previs::plan_exterior_cluster(&context, world, cluster_root(x), cluster_root(y), sink),
            _ => previs::plan_interior_cell(&context, form_id, sink),
        }
        .unwrap()
    };
    let mut occupancy = OccupancySink::new(params, footprint);
    let scene_out = std::env::var_os("SCENE_OUT");
    let plan = if let Some(path) = &scene_out {
        let mut scene = SceneBuilder::default();
        let plan = planner(&mut TeeSink(&mut occupancy, &mut scene));
        std::fs::write(path, scene.finish(plan.volume.unwrap()).encode()).unwrap();
        plan
    } else {
        planner(&mut occupancy)
    };
    for note in &plan.non_parity {
        eprintln!("note: {note}");
    }
    (occupancy.finish(), scene_volume(&[plan.volume.unwrap()]).unwrap(), reach)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut params = SolveParams::default();
    let env = |name: &str| std::env::var(name).ok().and_then(|v| v.parse().ok());
    if let Some(leaf) = env("MIN_TREE_LEAF") {
        params.min_tree_leaf = leaf;
    }
    if let Some(voxel) = env("VOXEL") {
        params.voxel_size = voxel;
    }
    if let Some(tile) = env("TILE") {
        params.tile_size = tile;
    }
    if let Some(cells) = env("MAX_TILE_CELLS") {
        params.max_tile_cells = cells as u32;
    }
    if let Some(block) = env("BLOCK") {
        params.block_size = block;
    }
    if let Some(fraction) = env("MERGE_OPEN") {
        params.merge_open_fraction = fraction;
    }
    if let Some(fill) = env("MERGE_FILL") {
        params.merge_fill = fill;
    }
    if let Some(longest) = env("MAX_MERGED") {
        params.max_merged_cell = longest;
    }
    let started = std::time::Instant::now();
    let ((occupancy, volume, reach), out) = if args[0].ends_with(".scene") {
        let scene = Scene::decode(&std::fs::read(&args[0]).unwrap()).unwrap();
        // CK's point view volumes are its camera seeds.
        let points = scene.volumes.iter().filter(|v| (0..3).all(|k| v[k] == v[k + 3])).map(|v| [v[0], v[1], v[2]]).collect();
        let reach = Reach { sky: std::env::var_os("SKY").is_some(), points };
        ((Occupancy::from_scene(&scene, &params), scene_volume(&scene.volumes).unwrap(), reach), PathBuf::from(&args[1]))
    } else {
        (plan(&args, &params), PathBuf::from(&args[4]))
    };
    let planned = started.elapsed();
    let pool = previs_native::worker_pool(env("WORKERS").map(|n| n as usize));
    let (solved, timing) = pool.install(|| solve_occupancy_profiled(occupancy, volume, &params, &reach));
    let Solved::Model(model) = solved else {
        panic!("no model");
    };
    if let Some(path) = std::env::var_os("TIMING_OUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&timing).unwrap()).unwrap();
    }
    let tome = previs_native::umbra_build::build_from_model(&model).unwrap();
    previs_native::umbra_validate::validate(&tome).unwrap();
    let bytes = previs_native::umbra_tome::write(&tome);
    std::fs::write(&out, &bytes).unwrap();
    let cells: usize = tome.tiles.iter().flatten().map(|t| t.num_cells()).sum();
    println!(
        "{}: {} objects, {} leaf tiles, {cells} cells, {} bytes, planned in {planned:.1?}, total {:.1?}",
        out.display(),
        tome.num_objects,
        tome.tiles.iter().flatten().filter(|t| t.is_leaf()).count(),
        bytes.len(),
        started.elapsed()
    );
}
