//! Conservative tome writer: a 512-unit grid of single-cell leaf tiles joined
//! by full-face portals, every cell listing every object. It never culls an
//! object the runtime's frustum test would keep, so it is a safe baseline for
//! proving the writer before a real visibility solve exists.
//!
//! `build_from_model` encodes the clean-room solve output (`umbra_solve`).

use rustc_hash::FxHashMap;

use crate::error::{PrevisError, Result};
use crate::scene::{INSTANCE_TARGET, Scene};
use crate::umbra_query::{
    KdView, LEAF, boundary_faces, cell_bounds, encode_node_stream, encode_path, lod_level,
    portal_quad, write_bits,
};
use crate::umbra_solve::{self, KdNode, PortalTarget, VisModel};
use crate::umbra_tome::{
    BitList, CellNode, ClusterNode, LeafMatch, PackedAabb, Portal, Tile, Tome, Tree, tree_map_words,
};

pub const GRID: f32 = 512.0;
pub const BANNER: &str = "T 512.0 SO 128.0 SH 16.000 BF 100 F 0 CS 0.0 - 3.3.17 F 1 0 OG 0";
const FULL: PackedAabb = PackedAabb([0, 0, 0xFFFF, 0, 0xFFFF, 0xFFFF]);
const FULL_RECT: u32 = 0x0000_FFFF;
const PORTAL_EXPAND: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Object {
    pub user_id: u32,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// Target instances of a scene with their world-space boxes.
pub fn scene_objects(scene: &Scene) -> Vec<Object> {
    scene
        .instances
        .iter()
        .filter(|i| i.flags & INSTANCE_TARGET != 0)
        .filter_map(|instance| {
            let geometry = scene.geometries.get(instance.model_index as usize)?;
            let t = &instance.transform;
            let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
            for v in &geometry.vertices {
                for axis in 0..3 {
                    let w = t[axis * 4] * v[0]
                        + t[axis * 4 + 1] * v[1]
                        + t[axis * 4 + 2] * v[2]
                        + t[axis * 4 + 3];
                    min[axis] = min[axis].min(w);
                    max[axis] = max[axis].max(w);
                }
            }
            (!geometry.vertices.is_empty()).then_some(Object {
                user_id: instance.object_id,
                min,
                max,
            })
        })
        .collect()
}

pub(crate) fn bit_width(value: u32) -> u32 {
    (32 - value.leading_zeros()).max(1)
}

struct Grid {
    min: [f32; 3],
    max: [f32; 3],
    counts: [u32; 3],
}

impl Grid {
    fn plane(&self, axis: usize, index: u32) -> f32 {
        if index == self.counts[axis] {
            self.max[axis]
        } else {
            self.min[axis] + index as f32 * GRID
        }
    }
}

/// Level-order kd-tree over grid index ranges: each inner node halves its
/// longest range, so node `k` gets children `2k-1`, `2k` exactly as the
/// runtime expects. Returns per-node axes, split values and leaf grid cells.
fn grid_tree(grid: &Grid) -> (Vec<u32>, Vec<f32>, Vec<Option<[u32; 3]>>) {
    let mut axes = Vec::new();
    let mut splits = Vec::new();
    let mut leaves = Vec::new();
    let mut queue = std::collections::VecDeque::from([([0u32; 3], grid.counts)]);
    while let Some((lo, hi)) = queue.pop_front() {
        let spans = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
        let axis = (0..3)
            .max_by_key(|&a| (spans[a], std::cmp::Reverse(a)))
            .unwrap();
        if spans[axis] <= 1 {
            axes.push(LEAF);
            splits.push(0.0);
            leaves.push(Some(lo));
            continue;
        }
        let mid = lo[axis] + spans[axis] / 2;
        axes.push(axis as u32);
        splits.push(grid.plane(axis, mid));
        leaves.push(None);
        let (mut lower_hi, mut upper_lo) = (hi, lo);
        lower_hi[axis] = mid;
        upper_lo[axis] = mid;
        queue.push_back((lo, lower_hi));
        queue.push_back((upper_lo, hi));
    }
    (axes, splits, leaves)
}

pub fn build_conservative(min: [f32; 3], max: [f32; 3], objects: &[Object]) -> Result<Tome> {
    if !(0..3).all(|a| min[a].is_finite() && max[a].is_finite() && min[a] < max[a]) {
        return Err(PrevisError::invalid("conservative tome bounds are empty"));
    }
    let counts = [0, 1, 2].map(|a| ((max[a] - min[a]) / GRID).ceil().max(1.0) as u32);
    let grid = Grid { min, max, counts };
    let (axes, splits, leaves) = grid_tree(&grid);
    let node_count = axes.len() as u32;
    let tree = Tree {
        node_count,
        map_width: 0,
        data: Some(encode_node_stream(&axes)),
        map: None,
        split_count: node_count,
        splits: Some(splits),
    };

    let mut tile_of_cell = std::collections::HashMap::new();
    for (node, leaf) in leaves.iter().enumerate() {
        if let Some(g) = leaf {
            tile_of_cell.insert(*g, node as u32);
        }
    }

    let num_objects = objects.len() as u32;
    let (object_lists, list_widths) = if num_objects == 0 {
        (None, 0)
    } else {
        let (ew, cw) = (bit_width(num_objects - 1), bit_width(num_objects));
        let mut words = vec![0u32; (ew + cw).div_ceil(32) as usize];
        write_bits(&mut words, ew as usize, cw, num_objects);
        (Some(BitList { count: 1, words }), ew | cw << 5)
    };

    let view_tree = tree.clone();
    let view = KdView::new(&view_tree)?;
    let mut tiles: Vec<Option<Tile>> = vec![None; node_count as usize];
    let mut lods = vec![(-1.0f32).to_bits(); node_count as usize];
    let mut paths = Vec::new();
    view.walk(min, max, |node, lo, hi, path| {
        paths.push((node, path.to_vec()));
        let Some(g) = leaves[node as usize] else {
            return;
        };
        lods[node as usize] = lod_level(*lo, *hi, 512.0).to_bits();
        let portals = (0..6u32)
            .map(|face| {
                let axis = (face >> 1) as usize;
                let upper = face & 1 == 1;
                let mut n = g;
                let neighbour = if upper {
                    n[axis] += 1;
                    (n[axis] < counts[axis]).then_some(n)
                } else {
                    (n[axis] > 0).then(|| {
                        n[axis] -= 1;
                        n
                    })
                };
                let z = if upper { 0xFFFF } else { 0 };
                match neighbour.and_then(|n| tile_of_cell.get(&n)) {
                    Some(&target) => Portal {
                        link: face << 29 | target,
                        z,
                        target_index: 0,
                        rect_a: FULL_RECT,
                        rect_b: FULL_RECT,
                    },
                    None => Portal {
                        link: face << 29 | 1 << 28 | Portal::NO_TARGET,
                        z,
                        target_index: 0,
                        rect_a: FULL_RECT,
                        rect_b: FULL_RECT,
                    },
                }
            })
            .collect();
        tiles[node as usize] = Some(Tile {
            tree_min: *lo,
            tree_max: *hi,
            tree: Tree::default(),
            flags: 1,
            portal_expand: PORTAL_EXPAND,
            num_clusters: 1,
            cell_nodes: Some(vec![CellNode {
                portal_index: 0,
                portal_count: 6,
                object_index: 0,
                object_count: num_objects,
                cluster_index: 0,
                cluster_count: 0,
                bounds: FULL,
            }]),
            portals: Some(portals),
            ..Tile::default()
        });
    })?;

    Ok(assemble(
        min,
        max,
        tree,
        tiles,
        lods,
        &paths,
        objects,
        None,
        list_widths,
        object_lists,
    ))
}

/// Nodes in level order with the box each gets during descent from
/// `[min, max]`.
fn level_order(root: &KdNode, min: [f32; 3], max: [f32; 3]) -> Vec<(&KdNode, [f32; 3], [f32; 3])> {
    let mut nodes = vec![(root, min, max)];
    let mut next = 0;
    while next < nodes.len() {
        let (node, lo, hi) = nodes[next];
        if let KdNode::Split {
            axis,
            value,
            low,
            high,
        } = node
        {
            let (mut low_hi, mut high_lo) = (hi, lo);
            low_hi[*axis as usize] = *value;
            high_lo[*axis as usize] = *value;
            nodes.push((low, lo, low_hi));
            nodes.push((high, high_lo, hi));
        }
        next += 1;
    }
    nodes
}

/// Serializes a model kd-tree. Split values are stored up to the last node
/// whose split is not its box midpoint (the runtime's default). With a non-zero
/// `map_width`, leaves map straight to cells and `None` becomes the
/// all-ones "empty" payload, so no BSP is ever needed.
fn encode_kd(nodes: &[(&KdNode, [f32; 3], [f32; 3])], map_width: u32) -> Tree {
    let node_count = nodes.len() as u32;
    let axes: Vec<u32> = nodes
        .iter()
        .map(|(n, _, _)| match n {
            KdNode::Split { axis, .. } => *axis as u32,
            KdNode::Leaf { .. } => LEAF,
        })
        .collect();
    let explicit = |(n, lo, hi): &(&KdNode, [f32; 3], [f32; 3])| match n {
        KdNode::Split { axis, value, .. } => {
            let a = *axis as usize;
            value.to_bits() != ((lo[a] + hi[a]) * 0.5).to_bits()
        }
        KdNode::Leaf { .. } => false,
    };
    let split_count = nodes.iter().rposition(explicit).map_or(0, |i| i + 1);
    let splits: Vec<f32> = nodes[..split_count]
        .iter()
        .map(|(n, _, _)| match n {
            KdNode::Split { value, .. } => *value,
            KdNode::Leaf { .. } => 0.0,
        })
        .collect();
    let map = (map_width > 0).then(|| {
        let empty = (1u32 << map_width) - 1;
        let mut words = vec![0u32; tree_map_words(node_count, map_width)];
        let leaves = nodes.iter().filter_map(|(n, _, _)| match n {
            KdNode::Leaf { cell } => Some(cell.unwrap_or(empty)),
            KdNode::Split { .. } => None,
        });
        for (index, value) in leaves.enumerate() {
            write_bits(&mut words, index * map_width as usize, map_width, value);
        }
        words
    });
    Tree {
        node_count,
        map_width,
        data: Some(encode_node_stream(&axes)),
        map,
        split_count: split_count as u32,
        splits: (split_count > 0).then_some(splits),
    }
}

/// Unlike tile cell trees, the top tree must store a split for every node, with
/// FLT_MAX on leaves, as every retail and CK tome does. The runtime found no
/// visible objects in a top tree with truncated splits.
fn encode_top_kd(nodes: &[(&KdNode, [f32; 3], [f32; 3])]) -> Tree {
    let splits: Vec<f32> = nodes
        .iter()
        .map(|(n, _, _)| match n {
            KdNode::Split { value, .. } => *value,
            KdNode::Leaf { .. } => f32::MAX,
        })
        .collect();
    Tree {
        split_count: splits.len() as u32,
        splits: Some(splits),
        ..encode_kd(nodes, 0)
    }
}

/// Quantizes `value` to the 16-bit lattice over `[lo, hi]`, rounding down or
/// up so that the runtime's dequantization never lands inside the true value.
pub(crate) fn quantize(lo: f32, hi: f32, value: f32, up: bool) -> u16 {
    let dequantize = |q: u16| lo + (hi - lo) * (q as f32 / 65535.0);
    let t = (value as f64 - lo as f64) / (hi as f64 - lo as f64) * 65535.0;
    let mut q = if up { t.ceil() } else { t.floor() }.clamp(0.0, 65535.0) as u16;
    if up {
        while q < u16::MAX && dequantize(q) < value {
            q += 1;
        }
    } else {
        while q > 0 && dequantize(q) > value {
            q -= 1;
        }
    }
    q
}

/// Appends each cell's sorted objects as `(first, count)` runs; cells with an
/// identical list share entries.
#[derive(Default)]
pub(crate) struct ObjectRuns {
    entries: Vec<(u32, u32)>,
    shared: std::collections::HashMap<Vec<u32>, u32>,
}

impl ObjectRuns {
    /// Continues an existing list: new cells append after `entries`.
    pub(crate) fn preloaded(entries: Vec<(u32, u32)>) -> Self {
        Self { entries, ..Self::default() }
    }

    pub(crate) fn add(&mut self, objects: &[u32]) -> (u32, u32) {
        let mut sorted = objects.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted.is_empty() {
            return (0, 0);
        }
        let count = sorted.len() as u32;
        if let Some(&index) = self.shared.get(&sorted) {
            return (index, count);
        }
        let index = self.entries.len() as u32;
        for &object in &sorted {
            let own = &mut self.entries[index as usize..];
            match own.last_mut() {
                Some((first, run)) if *first + *run == object => *run += 1,
                _ => self.entries.push((object, 1)),
            }
        }
        self.shared.insert(sorted, index);
        (index, count)
    }

    pub(crate) fn finish(self, num_objects: u32) -> (Option<BitList>, u32) {
        if self.entries.is_empty() {
            return (None, 0);
        }
        let ew = bit_width(num_objects - 1);
        let cw = bit_width(self.entries.iter().map(|e| e.1).max().unwrap());
        let mut words = vec![0u32; (self.entries.len() * (ew + cw) as usize).div_ceil(32)];
        for (index, &(first, count)) in self.entries.iter().enumerate() {
            let at = index * (ew + cw) as usize;
            write_bits(&mut words, at, ew, first);
            write_bits(&mut words, at + ew as usize, cw, count);
        }
        (
            Some(BitList {
                count: self.entries.len() as u32,
                words,
            }),
            ew | cw << 5,
        )
    }
}

/// Gate IDs and gate-box vertices shared by the user portals of every tile.
/// Only gates some portal passes are numbered: a tome's gate indices must
/// reach every gate it lists.
#[derive(Default)]
struct GateTables {
    index: FxHashMap<u32, u32>,
    ids: Vec<u32>,
    vertices: Vec<[f32; 3]>,
    boxes: FxHashMap<[u32; 6], u32>,
}

impl GateTables {
    /// `(gate index, vertex offset)` of a user portal through model gate
    /// `gate` (ID `id`) over `bounds`; both directions share the vertices.
    fn add(&mut self, gate: u32, id: u32, bounds: &umbra_solve::Aabb) -> Result<(u32, u32)> {
        let next = self.ids.len() as u32;
        let index = *self.index.entry(gate).or_insert_with(|| {
            self.ids.push(id);
            next
        });
        let key: [u32; 6] = std::array::from_fn(|k| if k < 3 { bounds.min[k] } else { bounds.max[k - 3] }.to_bits());
        let offset = *self.boxes.entry(key).or_insert_with(|| {
            self.vertices.extend([bounds.min, bounds.max]);
            self.vertices.len() as u32 - 2
        });
        // A user portal packs both into 20 bits above a 12-bit count.
        if index >= 1 << 20 || offset >= 1 << 20 {
            return Err(PrevisError::invalid("too many gates or gate portals for the format"));
        }
        Ok((index, offset))
    }
}

#[allow(clippy::too_many_arguments)]
fn encode_tile(
    tile: &umbra_solve::Tile,
    lo: [f32; 3],
    hi: [f32; 3],
    tile_node: &[u32],
    node: u32,
    runs: &mut ObjectRuns,
    model_gates: &[u32],
    gates: &mut GateTables,
) -> Result<Tile> {
    let cells = &tile.cells;
    if cells.len() > u16::MAX as usize {
        return Err(PrevisError::invalid(format!(
            "tile has {} cells; the format holds at most 65535",
            cells.len()
        )));
    }
    let cell_tree = &tile.tree;
    let trivial = matches!(cell_tree, KdNode::Leaf { cell: Some(0) }) && cells.len() == 1
        || matches!(cell_tree, KdNode::Leaf { cell: None }) && cells.is_empty();
    let tree = if trivial {
        Tree::default()
    } else {
        encode_kd(
            &level_order(cell_tree, lo, hi),
            bit_width(cells.len().saturating_sub(1) as u32) + 1,
        )
    };

    let mut cell_nodes = Vec::with_capacity(cells.len());
    let mut portals = Vec::new();
    for cell in cells {
        let (object_index, object_count) = runs.add(&cell.objects);
        let q = |axis: usize, up: bool| {
            let v = if up {
                cell.bounds.max[axis]
            } else {
                cell.bounds.min[axis]
            };
            quantize(lo[axis], hi[axis], v, up)
        };
        cell_nodes.push(CellNode {
            portal_index: portals.len() as u32,
            portal_count: (cell.portals.len() + cell.gates.len()) as u32,
            object_index,
            object_count,
            cluster_index: 0,
            cluster_count: 0,
            bounds: PackedAabb([
                q(1, false),
                q(0, false),
                q(0, true),
                q(2, false),
                q(2, true),
                q(1, true),
            ]),
        });
        for portal in &cell.portals {
            let face = portal.face as u32;
            let axis = (face >> 1) as usize;
            let upper = face & 1 == 1;
            let rect = |b: usize| {
                let e = portal.extent[usize::from(b > 3 - axis - b)];
                let (mut q0, mut q1) = (
                    quantize(lo[b], hi[b], e[0], false),
                    quantize(lo[b], hi[b], e[1], true),
                );
                // Border matching needs every outside quad to have area.
                if q1 <= q0 {
                    (q0, q1) = if q0 == u16::MAX {
                        (q0 - 1, q0)
                    } else {
                        (q0, q0 + 1)
                    };
                }
                (q0 as u32) << 16 | q1 as u32
            };
            let (link, target_index) = match portal.target {
                PortalTarget::Tile { tile, cell } => {
                    (face << 29 | tile_node[tile as usize], cell as u16)
                }
                PortalTarget::Outside => (face << 29 | 1 << 28 | Portal::NO_TARGET, 0),
            };
            portals.push(Portal {
                link,
                z: quantize(lo[axis], hi[axis], portal.plane, upper),
                target_index,
                rect_a: rect((axis + 1) % 3),
                rect_b: rect((axis + 2) % 3),
            });
        }
        // Retail user portals: face 0, z 0, a target in the same tile, one
        // gate and a two-vertex box.
        for gate in &cell.gates {
            let (index, offset) = gates.add(gate.gate, model_gates[gate.gate as usize], &gate.bounds)?;
            portals.push(Portal {
                link: 1 << 27 | node,
                z: 0,
                target_index: gate.cell as u16,
                rect_a: index << 12 | 1,
                rect_b: offset << 12 | 2,
            });
        }
    }
    Ok(Tile {
        tree_min: lo,
        tree_max: hi,
        tree,
        flags: 1,
        portal_expand: PORTAL_EXPAND,
        num_clusters: 1,
        cell_nodes: (!cell_nodes.is_empty()).then_some(cell_nodes),
        portals: (!portals.is_empty()).then_some(portals),
        ..Tile::default()
    })
}

/// Encodes the clean-room solve output. Clusters and border matching are
/// derived from the encoded leaf tiles by `assemble`, then
/// `umbra_hierarchy::add_hierarchy` gives the inner top-tree nodes their
/// coarse LOD tiles.
pub fn build_from_model(model: &VisModel) -> Result<Tome> {
    let (min, max) = (model.bounds.min, model.bounds.max);
    if !(0..3).all(|a| min[a].is_finite() && max[a].is_finite() && min[a] < max[a]) {
        return Err(PrevisError::invalid("vis model bounds are empty"));
    }
    let nodes = level_order(&model.top, min, max);
    let mut tile_node = vec![u32::MAX; model.tiles.len()];
    for (node, (n, _, _)) in nodes.iter().enumerate() {
        if let KdNode::Leaf { cell } = n {
            let index = cell
                .filter(|&i| (i as usize) < model.tiles.len() && tile_node[i as usize] == u32::MAX)
                .ok_or_else(|| PrevisError::invalid("top-tree leaf without a unique tile"))?;
            tile_node[index as usize] = node as u32;
        }
    }
    if tile_node.contains(&u32::MAX) {
        return Err(PrevisError::invalid(
            "model tile not reachable from the top tree",
        ));
    }
    let tree = encode_top_kd(&nodes);
    let node_count = tree.node_count as usize;

    let view_tree = tree.clone();
    let view = KdView::new(&view_tree)?;
    let mut boxes = vec![None; node_count];
    let mut paths = Vec::new();
    view.walk(min, max, |node, lo, hi, path| {
        paths.push((node, path.to_vec()));
        boxes[node as usize] = Some((*lo, *hi));
    })?;

    let mut runs = ObjectRuns::default();
    let mut gates = GateTables::default();
    let mut tiles: Vec<Option<Tile>> = vec![None; node_count];
    let mut lods = vec![(-1.0f32).to_bits(); node_count];
    for (index, tile) in model.tiles.iter().enumerate() {
        let node = tile_node[index] as usize;
        let (lo, hi) =
            boxes[node].ok_or_else(|| PrevisError::invalid("unreached top-tree node"))?;
        lods[node] = lod_level(lo, hi, 512.0).to_bits();
        tiles[node] = Some(encode_tile(
            tile,
            lo,
            hi,
            &tile_node,
            node as u32,
            &mut runs,
            &model.gates,
            &mut gates,
        )?);
    }

    let objects: Vec<Object> = model
        .objects
        .iter()
        .map(|o| Object {
            user_id: o.user_id,
            min: o.bounds.min,
            max: o.bounds.max,
        })
        .collect();
    let distances = model.objects.iter().map(object_distance).collect();
    let (object_lists, list_widths) = runs.finish(objects.len() as u32);
    let mut tome = assemble(
        min,
        max,
        tree,
        tiles,
        lods,
        &paths,
        &objects,
        Some(distances),
        list_widths,
        object_lists,
    );
    if !gates.ids.is_empty() {
        tome.num_gates = gates.ids.len() as u32;
        tome.gate_indices = Some((0..tome.num_gates).collect());
        tome.gate_ids = Some(gates.ids);
        tome.gate_vertices = Some(gates.vertices);
    }
    crate::umbra_hierarchy::add_hierarchy(&mut tome)?;
    Ok(tome)
}

/// Distance record `[min, 0, max, squared distance]` (`f32::MAX` when never
/// distance culled), as CK and retail tomes store it.
fn object_distance(object: &umbra_solve::Object) -> [u32; 8] {
    let (min, max) = (object.bounds.min, object.bounds.max);
    let squared = if object.max_distance == f32::MAX { f32::MAX } else { object.max_distance * object.max_distance };
    [min[0], min[1], min[2], 0.0, max[0], max[1], max[2], squared].map(f32::to_bits)
}

fn union_find_root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn cell_portals<'a>(tile: &'a Tile) -> impl Iterator<Item = (usize, &'a Portal)> + 'a {
    let portals = tile.portals.as_deref().unwrap_or(&[]);
    tile.cell_nodes
        .iter()
        .flatten()
        .enumerate()
        .flat_map(move |(c, cell)| {
            portals[cell.portal_index as usize..(cell.portal_index + cell.portal_count) as usize]
                .iter()
                .map(move |p| (c, p))
        })
}

pub(crate) fn pack_box(min: [f32; 3], max: [f32; 3], lo: [f32; 3], hi: [f32; 3]) -> PackedAabb {
    let q = |a: usize, v: f32, up: bool| quantize(min[a], max[a], v, up);
    PackedAabb([
        q(1, lo[1], false),
        q(0, lo[0], false),
        q(0, hi[0], true),
        q(2, lo[2], false),
        q(2, hi[2], true),
        q(1, hi[1], true),
    ])
}

/// Groups each leaf tile's cells into clusters, the connected components of
/// its in-tile regular portals, and links clusters wherever a cell portal
/// crosses between them: one cluster portal per (source, target, face, plane)
/// covering the bounding rect of those cell portals. This follows the retail
/// rules (UMBRA_TOME_FORMAT.md, "Clusters"), and the cluster graph is a
/// superset of the cell graph, so it never hides what the cells can see.
fn build_clusters(
    min: [f32; 3],
    max: [f32; 3],
    tiles: &mut [Option<Tile>],
) -> (Vec<ClusterNode>, Option<Vec<Portal>>) {
    let mut home: Vec<Vec<u32>> = vec![Vec::new(); tiles.len()];
    let mut next = 0u32;
    for (index, slot) in tiles.iter_mut().enumerate() {
        let Some(tile) = slot else { continue };
        let mut parent: Vec<usize> = (0..tile.num_cells()).collect();
        // Cells joined through a door share a cluster too, so the cluster
        // graph needs no gate links and stays a superset of the cell graph.
        for (c, p) in cell_portals(tile) {
            if !p.is_outside() && !p.is_hierarchy() && p.target() as usize == index {
                let a = union_find_root(&mut parent, c);
                let b = union_find_root(&mut parent, p.target_index as usize);
                parent[a.max(b)] = a.min(b);
            }
        }
        let mut local = vec![u32::MAX; parent.len()];
        let mut count = 0;
        for c in 0..parent.len() {
            let root = union_find_root(&mut parent, c);
            if local[root] == u32::MAX {
                local[root] = count;
                count += 1;
            }
            home[index].push(next + local[root]);
        }
        for (cell, &id) in tile.cell_nodes.iter_mut().flatten().zip(&home[index]) {
            cell.cluster_index = id;
            cell.cluster_count = 0;
        }
        tile.num_clusters = count as u16;
        next += count;
    }
    if next == 0 {
        let single = ClusterNode {
            portal_index: 0,
            portal_count: 0,
            bounds: FULL,
        };
        return (vec![single], None);
    }

    let mut boxes = vec![([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]); next as usize];
    let mut links = std::collections::BTreeMap::<(u32, u32, u32, u32), ([f32; 2], [f32; 2])>::new();
    for (index, tile) in tiles.iter().enumerate() {
        let Some(tile) = tile else { continue };
        for c in 0..tile.num_cells() {
            let (lo, hi) = cell_bounds(tile, c as u32).unwrap();
            let b = &mut boxes[home[index][c] as usize];
            for a in 0..3 {
                b.0[a] = b.0[a].min(lo[a]);
                b.1[a] = b.1[a].max(hi[a]);
            }
        }
        for (c, p) in cell_portals(tile) {
            if p.is_outside() || p.is_hierarchy() {
                continue;
            }
            let (from, to) = (
                home[index][c],
                home[p.target() as usize][p.target_index as usize],
            );
            if from == to {
                continue;
            }
            let (plane, lo, hi) = portal_quad(&tile.tree_min, &tile.tree_max, p);
            let rect = links
                .entry((from, to, p.face(), plane.to_bits()))
                .or_insert((lo, hi));
            for k in 0..2 {
                rect.0[k] = rect.0[k].min(lo[k]);
                rect.1[k] = rect.1[k].max(hi[k]);
            }
        }
    }

    let mut outgoing = vec![Vec::new(); next as usize];
    for ((from, to, face, plane), (lo, hi)) in links {
        let axis = (face >> 1) as usize;
        let rect = |k: usize| {
            let b = (axis + 1 + k) % 3;
            (quantize(min[b], max[b], lo[k], false) as u32) << 16
                | quantize(min[b], max[b], hi[k], true) as u32
        };
        outgoing[from as usize].push(Portal {
            link: face << 29 | to,
            z: quantize(min[axis], max[axis], f32::from_bits(plane), face & 1 == 1),
            target_index: u16::MAX,
            rect_a: rect(0),
            rect_b: rect(1),
        });
    }
    let mut nodes = Vec::with_capacity(next as usize);
    let mut portals = Vec::new();
    for (k, list) in outgoing.into_iter().enumerate() {
        nodes.push(ClusterNode {
            portal_index: portals.len() as u32,
            portal_count: list.len() as u32,
            bounds: pack_box(min, max, boxes[k].0, boxes[k].1),
        });
        portals.extend(list);
    }
    (nodes, (!portals.is_empty()).then_some(portals))
}

/// A matching-tree leaf candidate: an outside quad of `cell` on the face,
/// with its rect on the two in-plane axes.
type FaceQuad = (u32, [f32; 2], [f32; 2]);

/// Kd-tree over one tome-boundary face of a leaf tile whose leaves name the
/// cell that owns the outside portals there (`None` where there are none).
/// Cuts are quad edges strictly inside the node on the two in-plane axes, so
/// each leaf ends up with quads of one cell only.
fn face_tree(quads: &[FaceQuad], axes: [usize; 2], lo: [f32; 3], hi: [f32; 3]) -> KdNode {
    let live: Vec<FaceQuad> = quads
        .iter()
        .filter(|(_, a, b)| (0..2).all(|k| a[k] < hi[axes[k]] && b[k] > lo[axes[k]]))
        .copied()
        .collect();
    let mut cells: Vec<u32> = live.iter().map(|q| q.0).collect();
    cells.sort_unstable();
    cells.dedup();
    if cells.len() <= 1 {
        return KdNode::Leaf {
            cell: cells.first().copied(),
        };
    }
    let mut best: Option<((usize, usize), usize, f32)> = None;
    for quad in &live {
        for k in 0..2 {
            for edge in [quad.1[k], quad.2[k]] {
                if !(edge > lo[axes[k]] && edge < hi[axes[k]]) {
                    continue;
                }
                let below = live.iter().filter(|q| q.2[k] <= edge).count();
                let above = live.iter().filter(|q| q.1[k] >= edge).count();
                let score = (live.len() - below - above, below.max(above));
                if best.is_none_or(|(s, bk, be)| score < s || score == s && (k, edge) < (bk, be)) {
                    best = Some((score, k, edge));
                }
            }
        }
    }
    // `low: None` recurses into the lower half as well.
    let split = |axis: usize, value: f32, low: Option<KdNode>, high_quads: &[FaceQuad]| {
        let (mut high_lo, mut low_hi) = (lo, hi);
        high_lo[axis] = value;
        low_hi[axis] = value;
        KdNode::Split {
            axis: axis as u8,
            value,
            low: Box::new(low.unwrap_or_else(|| face_tree(high_quads, axes, lo, low_hi))),
            high: Box::new(face_tree(high_quads, axes, high_lo, hi)),
        }
    };
    if let Some((_, k, edge)) = best {
        return split(axes[k], edge, None, &live);
    }
    // Quads of several cells all span this node: give each cell its own leaf.
    let k = usize::from(hi[axes[1]] - lo[axes[1]] > hi[axes[0]] - lo[axes[0]]);
    let axis = axes[k];
    let mid = (lo[axis] + hi[axis]) * 0.5;
    if !(mid > lo[axis] && mid < hi[axis]) {
        return KdNode::Leaf {
            cell: Some(cells[0]),
        };
    }
    let rest: Vec<FaceQuad> = live.into_iter().filter(|q| q.0 != cells[0]).collect();
    split(
        axis,
        mid,
        Some(KdNode::Leaf {
            cell: Some(cells[0]),
        }),
        &rest,
    )
}

/// Border-matching data for stitching neighbouring tomes: per leaf tile, one
/// matching tree for each face on the tome boundary, in face order. Without
/// hierarchy tiles there are no ancestor cell maps (`bits_a = bits_b = 0`).
fn border_matching(
    min: [f32; 3],
    max: [f32; 3],
    tiles: &[Option<Tile>],
) -> (Vec<LeafMatch>, Vec<Tree>) {
    let mut matches = Vec::new();
    let mut trees = Vec::new();
    for tile in tiles.iter().flatten().filter(|t| t.is_leaf()) {
        let boundary = boundary_faces(&min, &max, tile);
        let faces: Vec<u32> = (0..6).filter(|f| boundary >> f & 1 != 0).collect();
        matches.push(if faces.is_empty() {
            LeafMatch::default()
        } else {
            LeafMatch {
                packed: (trees.len() as u32) << 3 | faces.len() as u32,
                ..LeafMatch::default()
            }
        });
        for face in faces {
            let axis = (face >> 1) as usize;
            let quads: Vec<FaceQuad> = cell_portals(tile)
                .filter(|(_, p)| p.is_outside() && p.face() == face)
                .map(|(c, p)| {
                    let (_, lo, hi) = portal_quad(&tile.tree_min, &tile.tree_max, p);
                    (c as u32, lo, hi)
                })
                .collect();
            let Some(top) = quads.iter().map(|q| q.0).max() else {
                trees.push(Tree::default());
                continue;
            };
            let axes = [(axis + 1) % 3, (axis + 2) % 3];
            let root = face_tree(&quads, axes, tile.tree_min, tile.tree_max);
            trees.push(encode_kd(
                &level_order(&root, tile.tree_min, tile.tree_max),
                bit_width(top + 1),
            ));
        }
    }
    (matches, trees)
}

/// Fills in the tome-wide blocks shared by every writer: slot paths, cell
/// starts, objects, clusters, border matching and the banner.
#[allow(clippy::too_many_arguments)]
fn assemble(
    min: [f32; 3],
    max: [f32; 3],
    tree: Tree,
    mut tiles: Vec<Option<Tile>>,
    lods: Vec<u32>,
    paths: &[(u32, Vec<bool>)],
    objects: &[Object],
    object_distances: Option<Vec<[u32; 8]>>,
    list_widths: u32,
    object_lists: Option<BitList>,
) -> Tome {
    let node_count = tree.node_count;
    let num_objects = objects.len() as u32;
    let max_depth = paths.iter().map(|(_, p)| p.len()).max().unwrap_or(0);
    let bits_per_slot_path = max_depth as u32 + 1;
    let mut tile_paths =
        vec![0u32; (bits_per_slot_path as usize * node_count as usize).div_ceil(32)];
    for (node, path) in paths {
        write_bits(
            &mut tile_paths,
            *node as usize * bits_per_slot_path as usize,
            bits_per_slot_path,
            encode_path(bits_per_slot_path, path),
        );
    }
    let mut cell_starts = vec![0u32];
    for tile in &tiles {
        cell_starts
            .push(cell_starts.last().unwrap() + tile.as_ref().map_or(0, Tile::num_cells) as u32);
    }
    let (cluster_nodes, cluster_portals) = build_clusters(min, max, &mut tiles);
    let (leaf_matches, matching_trees) = border_matching(min, max, &tiles);
    let mut build_info = BANNER.as_bytes().to_vec();
    build_info.resize(128, 0);

    Tome {
        lod_base_distance: 512.0,
        flags: 0,
        tree_min: min,
        tree_max: max,
        tree,
        num_objects,
        object_bounds: (num_objects > 0).then(|| {
            objects
                .iter()
                .map(|o| [o.min[0], o.min[1], o.min[2], o.max[0], o.max[1], o.max[2]])
                .collect()
        }),
        object_distances: object_distances.filter(|_| num_objects > 0),
        user_ids: (num_objects > 0).then(|| objects.iter().map(|o| o.user_id).collect()),
        list_widths,
        object_lists,
        cluster_lists: None,
        num_gates: 0,
        gate_ids: None,
        gate_vertices: None,
        gate_indices: None,
        cluster_nodes: Some(cluster_nodes),
        cluster_portals,
        cell_starts: Some(cell_starts),
        bits_per_slot_path,
        tile_paths: Some(tile_paths),
        tile_lod_levels: Some(lods),
        tiles,
        leaf_matches,
        matching_trees,
        build_info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::umbra_query::visible_objects;
    use crate::umbra_tome::{parse, write};
    use crate::umbra_validate::validate;

    #[test]
    fn conservative_tome_validates_round_trips_and_sees_everything() {
        let objects = [
            Object {
                user_id: 0xFD00_0000,
                min: [10.0, 10.0, 0.0],
                max: [100.0, 50.0, 30.0],
            },
            Object {
                user_id: 0xFD00_0001,
                min: [900.0, 1400.0, 0.0],
                max: [1000.0, 1500.0, 200.0],
            },
            Object {
                user_id: 0x0100_0801,
                min: [-128.0, 0.0, 0.0],
                max: [0.0, 128.0, 64.0],
            },
        ];
        let tome = build_conservative([-128.0, -256.0, -128.0], [1152.0, 1536.0, 640.0], &objects)
            .unwrap();
        validate(&tome).unwrap();
        let bytes = write(&tome);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(write(&parsed), bytes);
        validate(&parsed).unwrap();
        for p in [
            [0.0, 0.0, 0.0],
            [1100.0, 1500.0, 600.0],
            [500.0, -200.0, 300.0],
        ] {
            assert_eq!(visible_objects(&parsed, p).unwrap().unwrap(), vec![0, 1, 2]);
        }
    }

    use crate::scene::{Geometry, INSTANCE_GATE, INSTANCE_OCCLUDER, Instance};
    use crate::umbra_query::{CellHit, cell_bounds, find_cell};
    use crate::umbra_solve::{SolveParams, solve};

    const IDENTITY: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];

    fn volume(min: [f32; 3], max: [f32; 3]) -> [f32; 12] {
        [
            min[0], min[1], min[2], max[0], max[1], max[2], -1.0, 1.0, 1.0, 1.0, -1.0, 0.0,
        ]
    }

    fn quad_x(x: f32, y: [f32; 2], z: [f32; 2]) -> Geometry {
        Geometry {
            vertices: vec![
                [x, y[0], z[0]],
                [x, y[1], z[0]],
                [x, y[1], z[1]],
                [x, y[0], z[1]],
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        }
    }

    fn quad_y(y: f32, x: [f32; 2], z: [f32; 2]) -> Geometry {
        Geometry {
            vertices: vec![
                [x[0], y, z[0]],
                [x[1], y, z[0]],
                [x[1], y, z[1]],
                [x[0], y, z[1]],
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        }
    }

    fn scene(volume_max: [f32; 3], occluders: Vec<Geometry>, targets: &[([f32; 3], f32)]) -> Scene {
        let mut geometries = occluders;
        let mut instances: Vec<Instance> = (0..geometries.len() as u32)
            .map(|i| instance(0x100 + i, i, INSTANCE_OCCLUDER))
            .collect();
        for (k, &(min, size)) in targets.iter().enumerate() {
            let [x, y, z] = min;
            geometries.push(Geometry {
                vertices: vec![[x, y, z], [x + size, y + size, z + size], [x, y + size, z]],
                triangles: vec![[0, 1, 2]],
            });
            instances.push(instance(
                0xFD00_0000 + k as u32,
                geometries.len() as u32 - 1,
                INSTANCE_TARGET,
            ));
        }
        Scene {
            geometries,
            instances,
            volumes: vec![volume([0.0; 3], volume_max)],
        }
    }

    fn instance(object_id: u32, model_index: u32, flags: u32) -> Instance {
        Instance {
            object_id,
            model_index,
            transform: IDENTITY,
            flags,
            vector: [0.0; 3],
            bounds: [0.0; 6],
        }
    }

    fn descend(mut node: &KdNode, p: [f32; 3]) -> Option<u32> {
        loop {
            match node {
                KdNode::Leaf { cell } => return *cell,
                KdNode::Split {
                    axis,
                    value,
                    low,
                    high,
                } => {
                    node = if p[*axis as usize] < *value {
                        low
                    } else {
                        high
                    };
                }
            }
        }
    }

    /// Objects the model lists in every cell reachable from `(tile, cell)` by
    /// crossing non-outside portals.
    fn model_flood(model: &VisModel, tile: u32, cell: u32) -> Vec<u32> {
        let mut seen = std::collections::HashSet::from([(tile, cell)]);
        let mut queue = vec![(tile, cell)];
        let mut objects = Vec::new();
        while let Some((t, c)) = queue.pop() {
            let cell = &model.tiles[t as usize].cells[c as usize];
            objects.extend(&cell.objects);
            for portal in &cell.portals {
                if let PortalTarget::Tile { tile, cell } = portal.target
                    && seen.insert((tile, cell))
                {
                    queue.push((tile, cell));
                }
            }
        }
        objects.sort_unstable();
        objects.dedup();
        objects
    }

    fn check_model_tome(model: &VisModel) {
        let tome = build_from_model(model).unwrap();
        validate(&tome).unwrap();
        let bytes = write(&tome);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(write(&parsed), bytes);
        validate(&parsed).unwrap();

        let nodes = level_order(&model.top, model.bounds.min, model.bounds.max);
        let top_splits = parsed.tree.splits.as_deref().unwrap();
        assert_eq!(top_splits.len(), nodes.len());
        for ((n, _, _), &split) in nodes.iter().zip(top_splits) {
            if matches!(n, KdNode::Leaf { .. }) {
                assert_eq!(split, f32::MAX);
            }
        }
        let node_of_tile = |tile: u32| {
            nodes
                .iter()
                .position(|(n, _, _)| *n == &KdNode::Leaf { cell: Some(tile) })
                .unwrap() as u32
        };
        for (index, tile) in model.tiles.iter().enumerate() {
            let encoded = parsed.tiles[node_of_tile(index as u32) as usize]
                .as_ref()
                .unwrap();
            assert_eq!(
                (encoded.tree_min, encoded.tree_max),
                (tile.bounds.min, tile.bounds.max)
            );
            for (c, cell) in tile.cells.iter().enumerate() {
                let (lo, hi) = cell_bounds(encoded, c as u32).unwrap();
                // Voxel grids can overhang a tile edge; only the part inside the tile is encodable.
                let inside = |a: usize| {
                    (
                        cell.bounds.min[a].max(tile.bounds.min[a]),
                        cell.bounds.max[a].min(tile.bounds.max[a]),
                    )
                };
                assert!(
                    (0..3).all(|a| lo[a] <= inside(a).0 && hi[a] >= inside(a).1),
                    "tile {:?} cell {:?} encoded {lo:?}..{hi:?}",
                    tile.bounds,
                    cell.bounds
                );
                let first = encoded.cell_nodes.as_ref().unwrap()[c].portal_index as usize;
                let (tmin, tmax) = (tile.bounds.min, tile.bounds.max);
                let dq = |a: usize, q: u32| {
                    tmin[a] + (tmax[a] - tmin[a]) * ((q & 0xFFFF) as f32 / 65535.0)
                };
                for (k, portal) in cell.portals.iter().enumerate() {
                    let ours = &encoded.portals.as_ref().unwrap()[first + k];
                    let axis = (portal.face >> 1) as usize;
                    assert_eq!(ours.face(), portal.face as u32);
                    for (b, rect) in [((axis + 1) % 3, ours.rect_a), ((axis + 2) % 3, ours.rect_b)]
                    {
                        let e = portal.extent[usize::from(b > 3 - axis - b)];
                        let (e0, e1) = (e[0].max(tmin[b]), e[1].min(tmax[b]));
                        assert!(
                            dq(b, rect >> 16) <= e0 && dq(b, rect) >= e1,
                            "{portal:?} {ours:?}"
                        );
                    }
                }
            }
        }

        let (min, max) = (model.bounds.min, model.bounds.max);
        let steps = 9;
        for i in 0..steps * steps * steps {
            let p: [f32; 3] = std::array::from_fn(|a| {
                let k = [i % steps, i / steps % steps, i / (steps * steps)][a];
                min[a] + (max[a] - min[a]) * (k as f32 + 0.37) / steps as f32
            });
            let tile = descend(&model.top, p).unwrap();
            let cell = descend(&model.tiles[tile as usize].tree, p);
            let hit = find_cell(&parsed, p).unwrap();
            let Some(cell) = cell else {
                assert!(matches!(hit, CellHit::Empty { .. }), "{p:?}: {hit:?}");
                continue;
            };
            assert!(
                matches!(hit, CellHit::Cell { tile: t, cell: c } if t == node_of_tile(tile) && c == cell),
                "{p:?}: {hit:?}"
            );
            let visible = visible_objects(&parsed, p).unwrap().unwrap();
            let own = &model.tiles[tile as usize].cells[cell as usize].objects;
            assert!(own.iter().all(|o| visible.contains(o)));
            assert!(
                model_flood(model, tile, cell)
                    .iter()
                    .all(|o| visible.contains(o))
            );
        }
        check_clusters_cover_cells(&parsed);
    }

    /// Every leaf cell reachable by the leaf cell-portal walk is in a cluster
    /// reachable through cluster portals from the start cell's cluster.
    fn check_clusters_cover_cells(tome: &Tome) {
        let home = |t: u32, c: u32| {
            tome.tiles[t as usize]
                .as_ref()
                .unwrap()
                .cell_nodes
                .as_ref()
                .unwrap()[c as usize]
                .cluster_index
        };
        let nodes = tome.cluster_nodes.as_deref().unwrap();
        let portals = tome.cluster_portals.as_deref().unwrap_or(&[]);
        for (t, tile) in tome.tiles.iter().enumerate() {
            let Some(tile) = tile.as_ref().filter(|t| t.is_leaf()) else { continue };
            for c in 0..tile.num_cells() as u32 {
                let mut clusters = std::collections::HashSet::from([home(t as u32, c)]);
                let mut stack = vec![home(t as u32, c)];
                while let Some(k) = stack.pop() {
                    let n = &nodes[k as usize];
                    for p in &portals
                        [n.portal_index as usize..(n.portal_index + n.portal_count) as usize]
                    {
                        if clusters.insert(p.target()) {
                            stack.push(p.target());
                        }
                    }
                }
                let mut seen = std::collections::HashSet::from([(t as u32, c)]);
                let mut cells = vec![(t as u32, c)];
                while let Some((ct, cc)) = cells.pop() {
                    assert!(clusters.contains(&home(ct, cc)));
                    let tile = tome.tiles[ct as usize].as_ref().unwrap();
                    let node = &tile.cell_nodes.as_ref().unwrap()[cc as usize];
                    for p in &tile.portals.as_ref().unwrap()[node.portal_index as usize
                        ..(node.portal_index + node.portal_count) as usize]
                    {
                        if !p.is_outside() && !p.is_hierarchy() && seen.insert((p.target(), p.target_index as u32)) {
                            cells.push((p.target(), p.target_index as u32));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn face_tree_separates_pinwheel_and_stacked_quads() {
        // Face 0 (x min): in-plane axes y, z over a 0..4 square. A pinwheel of
        // four cells around an empty centre, plus cells 4 and 5 sharing a quad.
        let quads: Vec<FaceQuad> = vec![
            (0, [0.0, 0.0], [3.0, 1.0]),
            (1, [3.0, 0.0], [4.0, 3.0]),
            (2, [1.0, 3.0], [4.0, 4.0]),
            (3, [0.0, 1.0], [1.0, 4.0]),
            (4, [1.5, 1.5], [2.5, 2.5]),
            (5, [1.5, 1.5], [2.5, 2.5]),
        ];
        let (lo, hi) = ([0.0; 3], [1.0, 4.0, 4.0]);
        let root = face_tree(&quads, [1, 2], lo, hi);
        let mut leaves = Vec::new();
        for (node, lo, hi) in level_order(&root, lo, hi) {
            if let KdNode::Leaf { cell } = node {
                leaves.push((*cell, lo, hi));
            }
        }
        let overlaps = |q: &FaceQuad, lo: [f32; 3], hi: [f32; 3]| {
            (0..2).all(|k| q.1[k] < hi[k + 1] && q.2[k] > lo[k + 1])
        };
        for q in &quads {
            assert!(
                leaves
                    .iter()
                    .any(|(c, lo, hi)| *c == Some(q.0) && overlaps(q, *lo, *hi))
            );
        }
        for (cell, lo, hi) in &leaves {
            let here: Vec<u32> = quads
                .iter()
                .filter(|q| overlaps(q, *lo, *hi))
                .map(|q| q.0)
                .collect();
            match cell {
                Some(c) => assert!(here.contains(c)),
                None => assert!(here.is_empty()),
            }
        }
        assert!(leaves.iter().any(|l| l.0.is_none()));
    }

    #[test]
    fn border_matching_and_clusters_follow_retail_rules() {
        let walls = vec![
            quad_x(256.0, [0.0, 1024.0], [0.0, 512.0]),
            quad_x(1100.0, [0.0, 400.0], [0.0, 512.0]),
            quad_x(1100.0, [528.0, 1024.0], [0.0, 512.0]),
            quad_y(700.0, [256.0, 1100.0], [0.0, 512.0]),
        ];
        let params = SolveParams {
            max_tile_cells: 1,
            ..SolveParams::default()
        };
        let model = solve(
            &scene(
                [1536.0, 1024.0, 512.0],
                walls,
                &[([40.0, 40.0, 10.0], 30.0)],
            ),
            &params,
        )
        .unwrap();
        let tome = build_from_model(&model).unwrap();
        validate(&tome).unwrap();
        assert!(!tome.matching_trees.is_empty());
        assert!(tome.num_clusters() > 1 && tome.cluster_portals.is_some());
        let leaves = || tome.tiles.iter().flatten().filter(|t| t.is_leaf());
        for (tile, m) in leaves().zip(&tome.leaf_matches) {
            let faces = crate::umbra_query::boundary_faces(&tome.tree_min, &tome.tree_max, tile);
            assert_eq!(m.packed & 7, faces.count_ones());
        }

        let broken = |mutate: &dyn Fn(&mut Tome)| {
            let mut t = tome.clone();
            mutate(&mut t);
            validate(&t).is_err()
        };
        assert!(broken(&|t| {
            t.matching_trees.pop();
        }));
        assert!(broken(&|t| {
            let m = t.leaf_matches.iter_mut().find(|m| m.packed != 0).unwrap();
            m.packed += 1;
        }));
        assert!(broken(&|t| {
            let tree = t
                .matching_trees
                .iter_mut()
                .find(|t| t.map.is_some())
                .unwrap();
            let map = tree.map.as_mut().unwrap();
            map.iter_mut().for_each(|w| *w = u32::MAX);
        }));
        assert!(broken(&|t| {
            let nodes = t.cluster_nodes.as_mut().unwrap();
            let k = nodes.iter().position(|n| n.portal_count > 0).unwrap();
            let index = nodes[k].portal_index as usize;
            nodes[k].portal_count -= 1;
            for n in &mut nodes[k + 1..] {
                n.portal_index -= 1;
            }
            let portals = t.cluster_portals.as_mut().unwrap();
            portals.remove(index);
        }));
        assert!(broken(&|t| {
            let tile = t
                .tiles
                .iter_mut()
                .flatten()
                .find(|t| t.num_clusters > 0)
                .unwrap();
            tile.num_clusters += 1;
        }));
    }

    #[test]
    fn solved_models_encode_validate_round_trip_and_stay_conservative() {
        let targets = [
            ([40.0, 40.0, 10.0], 30.0),
            ([300.0, 200.0, 50.0], 20.0),
            ([1300.0, 900.0, 400.0], 50.0),
        ];
        let walls = || {
            vec![
                quad_x(256.0, [0.0, 1024.0], [0.0, 512.0]),
                // A wall with a doorway at y 400..528.
                quad_x(1100.0, [0.0, 400.0], [0.0, 512.0]),
                quad_x(1100.0, [528.0, 1024.0], [0.0, 512.0]),
                quad_y(700.0, [256.0, 1100.0], [0.0, 512.0]),
            ]
        };
        let one_tile = SolveParams::default();
        let many_tiles = SolveParams {
            max_tile_cells: 1,
            ..SolveParams::default()
        };
        let cases = [
            (scene([512.0; 3], Vec::new(), &[]), one_tile),
            (
                scene(
                    [512.0; 3],
                    vec![quad_x(256.0, [0.0, 512.0], [0.0, 512.0])],
                    &targets[..2],
                ),
                one_tile,
            ),
            (scene([1536.0, 1024.0, 512.0], walls(), &targets), one_tile),
            (
                scene([1536.0, 1024.0, 512.0], walls(), &targets),
                many_tiles,
            ),
            (
                scene([1600.0, 1000.0, 700.0], walls(), &targets),
                many_tiles,
            ),
        ];
        for (scene, params) in &cases {
            let model = solve(scene, params).unwrap();
            check_model_tome(&model);
        }
        let split = solve(&cases[3].0, &many_tiles).unwrap();
        assert!(split.tiles.len() > 1 && split.tiles.iter().any(|t| t.cells.len() > 1));
    }

    #[test]
    fn gates_and_distances_encode_like_retail() {
        // The doorway in the x = 1100 wall (y 400..528) is shut by a door.
        let targets = [([40.0, 40.0, 10.0], 30.0), ([1300.0, 900.0, 400.0], 50.0)];
        let walls = vec![
            quad_x(1100.0, [0.0, 400.0], [0.0, 512.0]),
            quad_x(1100.0, [528.0, 1024.0], [0.0, 512.0]),
        ];
        let mut scene = scene([1536.0, 1024.0, 512.0], walls, &targets);
        scene.instances[2].vector = [0.0, 0.0, 4096.0];
        scene.instances[3].vector = [0.0, 0.0, -1.0];
        scene.geometries.push(quad_x(1100.0, [400.0, 528.0], [0.0, 512.0]));
        scene.instances.push(instance(0xFE00_0ABC, scene.geometries.len() as u32 - 1, INSTANCE_GATE));
        let model = solve(&scene, &SolveParams::default()).unwrap();
        check_model_tome(&model);

        let tome = build_from_model(&model).unwrap();
        assert_eq!((tome.num_gates, tome.gate_ids.as_deref()), (1, Some(&[0xFE00_0ABC][..])));
        assert_eq!(tome.gate_indices.as_deref(), Some(&[0][..]));
        let vertices = tome.gate_vertices.as_deref().unwrap();
        let mut users = 0;
        for (node, tile) in tome.tiles.iter().enumerate() {
            let Some(tile) = tile else { continue };
            for p in tile.portals.iter().flatten().filter(|p| p.is_user()) {
                users += 1;
                assert_eq!((p.face(), p.z, p.target() as usize), (0, 0, node));
                assert_eq!((p.user_object_offset(), p.user_object_count(), p.gate_vertex_count()), (0, 1, 2));
                let at = p.gate_vertex_offset() as usize;
                let (lo, hi) = (vertices[at], vertices[at + 1]);
                assert!((0..3).all(|a| lo[a] <= hi[a]) && lo[0] <= 1100.0 && hi[0] >= 1100.0);
            }
        }
        assert!(users >= 2);

        let distances = tome.object_distances.as_deref().unwrap();
        let bounds = tome.object_bounds.as_deref().unwrap();
        for (k, (d, b)) in distances.iter().zip(bounds).enumerate() {
            let d = d.map(f32::from_bits);
            assert_eq!([d[0], d[1], d[2], d[4], d[5], d[6]], *b);
            assert_eq!(d[3], 0.0);
            assert_eq!(d[7], if k == 0 { 4096.0 * 4096.0 } else { f32::MAX });
        }
    }
}
