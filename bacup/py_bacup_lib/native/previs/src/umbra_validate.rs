//! Structural validation of a tome: the runtime's load checks plus every
//! index, range, count and derived-table invariant proven on the retail
//! corpus (see `UMBRA_TOME_FORMAT.md`, "Invariants").

use crate::error::{PrevisError, Result};
use crate::umbra_query::{
    CellLeaf, KdView, boundary_faces, cell_bounds, cell_leaf, cluster_widths, dequantize,
    encode_path, expand_runs, list_run, lod_level, node_words, object_widths, portal_quad,
    rank_lut,
};
use crate::umbra_tome::{
    BitList, CellNode, PackedAabb, Portal, Tile, Tome, Tree, tree_data_words, tree_map_words,
};

fn fail(message: impl Into<String>) -> PrevisError {
    PrevisError::invalid(message.into())
}

fn ensure(condition: bool, message: impl FnOnce() -> String) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(fail(message()))
    }
}

fn check_tree(tree: &Tree, what: &str) -> Result<()> {
    let n = tree.node_count;
    let Some(data) = &tree.data else {
        ensure(
            n == 0 && tree.map.is_none() && tree.splits.is_none(),
            || format!("{what}: streams without nodes"),
        )?;
        return Ok(());
    };
    ensure(n > 0, || format!("{what}: node stream with zero nodes"))?;
    ensure(data.len() == tree_data_words(n), || {
        format!("{what}: node stream length")
    })?;
    let words = node_words(n);
    let used_bits = 2 * n as usize;
    if !used_bits.is_multiple_of(32) {
        ensure(data[words - 1] >> (used_bits % 32) == 0, || {
            format!("{what}: node stream padding bits set")
        })?;
    }
    ensure(data[words..] == rank_lut(n, &data[..words])[..], || {
        format!("{what}: rank table does not match the node stream")
    })?;
    if let Some(map) = &tree.map {
        ensure(tree.map_width > 0, || format!("{what}: map without width"))?;
        ensure(map.len() == tree_map_words(n, tree.map_width), || {
            format!("{what}: map length")
        })?;
    }
    match &tree.splits {
        Some(splits) => ensure(
            splits.len() == tree.split_count as usize && tree.split_count <= n,
            || format!("{what}: split count"),
        )?,
        None => ensure(tree.split_count == 0, || {
            format!("{what}: split count without splits")
        })?,
    }
    let view = KdView::new(tree)?;
    ensure(view.leaf_count() == n.div_ceil(2) || n == 0, || {
        format!("{what}: not a full binary tree")
    })?;
    Ok(())
}

fn check_packed(bounds: &PackedAabb, what: &str) -> Result<()> {
    let (lo, hi) = (bounds.min(), bounds.max());
    ensure((0..3).all(|a| lo[a] <= hi[a]), || {
        format!("{what}: packed bounds inverted")
    })
}

fn check_runs(list: Option<&BitList>, widths: (u32, u32), limit: u32, what: &str) -> Result<u32> {
    let Some(list) = list else { return Ok(0) };
    for i in 0..list.count {
        let (first, count) = list_run(list, i, widths.0, widths.1);
        ensure(
            count >= 1 && first as u64 + count as u64 <= limit as u64,
            || format!("{what}: run {i} ({first}+{count}) exceeds {limit}"),
        )?;
    }
    Ok(list.count)
}

fn portal_kind_ok(portal: &Portal) -> Result<()> {
    if portal.is_outside() {
        ensure(portal.target() == Portal::NO_TARGET, || {
            "outside portal with a target".into()
        })?;
    }
    ensure(portal.face() < 6, || {
        format!("portal face {}", portal.face())
    })
}

struct TileCtx<'a> {
    index: usize,
    tile: &'a Tile,
}

