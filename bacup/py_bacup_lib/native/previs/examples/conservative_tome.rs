//! Plans retail cells with the existing previs planner, builds conservative
//! tomes from the scenes, validates and round-trips them, and compares their
//! structure with the shipped UVD for the same root cell.
//!
//! `cargo run --release --example conservative_tome -- <DataDir> <plugin> <corpus> <outdir> interiors [limit]`
//! `cargo run --release --example conservative_tome -- <DataDir> <plugin> <corpus> <outdir> world <EDID> [limit]`
//! `interiors-any [limit]` also accepts interiors that ship no UVD (no retail comparison).
//! `cargo run --release --example conservative_tome -- --from-retail <uvd>...` rebuilds from a shipped
//! tome's own bounds and object boxes (for interiors the planner cannot yet plan).
//! Set `EXTRACTED` to an extracted-assets folder to resolve loose files before the archives.
//! Reads the game data read-only; writes only into `<outdir>`.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use previs_native::assets::AssetResolver;
use previs_native::plugin::LoadOrder;
use previs_native::previs::{self, ClusterPlan, Context, ModelCache};
use previs_native::records::{self, ReferencePolicy};
use previs_native::scene::{SceneBuilder, SceneSink};
use previs_native::umbra_build::{build_conservative, scene_objects};
use previs_native::umbra_tome::{Tome, parse, write};
use previs_native::umbra_validate::validate;

type Planner<'a> = Box<dyn Fn(&mut dyn SceneSink) -> previs_native::error::Result<ClusterPlan> + 'a>;

struct Stats {
    bytes: usize,
    tiles: usize,
    leaves: usize,
    cells: usize,
    portals: usize,
    clusters: usize,
    objects: u32,
    bounds: ([f32; 3], [f32; 3]),
}

fn stats(tome: &Tome, bytes: usize) -> Stats {
    Stats {
        bytes,
        tiles: tome.tiles.iter().flatten().count(),
        leaves: tome.tiles.iter().flatten().filter(|t| t.is_leaf()).count(),
        cells: tome.tiles.iter().flatten().map(|t| t.num_cells()).sum(),
        portals: tome
            .tiles
            .iter()
            .flatten()
            .map(|t| t.portals.as_ref().map_or(0, Vec::len))
            .sum(),
        clusters: tome.num_clusters(),
        objects: tome.num_objects,
        bounds: (tome.tree_min, tome.tree_max),
    }
}

fn line(label: &str, s: &Stats) -> String {
    format!(
        "{label:12} {:>9} B  tiles {:>6} (leaf {:>6})  cells {:>6}  portals {:>7}  clusters {:>4}  objects {:>5}  bounds {:?}..{:?}",
        s.bytes,
        s.tiles,
        s.leaves,
        s.cells,
        s.portals,
        s.clusters,
        s.objects,
        s.bounds.0,
        s.bounds.1
    )
}

fn retail_uvd(corpus: &Path, plugin: &str, root: u32) -> Option<PathBuf> {
    let name = format!("{:08x}.uvd", root & 0x00FF_FFFF);
    let stem = plugin.to_ascii_lowercase();
    let mut found = Vec::new();
    for archive in std::fs::read_dir(corpus).ok()? {
        let path = archive.ok()?.path().join("vis").join(&stem).join(&name);
        if path.is_file() {
            found.push(path);
        }
    }
    // A DLC archive may override a master's UVD; prefer the plugin's own archive.
    let own = stem
        .trim_end_matches(".esm")
        .replace("fallout4", "fallout4-meshesextra");
    found.sort_by_key(|p| !p.to_string_lossy().to_ascii_lowercase().contains(&own));
    found.into_iter().next()
}

fn archives(data: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(data)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy().to_string();
            n.ends_with(".ba2")
                && (n.starts_with("Fallout4 - ") || n.starts_with("DLC"))
                && ["Meshes", "Main", "Materials"]
                    .iter()
                    .any(|k| n.contains(k))
        })
        .collect();
    out.sort();
    out
}

fn from_retail(paths: &[String]) {
    use previs_native::umbra_build::Object;
    for path in paths {
        let retail_bytes = std::fs::read(path).unwrap();
        let retail = parse(&retail_bytes).unwrap();
        let objects: Vec<Object> = retail
            .object_bounds
            .iter()
            .flatten()
            .zip(retail.user_ids.iter().flatten())
            .map(|(b, &user_id)| Object {
                user_id,
                min: [b[0], b[1], b[2]],
                max: [b[3], b[4], b[5]],
            })
            .collect();
        let tome = build_conservative(retail.tree_min, retail.tree_max, &objects).unwrap();
        validate(&tome).unwrap();
        let bytes = write(&tome);
        assert_eq!(write(&parse(&bytes).unwrap()), bytes);
        println!("== {path} (input: the retail tome's bounds and objects)");
        println!("   {}", line("conservative", &stats(&tome, bytes.len())));
        println!("   {}", line("retail", &stats(&retail, retail_bytes.len())));
    }
}

