//! Hierarchy (LOD) tiles for a tome that only has leaf tiles.
//!
//! FO4 queries exterior tomes through a runtime `TomeCollection`. Its start-cell
//! walk (`TileTraverseTree`, at most 4096 entries) always descends through an
//! inner top-tree node that has no tile, so a tome of leaf tiles only is walked
//! in full. Four such exterior tomes overflow it and every query fails with
//! Umbra error 2. An inner node with a tile stops the descent once the camera is
//! farther than its LOD distance, which keeps CK and retail walks small.
//!
//! Every inner node gets a coarse tile. Its cells group the leaf cells below it
//! by spatial bin and connectivity, and list every object and cluster of those
//! leaf cells. Portals link every pair of tiles that touch without overlapping,
//! at every level: regular portals into leaf tiles, hierarchy portals into inner
//! tiles. The coarse graph is a quotient of the leaf graph with merged portal
//! rects, so it never hides what the leaf cells can see.

use std::collections::BTreeMap;

use rustc_hash::FxHashMap;

use crate::error::{PrevisError, Result};
use crate::umbra_build::{ObjectRuns, bit_width, pack_box, quantize};
use crate::umbra_query::{KdView, cell_bounds, cell_objects, list_run, lod_level, portal_quad, write_bits};
use crate::umbra_tome::{CellNode, Portal, Tile, Tome, Tree};

/// Coarse cells are the connected groups of leaf cells within bins of the
/// tile's size (the cube root of its volume), at least `MIN_BIN` wide.
const BINS_PER_AXIS: f32 = 1.0;
const MIN_BIN: f32 = 512.0;

#[derive(Default)]
struct CoarseCell {
    lo: [f32; 3],
    hi: [f32; 3],
    objects: Vec<u32>,
    clusters: Vec<u32>,
}

/// Merged portal, keyed by `(source node, source cell, face, target node, target cell)`.
type PortalKey = (u32, u32, u32, u32, u32);
/// Plane and in-plane rect. Across tiles every leaf portal of a key lies on the
/// shared boundary; inside a coarse tile the plane nearest the source side is
/// kept, which only widens the portal as seen from there.
type PortalQuad = (f32, [f32; 2], [f32; 2]);
const OUTSIDE: u32 = u32::MAX;

fn find(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        parent[i as usize] = parent[parent[i as usize] as usize];
        i = parent[i as usize];
    }
    i
}

fn union(parent: &mut [u32], a: u32, b: u32) {
    let (a, b) = (find(parent, a), find(parent, b));
    parent[a.max(b) as usize] = a.min(b);
}

fn portal_expand(lo: [f32; 3], hi: [f32; 3]) -> f32 {
    let extent = (0..3).map(|a| hi[a] - lo[a]).fold(0.0f32, f32::max) / 64.0;
    let mut expand = 16.0f32;
    while expand * 2.0 <= extent && expand < 256.0 {
        expand *= 2.0;
    }
    expand
}

fn merge_rect(rects: &mut BTreeMap<PortalKey, PortalQuad>, key: PortalKey, (plane, lo, hi): PortalQuad) {
    let quad = rects.entry(key).or_insert((plane, lo, hi));
    quad.0 = if key.2 & 1 == 1 { quad.0.min(plane) } else { quad.0.max(plane) };
    for k in 0..2 {
        quad.1[k] = quad.1[k].min(lo[k]);
        quad.2[k] = quad.2[k].max(hi[k]);
    }
}