pub fn validate(tome: &Tome) -> Result<()> {
    ensure(
        tome.lod_base_distance.is_finite() && tome.lod_base_distance > 0.0,
        || "LOD base distance".into(),
    )?;
    ensure((0..3).all(|a| tome.tree_min[a] < tome.tree_max[a]), || {
        "tome bounds are empty".into()
    })?;
    ensure(tome.build_info.len() <= 128, || {
        "banner longer than 128 bytes".into()
    })?;

    // Top-level tile tree: one node per tile, node boxes are tile boxes.
    let tiles = &tome.tiles;
    ensure(
        !tiles.is_empty() && tome.tree.node_count as usize == tiles.len(),
        || "top tree node count != tile count".into(),
    )?;
    ensure(tome.tree.map.is_none(), || "top tree has a leaf map".into())?;
    check_tree(&tome.tree, "top tree")?;
    let top = KdView::new(&tome.tree)?;
    let paths = tome.tile_paths.as_deref().unwrap_or(&[]);
    let lods = tome.tile_lod_levels.as_deref().unwrap_or(&[]);
    ensure(lods.len() == tiles.len(), || "LOD level count".into())?;
    let bits = tome.bits_per_slot_path;
    ensure((1..=32).contains(&bits), || "bits per slot path".into())?;
    let mut max_depth = 0usize;
    let mut problem = None;
    top.walk(tome.tree_min, tome.tree_max, |node, lo, hi, path| {
        if problem.is_some() {
            return;
        }
        max_depth = max_depth.max(path.len());
        let i = node as usize;
        let field = crate::umbra_query::read_bits(paths, i * bits as usize, bits);
        if path.len() >= bits as usize || field != encode_path(bits, path) {
            problem = Some(format!("tile {i}: slot path"));
            return;
        }
        match &tiles[i] {
            None => {
                if top.is_leaf(node) {
                    problem = Some(format!("tile {i}: empty leaf slot"));
                } else if lods[i] != (-1.0f32).to_bits() {
                    problem = Some(format!("tile {i}: empty slot LOD level"));
                }
            }
            Some(tile) => {
                if tile.tree_min != *lo || tile.tree_max != *hi {
                    problem = Some(format!("tile {i}: box differs from its top-tree node"));
                } else if tile.is_leaf() != top.is_leaf(node) {
                    problem = Some(format!("tile {i}: leaf flag differs from the top tree"));
                } else if lods[i] != lod_level(*lo, *hi, tome.lod_base_distance).to_bits() {
                    problem = Some(format!("tile {i}: LOD level"));
                }
            }
        }
    })?;
    if let Some(problem) = problem {
        return Err(fail(problem));
    }
    ensure(bits as usize == max_depth + 1, || {
        format!("bits per slot path {bits} for depth {max_depth}")
    })?;
    ensure(
        paths.len() == (bits as usize * tiles.len()).div_ceil(32),
        || "slot path length".into(),
    )?;

    // Cell starts are prefix sums of per-tile cell counts.
    let starts = tome
        .cell_starts
        .as_deref()
        .ok_or_else(|| fail("missing cell starts"))?;
    ensure(starts.len() == tiles.len() + 1 && starts[0] == 0, || {
        "cell start count".into()
    })?;
    for (i, tile) in tiles.iter().enumerate() {
        let cells = tile.as_ref().map_or(0, Tile::num_cells) as u32;
        ensure(starts[i + 1] == starts[i] + cells, || {
            format!("tile {i}: cell start")
        })?;
    }

    let num_objects = tome.num_objects;
    let num_clusters = tome.num_clusters() as u32;
    ensure(num_clusters >= 1, || "no clusters".into())?;
    ensure(
        tome.object_bounds
            .as_ref()
            .is_none_or(|b| b.len() == num_objects as usize),
        || "object bounds count".into(),
    )?;
    ensure(
        tome.user_ids
            .as_ref()
            .is_none_or(|b| b.len() == num_objects as usize),
        || "user id count".into(),
    )?;
    ensure(
        tome.object_distances
            .as_ref()
            .is_none_or(|b| b.len() == num_objects as usize),
        || "object distance count".into(),
    )?;
    for (i, b) in tome.object_bounds.iter().flatten().enumerate() {
        ensure((0..3).all(|a| b[a] <= b[a + 3]), || {
            format!("object {i}: inverted bounds")
        })?;
    }
    let object_entries = check_runs(
        tome.object_lists.as_ref(),
        object_widths(tome),
        num_objects,
        "object list",
    )?;
    let cluster_entries = check_runs(
        tome.cluster_lists.as_ref(),
        cluster_widths(tome),
        num_clusters,
        "cluster list",
    )?;

    // Gates.
    let num_gates = tome.num_gates;
    ensure(
        tome.gate_ids.as_ref().map_or(0, Vec::len) == num_gates as usize,
        || "gate id count".into(),
    )?;
    for &g in tome.gate_indices.iter().flatten() {
        ensure(g < num_gates, || format!("gate index {g} >= {num_gates}"))?;
    }
    let gate_indices = tome.gate_indices.as_ref().map_or(0, Vec::len) as u32;
    let gate_vertices = tome.gate_vertices.as_ref().map_or(0, Vec::len) as u32;
    let check_user = |p: &Portal| -> Result<()> {
        if p.is_user() {
            ensure(
                p.user_object_offset() + p.user_object_count() <= gate_indices,
                || "gate index range".into(),
            )?;
            ensure(
                p.gate_vertex_offset() + p.gate_vertex_count() <= gate_vertices,
                || "gate vertex range".into(),
            )?;
        }
        Ok(())
    };

    // Clusters and cluster portals.
    let cluster_portals = tome.cluster_portals.as_deref().unwrap_or(&[]);
    let mut next_portal = 0;
    for (i, node) in tome.cluster_nodes.iter().flatten().enumerate() {
        ensure(node.portal_index == next_portal, || {
            format!("cluster {i}: portal ranges are not contiguous")
        })?;
        next_portal += node.portal_count;
        check_packed(&node.bounds, "cluster")?;
    }
    ensure(next_portal as usize == cluster_portals.len(), || {
        "cluster portal count".into()
    })?;
    for (k, node) in tome.cluster_nodes.iter().flatten().enumerate() {
        for p in &cluster_portals
            [node.portal_index as usize..(node.portal_index + node.portal_count) as usize]
        {
            portal_kind_ok(p)?;
            check_user(p)?;
            ensure(!p.is_outside() && !p.is_hierarchy(), || {
                format!("cluster {k}: outside or hierarchy cluster portal")
            })?;
            ensure(p.target() < num_clusters && p.target() != k as u32, || {
                format!("cluster {k}: cluster portal target {}", p.target())
            })?;
            ensure(p.target_index == u16::MAX, || {
                format!("cluster {k}: cluster portal target index")
            })?;
        }
    }

    // Tiles.
    let leaf_tiles: Vec<TileCtx> = tiles
        .iter()
        .enumerate()
        .filter_map(|(index, t)| t.as_ref().map(|tile| TileCtx { index, tile }))
        .collect();
    for TileCtx { index, tile } in &leaf_tiles {
        let what = format!("tile {index}");
        let cells = tile.cell_nodes.as_deref().unwrap_or(&[]);
        let portals = tile.portals.as_deref().unwrap_or(&[]);
        ensure(tile.flags & !1 == 0, || format!("{what}: unknown flags"))?;
        ensure(tile.portal_expand >= 0.0, || {
            format!("{what}: portal expand")
        })?;
        check_tree(&tile.tree, &what)?;
        if tile.tree.data.is_some() {
            ensure(tile.tree.map.is_some(), || {
                format!("{what}: cell tree without a map")
            })?;
        } else if tile.is_leaf() {
            ensure(cells.len() <= 1, || {
                format!("{what}: several cells but no cell tree")
            })?;
        }
        let bsp = tile.bsp_triangles.as_deref().unwrap_or(&[]);
        ensure(bsp.len() == tile.num_bsp_nodes as usize, || {
            format!("{what}: BSP count")
        })?;
        let planes = tile.planes.as_ref().map_or(0, Vec::len) as u32;
        if tile.tree.data.is_some() {
            let view = KdView::new(&tile.tree)?;
            for leaf in 0..view.leaf_count() {
                match cell_leaf(&tile.tree, leaf) {
                    CellLeaf::Cell(c) => ensure((c as usize) < cells.len(), || {
                        format!("{what}: cell tree leaf -> cell {c}")
                    })?,
                    CellLeaf::Bsp(root) => ensure(root < tile.num_bsp_nodes, || {
                        format!("{what}: cell tree leaf -> BSP {root}")
                    })?,
                    CellLeaf::Empty => {}
                }
            }
        }
        for (k, &[head, children]) in bsp.iter().enumerate() {
            ensure(head & 0x3FFF_FFFF < planes, || {
                format!("{what}: BSP {k} plane")
            })?;
            for (child, terminal) in [
                (children & 0xFFFF, head >> 30 & 1 != 0),
                (children >> 16, head >> 31 != 0),
            ] {
                let ok = if terminal {
                    child == 0xFFFF || (child as usize) < cells.len()
                } else {
                    child < tile.num_bsp_nodes
                };
                ensure(ok, || format!("{what}: BSP {k} child {child}"))?;
            }
        }
        let mut next = 0;
        let mut scratch = Vec::new();
        for (c, cell) in cells.iter().enumerate() {
            ensure(cell.portal_index == next, || {
                format!("{what} cell {c}: portal ranges are not contiguous")
            })?;
            next += cell.portal_count;
            scratch.clear();
            if cell.object_count > 0 {
                let list = tome
                    .object_lists
                    .as_ref()
                    .ok_or_else(|| fail(format!("{what} cell {c}: objects without a list")))?;
                ensure(cell.object_index < object_entries, || {
                    format!("{what} cell {c}: object entry")
                })?;
                expand_runs(
                    list,
                    object_widths(tome),
                    cell.object_index,
                    cell.object_count,
                    &mut scratch,
                )
                .ok_or_else(|| {
                    fail(format!(
                        "{what} cell {c}: object runs do not sum to the object count"
                    ))
                })?;
            }
            scratch.clear();
            if cell.cluster_count > 0 {
                let list = tome
                    .cluster_lists
                    .as_ref()
                    .ok_or_else(|| fail(format!("{what} cell {c}: clusters without a list")))?;
                ensure(cell.cluster_index < cluster_entries, || {
                    format!("{what} cell {c}: cluster entry")
                })?;
                expand_runs(
                    list,
                    cluster_widths(tome),
                    cell.cluster_index,
                    cell.cluster_count,
                    &mut scratch,
                )
                .ok_or_else(|| {
                    fail(format!(
                        "{what} cell {c}: cluster runs do not sum to the cluster count"
                    ))
                })?;
            }
            if tile.is_leaf() && cell.cluster_count == 0 {
                ensure(cell.cluster_index < num_clusters, || {
                    format!("{what} cell {c}: cluster {}", cell.cluster_index)
                })?;
            }
            check_packed(&cell.bounds, &what)?;
        }
        ensure(next as usize == portals.len(), || {
            format!("{what}: portal count")
        })?;
        for (c, cell) in cells.iter().enumerate() {
            for p in &portals
                [cell.portal_index as usize..(cell.portal_index + cell.portal_count) as usize]
            {
                portal_kind_ok(p)?;
                check_user(p)?;
                if p.is_outside() {
                    continue;
                }
                let target = tiles
                    .get(p.target() as usize)
                    .and_then(Option::as_ref)
                    .ok_or_else(|| {
                        fail(format!(
                            "{what} cell {c}: portal to missing tile {}",
                            p.target()
                        ))
                    })?;
                ensure((p.target_index as usize) < target.num_cells(), || {
                    format!("{what} cell {c}: portal to missing cell")
                })?;
                if p.is_hierarchy() {
                    ensure(!target.is_leaf(), || {
                        format!("{what} cell {c}: hierarchy portal into a leaf tile")
                    })?;
                    continue;
                }
                if !p.is_user() {
                    ensure(target.is_leaf(), || {
                        format!("{what} cell {c}: regular portal into an inner tile")
                    })?;
                }
                if tile.is_leaf() && !p.is_user() {
                    let back = &target.cell_nodes.as_ref().unwrap()[p.target_index as usize];
                    let back_portals = &target.portals.as_ref().unwrap()[back.portal_index as usize
                        ..(back.portal_index + back.portal_count) as usize];
                    let reciprocal = back_portals.iter().any(|q| {
                        !q.is_outside()
                            && q.target() as usize == *index
                            && q.target_index as usize == c
                            && q.face() == p.face() ^ 1
                    });
                    ensure(reciprocal, || {
                        format!("{what} cell {c}: portal has no reciprocal")
                    })?;
                }
            }
        }
    }

    check_clusters(tome)?;
    check_border_matching(tome)
}

