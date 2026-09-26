//! Samples a tome on a grid: which points land in a view cell, whether that
//! cell's box contains them (the runtime starts its portal walk there), and
//! how many objects each cell reaches with gates open.
//!
//! `cargo run --release --example tome_probe -- <file.uvd> [step] [x y z]...`
//! Explicit points print their own hit and reachable-object count.
//! `<file.uvd> cell <tile> <cell>` prints one cell's box and portals;
//! `<file.uvd> tiles` tallies leaf-tile sizes. `NO_REACH=1` skips reachability.

use previs_native::umbra_query::{CellHit, cell_bounds, cell_objects, find_cell, portal_quad, visible_objects};
use previs_native::umbra_tome::parse;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = std::fs::read(&args[0]).unwrap();
    let tome = parse(&data).unwrap();
    if args.get(1).map(String::as_str) == Some("tiles") {
        let mut sizes = std::collections::BTreeMap::new();
        for tile in tome.tiles.iter().flatten().filter(|t| t.is_leaf() && t.num_cells() > 1) {
            let size: Vec<i32> = (0..3).map(|k| (tile.tree_max[k] - tile.tree_min[k]).round() as i32).collect();
            let has_tree = tile.tree.data.is_some();
            *sizes.entry((size, has_tree)).or_insert(0usize) += 1;
        }
        for ((size, has_tree), count) in sizes {
            let dyadic = size.iter().all(|s| (*s as u32).is_power_of_two());
            println!("{size:?} tree={has_tree} power-of-two={dyadic}: {count}");
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("cell") {
        let (t, c): (usize, u32) = (args[2].parse().unwrap(), args[3].parse().unwrap());
        let tile = tome.tiles[t].as_ref().unwrap();
        println!("tile {t}: {:?} .. {:?} leaf={} cells={}", tile.tree_min, tile.tree_max, tile.is_leaf(), tile.num_cells());
        let node = &tile.cell_nodes.as_ref().unwrap()[c as usize];
        let mut objects = Vec::new();
        cell_objects(&tome, tile, c, &mut objects);
        println!("cell {c}: bounds {:?} objects {} portals {}", cell_bounds(tile, c), objects.len(), node.portal_count);
        let portals = tile.portals.as_deref().unwrap_or(&[]);
        for p in &portals[node.portal_index as usize..(node.portal_index + node.portal_count) as usize] {
            let kind = match (p.is_outside(), p.is_user(), p.is_hierarchy()) {
                (true, _, _) => "outside",
                (_, true, _) => "user",
                (_, _, true) => "hierarchy",
                _ => "regular",
            };
            let quad = portal_quad(&tile.tree_min, &tile.tree_max, p);
            let target = tome.tiles.get(p.target() as usize).and_then(Option::as_ref);
            let target_bounds = target.and_then(|tt| cell_bounds(tt, p.target_index as u32));
            println!(
                "  {kind:9} face {} -> tile {} cell {} quad plane={:.1} {:?}..{:?} target cell {:?}",
                p.face(),
                p.target(),
                p.target_index,
                quad.0,
                quad.1,
                quad.2,
                target_bounds
            );
        }
        return;
    }
    let step: f32 = args.get(1).map_or(128.0, |s| s.parse().unwrap());
    let points: Vec<[f32; 3]> = args[2.min(args.len())..]
        .chunks(3)
        .filter(|c| c.len() == 3)
        .map(|c| [c[0].parse().unwrap(), c[1].parse().unwrap(), c[2].parse().unwrap()])
        .collect();
    if !points.is_empty() {
        for p in points {
            let hit = find_cell(&tome, p).unwrap();
            let visible = visible_objects(&tome, p).unwrap().map(|v| v.len());
            println!("{p:?}: {hit:?} reachable={visible:?}");
        }
        return;
    }

    let (min, max) = (tome.tree_min, tome.tree_max);
    println!("bounds {min:?} .. {max:?}, {} objects, step {step}", tome.num_objects);
    let (mut outside, mut empty, mut cells) = (0usize, 0usize, 0usize);
    let mut outside_own_cell = 0usize;
    let skip_reach = std::env::var_os("NO_REACH").is_some();
    let mut miss_by_distance = [0usize; 5];
    let mut counts = Vec::new();
    let mut cache = std::collections::HashMap::new();
    let mut z = min[2] + step / 2.0;
    while z < max[2] {
        let mut y = min[1] + step / 2.0;
        while y < max[1] {
            let mut x = min[0] + step / 2.0;
            while x < max[0] {
                match find_cell(&tome, [x, y, z]).unwrap() {
                    CellHit::Outside => outside += 1,
                    CellHit::Empty { .. } => empty += 1,
                    CellHit::Cell { tile, cell } => {
                        cells += 1;
                        let t = tome.tiles[tile as usize].as_ref().unwrap();
                        if let Some((lo, hi)) = cell_bounds(t, cell)
                            && (0..3).any(|k| [x, y, z][k] < lo[k] - 1.0 || [x, y, z][k] > hi[k] + 1.0)
                        {
                            outside_own_cell += 1;
                            let p = [x, y, z];
                            let off = (0..3).map(|k| (lo[k] - p[k]).max(p[k] - hi[k]).max(0.0)).fold(0.0f32, f32::max);
                            let bucket = [16.0, 32.0, 64.0, 128.0].iter().position(|&b| off <= b).unwrap_or(4);
                            miss_by_distance[bucket] += 1;
                            if outside_own_cell <= 3 {
                                println!("  point {:?} -> tile {tile} cell {cell} whose box is {lo:?}..{hi:?}", [x, y, z]);
                            }
                        }
                        let n = *cache
                            .entry((tile, cell))
                            .or_insert_with(|| {
                                if skip_reach {
                                    return 0;
                                }
                                visible_objects(&tome, [x, y, z]).unwrap().map_or(0, |v| v.len())
                            });
                        counts.push((n, [x, y, z]));
                    }
                }
                x += step;
            }
            y += step;
        }
        z += step;
    }
    println!("samples: {cells} in a cell, {empty} empty/solid, {outside} outside the tree");
    println!("distinct cells sampled: {}", cache.len());
    println!("samples outside their own cell's box: {outside_own_cell} of {cells}");
    println!("  by distance outside it (<=16, <=32, <=64, <=128, more): {miss_by_distance:?}");
    counts.sort_by_key(|c| c.0);
    let pct = |p: f64| counts.get(((counts.len() as f64 - 1.0) * p) as usize).map_or(0, |c| c.0);
    println!(
        "reachable objects per sample: min {} p10 {} p50 {} p90 {} max {}",
        pct(0.0),
        pct(0.1),
        pct(0.5),
        pct(0.9),
        pct(1.0)
    );
    let mut sizes: Vec<usize> = cache.values().copied().collect();
    sizes.sort_unstable();
    sizes.dedup();
    println!("distinct reachable-set sizes (smallest 20): {:?}", &sizes[..sizes.len().min(20)]);
    let full = counts.last().map_or(0, |c| c.0);
    let isolated: Vec<&(usize, [f32; 3])> = counts.iter().filter(|c| c.0 * 10 < full * 9).collect();
    println!("samples reaching <90% of the max: {} of {}", isolated.len(), counts.len());
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for (_, p) in &isolated {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    println!("isolated samples span {lo:?} .. {hi:?}");
    for (n, p) in isolated.iter().step_by((isolated.len() / 12).max(1)) {
        println!("  isolated: {n} at {p:?}");
    }
}
