//! Clean-room point and visibility queries over a parsed Umbra tome.
//!
//! Written from `UMBRA_TOME_FORMAT.md` ("Kd-tree encoding" and "Queries"). The
//! walk is conservative: it follows every non-outside, non-hierarchy portal
//! without frustum or occlusion tests, so it returns the set of objects the
//! runtime could ever report from the start cell.

use std::collections::VecDeque;

use crate::error::{PrevisError, Result};
use crate::umbra_tome::{BitList, Portal, Tile, Tome, Tree};

pub const LEAF: u32 = 3;

pub fn node_words(node_count: u32) -> usize {
    (2 * node_count as usize).div_ceil(32)
}

/// Rank lookup table appended to a node stream: `u32` inner-node counts per
/// 65,536 nodes, then `u16` counts per 256 within that block, then `u8` counts
/// per 16 within the 256 block. Entries at block starts, which would be 0, are
/// omitted.
pub fn rank_lut(node_count: u32, node_stream: &[u32]) -> Vec<u32> {
    let n = node_count as usize;
    let axis = |i: usize| node_stream[i >> 4] >> ((i & 15) * 2) & 3;
    let top_len = n >> 16;
    let mid_len = (n >> 8) - (n >> 16);
    let bottom_len = (n >> 4) - (n >> 8);
    let mid_offset = top_len;
    let bottom_offset = top_len + mid_len.div_ceil(2);
    let mut lut = vec![0u32; bottom_offset + bottom_len.div_ceil(4)];
    let mut inner = 0u32;
    let (mut block_start_65536, mut block_start_256) = (0u32, 0u32);
    for j in 0..=n {
        if j > 0 && j % 16 == 0 {
            if j % 65536 == 0 {
                lut[(j >> 16) - 1] = inner;
                block_start_65536 = inner;
                block_start_256 = inner;
            } else if j % 256 == 0 {
                let m = j >> 8;
                let index = m - (m >> 8) - 1;
                let value = inner - block_start_65536;
                lut[mid_offset + index / 2] |= (value & 0xFFFF) << ((index & 1) * 16);
                block_start_256 = inner;
            } else {
                let q = j >> 4;
                let index = q - (q >> 4) - 1;
                let value = inner - block_start_256;
                lut[bottom_offset + index / 4] |= (value & 0xFF) << ((index & 3) * 8);
            }
        }
        if j < n && axis(j) != LEAF {
            inner += 1;
        }
    }
    lut
}

/// Full serialized node stream: two bits per node, then the rank table.
pub fn encode_node_stream(axes: &[u32]) -> Vec<u32> {
    let mut words = vec![0u32; node_words(axes.len() as u32)];
    for (i, &axis) in axes.iter().enumerate() {
        words[i >> 4] |= (axis & 3) << ((i & 15) * 2);
    }
    let lut = rank_lut(axes.len() as u32, &words);
    words.extend(lut);
    words
}

/// Tile slot path field: a sentinel bit at `bits - 1 - depth` with the
/// inverted branch choices (1 = lower child) above it, the root's choice
/// just above the sentinel and the last choice in the top bit.
pub fn encode_path(bits_per_path: u32, path: &[bool]) -> u32 {
    let depth = path.len() as u32;
    let mut value = 1u32 << (bits_per_path - 1 - depth);
    for (k, &upper) in path.iter().enumerate() {
        if !upper {
            value |= 1 << (bits_per_path - depth + k as u32);
        }
    }
    value
}

/// Per-tile LOD level: cube root of the tile volume over the LOD base distance.
pub fn lod_level(min: [f32; 3], max: [f32; 3], lod_base: f32) -> f32 {
    let volume = (max[0] - min[0]) * (max[1] - min[1]) * (max[2] - min[2]);
    ((volume as f64).cbrt() / lod_base as f64) as f32
}

pub fn read_bits(words: &[u32], bit: usize, width: u32) -> u32 {
    if width == 0 {
        return 0;
    }
    let lo = words.get(bit >> 5).copied().unwrap_or(0) as u64;
    let hi = words.get((bit >> 5) + 1).copied().unwrap_or(0) as u64;
    let joined = lo | hi << 32;
    (joined >> (bit & 31)) as u32 & ((1u64 << width) - 1) as u32
}