fn cell_portal_slice<'a>(tile: &'a Tile, cell: &CellNode) -> &'a [Portal] {
    &tile.portals.as_deref().unwrap_or(&[])
        [cell.portal_index as usize..(cell.portal_index + cell.portal_count) as usize]
}

/// Two quantization steps of `[min, max]` on each axis.
fn slack(min: &[f32; 3], max: &[f32; 3]) -> [f32; 3] {
    std::array::from_fn(|a| (max[a] - min[a]) / 65535.0 * 2.0 + 0.02)
}

/// Retail cluster rules [C 1422/1422]: every cluster lives in one leaf tile,
/// clusters are numbered contiguously in tile order, each leaf cell has one
/// home cluster, inner tiles own none, a cluster box holds its cells, and two
/// clusters are linked, with the same faces, exactly where a regular or gate
/// cell portal crosses from one to the other.
fn check_clusters(tome: &Tome) -> Result<()> {
    let num_clusters = tome.num_clusters() as u32;
    let mut home: Vec<Vec<u32>> = vec![Vec::new(); tome.tiles.len()];
    let mut next = 0u32;
    for (i, tile) in tome.tiles.iter().enumerate() {
        let Some(tile) = tile else { continue };
        if !tile.is_leaf() {
            ensure(tile.num_clusters == 0, || {
                format!("tile {i}: inner tile clusters")
            })?;
            continue;
        }
        let own = next..next + tile.num_clusters as u32;
        let mut distinct = std::collections::BTreeSet::new();
        for (c, cell) in tile.cell_nodes.iter().flatten().enumerate() {
            ensure(
                cell.cluster_count == 0 && own.contains(&cell.cluster_index),
                || format!("tile {i} cell {c}: home cluster outside the tile's range"),
            )?;
            distinct.insert(cell.cluster_index);
            home[i].push(cell.cluster_index);
        }
        ensure(distinct.len() == tile.num_clusters as usize, || {
            format!("tile {i}: cluster count")
        })?;
        next = own.end;
    }
    if next == 0 {
        return ensure(num_clusters == 1, || {
            "a tome without cells has one cluster".into()
        });
    }
    ensure(next == num_clusters, || {
        format!("{num_clusters} clusters but leaf tiles own {next}")
    })?;

    let nodes = tome.cluster_nodes.as_deref().unwrap_or(&[]);
    let (wmin, wmax) = (&tome.tree_min, &tome.tree_max);
    let wslack = slack(wmin, wmax);
    type Links = std::collections::HashMap<(u32, u32), std::collections::BTreeSet<u32>>;
    let (mut cell_links, mut planes) = (Links::new(), std::collections::HashMap::new());
    for (i, tile) in tome.tiles.iter().enumerate() {
        let Some(tile) = tile.as_ref().filter(|t| t.is_leaf()) else {
            continue;
        };
        for (c, cell) in tile.cell_nodes.iter().flatten().enumerate() {
            let from = home[i][c];
            let (lo, hi) = cell_bounds(tile, c as u32).unwrap();
            let node = &nodes[from as usize].bounds;
            let (blo, bhi) = (node.min(), node.max());
            ensure(
                (0..3).all(|a| {
                    dequantize(wmin[a], wmax[a], blo[a] as u32) <= lo[a] + wslack[a]
                        && dequantize(wmin[a], wmax[a], bhi[a] as u32) >= hi[a] - wslack[a]
                }),
                || format!("tile {i} cell {c}: outside its cluster's box"),
            )?;
            for p in cell_portal_slice(tile, cell) {
                if p.is_outside() || p.is_hierarchy() {
                    continue;
                }
                let to = home[p.target() as usize][p.target_index as usize];
                if to == from {
                    continue;
                }
                cell_links
                    .entry((from, to))
                    .or_default()
                    .insert(p.face() | u32::from(p.is_user()) << 3);
                if !p.is_user() {
                    planes
                        .entry((from, to, p.face()))
                        .or_insert_with(Vec::new)
                        .push(portal_quad(&tile.tree_min, &tile.tree_max, p).0);
                }
            }
        }
    }
    let cluster_portals = tome.cluster_portals.as_deref().unwrap_or(&[]);
    let mut cluster_links = Links::new();
    for (k, node) in nodes.iter().enumerate() {
        for p in &cluster_portals
            [node.portal_index as usize..(node.portal_index + node.portal_count) as usize]
        {
            let from = k as u32;
            cluster_links
                .entry((from, p.target()))
                .or_default()
                .insert(p.face() | u32::from(p.is_user()) << 3);
            if !p.is_user() {
                let (plane, _, _) = portal_quad(wmin, wmax, p);
                let axis = (p.face() >> 1) as usize;
                let matched = planes
                    .get(&(from, p.target(), p.face()))
                    .is_some_and(|v| v.iter().any(|&z| (z - plane).abs() <= wslack[axis]));
                ensure(matched, || {
                    format!("cluster {k}: cluster portal plane has no cell portal")
                })?;
            }
        }
    }
    ensure(cluster_links == cell_links, || {
        "cluster portals differ from the cluster crossings of the cell portals".into()
    })
}

