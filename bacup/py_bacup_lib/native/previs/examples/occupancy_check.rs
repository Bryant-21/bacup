//! Checks that the occupancy a planner streams equals the one read back from
//! the scene the same plan builds, inside the tome's bounds, and dumps its
//! voxels for comparison with other voxelizers.
//!
//! `cargo run --release --example occupancy_check -- <plugin> <game data> <extracted root> <cell hex> <out prefix>`
//! writes `<prefix>.scene` and `<prefix>.vox`: the tome bounds (6 f32), then a
//! u32 count of solid voxels and their `[i32; 3]` world voxel coordinates,
//! then a u32 count of gate voxels, each `[i32; 3]` and a u32 gate index.
//! An exterior CELL plans the 3x3 cluster it belongs to.
use std::path::PathBuf;

use previs_native::assets::AssetResolver;
use previs_native::plugin::LoadOrder;
use previs_native::previs::{self, ClusterPlan, Context, ModelCache};
use previs_native::records::{self, ReferencePolicy};
use previs_native::scene::{SceneBuilder, SceneSink, TeeSink, exterior_volume};
use previs_native::umbra_solve::{Aabb, Occupancy, OccupancySink, SolveParams, scene_volume, tome_bounds};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn cluster_root(value: i32) -> i32 {
    3 * (value + 1).div_euclid(3)
}

/// Solid and gate voxels lying inside `bounds`.
fn voxels_within(occupancy: &Occupancy, bounds: &Aabb, voxel: f32) -> (Vec<[i32; 3]>, Vec<([i32; 3], u32)>) {
    let lo = bounds.min.map(|v| (v / voxel).round() as i32);
    let hi = bounds.max.map(|v| (v / voxel).round() as i32);
    let inside = |at: &[i32; 3]| (0..3).all(|a| lo[a] <= at[a] && at[a] < hi[a]);
    let (solid, gates) = occupancy.voxels();
    (solid.into_iter().filter(inside).collect(), gates.into_iter().filter(|(at, _)| inside(at)).collect())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let plugin = PathBuf::from(&args[0]);
    let data = PathBuf::from(&args[1]);
    let extracted = PathBuf::from(&args[2]);
    let cell = u32::from_str_radix(&args[3], 16).unwrap();
    let prefix = &args[4];
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
    let params = SolveParams::default();
    let (footprint, planner): (_, Box<dyn Fn(&mut dyn SceneSink) -> ClusterPlan>) = match (world, record.grid) {
        (Some(world), Some((x, y))) => {
            let (x, y) = (cluster_root(x), cluster_root(y));
            let v = exterior_volume(x, y, 0.0);
            let context = &context;
            (
                Some(Aabb { min: [v[0], v[1], f32::NEG_INFINITY], max: [v[3], v[4], f32::INFINITY] }),
                Box::new(move |sink| previs::plan_exterior_cluster(context, world, x, y, sink).unwrap()),
            )
        }
        _ => (None, Box::new(|sink| previs::plan_interior_cell(&context, form_id, sink).unwrap())),
    };
    let mut streamed = OccupancySink::new(&params, footprint);
    let mut builder = SceneBuilder::default();
    let plan = planner(&mut TeeSink(&mut streamed, &mut builder));
    let scene = builder.finish(plan.volume.unwrap());
    std::fs::write(format!("{prefix}.scene"), scene.encode()).unwrap();
    let streamed = streamed.finish();
    let read = Occupancy::from_scene(&scene, &params);
    let volume = scene_volume(&scene.volumes).unwrap();
    let bounds = tome_bounds(&streamed, volume, &params).unwrap();
    assert_eq!(Some(bounds), tome_bounds(&read, volume, &params), "tome bounds differ");
    assert!(streamed.objects == read.objects, "targets differ");
    assert_eq!(streamed.gates, read.gates, "gates differ");
    let (solid, gates) = voxels_within(&streamed, &bounds, params.voxel_size);
    assert!((solid.clone(), gates.clone()) == voxels_within(&read, &bounds, params.voxel_size), "voxels differ");

    let mut out = Vec::new();
    for v in bounds.min.iter().chain(&bounds.max) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(solid.len() as u32).to_le_bytes());
    for at in &solid {
        at.iter().for_each(|c| out.extend_from_slice(&c.to_le_bytes()));
    }
    out.extend_from_slice(&(gates.len() as u32).to_le_bytes());
    for (at, gate) in &gates {
        at.iter().for_each(|c| out.extend_from_slice(&c.to_le_bytes()));
        out.extend_from_slice(&gate.to_le_bytes());
    }
    std::fs::write(format!("{prefix}.vox"), out).unwrap();
    println!(
        "{:08X}: streamed == scene; bounds {:?}..{:?}, {} solid voxels, {} gate voxels, {} targets, {} gates",
        plan.root_cell,
        bounds.min,
        bounds.max,
        solid.len(),
        gates.len(),
        read.objects.len(),
        read.gates.len()
    );
}