pub fn write_bits(words: &mut [u32], bit: usize, width: u32, value: u32) {
    for k in 0..width as usize {
        if value >> k & 1 != 0 {
            words[(bit + k) >> 5] |= 1 << ((bit + k) & 31);
        }
    }
}

/// `(first, count)` run `i` of a bit-packed list with the given field widths.
pub fn list_run(list: &BitList, index: u32, elem_width: u32, count_width: u32) -> (u32, u32) {
    let bit = index as usize * (elem_width + count_width) as usize;
    (
        read_bits(&list.words, bit, elem_width),
        read_bits(&list.words, bit + elem_width as usize, count_width),
    )
}

/// Read-only view of a serialized level-order kd-tree.
pub struct KdView<'a> {
    pub node_count: u32,
    stream: &'a [u32],
    splits: &'a [f32],
    inner_through: Vec<u32>,
}

impl<'a> KdView<'a> {
    pub fn new(tree: &'a Tree) -> Result<Self> {
        let stream = tree.data.as_deref().unwrap_or(&[]);
        if stream.len() < node_words(tree.node_count) {
            return Err(PrevisError::invalid(
                "kd-tree node stream is shorter than its node count",
            ));
        }
        let mut inner_through = Vec::with_capacity(tree.node_count as usize);
        let mut inner = 0;
        for i in 0..tree.node_count as usize {
            if stream[i >> 4] >> ((i & 15) * 2) & 3 != LEAF {
                inner += 1;
            }
            inner_through.push(inner);
        }
        Ok(Self {
            node_count: tree.node_count,
            stream,
            splits: tree.splits.as_deref().unwrap_or(&[]),
            inner_through,
        })
    }

    pub fn axis(&self, node: u32) -> u32 {
        self.stream[node as usize >> 4] >> ((node as usize & 15) * 2) & 3
    }

    pub fn is_leaf(&self, node: u32) -> bool {
        self.axis(node) == LEAF
    }

    /// `(lower, upper)` children: `2k - 1` and `2k` for the `k`-th inner node.
    pub fn children(&self, node: u32) -> (u32, u32) {
        let k = self.inner_through[node as usize];
        (2 * k - 1, 2 * k)
    }

    /// Index among leaves, in node order.
    pub fn leaf_index(&self, node: u32) -> u32 {
        node - self.inner_through[node as usize]
    }

    pub fn leaf_count(&self) -> u32 {
        self.node_count - self.inner_through.last().copied().unwrap_or(0)
    }

    /// Explicit value for nodes below the split count, else the box midpoint.
    pub fn split_value(&self, node: u32, min: &[f32; 3], max: &[f32; 3]) -> f32 {
        let axis = self.axis(node) as usize;
        match self.splits.get(node as usize) {
            Some(&value) => value,
            None => (min[axis] + max[axis]) * 0.5,
        }
    }

    /// Depth-first visit of every node with its box and branch path (`true` = upper).
    pub fn walk(
        &self,
        min: [f32; 3],
        max: [f32; 3],
        mut visit: impl FnMut(u32, &[f32; 3], &[f32; 3], &[bool]),
    ) -> Result<()> {
        if self.node_count == 0 {
            return Ok(());
        }
        let mut stack = vec![(0u32, min, max, Vec::new())];
        let mut seen = 0u32;
        while let Some((node, lo, hi, path)) = stack.pop() {
            if node >= self.node_count || seen >= self.node_count {
                return Err(PrevisError::invalid("kd-tree child index out of range"));
            }
            seen += 1;
            visit(node, &lo, &hi, &path);
            if self.is_leaf(node) {
                continue;
            }
            let axis = self.axis(node) as usize;
            let split = self.split_value(node, &lo, &hi);
            let (lower, upper) = self.children(node);
            let (mut lower_max, mut upper_min) = (hi, lo);
            lower_max[axis] = split;
            upper_min[axis] = split;
            let mut upper_path = path.clone();
            upper_path.push(true);
            let mut lower_path = path;
            lower_path.push(false);
            stack.push((upper, upper_min, hi, upper_path));
            stack.push((lower, lo, lower_max, lower_path));
        }
        if seen != self.node_count {
            return Err(PrevisError::invalid("kd-tree has unreachable nodes"));
        }
        Ok(())
    }