/// Border-matching rules [C 1422/1422] (UMBRA_TOME_FORMAT.md, "Border matching").
fn check_border_matching(tome: &Tome) -> Result<()> {
    let tiles = &tome.tiles;
    let leaf_count = tiles.iter().flatten().filter(|t| t.is_leaf()).count();
    ensure(tome.leaf_matches.len() == leaf_count, || {
        "leaf matching entry count".into()
    })?;
    let top = KdView::new(&tome.tree)?;
    let mut parent = vec![None; tiles.len()];
    for n in 0..tome.tree.node_count {
        if !top.is_leaf(n) {
            let (low, high) = top.children(n);
            parent[low as usize] = Some(n as usize);
            parent[high as usize] = Some(n as usize);
        }
    }
    let (min, max) = (&tome.tree_min, &tome.tree_max);
    let leaves = tiles
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.as_ref().filter(|t| t.is_leaf()).map(|t| (i, t)));
    let mut next_tree = 0usize;
    for ((i, tile), m) in leaves.zip(&tome.leaf_matches) {
        let what = format!("leaf match for tile {i}");
        let boundary = boundary_faces(min, max, tile);
        let n = m.packed & 7;
        ensure(n == boundary.count_ones(), || {
            format!(
                "{what}: {n} trees for {} boundary faces",
                boundary.count_ones()
            )
        })?;
        if n == 0 {
            ensure(
                m.packed == 0 && m.bits_a == 0 && m.bits_b == 0 && m.cell_map.is_none(),
                || format!("{what}: interior leaf with matching data"),
            )?;
            continue;
        }
        ensure(m.first_tree() as usize == next_tree, || {
            format!("{what}: first tree")
        })?;
        next_tree += n as usize;
        ensure(next_tree <= tome.matching_trees.len(), || {
            format!("{what}: trees past the end")
        })?;

        // Cell map: each cell's containing cell in every stored ancestor tile, nearest first.
        let mut ancestors = Vec::new();
        let mut node = i;
        while let Some(up) = parent[node] {
            node = up;
            ancestors.extend(tiles[up].as_ref());
        }
        ensure(m.bits_b as usize == ancestors.len(), || {
            format!(
                "{what}: bits_b {} for {} ancestor tiles",
                m.bits_b,
                ancestors.len()
            )
        })?;
        let cells = tile.num_cells();
        ensure(m.cell_map.is_some() == (m.bits_a > 0 && cells > 0), || {
            format!("{what}: cell map presence")
        })?;
        let mut widest = 0;
        if let Some(map) = &m.cell_map {
            for c in 0..cells {
                let (lo, hi) = cell_bounds(tile, c as u32).unwrap();
                for (level, ancestor) in ancestors.iter().enumerate() {
                    let bit = (c * ancestors.len() + level) * m.bits_a as usize;
                    let v = crate::umbra_query::read_bits(map, bit, m.bits_a);
                    widest = widest.max(v);
                    let (alo, ahi) = cell_bounds(ancestor, v).ok_or_else(|| {
                        fail(format!(
                            "{what}: cell {c} maps to missing ancestor cell {v}"
                        ))
                    })?;
                    let s = slack(&ancestor.tree_min, &ancestor.tree_max);
                    ensure(
                        (0..3).all(|a| alo[a] <= lo[a] + s[a] && ahi[a] >= hi[a] - s[a]),
                        || format!("{what}: cell {c} is not inside ancestor cell {v}"),
                    )?;
                }
            }
        }
        ensure(m.bits_a == 32 - widest.leading_zeros(), || {
            format!("{what}: bits_a {} for widest value {widest}", m.bits_a)
        })?;

        // Matching trees: one per boundary face in face order, over the tile box.
        let faces = (0..6).filter(|f| boundary >> f & 1 != 0);
        for (k, face) in faces.enumerate() {
            let tree = &tome.matching_trees[m.first_tree() as usize + k];
            check_face_tree(tile, face, tree)
                .map_err(|e| fail(format!("{what} face {face}: {e}")))?;
        }
    }
    ensure(next_tree == tome.matching_trees.len(), || {
        "matching trees not owned by a leaf".into()
    })?;
    for (i, tree) in tome.matching_trees.iter().enumerate() {
        check_tree(tree, &format!("matching tree {i}"))?;
    }
    for (i, tile) in tiles.iter().enumerate() {
        let Some(tile) = tile else { continue };
        let boundary = boundary_faces(min, max, tile);
        for p in tile.portals.iter().flatten().filter(|p| p.is_outside()) {
            ensure(boundary >> p.face() & 1 != 0, || {
                format!("tile {i}: outside portal on interior face {}", p.face())
            })?;
        }
    }
    Ok(())
}