fn encode_portal(key: &PortalKey, (plane, lo, hi): PortalQuad, box_lo: [f32; 3], box_hi: [f32; 3], leaf_target: bool) -> Portal {
    let &(_, _, face, target, target_cell) = key;
    let axis = (face >> 1) as usize;
    let rect = |k: usize| {
        let b = (axis + 1 + k) % 3;
        let (mut q0, mut q1) = (quantize(box_lo[b], box_hi[b], lo[k], false), quantize(box_lo[b], box_hi[b], hi[k], true));
        if q1 <= q0 {
            (q0, q1) = if q0 == u16::MAX { (q0 - 1, q0) } else { (q0, q0 + 1) };
        }
        (q0 as u32) << 16 | q1 as u32
    };
    let link = if target == OUTSIDE {
        face << 29 | 1 << 28 | Portal::NO_TARGET
    } else {
        face << 29 | u32::from(!leaf_target) << 26 | target
    };
    Portal {
        link,
        z: quantize(box_lo[axis], box_hi[axis], plane, face & 1 == 1),
        target_index: if target == OUTSIDE { 0 } else { target_cell as u16 },
        rect_a: rect(0),
        rect_b: rect(1),
    }
}

pub fn add_hierarchy(tome: &mut Tome) -> Result<()> {
    let view_tree = tome.tree.clone();
    let kd = KdView::new(&view_tree)?;
    let n = kd.node_count as usize;
    if tome.tiles.len() != n || (0..n).any(|i| tome.tiles[i].is_some() != kd.is_leaf(i as u32)) {
        return Err(PrevisError::invalid("hierarchy tiles need a tome with exactly one tile per leaf"));
    }
    let (min, max) = (tome.tree_min, tome.tree_max);
    let mut boxes = vec![([0.0f32; 3], [0.0f32; 3]); n];
    kd.walk(min, max, |node, lo, hi, _| boxes[node as usize] = (*lo, *hi))?;
    let mut parent = vec![u32::MAX; n];
    for i in 0..n as u32 {
        if !kd.is_leaf(i) {
            let (a, b) = kd.children(i);
            parent[a as usize] = i;
            parent[b as usize] = i;
        }
    }
    // Leaf and its ancestors, nearest first.
    let chain = |leaf: u32| {
        let mut out = vec![leaf];
        while let Some(&up) = out.last().map(|&x| &parent[x as usize]).filter(|&&p| p != u32::MAX) {
            out.push(up);
        }
        out
    };

    // Leaf cells, numbered globally in node order.
    let mut first_cell = vec![0u32; n + 1];
    for i in 0..n {
        first_cell[i + 1] = first_cell[i] + tome.tiles[i].as_ref().map_or(0, |t| t.num_cells() as u32);
    }
    let total = first_cell[n] as usize;
    let mut cell_leaf = vec![0u32; total];
    let mut cell_center = vec![[0.0f32; 3]; total];
    let mut cell_box = vec![([0.0f32; 3], [0.0f32; 3]); total];
    let mut cell_objs: Vec<Vec<u32>> = vec![Vec::new(); total];
    let mut cell_cluster = vec![0u32; total];
    for (i, tile) in tome.tiles.iter().enumerate() {
        let Some(tile) = tile else { continue };
        for c in 0..tile.num_cells() {
            let g = (first_cell[i] + c as u32) as usize;
            let (lo, hi) = cell_bounds(tile, c as u32).unwrap();
            cell_leaf[g] = i as u32;
            cell_box[g] = (lo, hi);
            cell_center[g] = std::array::from_fn(|a| (lo[a] + hi[a]) * 0.5);
            cell_objects(tome, tile, c as u32, &mut cell_objs[g])
                .ok_or_else(|| PrevisError::invalid("leaf cell object runs are broken"))?;
            cell_cluster[g] = tile.cell_nodes.as_ref().unwrap()[c].cluster_index;
        }
    }
    let global = |node: u32, cell: u32| first_cell[node as usize] + cell;

    let mut leaves_below: Vec<Vec<u32>> = vec![Vec::new(); n];
    for i in 0..n as u32 {
        if kd.is_leaf(i) {
            for node in chain(i) {
                leaves_below[node as usize].push(i);
            }
        }
    }

    // Coarse cell of each leaf cell in every ancestor, nearest first. Parents precede
    // children in level order, so walking nodes backwards fills each chain nearest first.
    let mut up: Vec<Vec<u32>> = vec![Vec::new(); total];
    let mut coarse: Vec<Vec<CoarseCell>> = (0..n).map(|_| Vec::new()).collect();
    for node in (0..n).rev().filter(|&i| !kd.is_leaf(i as u32)) {
        let (lo, hi) = boxes[node];
        let edge = (lod_level(lo, hi, 1.0) / BINS_PER_AXIS).max(MIN_BIN);
        let bin = |p: [f32; 3]| -> [u32; 3] { std::array::from_fn(|a| (((p[a] - lo[a]) / edge).max(0.0)) as u32) };
        let cells: Vec<u32> = leaves_below[node]
            .iter()
            .flat_map(|&leaf| first_cell[leaf as usize]..first_cell[leaf as usize + 1])
            .collect();
        let local: FxHashMap<u32, u32> = cells.iter().enumerate().map(|(k, &g)| (g, k as u32)).collect();
        let mut sets: Vec<u32> = (0..cells.len() as u32).collect();
        for (k, &g) in cells.iter().enumerate() {
            let leaf = cell_leaf[g as usize];
            let tile = tome.tiles[leaf as usize].as_ref().unwrap();
            let cell = &tile.cell_nodes.as_ref().unwrap()[(g - first_cell[leaf as usize]) as usize];
            for p in &tile.portals.as_deref().unwrap_or(&[])[cell.portal_index as usize..(cell.portal_index + cell.portal_count) as usize] {
                if p.is_outside() || p.is_hierarchy() {
                    continue;
                }
                let Some(&other) = local.get(&global(p.target(), p.target_index as u32)) else { continue };
                // A door joins its cells whatever their bins: coarse cells carry no gate portals.
                if p.is_user() || bin(cell_center[g as usize]) == bin(cell_center[cells[other as usize] as usize]) {
                    union(&mut sets, k as u32, other);
                }
            }
        }
        let mut index = vec![u32::MAX; cells.len()];
        let list = &mut coarse[node];
        for (k, &g) in cells.iter().enumerate() {
            let root = find(&mut sets, k as u32) as usize;
            if index[root] == u32::MAX {
                index[root] = list.len() as u32;
                list.push(CoarseCell { lo: [f32::INFINITY; 3], hi: [f32::NEG_INFINITY; 3], ..CoarseCell::default() });
            }
            let c = &mut list[index[root] as usize];
            let (clo, chi) = cell_box[g as usize];
            for a in 0..3 {
                c.lo[a] = c.lo[a].min(clo[a]).max(lo[a]);
                c.hi[a] = c.hi[a].max(chi[a]).min(hi[a]);
            }
            c.objects.extend_from_slice(&cell_objs[g as usize]);
            c.clusters.push(cell_cluster[g as usize]);
            up[g as usize].push(index[root]);
        }
        for c in list.iter_mut() {
            c.objects.sort_unstable();
            c.objects.dedup();
            c.clusters.sort_unstable();
            c.clusters.dedup();
        }
    }
    if coarse.iter().enumerate().any(|(i, c)| c.len() > u16::MAX as usize && !kd.is_leaf(i as u32)) {
        return Err(PrevisError::invalid("coarse tile has more than 65535 cells"));
    }
    // Cell of leaf cell `g` in chain element `level` (0 = its own leaf tile).
    let cell_at = |g: u32, level: usize| {
        if level == 0 { g - first_cell[cell_leaf[g as usize] as usize] } else { up[g as usize][level - 1] }
    };

    // New portals: into and out of coarse tiles, keyed by source.
    let mut rects: BTreeMap<PortalKey, PortalQuad> = BTreeMap::new();
    for (leaf_a, tile) in tome.tiles.iter().enumerate() {
        let Some(tile) = tile else { continue };
        let chain_a = chain(leaf_a as u32);
        for (c, cell) in tile.cell_nodes.iter().flatten().enumerate() {
            let a = global(leaf_a as u32, c as u32);
            for p in &tile.portals.as_deref().unwrap_or(&[])[cell.portal_index as usize..(cell.portal_index + cell.portal_count) as usize] {
                if p.is_user() || p.is_hierarchy() {
                    continue;
                }
                let quad = portal_quad(&tile.tree_min, &tile.tree_max, p);
                let face = p.face();
                let axis = (face >> 1) as usize;
                if p.is_outside() {
                    for (level, &s) in chain_a.iter().enumerate().skip(1) {
                        let (slo, shi) = boxes[s as usize];
                        let on_boundary = if face & 1 == 1 { shi[axis] == max[axis] } else { slo[axis] == min[axis] };
                        if on_boundary {
                            merge_rect(&mut rects, (s, cell_at(a, level), face, OUTSIDE, 0), quad);
                        }
                    }
                    continue;
                }
                let b = global(p.target(), p.target_index as u32);
                let chain_b = chain(p.target());
                // Chains share their tail from the lowest common ancestor up.
                let common = chain_a.iter().rev().zip(chain_b.iter().rev()).take_while(|(x, y)| x == y).count();
                let (ka, kb) = (chain_a.len() - common, chain_b.len() - common);
                for level in ka.max(1)..chain_a.len() {
                    let (ca, cb) = (cell_at(a, level), up[b as usize][level - ka + kb - 1]);
                    if ca != cb {
                        merge_rect(&mut rects, (chain_a[level], ca, face, chain_a[level], cb), quad);
                    }
                }
                for (i, &s) in chain_a[..ka].iter().enumerate() {
                    for (j, &t) in chain_b[..kb].iter().enumerate() {
                        if i + j > 0 {
                            merge_rect(&mut rects, (s, cell_at(a, i), face, t, cell_at(b, j)), quad);
                        }
                    }
                }
            }
        }
    }
    let mut outgoing: FxHashMap<(u32, u32), Vec<Portal>> = FxHashMap::default();
    for (key, quad) in &rects {
        let (slo, shi) = boxes[key.0 as usize];
        let leaf_target = key.3 != OUTSIDE && kd.is_leaf(key.3);
        outgoing.entry((key.0, key.1)).or_default().push(encode_portal(key, *quad, slo, shi, leaf_target));
    }

    // Object and cluster runs: coarse cells append after the leaf cells' entries.
    let (ew, cw) = (tome.list_widths & 31, tome.list_widths >> 5 & 31);
    let entries = tome.object_lists.as_ref().map_or_else(Vec::new, |list| (0..list.count).map(|i| list_run(list, i, ew, cw)).collect());
    let mut object_runs = ObjectRuns::preloaded(entries);
    let mut cluster_runs = ObjectRuns::default();

    let lods = tome.tile_lod_levels.get_or_insert_with(|| vec![(-1.0f32).to_bits(); n]);
    for node in 0..n {
        let Some(tile) = tome.tiles[node].as_mut() else {
            let (lo, hi) = boxes[node];
            let mut cell_nodes = Vec::with_capacity(coarse[node].len());
            let mut portals = Vec::new();
            for (k, c) in coarse[node].iter().enumerate() {
                let (object_index, object_count) = object_runs.add(&c.objects);
                let (cluster_index, cluster_count) = if c.clusters.len() == 1 { (c.clusters[0], 0) } else { cluster_runs.add(&c.clusters) };
                let own = outgoing.remove(&(node as u32, k as u32)).unwrap_or_default();
                cell_nodes.push(CellNode {
                    portal_index: portals.len() as u32,
                    portal_count: own.len() as u32,
                    object_index,
                    object_count,
                    cluster_index,
                    cluster_count,
                    bounds: pack_box(lo, hi, c.lo, c.hi),
                });
                portals.extend(own);
            }
            lods[node] = lod_level(lo, hi, 512.0).to_bits();
            tome.tiles[node] = Some(Tile {
                tree_min: lo,
                tree_max: hi,
                tree: Tree::default(),
                flags: 0,
                portal_expand: portal_expand(lo, hi),
                num_clusters: 0,
                cell_nodes: (!cell_nodes.is_empty()).then_some(cell_nodes),
                portals: (!portals.is_empty()).then_some(portals),
                ..Tile::default()
            });
            continue;
        };
        let old = tile.portals.take().unwrap_or_default();
        let mut portals = Vec::with_capacity(old.len());
        for (k, cell) in tile.cell_nodes.iter_mut().flatten().enumerate() {
            let start = portals.len() as u32;
            portals.extend_from_slice(&old[cell.portal_index as usize..(cell.portal_index + cell.portal_count) as usize]);
            portals.extend(outgoing.remove(&(node as u32, k as u32)).unwrap_or_default());
            cell.portal_index = start;
            cell.portal_count = portals.len() as u32 - start;
        }
        tile.portals = (!portals.is_empty()).then_some(portals);
    }
    let (object_lists, object_widths) = object_runs.finish(tome.num_objects);
    let (cluster_lists, cluster_widths) = cluster_runs.finish(tome.num_clusters() as u32);
    tome.object_lists = object_lists;
    tome.cluster_lists = cluster_lists;
    tome.list_widths = object_widths | cluster_widths << 10;

    let mut starts = vec![0u32];
    for tile in &tome.tiles {
        starts.push(starts.last().unwrap() + tile.as_ref().map_or(0, |t| t.num_cells() as u32));
    }
    tome.cell_starts = Some(starts);

    // Border matching: each boundary leaf cell's coarse cell in every ancestor.
    let leaves: Vec<u32> = (0..n as u32).filter(|&i| kd.is_leaf(i)).collect();
    for (&leaf, m) in leaves.iter().zip(tome.leaf_matches.iter_mut()) {
        if m.packed & 7 == 0 {
            continue;
        }
        let depth = chain(leaf).len() - 1;
        let cells = first_cell[leaf as usize]..first_cell[leaf as usize + 1];
        let widest = cells.clone().flat_map(|g| up[g as usize].iter().copied()).max().unwrap_or(0);
        m.bits_b = depth as u32;
        m.bits_a = bit_width(widest) * u32::from(widest > 0);
        m.cell_map = (m.bits_a > 0 && !cells.is_empty()).then(|| {
            let mut words = vec![0u32; (cells.len() * depth * m.bits_a as usize).div_ceil(32)];
            for (c, g) in cells.enumerate() {
                for (level, &v) in up[g as usize].iter().enumerate() {
                    write_bits(&mut words, (c * depth + level) * m.bits_a as usize, m.bits_a, v);
                }
            }
            words
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::umbra_build::{Object, build_conservative};
    use crate::umbra_query::visible_objects;
    use crate::umbra_tome::{parse, write};
    use crate::umbra_validate::validate;

    #[test]
    fn hierarchy_tiles_validate_and_keep_leaf_visibility() {
        let objects = [
            Object { user_id: 0xFD00_0000, min: [10.0, 10.0, 0.0], max: [100.0, 50.0, 30.0] },
            Object { user_id: 0xFD00_0001, min: [900.0, 1400.0, 0.0], max: [1000.0, 1500.0, 200.0] },
            Object { user_id: 0x0100_0801, min: [-128.0, 0.0, 0.0], max: [0.0, 128.0, 64.0] },
        ];
        let mut tome = build_conservative([-128.0, -256.0, -128.0], [1152.0, 1536.0, 640.0], &objects).unwrap();
        let points = [[0.0, 0.0, 0.0], [1100.0, 1500.0, 600.0], [500.0, -200.0, 300.0]];
        let before: Vec<_> = points.iter().map(|&p| visible_objects(&tome, p).unwrap()).collect();
        add_hierarchy(&mut tome).unwrap();
        validate(&tome).unwrap();
        let bytes = write(&tome);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(write(&parsed), bytes);
        validate(&parsed).unwrap();
        let kd = KdView::new(&parsed.tree).unwrap();
        for (i, tile) in parsed.tiles.iter().enumerate() {
            let tile = tile.as_ref().unwrap();
            assert_eq!(tile.is_leaf(), kd.is_leaf(i as u32));
            assert!(f32::from_bits(parsed.tile_lod_levels.as_ref().unwrap()[i]) > 0.0);
        }
        let after: Vec<_> = points.iter().map(|&p| visible_objects(&parsed, p).unwrap()).collect();
        assert_eq!(after, before);
    }
}