    /// Leaf node containing `point`, with its box.
    pub fn locate(
        &self,
        point: [f32; 3],
        mut min: [f32; 3],
        mut max: [f32; 3],
    ) -> (u32, [f32; 3], [f32; 3]) {
        let mut node = 0;
        while self.node_count > 0 && !self.is_leaf(node) {
            let axis = self.axis(node) as usize;
            let split = self.split_value(node, &min, &max);
            let (lower, upper) = self.children(node);
            if point[axis] < split {
                max[axis] = split;
                node = lower;
            } else {
                min[axis] = split;
                node = upper;
            }
        }
        (node, min, max)
    }
}

/// Decoded leaf payload of a tile's cell tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellLeaf {
    Cell(u32),
    /// Root of a BSP over the tile's planes that resolves the cell.
    Bsp(u32),
    /// Solid or unreachable space.
    Empty,
}

pub fn cell_leaf(tree: &Tree, leaf_index: u32) -> CellLeaf {
    let width = tree.map_width;
    let value = read_bits(
        tree.map.as_deref().unwrap_or(&[]),
        (leaf_index * width) as usize,
        width,
    );
    let mask = if width >= 32 {
        u32::MAX
    } else {
        (1 << width) - 1
    };
    if width > 0 && value >> (width - 1) & 1 != 0 {
        if value == mask {
            CellLeaf::Empty
        } else {
            CellLeaf::Bsp(value & (mask >> 1))
        }
    } else {
        CellLeaf::Cell(value)
    }
}

