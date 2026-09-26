//! Dev harness: dumps each combinable reference's runtime bound beside the
//! group key a CK-generated plugin's XCRI gave it, for recovering CK's
//! exterior group key.
//!
//! Args: <plugin> <ck plugin> <out json> <cell hex,cell hex,...> <master dir>... [--loose <dir>...] [--archive-dir <FO4 Data>]
use std::collections::BTreeMap;
use std::path::PathBuf;

use previs_native::assets::AssetResolver;
use previs_native::plugin::{LoadOrder, Plugin, read_i32, read_u32};
use previs_native::precombine::{
    Context, SourceCache, candidate_bounds, exterior_persistent_references, linked_references,
};

fn ck_xcri(plugin: &Plugin, cell_local: u32) -> (Vec<u32>, BTreeMap<(String, u32), u32>) {
    let Some(record) = plugin.records_of(b"CELL").find(|r| r.form_id & 0x00FF_FFFF == cell_local) else {
        return (Vec::new(), BTreeMap::new());
    };
    let subrecords = plugin.subrecords_at(&record).unwrap();
    let Some(xcri) = subrecords.first(b"XCRI") else { return (Vec::new(), BTreeMap::new()) };
    let meshes = read_u32(xcri, 0).unwrap() as usize;
    let words = read_u32(xcri, 4).unwrap() as usize;
    let keys = (0..meshes).map(|i| read_u32(xcri, 8 + 4 * i).unwrap()).collect();
    let mut pairs = BTreeMap::new();
    for pair in 0..words / 2 {
        let offset = 8 + 4 * meshes + 8 * pair;
        let (owner, id) = plugin.owner_of(read_u32(xcri, offset).unwrap());
        pairs.insert((owner.to_ascii_lowercase(), id), read_u32(xcri, offset + 4).unwrap());
    }
    (keys, pairs)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let plugin_path = PathBuf::from(args.next().unwrap());
    let ck_path = PathBuf::from(args.next().unwrap());
    let out_path = PathBuf::from(args.next().unwrap());
    // `landless` probes every XCRI-stamped exterior CELL without a LAND.
    let cell_arg = args.next().unwrap();
    let (mut master_dirs, mut loose_roots, mut archives) = (Vec::new(), Vec::new(), Vec::new());
    let mut mode = "master";
    for arg in args {
        if arg == "--archive-dir" || arg == "--loose" {
            mode = if arg == "--loose" { "loose" } else { "archive" };
        } else if mode == "loose" {
            loose_roots.push(PathBuf::from(arg));
        } else if mode == "archive" {
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
        } else {
            master_dirs.push(PathBuf::from(arg));
        }
    }
    let load_order = LoadOrder::open(&plugin_path, &master_dirs).unwrap();
    let assets = AssetResolver::new(loose_roots, &archives).unwrap();
    let sources = SourceCache::default();
    let target = load_order.target();
    let linked_from = linked_references(target).unwrap();
    let exterior_persistent = exterior_persistent_references(target).unwrap();
    let context = Context {
        load_order: &load_order,
        assets: &assets,
        sources: &sources,
        linked_from: &linked_from,
        exterior_persistent: &exterior_persistent,
    };
    let ck = Plugin::open(&ck_path).unwrap();
    let cells: Vec<u32> = if cell_arg == "landless" {
        target
            .world_cells
            .values()
            .flatten()
            .filter(|&&cell| cell >> 24 == target.self_index())
            .filter(|&&cell| {
                let topology = target.cells.get(&cell);
                !topology.is_some_and(|t| t.temporary.iter().any(|r| &r.signature == b"LAND"))
                    && target.record(cell).is_some_and(|r| target.subrecords_at(&r).unwrap().first(b"XCRI").is_some())
            })
            .map(|&cell| cell & 0x00FF_FFFF)
            .take(40)
            .collect()
    } else {
        cell_arg.split(',').map(|c| u32::from_str_radix(c, 16).unwrap()).collect()
    };
    let mut out = Vec::new();
    for cell_local in cells {
        let cell = (target.self_index() << 24) | cell_local;
        let record = target.record(cell).unwrap();
        let subrecords = target.subrecords_at(&record).unwrap();
        let xclc = subrecords.first(b"XCLC").unwrap();
        let grid = (read_i32(xclc, 0).unwrap(), read_i32(xclc, 4).unwrap());
        let (ck_keys, ck_pairs) = ck_xcri(&ck, cell_local);
        let (bounds, excluded) = candidate_bounds(&context, cell).unwrap();
        let mut seen = std::collections::BTreeSet::new();
        let excluded: Vec<serde_json::Value> = excluded
            .iter()
            .map(|e| {
                let (owner, id) = target.owner_of(e.form_id);
                let key = (owner.to_ascii_lowercase(), id);
                seen.insert(key.clone());
                serde_json::json!({
                    "ref": format!("{:08X}", e.form_id),
                    "reason": e.reason,
                    "ck": ck_pairs.get(&key),
                })
            })
            .collect();
        let mut refs = Vec::new();
        for bound in &bounds {
            let (owner, id) = target.owner_of(bound.form_id);
            let key = (owner.to_ascii_lowercase(), id);
            seen.insert(key.clone());
            let base = load_order.resolve(target, bound.base_form_id);
            refs.push(serde_json::json!({
                "ref": format!("{:08X}", bound.form_id),
                "base": format!("{:08X}", bound.base_form_id),
                "base_sig": base.as_ref().map(|(_, r)| String::from_utf8_lossy(&r.signature).into_owned()),
                "base_flags": base.as_ref().map(|(_, r)| r.flags),
                "center": bound.center,
                "radius": bound.radius,
                "non_occluder": bound.non_occluder,
                "ck": ck_pairs.get(&key),
            }));
        }
        let ck_only: Vec<String> =
            ck_pairs.keys().filter(|k| !seen.contains(*k)).map(|(o, id)| format!("{o}:{id:06X}")).collect();
        let mut land_plugin = target;
        let mut land = target.cells.get(&cell).and_then(|t| t.temporary.iter().find(|r| &r.signature == b"LAND").copied());
        if land.is_none() {
            let world = target.cells.get(&cell).and_then(|t| t.world).and_then(|w| target.record(w)).unwrap();
            let parent = previs_native::records::read_world(target, &world).unwrap().land_parent;
            if let Some((parent_plugin, parent_record)) = parent.and_then(|p| load_order.resolve(target, p)) {
                if let [parent_cell] = parent_plugin.cells_at(parent_record.form_id, grid.0, grid.1) {
                    land_plugin = parent_plugin;
                    land = parent_plugin
                        .cells
                        .get(parent_cell)
                        .and_then(|t| t.temporary.iter().find(|r| &r.signature == b"LAND").copied());
                }
            }
        }
        let (land_min, land_max) = match land.map(|l| previs_native::records::read_land(land_plugin, &l).unwrap()) {
            Some(previs_native::records::LandHeights::Vhgt { base, deltas }) => {
                let heights = previs_native::scene::vhgt_heights(base, &deltas);
                let flat = heights.iter().flatten().copied();
                (Some(flat.clone().fold(f32::MAX, f32::min)), Some(flat.fold(f32::MIN, f32::max)))
            }
            _ => (None, None),
        };
        let world = target.cells.get(&cell).and_then(|t| t.world);
        let world_dnam: Option<Vec<f32>> = world.and_then(|w| {
            let record = target.record(w)?;
            let subrecords = target.subrecords_at(&record).ok()?;
            let dnam = subrecords.first(b"DNAM")?;
            Some(dnam.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect())
        });
        out.push(serde_json::json!({
            "cell": format!("{cell:08X}"),
            "world": world.map(|w| format!("{w:08X}")),
            "world_dnam": world_dnam,
            "grid": [grid.0, grid.1],
            "land_min": land_min,
            "land_max": land_max,
            "ck_keys": ck_keys,
            "refs": refs,
            "excluded": excluded,
            "ck_only": ck_only,
        }));
    }
    std::fs::write(&out_path, serde_json::to_string(&out).unwrap()).unwrap();
}