fn main() {
    rayon::ThreadPoolBuilder::new().num_threads(previs_native::default_workers()).build_global().unwrap();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args[0] == "--from-retail" {
        return from_retail(&args[1..]);
    }
    let (data, plugin_name, corpus, out) = (
        PathBuf::from(&args[0]),
        &args[1],
        PathBuf::from(&args[2]),
        PathBuf::from(&args[3]),
    );
    let mode = args[4].as_str();
    std::fs::create_dir_all(&out).unwrap();
    let load_order = LoadOrder::open(&data.join(plugin_name), std::slice::from_ref(&data)).unwrap();
    let loose_roots = std::env::var_os("EXTRACTED").map(PathBuf::from).into_iter().collect();
    let assets = AssetResolver::new(loose_roots, &archives(&data)).unwrap();
    let models = ModelCache::default();
    let context = Context {
        load_order: &load_order,
        assets: &assets,
        models: &models,
        policy: ReferencePolicy { accept_layer: true },
    };
    let context = &context;
    let plugin = load_order.target();

    let mut plans: Vec<(String, Planner)> = Vec::new();
    let limit: usize;
    let any = mode == "interiors-any";
    if mode == "interiors" || any {
        limit = args
            .get(5)
            .and_then(|v| v.parse().ok())
            .unwrap_or(usize::MAX);
        for cell in plugin.records_of(b"CELL") {
            let interior = plugin
                .cells
                .get(&cell.form_id)
                .is_none_or(|t| t.world.is_none())
                && records::read_cell(plugin, &cell).is_ok_and(|c| c.interior);
            if interior && (any || retail_uvd(&corpus, &plugin.name, cell.form_id).is_some()) {
                let id = cell.form_id;
                plans.push((
                    format!("interior {id:08X}"),
                    Box::new(move |sink| previs::plan_interior_cell(context, id, sink)),
                ));
            }
        }
    } else {
        let wanted = args[5].to_ascii_lowercase();
        limit = args
            .get(6)
            .and_then(|v| v.parse().ok())
            .unwrap_or(usize::MAX);
        for world_record in plugin.records_of(b"WRLD") {
            let Ok(world) = records::read_world(plugin, &world_record) else {
                continue;
            };
            if world.editor_id.to_ascii_lowercase() != wanted {
                continue;
            }
            let mut roots = BTreeSet::new();
            for &cell in plugin
                .world_cells
                .get(&world_record.form_id)
                .map(Vec::as_slice)
                .unwrap_or(&[])
            {
                if let Ok(records::Cell {
                    grid: Some((x, y)), ..
                }) = records::read_cell(plugin, &plugin.record(cell).unwrap())
                {
                    roots.insert((3 * (x + 1).div_euclid(3), 3 * (y + 1).div_euclid(3)));
                }
            }
            for (x, y) in roots {
                let wid = world_record.form_id;
                plans.push((
                    format!("{} {x},{y}", world.editor_id),
                    Box::new(move |sink| previs::plan_exterior_cluster(context, wid, x, y, sink)),
                ));
            }
        }
    }

    let (mut planned, mut compared, mut skipped) = (0, 0, Vec::new());
    for (label, plan) in plans {
        if compared >= limit {
            break;
        }
        let mut builder = SceneBuilder::default();
        let plan = match plan(&mut builder) {
            Ok(p) => p,
            Err(e) => {
                skipped.push(format!("{label}: {e}"));
                continue;
            }
        };
        let Some(volume) = plan.volume else { continue };
        let scene = &builder.finish(volume);
        let retail_path = retail_uvd(&corpus, &plugin.name, plan.root_cell);
        if retail_path.is_none() && !any {
            continue;
        }
        planned += 1;
        let objects = scene_objects(scene);
        let (min, max) = match plan.task_bounds {
            Some(b) => ([b[0], b[1], b[2]], [b[3], b[4], b[5]]),
            None => {
                let v = scene.volumes[0];
                ([v[0], v[1], v[2]], [v[3], v[4], v[5]])
            }
        };
        let tome = build_conservative(min, max, &objects).unwrap();
        validate(&tome).unwrap();
        let bytes = write(&tome);
        let reparsed = parse(&bytes).unwrap();
        assert_eq!(write(&reparsed), bytes, "{label}: round trip");
        validate(&reparsed).unwrap();
        std::fs::write(
            out.join(format!("{:08X}.uvd", plan.root_cell & 0x00FF_FFFF)),
            &bytes,
        )
        .unwrap();

        println!(
            "== {label} root {:08X}  ({} scene instances)",
            plan.root_cell,
            scene.instances.len()
        );
        println!("   {}", line("conservative", &stats(&tome, bytes.len())));
        if let Some(retail_path) = retail_path {
            let retail_bytes = std::fs::read(&retail_path).unwrap();
            let retail = parse(&retail_bytes).unwrap();
            let ours_ids: HashSet<u32> = tome.user_ids.iter().flatten().copied().collect();
            let retail_ids: HashSet<u32> = retail.user_ids.iter().flatten().copied().collect();
            println!("   {}", line("retail", &stats(&retail, retail_bytes.len())));
            println!(
                "   target ids: ours {}, retail {}, shared {}, ours-only {}, retail-only {}",
                ours_ids.len(),
                retail_ids.len(),
                ours_ids.intersection(&retail_ids).count(),
                ours_ids.difference(&retail_ids).count(),
                retail_ids.difference(&ours_ids).count()
            );
        } else {
            println!("   retail       (no shipped UVD for this cell)");
        }
        compared += 1;
    }
    println!(
        "compared {compared} (planned with scene + retail UVD {planned}), skipped {}",
        skipped.len()
    );
    let mut reasons = std::collections::BTreeMap::<String, usize>::new();
    for s in &skipped {
        let reason: String = s
            .split(": ")
            .skip(1)
            .collect::<Vec<_>>()
            .join(": ")
            .chars()
            .take(90)
            .collect();
        *reasons.entry(reason).or_default() += 1;
    }
    for (r, n) in reasons.iter().take(12) {
        println!("  skip x{n}: {r}");
    }
}