/// Walks the tile BSP: `(plane, flags | children)` per node, where a negative
/// plane distance takes the low child and bit 30 marks it terminal, and a
/// non-negative distance takes the high child with terminal bit 31. A
/// terminal `0xFFFF` is empty space.
pub fn bsp_cell(tile: &Tile, root: u32, point: [f32; 3]) -> Option<u32> {
    let nodes = tile.bsp_triangles.as_deref()?;
    let planes = tile.planes.as_deref()?;
    let mut index = root as usize;
    for _ in 0..=nodes.len() {
        let [head, children] = *nodes.get(index)?;
        let plane = planes.get((head & 0x3FFF_FFFF) as usize)?;
        let distance = plane[0] * point[0] + plane[1] * point[1] + plane[2] * point[2] + plane[3];
        let (next, terminal) = if distance < 0.0 {
            (children & 0xFFFF, head >> 30 & 1 != 0)
        } else {
            (children >> 16, head >> 31 != 0)
        };
        if terminal {
            return (next != 0xFFFF).then_some(next);
        }
        index = next as usize;
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellHit {
    Cell {
        tile: u32,
        cell: u32,
    },
    /// Inside the tome but in solid or unreachable space.
    Empty {
        tile: u32,
    },
    Outside,
}

pub fn contains(min: &[f32; 3], max: &[f32; 3], point: [f32; 3]) -> bool {
    (0..3).all(|a| point[a] >= min[a] && point[a] <= max[a])
}

/// Leaf tile containing `point` (top-tree node `i` is tile `i`).
pub fn find_tile(tome: &Tome, point: [f32; 3]) -> Result<Option<u32>> {
    if !contains(&tome.tree_min, &tome.tree_max, point) {
        return Ok(None);
    }
    let view = KdView::new(&tome.tree)?;
    Ok(Some(view.locate(point, tome.tree_min, tome.tree_max).0))
}

pub fn find_cell(tome: &Tome, point: [f32; 3]) -> Result<CellHit> {
    let Some(tile_index) = find_tile(tome, point)? else {
        return Ok(CellHit::Outside);
    };
    let tile = tome.tiles[tile_index as usize]
        .as_ref()
        .ok_or_else(|| PrevisError::invalid("point resolves to an empty tile slot"))?;
    let leaf = if tile.tree.data.is_none() {
        if tile.num_cells() == 0 {
            CellLeaf::Empty
        } else {
            CellLeaf::Cell(0)
        }
    } else {
        let view = KdView::new(&tile.tree)?;
        let (node, _, _) = view.locate(point, tile.tree_min, tile.tree_max);
        cell_leaf(&tile.tree, view.leaf_index(node))
    };
    let cell = match leaf {
        CellLeaf::Cell(cell) => Some(cell),
        CellLeaf::Bsp(root) => bsp_cell(tile, root, point),
        CellLeaf::Empty => None,
    };
    Ok(match cell {
        Some(cell) => CellHit::Cell {
            tile: tile_index,
            cell,
        },
        None => CellHit::Empty { tile: tile_index },
    })
}

/// World-space box of a cell from its quantized bounds in the tile box.
pub fn cell_bounds(tile: &Tile, cell: u32) -> Option<([f32; 3], [f32; 3])> {
    let node = tile.cell_nodes.as_ref()?.get(cell as usize)?;
    let dequantize = |axis: usize, v: u16| {
        let t = v as f32 / 65535.0;
        tile.tree_min[axis] + (tile.tree_max[axis] - tile.tree_min[axis]) * t
    };
    let (lo, hi) = (node.bounds.min(), node.bounds.max());
    Some((
        [
            dequantize(0, lo[0]),
            dequantize(1, lo[1]),
            dequantize(2, lo[2]),
        ],
        [
            dequantize(0, hi[0]),
            dequantize(1, hi[1]),
            dequantize(2, hi[2]),
        ],
    ))
}

/// Faces (bit `2 * axis + side`) of the tome box `[min, max]` that the tile
/// lies on. Its outside portals and matching trees sit on exactly these.
pub fn boundary_faces(min: &[f32; 3], max: &[f32; 3], tile: &Tile) -> u32 {
    (0..3).fold(0, |mask, a| {
        mask | u32::from(tile.tree_min[a] == min[a]) << (2 * a)
            | u32::from(tile.tree_max[a] == max[a]) << (2 * a + 1)
    })
}

/// `lerp(lo, hi, q / 65535)`, the runtime's decoding of 16-bit quantized coordinates.
pub fn dequantize(lo: f32, hi: f32, q: u32) -> f32 {
    lo + (hi - lo) * (q as f32 / 65535.0)
}

/// A portal quad in world space from its quantized fields in the `[min, max]`
/// box (the tile box for cell portals, the tome box for cluster portals):
/// `(plane, rect min, rect max)`, the rect on axes `(axis + 1) % 3` and
/// `(axis + 2) % 3` in that order.
pub fn portal_quad(min: &[f32; 3], max: &[f32; 3], portal: &Portal) -> (f32, [f32; 2], [f32; 2]) {
    let axis = (portal.face() >> 1) as usize;
    let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
    let dq = |k: usize, q: u32| dequantize(min[k], max[k], q & 0xFFFF);
    (
        dq(axis, portal.z as u32),
        [dq(a, portal.rect_a >> 16), dq(b, portal.rect_b >> 16)],
        [dq(a, portal.rect_a), dq(b, portal.rect_b)],
    )
}

pub fn object_widths(tome: &Tome) -> (u32, u32) {
    (tome.list_widths & 31, tome.list_widths >> 5 & 31)
}

pub fn cluster_widths(tome: &Tome) -> (u32, u32) {
    (tome.list_widths >> 10 & 31, tome.list_widths >> 15 & 31)
}

/// Expands the runs starting at entry `first_entry` until they cover `total`
/// items; `None` when the runs overshoot or leave the list.
pub fn expand_runs(
    list: &BitList,
    widths: (u32, u32),
    first_entry: u32,
    total: u32,
    out: &mut Vec<u32>,
) -> Option<()> {
    let mut covered = 0u32;
    let mut entry = first_entry;
    while covered < total {
        if entry >= list.count {
            return None;
        }
        let (first, count) = list_run(list, entry, widths.0, widths.1);
        if count == 0 {
            return None;
        }
        out.extend(first..first + count);
        covered += count;
        entry += 1;
    }
    (covered == total).then_some(())
}

/// Objects listed in one cell: `object_count` objects taken from the runs
/// that start at entry `object_index`.
pub fn cell_objects(tome: &Tome, tile: &Tile, cell: u32, out: &mut Vec<u32>) -> Option<()> {
    let node = tile.cell_nodes.as_ref()?.get(cell as usize)?;
    if node.object_count == 0 {
        return Some(());
    }
    expand_runs(
        tome.object_lists.as_ref()?,
        object_widths(tome),
        node.object_index,
        node.object_count,
        out,
    )
}

fn walkable(portal: &Portal) -> bool {
    !portal.is_outside() && !portal.is_hierarchy()
}

/// Every object in any leaf cell reachable from the cell at `point`, gates open.
pub fn visible_objects(tome: &Tome, point: [f32; 3]) -> Result<Option<Vec<u32>>> {
    let CellHit::Cell { tile, cell } = find_cell(tome, point)? else {
        return Ok(None);
    };
    let starts = tome.cell_starts.as_deref().unwrap_or(&[]);
    let total = starts.last().copied().unwrap_or(0) as usize;
    let global = |tile: u32, cell: u32| starts[tile as usize] as usize + cell as usize;
    let mut seen = vec![false; total];
    let mut queue = VecDeque::from([(tile, cell)]);
    seen[global(tile, cell)] = true;
    let mut objects = Vec::new();
    while let Some((t, c)) = queue.pop_front() {
        let tile = tome.tiles[t as usize].as_ref().unwrap();
        cell_objects(tome, tile, c, &mut objects)
            .ok_or_else(|| PrevisError::invalid("cell object runs"))?;
        let node = &tile.cell_nodes.as_ref().unwrap()[c as usize];
        let portals = tile.portals.as_deref().unwrap_or(&[]);
        for portal in
            &portals[node.portal_index as usize..(node.portal_index + node.portal_count) as usize]
        {
            if !walkable(portal) {
                continue;
            }
            let (nt, nc) = (portal.target(), portal.target_index as u32);
            if tome
                .tiles
                .get(nt as usize)
                .and_then(Option::as_ref)
                .is_none_or(|t| !t.is_leaf())
            {
                continue;
            }
            let g = global(nt, nc);
            if g < total && !seen[g] {
                seen[g] = true;
                queue.push_back((nt, nc));
            }
        }
    }
    objects.sort_unstable();
    objects.dedup();
    Ok(Some(objects))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_encoding_matches_retail_examples() {
        assert_eq!(encode_path(13, &[]), 0b1_0000_0000_0000);
        assert_eq!(encode_path(13, &[false]), 0b1_1000_0000_0000);
        assert_eq!(encode_path(13, &[true, false]), 0b1_0100_0000_0000);
        assert_eq!(encode_path(13, &[false, true]), 0b0_1100_0000_0000);
    }

    #[test]
    fn rank_lut_counts_inner_nodes_per_block() {
        let axes: Vec<u32> = (0..600)
            .map(|i| if i % 3 == 0 { LEAF } else { 0 })
            .collect();
        let stream = encode_node_stream(&axes);
        let tree = Tree {
            node_count: 600,
            data: Some(stream.clone()),
            ..Tree::default()
        };
        let view = KdView::new(&tree).unwrap();
        // Reconstruct lookup(j) for multiples of 16 from the table and compare.
        let lut = &stream[node_words(600)..];
        let (mid_offset, bottom_offset) = (0usize, (600usize >> 8).div_ceil(2));
        for j in (16..=600).step_by(16) {
            let mut value = 0;
            if j & 0xFF00 != 0 {
                let i = (j >> 8) - 1;
                value += lut[mid_offset + i / 2] >> ((i & 1) * 16) & 0xFFFF;
            }
            if j & 0xF0 != 0 {
                let q = j >> 4;
                let i = q - (q >> 4) - 1;
                value += lut[bottom_offset + i / 4] >> ((i & 3) * 8) & 0xFF;
            }
            assert_eq!(value, view.inner_through[j - 1], "j={j}");
        }
    }
}
