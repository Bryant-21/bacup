//! Scene -> clean-room solve -> tome -> validate -> `.uvd`, with structural
//! stats next to the shipped tome for the same root cell.
//!
//! Args: <retail vis dir> <out dir> <scene file or dir>...
//! A scene named `01003511.scene` is compared with `<retail vis dir>/00003511.uvd`.
//! Writes only into `<out dir>`.
use std::path::PathBuf;
use std::time::Instant;

use previs_native::scene::Scene;
use previs_native::umbra_build::build_from_model;
use previs_native::umbra_query::visible_objects;
use previs_native::umbra_solve::{SolveParams, solve};
use previs_native::umbra_tome::{Tome, parse, write};
use previs_native::umbra_validate::validate;

fn line(label: &str, tome: &Tome, bytes: usize) -> String {
    let tiles = || tome.tiles.iter().flatten();
    let max_cells = tiles().map(|t| t.num_cells()).max().unwrap_or(0);
    let max_portals = tiles()
        .map(|t| t.portals.as_ref().map_or(0, Vec::len))
        .max()
        .unwrap_or(0);
    format!(
        "{label:6} {bytes:>9} B  tiles {:>3} (leaf {:>3})  cells {:>5} (max/tile {max_cells:>4})  portals {:>6} (max/tile {max_portals:>5})  clusters {:>4}  objects {:>4}  list entries {:>5}  cell-tree nodes {:>8} (max/tile {:>7}, splits {:>8})",
        tiles().count(),
        tiles().filter(|t| t.is_leaf()).count(),
        tiles().map(|t| t.num_cells()).sum::<usize>(),
        tiles()
            .map(|t| t.portals.as_ref().map_or(0, Vec::len))
            .sum::<usize>(),
        tome.num_clusters(),
        tome.num_objects,
        tome.object_lists.as_ref().map_or(0, |l| l.count),
        tiles().map(|t| t.tree.node_count as usize).sum::<usize>(),
        tiles().map(|t| t.tree.node_count).max().unwrap_or(0),
        tiles().map(|t| t.tree.split_count as usize).sum::<usize>(),
    )
}

/// Mean visible-object count over an interior lattice of `bounds`, and how
/// many lattice points resolved to a cell.
fn mean_visible(tome: &Tome, min: [f32; 3], max: [f32; 3]) -> (f64, usize) {
    let steps = 8;
    let (mut total, mut hits) = (0usize, 0usize);
    for i in 0..steps * steps * steps {
        let k = [i % steps, i / steps % steps, i / (steps * steps)];
        let p = std::array::from_fn(|a| {
            min[a] + (max[a] - min[a]) * (k[a] as f32 + 0.5) / steps as f32
        });
        if let Ok(Some(objects)) = visible_objects(tome, p) {
            total += objects.len();
            hits += 1;
        }
    }
    (total as f64 / hits.max(1) as f64, hits)
}

fn main() {
    rayon::ThreadPoolBuilder::new().num_threads(previs_native::default_workers()).build_global().unwrap();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (retail_dir, out) = (PathBuf::from(&args[0]), PathBuf::from(&args[1]));
    std::fs::create_dir_all(&out).unwrap();
    let mut files: Vec<PathBuf> = Vec::new();
    for arg in &args[2..] {
        let path = PathBuf::from(arg);
        if path.is_dir() {
            let mut scenes: Vec<PathBuf> = std::fs::read_dir(&path)
                .unwrap()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "scene"))
                .collect();
            scenes.sort();
            files.extend(scenes);
        } else {
            files.push(path);
        }
    }
    for path in &files {
        let path = path.as_path();
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let root = u32::from_str_radix(&stem, 16).unwrap() & 0x00FF_FFFF;
        let scene = Scene::decode(&std::fs::read(path).unwrap()).unwrap();

        let started = Instant::now();
        let Some(model) = solve(&scene, &SolveParams::default()) else {
            println!("== {stem}: no view volume");
            continue;
        };
        let solved = started.elapsed().as_secs_f64();
        let tome = match build_from_model(&model) {
            Ok(t) => t,
            Err(e) => {
                println!("== {stem}: cannot encode: {e}");
                continue;
            }
        };
        validate(&tome).unwrap();
        let bytes = write(&tome);
        let reparsed = parse(&bytes).unwrap();
        assert_eq!(write(&reparsed), bytes, "{stem}: round trip");
        validate(&reparsed).unwrap();
        let encoded = started.elapsed().as_secs_f64() - solved;
        let name = format!("{root:08x}.uvd");
        std::fs::write(out.join(&name), &bytes).unwrap();

        println!(
            "== {stem}  solve {solved:.2}s  encode+validate {encoded:.2}s  -> {}",
            out.join(&name).display()
        );
        println!("   {}", line("ours", &tome, bytes.len()));
        let (min, max) = (tome.tree_min, tome.tree_max);
        let (ours_mean, ours_hits) = mean_visible(&tome, min, max);
        let retail_path = retail_dir.join(&name);
        let Ok(retail_bytes) = std::fs::read(&retail_path) else {
            println!("   retail (none at {})", retail_path.display());
            continue;
        };
        let retail = parse(&retail_bytes).unwrap();
        println!("   {}", line("retail", &retail, retail_bytes.len()));
        let (retail_mean, retail_hits) = mean_visible(&retail, min, max);
        println!(
            "   bounds ours {:?}..{:?} retail {:?}..{:?}",
            min, max, retail.tree_min, retail.tree_max
        );
        println!(
            "   mean objects per portal walk on an 8^3 lattice: ours {ours_mean:.1} ({ours_hits} pts), retail {retail_mean:.1} ({retail_hits} pts)"
        );
    }
}