/// A matching tree splits only on the face's two in-plane axes, strictly
/// inside each node's box; each leaf names a cell whose outside portal on the
/// face overlaps it, or is all ones for none, with the map just wide enough;
/// every outside portal on the face overlaps a leaf naming its cell; and a
/// face without outside portals has an empty tree.
fn check_face_tree(tile: &Tile, face: u32, tree: &Tree) -> Result<()> {
    let axis = (face >> 1) as usize;
    let cells = tile.cell_nodes.as_deref().unwrap_or(&[]);
    let mut quads = Vec::new();
    for (c, cell) in cells.iter().enumerate() {
        for p in cell_portal_slice(tile, cell) {
            if p.is_outside() && p.face() == face {
                let (_, lo, hi) = portal_quad(&tile.tree_min, &tile.tree_max, p);
                quads.push((c as u32, lo, hi));
            }
        }
    }
    if tree.data.is_none() {
        return ensure(quads.is_empty(), || {
            "empty matching tree for a face with outside portals".into()
        });
    }
    ensure(!quads.is_empty(), || {
        "matching tree for a face without outside portals".into()
    })?;
    let view = KdView::new(tree)?;
    let width = tree.map_width;
    let empty = (1u64 << width) as u32 - 1;
    let plane = [(axis + 1) % 3, (axis + 2) % 3];
    let overlaps = |q: &([f32; 2], [f32; 2]), lo: &[f32; 3], hi: &[f32; 3]| {
        (0..2).all(|k| q.0[k] < hi[plane[k]] && q.1[k] > lo[plane[k]])
    };
    let mut leaves = Vec::new();
    let mut problem = None;
    view.walk(tile.tree_min, tile.tree_max, |node, lo, hi, _| {
        if problem.is_some() {
            return;
        }
        if !view.is_leaf(node) {
            let a = view.axis(node) as usize;
            let s = view.split_value(node, lo, hi);
            if a == axis || !(s > lo[a] && s < hi[a]) {
                problem = Some(format!("node {node} splits axis {a} at {s}"));
            }
            return;
        }
        let v = crate::umbra_query::read_bits(
            tree.map.as_deref().unwrap_or(&[]),
            (view.leaf_index(node) * width) as usize,
            width,
        );
        if v == empty {
            return;
        }
        let named = quads
            .iter()
            .any(|(c, qlo, qhi)| *c == v && overlaps(&(*qlo, *qhi), lo, hi));
        if !named {
            problem = Some(format!(
                "leaf {node} names cell {v} without an outside portal there"
            ));
        }
        leaves.push((v, *lo, *hi));
    })?;
    if let Some(problem) = problem {
        return Err(fail(problem));
    }
    let top = leaves.iter().map(|l| l.0).max().unwrap_or(0);
    ensure(width == 32 - (top + 1).leading_zeros(), || {
        format!("map width {width} for largest cell {top}")
    })?;
    for (c, qlo, qhi) in &quads {
        ensure(
            leaves
                .iter()
                .any(|(v, lo, hi)| v == c && overlaps(&(*qlo, *qhi), lo, hi)),
            || format!("outside portal of cell {c} is not named by a leaf"),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::umbra_query::{
        CellHit, CellLeaf, KdView, cell_bounds, cell_leaf, find_cell, visible_objects,
    };
    use crate::umbra_tome::parse;

    fn corpus() -> Vec<std::path::PathBuf> {
        let root = std::env::var_os("UMBRA_TOME_CORPUS").expect("UMBRA_TOME_CORPUS is not set");
        let mut files = Vec::new();
        let mut stack = vec![std::path::PathBuf::from(root)];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("uvd"))
                {
                    files.push(path);
                }
            }
        }
        files.sort();
        files
    }

    #[test]
    #[ignore = "set UMBRA_TOME_CORPUS to a directory of retail .uvd files"]
    fn retail_uvds_validate() {
        let files = corpus();
        let mut failures = std::collections::BTreeMap::<String, Vec<String>>::new();
        for path in &files {
            let tome = parse(&std::fs::read(path).unwrap()).unwrap();
            if let Err(e) = validate(&tome) {
                let key = e
                    .to_string()
                    .split(':')
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(":");
                let key: String = key.chars().filter(|c| !c.is_ascii_digit()).collect();
                failures
                    .entry(key)
                    .or_default()
                    .push(format!("{} ({e})", path.display()));
            }
        }
        let failed: usize = failures.values().map(Vec::len).sum();
        println!(
            "umbra tome validate: {}/{} valid",
            files.len() - failed,
            files.len()
        );
        for (k, v) in &failures {
            println!("  {} x {k}: e.g. {}", v.len(), v[0]);
        }
        assert!(!files.is_empty());
        assert_eq!(failed, 0);
    }

    /// Samples a lattice of points in every tome and checks the query.
    #[test]
    #[ignore = "set UMBRA_TOME_CORPUS to a directory of retail .uvd files"]
    fn retail_uvds_query_is_consistent() {
        let files = corpus();
        let steps = 12;
        let (mut cells, mut empty, mut outside_box, mut walks, mut objects_seen, mut bsp_points) =
            (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
        let mut problems = Vec::new();
        for path in &files {
            let tome = parse(&std::fs::read(path).unwrap()).unwrap();
            let mut walked = 0;
            for i in 0..steps {
                for j in 0..steps {
                    for k in 0..steps {
                        let f = |axis: usize, s: usize| {
                            let t = (s as f32 + 0.37) / steps as f32;
                            tome.tree_min[axis] + (tome.tree_max[axis] - tome.tree_min[axis]) * t
                        };
                        let p = [f(0, i), f(1, j), f(2, k)];
                        match find_cell(&tome, p) {
                            Err(e) => problems.push(format!("{}: {e}", path.display())),
                            Ok(CellHit::Outside) => {
                                problems.push(format!("{}: interior point outside", path.display()))
                            }
                            Ok(CellHit::Empty { .. }) => empty += 1,
                            Ok(CellHit::Cell { tile, cell }) => {
                                cells += 1;
                                let t = tome.tiles[tile as usize].as_ref().unwrap();
                                let (lo, hi) = cell_bounds(t, cell).unwrap();
                                let slack: Vec<f32> = (0..3)
                                    .map(|a| (t.tree_max[a] - t.tree_min[a]) / 65535.0 + 1e-2)
                                    .collect();
                                if !(0..3)
                                    .all(|a| p[a] >= lo[a] - slack[a] && p[a] <= hi[a] + slack[a])
                                {
                                    outside_box += 1;
                                }
                                // The kd leaf that resolved the point must overlap the cell's box.
                                let (llo, lhi, via_bsp) = match &t.tree.data {
                                    Some(_) => {
                                        let view = KdView::new(&t.tree).unwrap();
                                        let (node, llo, lhi) =
                                            view.locate(p, t.tree_min, t.tree_max);
                                        let bsp = matches!(
                                            cell_leaf(&t.tree, view.leaf_index(node)),
                                            CellLeaf::Bsp(_)
                                        );
                                        (llo, lhi, bsp)
                                    }
                                    None => (t.tree_min, t.tree_max, false),
                                };
                                bsp_points += via_bsp as u64;
                                if !via_bsp
                                    && !(0..3).all(|a| {
                                        llo[a] <= hi[a] + slack[a] && lhi[a] >= lo[a] - slack[a]
                                    })
                                {
                                    problems.push(format!(
                                        "{}: {p:?} leaf box misses tile {tile} cell {cell}",
                                        path.display()
                                    ));
                                }
                                if walked < 2 {
                                    walked += 1;
                                    walks += 1;
                                    let objects = visible_objects(&tome, p).unwrap().unwrap();
                                    objects_seen += objects.len() as u64;
                                    if objects.iter().any(|&o| o >= tome.num_objects) {
                                        problems.push(format!(
                                            "{}: object out of range",
                                            path.display()
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        println!(
            "umbra query: {} tomes, {cells} points in cells, {empty} in empty space, {outside_box} outside their tight cell box, {bsp_points} resolved by BSP, {walks} walks, {:.1} objects/walk",
            files.len(),
            objects_seen as f64 / walks.max(1) as f64
        );
        for p in problems.iter().take(20) {
            println!("  {p}");
        }
        assert!(problems.is_empty());
    }
}
