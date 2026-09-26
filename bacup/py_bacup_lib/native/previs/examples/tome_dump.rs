//! Prints the structure of Umbra tomes (`.uvd`).
//!
//! `cargo run --release --example tome_dump -- <file.uvd> [--tiles] [--portals]`
//! `cargo run --release --example tome_dump -- --census <dir>` summarizes a corpus.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use previs_native::umbra_tome::{Portal, Tile, Tome, parse};

fn len<T>(v: &Option<Vec<T>>) -> usize {
    v.as_ref().map_or(0, Vec::len)
}

fn portal_kind(p: &Portal) -> &'static str {
    match (p.is_outside(), p.is_user(), p.is_hierarchy()) {
        (true, _, _) => "outside",
        (_, true, _) => "user",
        (_, _, true) => "hierarchy",
        _ => "regular",
    }
}

fn dump(path: &Path, tiles: bool, portals: bool) {
    let data = std::fs::read(path).unwrap();
    let tome = parse(&data).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    println!("{}  {} bytes", path.display(), data.len());
    println!("  build     {}", tome.build_info_text());
    println!("  bounds    {:?} .. {:?}", tome.tree_min, tome.tree_max);
    println!(
        "  lod base  {}  flags 0x{:X}",
        tome.lod_base_distance, tome.flags
    );
    println!(
        "  top tree  {} nodes, {} splits, {} lod levels, {} slot-path bits/tile",
        tome.tree.node_count,
        len(&tome.tree.splits),
        len(&tome.tile_lod_levels),
        tome.bits_per_slot_path
    );
    let present = tome.tiles.iter().flatten().count();
    let leaves = tome.tiles.iter().flatten().filter(|t| t.is_leaf()).count();
    println!(
        "  tiles     {} slots, {present} stored, {leaves} leaves",
        tome.tiles.len()
    );
    println!(
        "  cells     {} (cell starts end)",
        tome.cell_starts
            .as_ref()
            .and_then(|s| s.last())
            .copied()
            .unwrap_or(0)
    );
    println!(
        "  clusters  {} nodes, {} cluster portals",
        tome.num_clusters(),
        len(&tome.cluster_portals)
    );
    println!(
        "  objects   {} (bounds {}, distances {}, user ids {})",
        tome.num_objects,
        tome.object_bounds.is_some(),
        tome.object_distances.is_some(),
        tome.user_ids.is_some()
    );
    let w = tome.list_widths;
    println!(
        "  lists     object {} entries ({}+{} bits), cluster {} entries ({}+{} bits)",
        tome.object_lists.as_ref().map_or(0, |l| l.count),
        w & 31,
        w >> 5 & 31,
        tome.cluster_lists.as_ref().map_or(0, |l| l.count),
        w >> 10 & 31,
        w >> 15 & 31
    );
    println!(
        "  gates     {} ids, {} vertices, {} indices",
        tome.num_gates,
        len(&tome.gate_vertices),
        len(&tome.gate_indices)
    );
    println!(
        "  matching  {} leaf entries, {} trees",
        tome.leaf_matches.len(),
        tome.matching_trees.len()
    );
    if let Some(ids) = &tome.user_ids {
        let preview: Vec<String> = ids.iter().take(8).map(|id| format!("{id:08X}")).collect();
        println!("  user ids  {} ...", preview.join(" "));
    }
    if let Some(ids) = &tome.gate_ids {
        let all: Vec<String> = ids.iter().map(|id| format!("{id:08X}")).collect();
        println!("  gate ids  {}", all.join(" "));
    }
    if tiles {
        for (i, tile) in tome.tiles.iter().enumerate() {
            let Some(tile) = tile else {
                println!("  tile {i:4}: empty");
                continue;
            };
            print_tile(i, tile, portals);
        }
    }
}

fn print_tile(i: usize, tile: &Tile, portals: bool) {
    let mut kinds = BTreeMap::<&str, usize>::new();
    for p in tile.portals.iter().flatten() {
        *kinds.entry(portal_kind(p)).or_default() += 1;
    }
    println!(
        "  tile {i:4}: {}{:?}..{:?} cells {} clusters {} tree {} nodes/{} map bits planes {} bsp {} portals {kinds:?}",
        if tile.is_leaf() { "leaf " } else { "" },
        tile.tree_min,
        tile.tree_max,
        tile.num_cells(),
        tile.num_clusters,
        tile.tree.node_count,
        tile.tree.map_width,
        len(&tile.planes),
        tile.num_bsp_nodes,
    );
    if portals {
        for p in tile.portals.iter().flatten() {
            println!(
                "      {:9} face {} target {:7} idx {:5} z {:5} a {:08X} b {:08X}",
                portal_kind(p),
                p.face(),
                p.target(),
                p.target_index,
                p.z,
                p.rect_a,
                p.rect_b
            );
        }
    }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("uvd"))
        {
            out.push(path);
        }
    }
}

fn census(dir: &Path) {
    let mut files = Vec::new();
    collect(dir, &mut files);
    let mut counts = BTreeMap::<String, usize>::new();
    let mut bump = |key: String| *counts.entry(key).or_default() += 1;
    for path in &files {
        let tome: Tome = parse(&std::fs::read(path).unwrap()).unwrap();
        bump(format!("build {}", tome.build_info_text()));
        bump(format!(
            "lod base {} flags {:X}",
            tome.lod_base_distance, tome.flags
        ));
        bump(format!(
            "object distances present {}",
            tome.object_distances.is_some()
        ));
        bump(format!(
            "cluster lists present {}",
            tome.cluster_lists.is_some()
        ));
        for tile in tome.tiles.iter().flatten() {
            bump(format!(
                "tile flags {:02X} portal expand {}",
                tile.flags, tile.portal_expand
            ));
            if tile.bsp_triangles.is_some() {
                bump("tile with bsp".into());
            }
        }
        bump(format!(
            "empty tile slots {}",
            tome.tiles.iter().any(Option::is_none)
        ));
    }
    println!("{} tomes", files.len());
    for (key, n) in counts {
        println!("{n:6}  {key}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--census") {
        census(Path::new(&args[1]));
        return;
    }
    let tiles = args.iter().any(|a| a == "--tiles");
    let portals = args.iter().any(|a| a == "--portals");
    for path in args.iter().filter(|a| !a.starts_with("--")) {
        dump(Path::new(path), tiles || portals, portals);
    }
}
