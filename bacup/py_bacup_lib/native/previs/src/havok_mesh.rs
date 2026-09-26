//! Havok 2014 `hknpCompressedMeshShape` model, source parsing, codecs and the
//! static-mesh-tree build CK's `GenerateCombinedShape` runs, reproducing
//! `tools/re/ck_havok_pairing_probe.py` operation for operation.
//!
//! Every value is float32 and every operation is rounded as CK's SSE code
//! rounds it. Python `min`/`max` keep the first of equal values, which matters
//! for signed zeros, so they are spelled out as [`first_min`]/[`first_max`].

use std::collections::HashMap;

use havok_native::hkx::packfile::parse_packfile;

use crate::error::{PrevisError, Result};
use crate::f32ops::Mat3;

pub type V3 = [f32; 3];
pub type Bounds = (V3, V3);

pub const HKX_MAGIC: [u8; 8] = [0x57, 0xE0, 0xE0, 0x57, 0x10, 0xC0, 0xC0, 0x10];
pub const GAME_TO_HAVOK_SCALE: f32 = f32::from_bits(0x3C6A_161E);
pub const HAVOK_TO_GAME_SCALE: f32 = f32::from_bits(0x428B_FB85);
const CODEC3AXIS_SCALE: f32 = f32::from_bits(0x3B90_FDBC);
/// `_f32(3.402820018375656e38)`, Havok's "no cost yet" sentinel.
pub(crate) const BIG_COST: f32 = f32::from_bits(0x7F7F_FFEE);
/// A compressed-mesh primitive slot Havok has marked removed.
pub const DEAD_PRIMITIVE: [u8; 4] = [0xDE, 0xAD, 0xDE, 0xAD];
/// CK's `m_triangleDegeneracyTolerance`.
const TRIANGLE_DEGENERACY_TOLERANCE: f32 = 1.0e-7;

#[inline]
pub(crate) fn first_min(a: f32, b: f32) -> f32 {
    if b < a { b } else { a }
}

#[inline]
pub(crate) fn first_max(a: f32, b: f32) -> f32 {
    if b > a { b } else { a }
}

fn min3(a: V3, b: V3) -> V3 {
    [0, 1, 2].map(|i| first_min(a[i], b[i]))
}

fn max3(a: V3, b: V3) -> V3 {
    [0, 1, 2].map(|i| first_max(a[i], b[i]))
}

/// Python `min(point[axis] for point in points)` per axis, over any order.
pub(crate) fn bounds_of<'a>(mut points: impl Iterator<Item = &'a V3>) -> Bounds {
    let first = *points.next().expect("bounds of an empty point set");
    points.fold((first, first), |(lo, hi), p| (min3(lo, *p), max3(hi, *p)))
}

pub(crate) fn bounds_of_indices(vertices: &[V3], indices: &[usize]) -> Bounds {
    bounds_of(indices.iter().map(|&i| &vertices[i]))
}

// ---------------------------------------------------------------------------
// Compressed-mesh model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct CustomPrimitive {
    pub primitive_index: usize,
    pub tree_node_index: usize,
    pub header: u16,
    pub custom_type: u8,
    pub compression_mode: u8,
    pub vertex_count: usize,
    pub first_shared_vertex: usize,
    pub tags: Vec<u16>,
    pub convex_radius: f32,
    pub vertices: Vec<V3>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Section {
    pub tree_minimum: V3,
    pub tree_maximum: V3,
    pub tree_minimum_w: f32,
    pub tree_maximum_w: f32,
    pub base: V3,
    pub scale: V3,
    pub packed_vertices: Vec<u32>,
    /// Decoded packed vertices, then one entry per shared-index slot.
    pub vertices: Vec<V3>,
    pub primitives: Vec<[u8; 4]>,
    /// `(value, first primitive, count)`.
    pub primitive_data_runs: Vec<(u16, u8, u8)>,
    pub tree_nodes: Vec<[u8; 4]>,
    /// Page-relative: the global shared-vertex index is `page << 16 | index`.
    pub shared_vertex_indices: Vec<u16>,
    pub leaf_index: u16,
    /// The 64K window of the shared-vertex stream this section indexes.
    pub page: u8,
    pub section_flags: u8,
    pub custom_primitives: Vec<CustomPrimitive>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompressedMesh {
    pub object_aabb_min: V3,
    pub object_aabb_max: V3,
    pub object_aabb_min_w: f32,
    pub object_aabb_max_w: f32,
    pub quad_is_flat_words: Vec<u32>,
    pub quad_is_flat_num_bits: u32,
    pub triangle_is_interior_words: Vec<u32>,
    pub triangle_is_interior_num_bits: u32,
    pub sections: Vec<Section>,
    pub convex_radius: f32,
    pub shape_user_data: u32,
    pub collision_filter_info: u32,
    /// Body cinfo bytes `0x20..0x60` (placement) when parsed from a packfile.
    pub body_cinfo_tail: Option<[u8; 0x40]>,
    pub material_entries: Vec<(u32, u32)>,
    pub shared_vertices: Vec<u64>,
    pub master_tree_nodes: Vec<[u8; 5]>,
    /// Shape keys counted before CK's bad-primitive pass, so still including
    /// the keys of primitives it overwrote; `None` counts the primitives.
    pub key_count: Option<u32>,
}

impl CompressedMesh {
    pub fn empty() -> Self {
        CompressedMesh {
            object_aabb_min: [0.0; 3],
            object_aabb_max: [0.0; 3],
            object_aabb_min_w: 0.0,
            object_aabb_max_w: 0.0,
            quad_is_flat_words: Vec::new(),
            quad_is_flat_num_bits: 0,
            triangle_is_interior_words: Vec::new(),
            triangle_is_interior_num_bits: 0,
            sections: Vec::new(),
            convex_radius: 0.0,
            shape_user_data: 0,
            collision_filter_info: 1,
            body_cinfo_tail: None,
            material_entries: Vec::new(),
            shared_vertices: Vec::new(),
            master_tree_nodes: Vec::new(),
            key_count: None,
        }
    }

    pub fn primitive_count(&self) -> usize {
        self.sections.iter().map(|s| s.primitives.len()).sum()
    }
}

// ---------------------------------------------------------------------------
// Placement rotation
// ---------------------------------------------------------------------------

/// The row-major rotation CK applies to collision: the visual rotation
/// converted to a quaternion and back, rounded at every step.
///
/// Only the positive-trace branch is recovered from CK; the others use the
/// standard largest-diagonal branch and are not yet verified against a capture.
pub fn physics_rotation(m: &Mat3) -> Mat3 {
    let trace = m[0][0] + m[1][1] + m[2][2];
    let (x, y, z, w);
    if trace > 0.0 {
        let root = (trace + 1.0).sqrt();
        let inverse = 0.5 / root;
        x = (m[1][2] - m[2][1]) * inverse;
        y = (m[2][0] - m[0][2]) * inverse;
        z = (m[0][1] - m[1][0]) * inverse;
        w = root * 0.5;
    } else {
        // `a(r, c)` is the column-vector matrix the quaternion encodes.
        let a = |r: usize, c: usize| m[c][r];
        let mut i = 0;
        if a(1, 1) > a(0, 0) {
            i = 1;
        }
        if a(2, 2) > a(i, i) {
            i = 2;
        }
        let j = (i + 1) % 3;
        let k = (j + 1) % 3;
        let root = (a(i, i) - (a(j, j) + a(k, k)) + 1.0).sqrt();
        let inverse = 0.5 / root;
        let mut q = [0.0f32; 3];
        q[i] = root * 0.5;
        q[j] = (a(j, i) + a(i, j)) * inverse;
        q[k] = (a(k, i) + a(i, k)) * inverse;
        w = (a(k, j) - a(j, k)) * inverse;
        [x, y, z] = q;
    }
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (xx, xy, xz) = (x * x2, x * y2, x * z2);
    let (yy, yz, zz) = (y * y2, y * z2, z * z2);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);
    [
        [1.0 - zz - yy, wz + xy, xz - wy],
        [xy - wz, 1.0 - zz - xx, wx + yz],
        [xz + wy, yz - wx, 1.0 - xx - yy],
    ]
}

// ---------------------------------------------------------------------------
// Dynamic SAH tree
// ---------------------------------------------------------------------------

pub(crate) const NONE: usize = usize::MAX;

#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub min: V3,
    pub max: V3,
    pub payload: Option<usize>,
    pub left: usize,
    pub right: usize,
    pub parent: usize,
}

impl Node {
    fn leaf(bounds: Bounds, payload: Option<usize>) -> Node {
        Node {
            min: bounds.0,
            max: bounds.1,
            payload,
            left: NONE,
            right: NONE,
            parent: NONE,
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.left == NONE
    }

    fn area(&self) -> f32 {
        area(self.min, self.max)
    }
}

fn area(min: V3, max: V3) -> f32 {
    let e = [0, 1, 2].map(|a| max[a] - min[a]);
    e[1] * e[2] + e[0] * e[1] + e[2] * e[0]
}

/// An arena of SAH nodes; indices stand in for the prototype's object identity.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tree {
    pub nodes: Vec<Node>,
}

impl Tree {
    fn push(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn union(&self, a: usize, b: usize) -> Bounds {
        let (a, b) = (&self.nodes[a], &self.nodes[b]);
        (min3(a.min, b.min), max3(a.max, b.max))
    }

    fn union_area(&self, a: usize, b: usize) -> f32 {
        let (min, max) = self.union(a, b);
        area(min, max)
    }

    fn set_children(&mut self, node: usize, left: usize, right: usize) {
        self.nodes[node].left = left;
        self.nodes[node].right = right;
        self.nodes[left].parent = node;
        self.nodes[right].parent = node;
        let (min, max) = self.union(left, right);
        self.nodes[node].min = min;
        self.nodes[node].max = max;
    }

    fn insertion_metric(&self, leaf: usize, child: usize) -> f32 {
        let (l, c) = (&self.nodes[leaf], &self.nodes[child]);
        let le = [0, 1, 2].map(|a| l.max[a] - l.min[a]);
        let ce = [0, 1, 2].map(|a| c.max[a] - c.min[a]);
        let lc = [0, 1, 2].map(|a| l.max[a] + l.min[a]);
        let cc = [0, 1, 2].map(|a| c.max[a] + c.min[a]);
        let extent_sum = ((ce[0] + le[0]) + (ce[1] + le[1])) + (ce[2] + le[2]);
        let mut distance = 0.0f32;
        for a in 0..3 {
            let d = cc[a] - lc[a];
            distance += d * d;
        }
        distance * extent_sum
    }

    fn insert_leaf(&mut self, root: usize, leaf: usize) -> usize {
        if root == NONE {
            return leaf;
        }
        let mut cursor = root;
        while !self.nodes[cursor].is_leaf() {
            let (min, max) = self.union(cursor, leaf);
            self.nodes[cursor].min = min;
            self.nodes[cursor].max = max;
            let (left, right) = (self.nodes[cursor].left, self.nodes[cursor].right);
            cursor = if self.insertion_metric(leaf, left) > self.insertion_metric(leaf, right) { right } else { left };
        }
        let old_parent = self.nodes[cursor].parent;
        let bounds = self.union(cursor, leaf);
        let replacement = self.push(Node {
            left: cursor,
            right: leaf,
            parent: old_parent,
            ..Node::leaf(bounds, None)
        });
        self.nodes[cursor].parent = replacement;
        self.nodes[leaf].parent = replacement;
        if old_parent == NONE {
            return replacement;
        }
        if self.nodes[old_parent].left == cursor {
            self.nodes[old_parent].left = replacement;
        } else {
            self.nodes[old_parent].right = replacement;
        }
        root
    }

    /// Leaves in left-first order.
    pub fn leaves(&self, root: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            let n = &self.nodes[node];
            if n.is_leaf() {
                out.push(node);
            } else {
                stack.push(n.right);
                stack.push(n.left);
            }
        }
        out
    }

    pub fn leaf_payloads(&self, root: usize) -> Vec<usize> {
        self.leaves(root).into_iter().filter_map(|n| self.nodes[n].payload).collect()
    }

    fn merge_small_branch(&mut self, nodes: &[usize]) -> usize {
        let mut working = nodes.to_vec();
        while working.len() > 1 {
            let mut best_cost = BIG_COST;
            let (mut best_first, mut best_second) = (NONE, NONE);
            for first in 0..working.len() {
                for second in first + 1..working.len() {
                    let cost = self.union_area(working[first], working[second]);
                    if cost < best_cost {
                        best_cost = cost;
                        best_first = first;
                        best_second = second;
                    }
                }
            }
            let (left, right) = (working[best_first], working[best_second]);
            let bounds = self.union(left, right);
            let parent = self.push(Node {
                left,
                right,
                ..Node::leaf(bounds, None)
            });
            self.nodes[left].parent = parent;
            self.nodes[right].parent = parent;
            let last = working.pop().unwrap();
            if best_second < working.len() {
                working[best_second] = last;
            }
            working[best_first] = parent;
        }
        working[0]
    }

    fn axis_center(&self, node: usize, axis: usize) -> f32 {
        let n = &self.nodes[node];
        (n.min[axis] + n.max[axis]) * 0.5
    }

    fn sort_by_center(&self, nodes: &mut [usize], axis: usize) {
        havok_quicksort(nodes, |&n| self.axis_center(n, axis), |a: &f32, b: &f32| a < b);
    }

    fn rebuild_branch(&mut self, nodes: Vec<usize>, force_split: bool) -> usize {
        const BIN_COUNT: usize = 32;
        const SMALL_BRANCH_LIMIT: usize = 16;
        if nodes.len() <= SMALL_BRANCH_LIMIT && !force_split {
            return self.merge_small_branch(&nodes);
        }
        let (overall_min, overall_max) = {
            let first = &self.nodes[nodes[0]];
            let mut lo = first.min;
            let mut hi = first.max;
            for &n in &nodes[1..] {
                lo = min3(lo, self.nodes[n].min);
                hi = max3(hi, self.nodes[n].max);
            }
            (lo, hi)
        };
        let mut axis_bins: Vec<Vec<Vec<usize>>> = vec![vec![Vec::new(); BIN_COUNT]; 3];
        let scales: [f32; 3] = [0, 1, 2].map(|axis| {
            let extent = overall_max[axis] - overall_min[axis];
            if extent == 0.0 { 0.0 } else { (1.0 / extent) * BIN_COUNT as f32 }
        });
        for &node in &nodes {
            for axis in 0..3 {
                let coordinate = (self.axis_center(node, axis) - overall_min[axis]) * scales[axis] + 0.5;
                let clamped = first_max(0.0, first_min((BIN_COUNT - 1) as f32, coordinate));
                axis_bins[axis][clamped as usize].push(node);
            }
        }

        let mut best_cost = BIG_COST;
        let (mut best_axis, mut best_bin) = (NONE, NONE);
        for (axis, bins) in axis_bins.iter().enumerate() {
            let occupied: Vec<usize> = (0..BIN_COUNT).filter(|&i| !bins[i].is_empty()).collect();
            let (first_occupied, last_occupied) = (occupied[0], *occupied.last().unwrap());
            let mut costs = [0.0f32; BIN_COUNT];
            let mut prefix: Option<(Bounds, usize)> = None;
            let mut suffix: Option<(Bounds, usize)> = None;
            for step in 0..=last_occupied - first_occupied {
                let forward = first_occupied + step;
                let backward = last_occupied - step;
                prefix = self.extend_bounds(prefix, &bins[forward]);
                suffix = self.extend_bounds(suffix, &bins[backward]);
                if let Some(((lo, hi), count)) = prefix {
                    costs[forward] += count as f32 * area(lo, hi);
                }
                if let Some(((lo, hi), count)) = suffix {
                    costs[backward] += count as f32 * area(lo, hi);
                }
            }
            for bin in first_occupied..=last_occupied {
                if !bins[bin].is_empty() && costs[bin] < best_cost {
                    best_cost = costs[bin];
                    best_axis = axis;
                    best_bin = bin;
                }
            }
        }

        let selected = &axis_bins[best_axis];
        let mut left_nodes: Vec<usize> = selected[..best_bin].iter().flatten().copied().collect();
        let mut right_nodes: Vec<usize> = selected[best_bin + 1..].iter().flatten().copied().collect();
        let mut middle = selected[best_bin].clone();
        match (left_nodes.is_empty(), right_nodes.is_empty()) {
            (false, true) => right_nodes.extend(middle),
            (true, false) => left_nodes.extend(middle),
            _ => {
                // The occupied bin is center-sorted only when it must be divided.
                self.sort_by_center(&mut middle, best_axis);
                let midpoint = middle.len() / 2;
                left_nodes.extend_from_slice(&middle[..midpoint]);
                right_nodes.extend_from_slice(&middle[midpoint..]);
            }
        }
        let left = self.rebuild_branch(left_nodes, false);
        let right = self.rebuild_branch(right_nodes, false);
        let parent = self.push(Node {
            left,
            right,
            ..Node::leaf((overall_min, overall_max), None)
        });
        self.nodes[left].parent = parent;
        self.nodes[right].parent = parent;
        parent
    }

    /// Running union of an appended node list (equal to recomputing it).
    fn extend_bounds(&self, current: Option<(Bounds, usize)>, added: &[usize]) -> Option<(Bounds, usize)> {
        let mut current = current;
        for &n in added {
            let node = &self.nodes[n];
            current = Some(match current {
                None => ((node.min, node.max), 1),
                Some(((lo, hi), count)) => ((min3(lo, node.min), max3(hi, node.max)), count + 1),
            });
        }
        current
    }

    /// Descendants of `root` in left-first preorder, reading each node's
    /// children only after `visit` has run on it.
    fn visit_descendants(&mut self, root: usize, mut visit: impl FnMut(&mut Tree, usize)) {
        let mut pending = Vec::new();
        let r = &self.nodes[root];
        if r.right != NONE {
            pending.push(r.right);
        }
        if r.left != NONE {
            pending.push(r.left);
        }
        while let Some(node) = pending.pop() {
            visit(self, node);
            let n = &self.nodes[node];
            if n.right != NONE {
                pending.push(n.right);
            }
            if n.left != NONE {
                pending.push(n.left);
            }
        }
    }

    fn optimize_descendants(&mut self, root: usize) {
        loop {
            let mut changed = false;
            self.visit_descendants(root, |tree, node| {
                let n = &tree.nodes[node];
                if n.is_leaf() {
                    return;
                }
                let (left, right) = (n.left, n.right);
                if tree.nodes[left].is_leaf() || tree.nodes[right].is_leaf() {
                    return;
                }
                let (ll, lr) = (tree.nodes[left].left, tree.nodes[left].right);
                let (rl, rr) = (tree.nodes[right].left, tree.nodes[right].right);
                let current = tree.nodes[left].area() + tree.nodes[right].area();
                let straight = tree.union_area(ll, rl) + tree.union_area(lr, rr);
                let crossed = tree.union_area(ll, rr) + tree.union_area(lr, rl);
                let use_crossed = straight >= crossed;
                let best = if use_crossed { crossed } else { straight };
                if best >= current {
                    return;
                }
                if use_crossed {
                    tree.set_children(left, ll, rr);
                    tree.set_children(right, lr, rl);
                } else {
                    tree.set_children(left, ll, rl);
                    tree.set_children(right, lr, rr);
                }
                changed = true;
            });
            if !changed {
                break;
            }
        }
        self.visit_descendants(root, |tree, node| {
            let n = &tree.nodes[node];
            if n.is_leaf() {
                return;
            }
            let (left, right) = (n.left, n.right);
            if tree.nodes[right].area() > tree.nodes[left].area() {
                tree.nodes[node].left = right;
                tree.nodes[node].right = left;
            }
        });
    }

    fn preorder(&self, root: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            out.push(node);
            let n = &self.nodes[node];
            if !n.is_leaf() {
                stack.push(n.right);
                stack.push(n.left);
            }
        }
        out
    }

    /// A detached copy of the subtree at `node`, leaves mapped by `leaf_payload`.
    fn copy_subtree(&self, node: usize, target: &mut Tree, leaf_payload: &dyn Fn(&Node) -> Option<usize>) -> usize {
        let n = &self.nodes[node];
        if n.is_leaf() {
            return target.push(Node::leaf((n.min, n.max), leaf_payload(n)));
        }
        let copy = target.push(Node::leaf((n.min, n.max), None));
        let left = self.copy_subtree(n.left, target, leaf_payload);
        let right = self.copy_subtree(n.right, target, leaf_payload);
        target.set_children(copy, left, right);
        copy
    }
}

/// Havok's in-place Hoare quicksort: middle pivot, strict comparisons both
/// ways, equal elements still swapped. Unstable, and CK's tie order depends on it.
pub(crate) fn havok_quicksort<T, K>(items: &mut [T], key: impl Fn(&T) -> K + Copy, less: impl Fn(&K, &K) -> bool + Copy) {
    fn sort<T, K>(
        items: &mut [T],
        mut first: isize,
        last: isize,
        key: impl Fn(&T) -> K + Copy,
        less: impl Fn(&K, &K) -> bool + Copy,
    ) {
        while first < last {
            let (mut left, mut right) = (first, last);
            let pivot = key(&items[((first + last) / 2) as usize]);
            while left <= right {
                while less(&key(&items[left as usize]), &pivot) {
                    left += 1;
                }
                while less(&pivot, &key(&items[right as usize])) {
                    right -= 1;
                }
                if left > right {
                    break;
                }
                if left != right {
                    items.swap(left as usize, right as usize);
                }
                left += 1;
                right -= 1;
            }
            if first < right {
                sort(items, first, right, key, less);
            }
            first = left;
        }
    }
    if items.len() > 1 {
        let last = items.len() as isize - 1;
        sort(items, 0, last, key, less);
    }
}

/// CK's dynamic tree: incremental insertion, then (for three or more leaves)
/// a forced 32-bin SAH root split and local rotation passes.
pub(crate) fn sah_tree_for_bounds(bounds: &[Bounds]) -> (Tree, usize) {
    // Insertion makes 2n - 1 nodes and the root rebuild n - 1 more.
    let mut tree = Tree { nodes: Vec::with_capacity(3 * bounds.len()) };
    let mut root = NONE;
    for (payload, &b) in bounds.iter().enumerate() {
        let leaf = tree.push(Node::leaf(b, Some(payload)));
        root = tree.insert_leaf(root, leaf);
    }
    assert!(root != NONE, "SAH tree needs at least one leaf");
    if bounds.len() < 3 {
        return (tree, root);
    }
    let leaves = tree.leaves(root);
    let rebuilt = tree.rebuild_branch(leaves, true);
    tree.nodes[rebuilt].parent = NONE;
    tree.optimize_descendants(rebuilt);
    (tree, rebuilt)
}

pub(crate) struct Partition {
    /// Detached section trees with section-local leaf payloads.
    pub sections: Vec<(Tree, usize)>,
    /// Master tree whose leaves carry section indices.
    pub master: (Tree, usize),
    /// Global payloads of each section in leaf order.
    pub payloads: Vec<Vec<usize>>,
}

/// Splits sections while they hold at least 128 leaves or 256 distinct
/// vertex tokens (`add_leaf_tokens` adds a leaf payload's), then detaches
/// them under a master tree.
pub(crate) fn partition_sections(
    tree: &Tree,
    root: usize,
    add_leaf_tokens: impl Fn(usize, &mut rustc_hash::FxHashSet<usize>),
) -> Result<Partition> {
    let mut roots = vec![root];
    let mut index = 0;
    let mut distinct = rustc_hash::FxHashSet::default();
    while index < roots.len() {
        let section_root = roots[index];
        let payloads = tree.leaf_payloads(section_root);
        distinct.clear();
        for &payload in &payloads {
            add_leaf_tokens(payload, &mut distinct);
        }
        if payloads.len() >= 128 || distinct.len() >= 256 {
            let n = &tree.nodes[section_root];
            if n.is_leaf() {
                return Err(PrevisError::unsupported("an over-limit collision section cannot be split"));
            }
            roots[index] = n.left;
            roots.push(n.right);
            continue;
        }
        index += 1;
    }
    let section_of: HashMap<usize, usize> = roots.iter().enumerate().map(|(i, &r)| (r, i)).collect();
    let payloads: Vec<Vec<usize>> = roots.iter().map(|&r| tree.leaf_payloads(r)).collect();

    let mut sections = Vec::with_capacity(roots.len());
    for (&section_root, section_payloads) in roots.iter().zip(&payloads) {
        let local: HashMap<usize, usize> = section_payloads.iter().enumerate().map(|(i, &p)| (p, i)).collect();
        let mut detached = Tree::default();
        let detached_root = tree.copy_subtree(section_root, &mut detached, &|n: &Node| n.payload.map(|p| local[&p]));
        sections.push((detached, detached_root));
    }

    fn collapse(tree: &Tree, node: usize, section_of: &HashMap<usize, usize>, target: &mut Tree) -> Result<usize> {
        let n = &tree.nodes[node];
        if let Some(&section) = section_of.get(&node) {
            return Ok(target.push(Node::leaf((n.min, n.max), Some(section))));
        }
        if n.is_leaf() {
            return Err(PrevisError::invalid("collision section roots do not cover the tree"));
        }
        let collapsed = target.push(Node::leaf((n.min, n.max), None));
        let left = collapse(tree, n.left, section_of, target)?;
        let right = collapse(tree, n.right, section_of, target)?;
        target.set_children(collapsed, left, right);
        Ok(collapsed)
    }
    let mut master = Tree::default();
    let master_root = collapse(tree, root, &section_of, &mut master)?;
    Ok(Partition {
        sections,
        master: (master, master_root),
        payloads,
    })
}

/// `buildStep3`: unions each leaf with its quantized bounds, clamps to the
/// root domain, and propagates only expansions through the parents.
pub(crate) fn refit_for_leaf_bounds(tree: &mut Tree, root: usize, bounds: &[Option<Bounds>]) {
    let (domain_min, domain_max) = (tree.nodes[root].min, tree.nodes[root].max);
    fn refit(tree: &mut Tree, node: usize, bounds: &[Option<Bounds>], domain_min: V3, domain_max: V3) {
        let n = &tree.nodes[node];
        if let Some(payload) = n.payload {
            let Some((qmin, qmax)) = bounds[payload] else { return };
            let (min, max) = (n.min, n.max);
            tree.nodes[node].min = [0, 1, 2].map(|a| first_max(domain_min[a], first_min(min[a], qmin[a])));
            tree.nodes[node].max = [0, 1, 2].map(|a| first_min(domain_max[a], first_max(max[a], qmax[a])));
            return;
        }
        let (left, right) = (n.left, n.right);
        refit(tree, left, bounds, domain_min, domain_max);
        refit(tree, right, bounds, domain_min, domain_max);
        let (min, max) = tree.union(left, right);
        tree.nodes[node].min = min;
        tree.nodes[node].max = max;
    }
    refit(tree, root, bounds, domain_min, domain_max);
}

// ---------------------------------------------------------------------------
// Static AABB trees
// ---------------------------------------------------------------------------

fn unpack_codec_axis(minimum: f32, maximum: f32, packed: u8) -> (f32, f32) {
    let scale = (maximum - minimum) * CODEC3AXIS_SCALE;
    let lower = (packed >> 4) as f32;
    let upper = (packed & 0x0F) as f32;
    (minimum + lower * lower * scale, maximum - upper * upper * scale)
}

pub(crate) fn unpack_codec3axis(minimum: V3, maximum: V3, packed: [u8; 3]) -> Bounds {
    let d = [0, 1, 2].map(|a| unpack_codec_axis(minimum[a], maximum[a], packed[a]));
    (d.map(|x| x.0), d.map(|x| x.1))
}

fn pack_codec3axis(domain_min: V3, domain_max: V3, target_min: V3, target_max: V3) -> [u8; 3] {
    [0, 1, 2].map(|axis| {
        let mut value: u8 = 0;
        while value & 0xF0 < 0xF0 {
            let previous = value;
            value += 0x10;
            let (decoded_min, _) = unpack_codec_axis(domain_min[axis], domain_max[axis], value);
            if target_min[axis] < decoded_min {
                value = previous;
                break;
            }
        }
        while value & 0x0F < 0x0F {
            let previous = value;
            value += 1;
            let (_, decoded_max) = unpack_codec_axis(domain_min[axis], domain_max[axis], value);
            if target_max[axis] > decoded_max {
                value = previous;
                break;
            }
        }
        value
    })
}

/// Preorder codec bytes; each node is encoded against its parent's decoded box.
fn static_codec_nodes(tree: &Tree, root: usize) -> (Vec<usize>, Vec<[u8; 3]>) {
    let preorder = tree.preorder(root);
    let mut decoded: HashMap<usize, Bounds> = HashMap::with_capacity(preorder.len());
    let mut packed = Vec::with_capacity(preorder.len());
    for &node in &preorder {
        let n = &tree.nodes[node];
        let (domain_min, domain_max) = if node == root || n.parent == NONE {
            (tree.nodes[root].min, tree.nodes[root].max)
        } else {
            decoded[&n.parent]
        };
        let bytes = pack_codec3axis(domain_min, domain_max, n.min, n.max);
        decoded.insert(node, unpack_codec3axis(domain_min, domain_max, bytes));
        packed.push(bytes);
    }
    (preorder, packed)
}

pub(crate) fn static_aabb4_nodes(tree: &Tree, root: usize, leaf_payloads: Option<&[usize]>) -> Vec<[u8; 4]> {
    let (preorder, packed) = static_codec_nodes(tree, root);
    let index: HashMap<usize, usize> = preorder.iter().enumerate().map(|(i, &n)| (n, i)).collect();
    preorder
        .iter()
        .zip(packed)
        .enumerate()
        .map(|(i, (&node, bytes))| {
            let n = &tree.nodes[node];
            let data = if n.is_leaf() {
                let payload = n.payload.expect("static tree leaf payload");
                (leaf_payloads.map_or(payload, |p| p[payload]) << 1) as u8
            } else {
                ((index[&n.right] - i) | 1) as u8
            };
            [bytes[0], bytes[1], bytes[2], data]
        })
        .collect()
}

pub(crate) fn static_aabb5_nodes(tree: &Tree, root: usize) -> Result<Vec<[u8; 5]>> {
    let (preorder, packed) = static_codec_nodes(tree, root);
    let index: HashMap<usize, usize> = preorder.iter().enumerate().map(|(i, &n)| (n, i)).collect();
    preorder
        .iter()
        .zip(packed)
        .enumerate()
        .map(|(i, (&node, bytes))| {
            let n = &tree.nodes[node];
            let data = if n.is_leaf() {
                n.payload.expect("static tree leaf payload")
            } else {
                let delta = index[&n.right] - i;
                if delta & 1 != 0 {
                    return Err(PrevisError::invalid("five-byte tree right-child delta must be even"));
                }
                0x8000 | (delta >> 1)
            };
            Ok([bytes[0], bytes[1], bytes[2], (data >> 8) as u8, data as u8])
        })
        .collect()
}

pub(crate) fn static_aabb4_node_bounds(root_min: V3, root_max: V3, nodes: &[[u8; 4]]) -> Result<Vec<Bounds>> {
    if nodes.is_empty() {
        return Ok(Vec::new());
    }
    let mut bounds: Vec<Option<Bounds>> = vec![None; nodes.len()];
    let mut stack = vec![(0usize, root_min, root_max)];
    while let Some((index, domain_min, domain_max)) = stack.pop() {
        if index >= nodes.len() {
            return Err(PrevisError::invalid("collision tree child is out of range"));
        }
        if bounds[index].is_some() {
            return Err(PrevisError::invalid("collision tree node is reachable twice"));
        }
        let decoded = unpack_codec3axis(domain_min, domain_max, [nodes[index][0], nodes[index][1], nodes[index][2]]);
        bounds[index] = Some(decoded);
        let data = nodes[index][3] as usize;
        if data & 1 != 0 {
            stack.push((index + (data & 0xFE), decoded.0, decoded.1));
            stack.push((index + 1, decoded.0, decoded.1));
        }
    }
    bounds
        .into_iter()
        .map(|b| b.ok_or_else(|| PrevisError::invalid("collision tree has unreachable nodes")))
        .collect()
}

// ---------------------------------------------------------------------------
// Vertex codecs
// ---------------------------------------------------------------------------

fn quantize(minimum: f32, maximum: f32, value: f32, limit: u32) -> u64 {
    let extent = maximum - minimum;
    let normalized = if extent == 0.0 { 0.0 } else { (value - minimum) / extent };
    let normalized = first_max(0.0, first_min(1.0, normalized));
    (normalized * limit as f32 + 0.5) as u64
}

pub(crate) fn encode_vertex_11_11_10(minimum: V3, maximum: V3, vertex: V3) -> u32 {
    let q = [(0, 0x7FF), (1, 0x7FF), (2, 0x3FF)].map(|(a, l)| quantize(minimum[a], maximum[a], vertex[a], l));
    (q[0] | q[1] << 11 | q[2] << 22) as u32
}

pub(crate) fn decode_vertex_11_11_10(minimum: V3, scale: V3, packed: u32) -> V3 {
    let q = [packed & 0x7FF, packed >> 11 & 0x7FF, packed >> 22];
    [0, 1, 2].map(|a| minimum[a] + q[a] as f32 * scale[a])
}

fn encode_vertex_5_5_6(minimum: V3, maximum: V3, vertex: V3) -> u64 {
    let q = [(0, 0x1F), (1, 0x1F), (2, 0x3F)].map(|(a, l)| quantize(minimum[a], maximum[a], vertex[a], l));
    q[0] | q[1] << 5 | q[2] << 10
}

fn decode_vertex_5_5_6(minimum: V3, maximum: V3, packed: u64) -> V3 {
    let q = [packed & 0x1F, packed >> 5 & 0x1F, packed >> 10];
    let scales = [0, 1, 2].map(|a| (maximum[a] - minimum[a]) * (1.0 / if a == 2 { 63.0f64 } else { 31.0 }) as f32);
    [0, 1, 2].map(|a| minimum[a] + q[a] as f32 * scales[a])
}

pub(crate) fn encode_vertex_21_21_22(minimum: V3, maximum: V3, vertex: V3) -> u64 {
    let q = [(0, 0x1F_FFFF), (1, 0x1F_FFFF), (2, 0x3F_FFFF)].map(|(a, l)| quantize(minimum[a], maximum[a], vertex[a], l));
    q[0] | q[1] << 21 | q[2] << 42
}

pub(crate) fn decode_vertex_21_21_22(minimum: V3, maximum: V3, packed: u64) -> V3 {
    let scales = [
        (maximum[0] - minimum[0]) * 4.768373855768004e-07_f64 as f32,
        (maximum[1] - minimum[1]) * 4.768373855768004e-07_f64 as f32,
        (maximum[2] - minimum[2]) * 2.3841863594498136e-07_f64 as f32,
    ];
    [0, 1, 2].map(|a| {
        let q = packed >> (a * 21) & if a == 2 { 0x3F_FFFF } else { 0x1F_FFFF };
        minimum[a] + q as f32 * scales[a]
    })
}

pub(crate) fn encode_custom_vertices(
    mode: u8,
    vertices: &[V3],
    object: Bounds,
    leaf: Bounds,
) -> Result<Vec<u64>> {
    match mode {
        0 => Ok(vertices.iter().map(|v| encode_vertex_21_21_22(object.0, object.1, *v)).collect()),
        1 => {
            let mut packed: Vec<u64> = vertices.iter().map(|v| encode_vertex_11_11_10(leaf.0, leaf.1, *v) as u64).collect();
            packed.resize(packed.len().div_ceil(2) * 2, 0);
            Ok(packed.chunks(2).map(|c| c[0] << 32 | c[1]).collect())
        }
        2 => {
            let mut packed: Vec<u64> = vertices.iter().map(|v| encode_vertex_5_5_6(leaf.0, leaf.1, *v)).collect();
            packed.resize(packed.len().div_ceil(4) * 4, 0);
            Ok(packed.chunks(4).map(|c| c[0] << 48 | c[1] << 32 | c[2] << 16 | c[3]).collect())
        }
        other => Err(PrevisError::unsupported(format!("custom compression mode {other} is not recovered"))),
    }
}

pub(crate) fn decode_custom_vertices(
    mode: u8,
    vertex_count: usize,
    first: usize,
    shared: &[u64],
    object: Bounds,
    leaf: Bounds,
) -> Result<Vec<V3>> {
    let count = vertex_count.min(0x70);
    let words = |per_word: usize| -> Result<&[u64]> {
        let end = first + count.div_ceil(per_word);
        shared.get(first..end).ok_or_else(|| PrevisError::invalid("custom-vertex stream is out of bounds"))
    };
    match mode {
        0 => Ok(words(1)?.iter().map(|&p| decode_vertex_21_21_22(object.0, object.1, p)).collect()),
        1 => {
            let scale = [0, 1, 2].map(|a| (leaf.1[a] - leaf.0[a]) * (1.0 / if a == 2 { 1023.0f64 } else { 2047.0 }) as f32);
            Ok(words(2)?
                .iter()
                .flat_map(|&w| [(w >> 32) as u32, w as u32])
                .take(count)
                .map(|p| decode_vertex_11_11_10(leaf.0, scale, p))
                .collect())
        }
        2 => Ok(words(4)?
            .iter()
            .flat_map(|&w| [w >> 48, w >> 32 & 0xFFFF, w >> 16 & 0xFFFF, w & 0xFFFF])
            .take(count)
            .map(|p| decode_vertex_5_5_6(leaf.0, leaf.1, p))
            .collect()),
        other => Err(PrevisError::unsupported(format!("custom compression mode {other} is not recovered"))),
    }
}

pub(crate) fn encode_radius_tag(radius: f32) -> u16 {
    ((radius * 1.00390625).to_bits() >> 16) as u16
}

fn decode_radius_tag(tag: u16) -> f32 {
    f32::from_bits((tag as u32) << 16)
}

// ---------------------------------------------------------------------------
// Flat-quad and interior-triangle flags
// ---------------------------------------------------------------------------

fn dot3(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub3(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross3(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize3(v: V3) -> V3 {
    let length_squared = dot3(v, v);
    if length_squared <= 0.0 {
        return [0.0; 3];
    }
    let inverse = 1.0 / length_squared.sqrt();
    v.map(|c| c * inverse)
}

/// `hkcdTriangleUtil::checkForFlatConvexQuad`. The tolerance is the
/// prototype's float64 `0.01`, so comparisons happen in float64.
fn is_flat_convex_quad(first: V3, second: V3, third: V3, fourth: V3) -> bool {
    const TOLERANCE: f64 = 0.01;
    let first_edge = sub3(second, first);
    let normal = normalize3(cross3(first_edge, sub3(third, first)));
    let first_to_fourth = sub3(first, fourth);
    if dot3(first_to_fourth, normal).abs() as f64 > TOLERANCE {
        return false;
    }
    let first_side = dot3(normalize3(cross3(normal, first_edge)), first_to_fourth);
    let third_edge = sub3(third, second);
    let third_side = dot3(normalize3(cross3(normal, third_edge)), sub3(third, fourth));
    (first_max(first_side, third_side) as f64) < TOLERANCE
}

pub(crate) fn flat_convex_quad_words(section: &Section) -> Vec<u32> {
    let mut words = vec![0u32; section.primitives.len().div_ceil(32)];
    for (index, p) in section.primitives.iter().enumerate() {
        if p[2] == p[3] || *p == DEAD_PRIMITIVE {
            continue;
        }
        let v = |i: usize| section.vertices[p[i] as usize];
        if is_flat_convex_quad(v(0), v(1), v(2), v(3)) {
            words[index >> 5] |= 1 << (index & 31);
        }
    }
    words
}

fn interior_dot3(a: V3, b: V3) -> f32 {
    a[1] * b[1] + a[0] * b[0] + a[2] * b[2]
}

fn interior_normalize3(v: V3) -> V3 {
    let length_squared = interior_dot3(v, v);
    if length_squared <= 0.0 {
        return [0.0; 3];
    }
    let inverse = 1.0 / length_squared.sqrt();
    v.map(|c| c * inverse)
}

/// Havok's SIMD cosine polynomial.
fn havok_cosine(value: f32) -> f32 {
    let four_over_pi = 1.2732394933700562_f64 as f32;
    let dp = [-0.78515625_f64 as f32, -0.00024187564849853516_f64 as f32, -3.774894977445975e-08_f64 as f32];
    let sin_c = [-0.00019515295571181923_f64 as f32, 0.008332161232829094_f64 as f32, -0.16666655242443085_f64 as f32];
    let cos_c = [2.44331567955669e-05_f64 as f32, -0.0013887316454201937_f64 as f32, 0.04166664555668831_f64 as f32];
    let absolute = value.abs();
    let mut quadrant = (((absolute * four_over_pi) as i64) + 1) & !1;
    let q = quadrant as f32;
    let reduced = ((q * dp[0] + absolute) + q * dp[1]) + q * dp[2];
    let squared = reduced * reduced;
    let sine = ((sin_c[0] * squared + sin_c[1]) * squared + sin_c[2]) * squared * reduced + reduced;
    let cosine = ((cos_c[0] * squared + cos_c[1]) * squared + cos_c[2]) * squared * squared - squared * 0.5 + 1.0;
    quadrant -= 2;
    let result = first_min(if quadrant & 2 == 0 { sine } else { cosine }, 1.0);
    if !quadrant & 4 != 0 { -result } else { result }
}

/// A triangle or flat quad of the final mesh. Its edge `e` belongs to leaf
/// `key`, except a flat quad's last two edges, which belong to leaf `key + 1`.
struct InteriorPolygon {
    key: u32,
    section: u32,
    vertices: [u8; 4],
    vertex_count: u8,
    normal: V3,
    /// A flat quad's second leaf normal.
    second_normal: V3,
    twice_area: f32,
}

impl InteriorPolygon {
    fn edge_normal(&self, edge: usize) -> V3 {
        if self.vertex_count == 4 && edge >= 2 { self.second_normal } else { self.normal }
    }

    fn edge_key(&self, edge: usize) -> u32 {
        self.key + u32::from(self.vertex_count == 4 && edge >= 2)
    }
}

/// A section vertex's identity across sections: `(section + 1, index)` for a
/// packed vertex, `(0, shared index)` for a shared one, packed as `high << 16 | low`.
fn interior_vertex_id(section_index: usize, section: &Section, vertex: u8) -> u32 {
    let i = vertex as usize;
    if i < section.packed_vertices.len() || section.shared_vertex_indices.is_empty() {
        (section_index as u32 + 1) << 16 | i as u32
    } else {
        section.shared_vertex_indices[i - section.packed_vertices.len()] as u32
    }
}

const NO_LINK: u32 = u32::MAX;

/// `flagInteriorTriangles(70 degrees)` over the final mesh.
pub(crate) fn flag_interior_triangle_words(mesh: &CompressedMesh) -> Result<Vec<u32>> {
    let angle = 1.2217304706573486_f64 as f32;
    let mut polygons: Vec<InteriorPolygon> = Vec::with_capacity(mesh.primitive_count());
    for (section_index, section) in mesh.sections.iter().enumerate() {
        let custom: rustc_hash::FxHashSet<usize> = section.custom_primitives.iter().map(|c| c.primitive_index).collect();
        for (primitive_index, p) in section.primitives.iter().enumerate() {
            if custom.contains(&primitive_index) || *p == DEAD_PRIMITIVE {
                continue;
            }
            let [a, b, c, d] = *p;
            let key = (section_index as u32) << 8 | (primitive_index as u32) << 1;
            let flat_index = key >> 1;
            let v = |i: u8| section.vertices[i as usize];
            let (first, second, third) = (v(a), v(b), v(c));
            let first_normal = interior_normalize3(cross3(sub3(second, first), sub3(third, first)));
            let mut push = |polygon_key: u32, vertices: [u8; 4], vertex_count: u8, normal: V3, second_normal: V3| {
                let (first, second, third) = (v(vertices[0]), v(vertices[1]), v(vertices[2]));
                let area_normal = cross3(sub3(second, first), sub3(third, first));
                polygons.push(InteriorPolygon {
                    key: polygon_key,
                    section: section_index as u32,
                    vertices,
                    vertex_count,
                    normal,
                    second_normal,
                    twice_area: interior_dot3(area_normal, area_normal).sqrt(),
                });
            };
            if c == d {
                push(key, [a, b, c, c], 3, first_normal, first_normal);
                continue;
            }
            let second_normal = interior_normalize3(cross3(sub3(third, first), sub3(v(d), first)));
            if mesh.quad_is_flat_words[(flat_index >> 5) as usize] >> (flat_index & 0x1F) & 1 != 0 {
                push(key, [a, b, c, d], 4, first_normal, second_normal);
            } else {
                push(key, [a, b, c, c], 3, first_normal, first_normal);
                push(key + 1, [a, c, d, d], 3, second_normal, second_normal);
            }
        }
    }

    // `links[polygon * 4 + edge]` is the neighbouring `polygon * 4 + edge`.
    let mut links = vec![NO_LINK; polygons.len() * 4];
    let mut internal_quad_edge = vec![false; polygons.len() * 4];
    for index in 0..polygons.len().saturating_sub(1) {
        let (first, second) = (&polygons[index], &polygons[index + 1]);
        if first.key & 1 != 0 || second.key != first.key + 1 {
            continue;
        }
        let (first_edge, second_edge) = (index * 4 + 2, (index + 1) * 4);
        links[first_edge] = second_edge as u32;
        links[second_edge] = first_edge as u32;
        internal_quad_edge[first_edge] = true;
        internal_quad_edge[second_edge] = true;
    }
    let mut buckets: Vec<EdgeBucket> = (0..64).map(|_| EdgeBucket::default()).collect();
    for (polygon_index, polygon) in polygons.iter().enumerate() {
        let count = polygon.vertex_count as usize;
        let section = &mesh.sections[polygon.section as usize];
        for edge_index in 0..count {
            let edge = polygon_index * 4 + edge_index;
            if internal_quad_edge[edge] {
                continue;
            }
            let first = interior_vertex_id(polygon.section as usize, section, polygon.vertices[edge_index]);
            let second = interior_vertex_id(polygon.section as usize, section, polygon.vertices[(edge_index + 1) % count]);
            let bucket = &mut buckets[(((second as i64) ^ (first as i64)).wrapping_mul(-27) & 0x3F) as usize];
            match bucket.take_last(second, first) {
                Some(linked) => {
                    links[edge] = linked;
                    links[linked as usize] = edge as u32;
                }
                None => bucket.push(first, second, edge as u32),
            }
        }
    }
    drop(buckets);
    drop(internal_quad_edge);

    let epsilon = f32::EPSILON;
    let neighbor = |edge: usize| -> (&InteriorPolygon, usize) {
        let linked = links[edge] as usize;
        (&polygons[linked / 4], linked % 4)
    };
    let signed_dihedral = |polygon_index: usize, edge_index: usize| -> f32 {
        let polygon = &polygons[polygon_index];
        let (neighbor, neighbor_edge) = neighbor(polygon_index * 4 + edge_index);
        let section = &mesh.sections[polygon.section as usize];
        let count = polygon.vertex_count as usize;
        let first = section.vertices[polygon.vertices[edge_index] as usize];
        let second = section.vertices[polygon.vertices[(edge_index + 1) % count] as usize];
        let edge = interior_normalize3(sub3(second, first));
        interior_dot3(edge, cross3(polygon.normal, neighbor.edge_normal(neighbor_edge)))
    };

    let mut candidates: Vec<(f32, u32)> = Vec::new();
    for (polygon_index, polygon) in polygons.iter().enumerate() {
        let count = polygon.vertex_count as usize;
        if (0..count).any(|e| links[polygon_index * 4 + e] == NO_LINK) {
            continue;
        }
        let positive = (0..count).filter(|&e| signed_dihedral(polygon_index, e) > epsilon).count();
        candidates.push((polygon.twice_area + positive as f32 * 10.0, polygon_index as u32));
    }
    havok_quicksort(&mut candidates, |c| c.0, |a: &f32, b: &f32| a < b);

    let mut interior_keys: rustc_hash::FxHashSet<u32> = Default::default();
    let cosine_threshold = havok_cosine(angle);
    for &(_, polygon_index) in &candidates {
        let polygon_index = polygon_index as usize;
        let polygon = &polygons[polygon_index];
        let mut rejected = false;
        for edge_index in 0..polygon.vertex_count as usize {
            let (neighbor, neighbor_edge) = neighbor(polygon_index * 4 + edge_index);
            let neighbor_key = neighbor.edge_key(neighbor_edge);
            let neighbor_normal = neighbor.edge_normal(neighbor_edge);
            if signed_dihedral(polygon_index, edge_index) <= epsilon {
                continue;
            }
            if interior_keys.contains(&neighbor_key) || interior_dot3(neighbor_normal, polygon.normal) < cosine_threshold {
                rejected = true;
                break;
            }
        }
        if !rejected {
            interior_keys.insert(polygon.key);
        }
    }

    let mut words = vec![0u32; (mesh.triangle_is_interior_num_bits as usize).div_ceil(32)];
    for key in interior_keys {
        if key >= mesh.triangle_is_interior_num_bits {
            return Err(PrevisError::invalid("interior classifier produced a key outside the shape domain"));
        }
        words[(key >> 5) as usize] |= 1 << (key & 0x1F);
    }
    Ok(words)
}

/// Positions in an [`EdgeBucket`] holding one open-edge key; nearly every key
/// has a single position.
enum Slots {
    One(u32),
    Many(Vec<u32>),
}

/// One of the classifier's 64 open-edge buckets. Matching takes the last
/// entry with the reversed key and swap-removes it; the per-key positions
/// find that entry without a scan.
#[derive(Default)]
struct EdgeBucket {
    /// `(first vertex, second vertex, polygon * 4 + edge)`.
    entries: Vec<(u32, u32, u32)>,
    positions: rustc_hash::FxHashMap<u64, Slots>,
}

impl EdgeBucket {
    fn key(first: u32, second: u32) -> u64 {
        (first as u64) << 32 | second as u64
    }

    fn push(&mut self, first: u32, second: u32, edge: u32) {
        let position = self.entries.len() as u32;
        self.entries.push((first, second, edge));
        match self.positions.entry(Self::key(first, second)) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(Slots::One(position));
            }
            std::collections::hash_map::Entry::Occupied(mut slot) => {
                let slots = slot.get_mut();
                match slots {
                    Slots::One(existing) => *slots = Slots::Many(vec![*existing, position]),
                    Slots::Many(list) => list.push(position),
                }
            }
        }
    }

    fn take_last(&mut self, first: u32, second: u32) -> Option<u32> {
        let key = Self::key(first, second);
        let position = match self.positions.get_mut(&key)? {
            Slots::One(position) => {
                let position = *position;
                self.positions.remove(&key);
                position
            }
            Slots::Many(list) => {
                let at = (0..list.len()).max_by_key(|&i| list[i]).expect("slot lists are never empty");
                let position = list.swap_remove(at);
                if list.len() == 1 {
                    let remaining = list[0];
                    self.positions.insert(key, Slots::One(remaining));
                }
                position
            }
        } as usize;
        let linked = self.entries[position].2;
        let last = self.entries.len() - 1;
        if position != last {
            let moved = self.entries[last];
            match self.positions.get_mut(&Self::key(moved.0, moved.1)).expect("every entry has a slot") {
                Slots::One(slot) => *slot = position as u32,
                Slots::Many(list) => {
                    let at = list.iter().position(|&p| p as usize == last).expect("the moved entry has a slot");
                    list[at] = position as u32;
                }
            }
            self.entries[position] = moved;
        }
        self.entries.pop();
        Some(linked)
    }
}

// ---------------------------------------------------------------------------
// Leaf expansion, welding and quad pairing
// ---------------------------------------------------------------------------

pub(crate) struct ExpandedLeaves {
    pub vertices: Vec<V3>,
    pub triangles: Vec<[u32; 3]>,
    pub vertex_sources: Vec<u32>,
    pub triangle_sources: Vec<usize>,
}

/// `getShapeKeys`/`getLeafShapes`: four vertices per leaf triangle; a flat
/// quad's first half keeps its real fourth vertex, which still gets welded.
pub(crate) fn expand_leaves(mesh: &CompressedMesh, vertices: &[V3], reverse_winding: bool) -> Result<ExpandedLeaves> {
    let mut out = ExpandedLeaves {
        vertices: Vec::new(),
        triangles: Vec::new(),
        vertex_sources: Vec::new(),
        triangle_sources: Vec::new(),
    };
    let mut vertex_offset = 0;
    let mut primitive_offset = 0;
    for (section_index, section) in mesh.sections.iter().enumerate() {
        let custom: rustc_hash::FxHashSet<usize> = section.custom_primitives.iter().map(|c| c.primitive_index).collect();
        for (primitive_index, p) in section.primitives.iter().enumerate() {
            let [a, b, c, d] = p.map(|x| x as usize);
            let leaves: Vec<[usize; 4]> = if c == d {
                if b == c {
                    if custom.contains(&primitive_index) {
                        continue;
                    }
                    return Err(PrevisError::invalid("unparsed compressed custom primitive"));
                }
                vec![[a, b, c, a]]
            } else if *p == DEAD_PRIMITIVE {
                // `getShapeKeys` skips Havok's marker for a removed primitive.
                continue;
            } else {
                // Converted FO76 meshes can ship an empty flatness bitfield;
                // `getLeafShapes` reads every quad of such a mesh as not flat.
                let flat_index = section_index * 128 + primitive_index;
                let flat = flat_index < mesh.quad_is_flat_num_bits as usize
                    && mesh.quad_is_flat_words.get(flat_index >> 5).is_some_and(|w| w >> (flat_index & 0x1F) & 1 != 0);
                vec![[a, b, c, if flat { d } else { a }], [a, c, d, a]]
            };
            for mut leaf in leaves {
                if reverse_winding {
                    leaf = [leaf[0], leaf[2], leaf[1], leaf[3]];
                }
                let base = out.vertices.len() as u32;
                for index in leaf {
                    out.vertices.push(vertices[vertex_offset + index]);
                    out.vertex_sources.push((vertex_offset + index) as u32);
                }
                out.triangles.push([base, base + 1, base + 2]);
                out.triangle_sources.push(primitive_offset + primitive_index);
            }
        }
        vertex_offset += section.vertices.len();
        primitive_offset += section.primitives.len();
    }
    Ok(out)
}

/// Zero-tolerance `weldVerticesVirtual`: X-only unstable sort, first
/// surviving equal vertex is the representative.
pub(crate) fn weld_mapping(vertices: &[V3]) -> Vec<u32> {
    let mut refs: Vec<(u32, V3)> = vertices.iter().enumerate().map(|(i, &v)| (i as u32, v)).collect();
    havok_quicksort(&mut refs, |r| r.1[0], |a: &f32, b: &f32| a < b);
    let mut mapping: Vec<u32> = (0..vertices.len() as u32).collect();
    let mut start = 0;
    while start < refs.len() {
        // Candidates are scanned while their X is not greater than the reference's.
        let x = refs[start].1[0];
        let mut end = start + 1;
        while end < refs.len() && !(0.0 < refs[end].1[0] - x) {
            end += 1;
        }
        weld_run(&refs[start..end], &mut mapping);
        start = end;
    }
    mapping
}

/// Welds one run of equal-X references. Distinct float32 values only weld
/// when both are small enough for `delta*delta` to underflow; without such
/// values welding is exact (y, z) equality, grouped by hash.
fn weld_run(run: &[(u32, V3)], mapping: &mut [u32]) {
    let tiny = |v: f32| v != 0.0 && v.abs() < 1.0e-12;
    if run.iter().any(|r| tiny(r.1[1]) || tiny(r.1[2])) {
        let mut welded = vec![false; run.len()];
        for position in 0..run.len() {
            if welded[position] {
                continue;
            }
            let (representative, reference) = run[position];
            mapping[representative as usize] = representative;
            for candidate in position + 1..run.len() {
                if welded[candidate] {
                    continue;
                }
                let v = run[candidate].1;
                let mut distance = 0.0f32;
                for axis in 0..3 {
                    let delta = reference[axis] - v[axis];
                    distance += delta * delta;
                }
                if distance <= 0.0 {
                    mapping[run[candidate].0 as usize] = representative;
                    welded[candidate] = true;
                }
            }
        }
        return;
    }
    let key = |v: V3| (if v[1] == 0.0 { 0 } else { v[1].to_bits() }, if v[2] == 0.0 { 0 } else { v[2].to_bits() });
    let mut representatives: HashMap<(u32, u32), u32> = HashMap::with_capacity(run.len());
    for &(index, vertex) in run {
        let representative = *representatives.entry(key(vertex)).or_insert(index);
        mapping[index as usize] = representative;
    }
}

/// `buildStep12` invalidates a welded triangle whose second and third corners
/// coincide or that `isDegenerate` rejects; an invalid triangle gets no
/// half-edges and is never emitted.
pub(crate) fn valid_triangles(vertices: &[V3], triangles: &[[u32; 3]], vertex_map: &[u32]) -> Vec<bool> {
    triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| vertex_map[i as usize] as usize);
            b != c && !is_degenerate(vertices[a], vertices[b], vertices[c], TRIANGLE_DEGENERACY_TOLERANCE)
        })
        .collect()
}

/// `hkcdTriangleUtil::isDegenerate` in its SIMD evaluation order: a squared
/// cross-product length below the tolerance at corner `a` or `b`, or edges
/// from `b` whose Gram determinant is exactly zero (or NaN).
fn is_degenerate(a: V3, b: V3, c: V3, tolerance: f32) -> bool {
    let sub = |p: V3, q: V3| [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
    let cross_squared = |e1: V3, e2: V3| {
        let n = [e1[2] * e2[1] - e2[2] * e1[1], e1[0] * e2[2] - e2[0] * e1[2], e1[1] * e2[0] - e2[1] * e1[0]];
        (n[1] * n[1] + n[0] * n[0]) + n[2] * n[2]
    };
    if cross_squared(sub(a, c), sub(a, b)) < tolerance || cross_squared(sub(b, c), sub(b, a)) < tolerance {
        return true;
    }
    let (ab, cb) = (sub(a, b), sub(c, b));
    let length_squared = |v: V3| (v[1] * v[1] + v[0] * v[0]) + v[2] * v[2];
    let dot = (cb[1] * ab[1] + cb[0] * ab[0]) + cb[2] * ab[2];
    let determinant = length_squared(cb) * length_squared(ab) - dot * dot;
    determinant == 0.0 || determinant.is_nan()
}

/// `hkcdStaticMeshTree::build`'s bad-primitive pass over quantized vertices:
/// a quad drops a degenerate second triangle, then a degenerate first
/// triangle leaves only the quad's second one or turns a triangle into
/// `DEAD_PRIMITIVE`. The tree and shape-key domain keep the old primitives.
pub(crate) fn mark_bad_primitives(section: &mut Section) {
    for p in &mut section.primitives {
        if p[1] == p[2] && p[2] == p[3] {
            continue;
        }
        let v = p.map(|i| section.vertices[i as usize]);
        if p[2] != p[3] && is_degenerate(v[0], v[2], v[3], TRIANGLE_DEGENERACY_TOLERANCE) {
            p[3] = p[2];
        }
        if is_degenerate(v[0], v[1], v[2], TRIANGLE_DEGENERACY_TOLERANCE) {
            *p = if p[2] == p[3] { DEAD_PRIMITIVE } else { [p[0], p[2], p[3], p[3]] };
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct QuadCandidate {
    pub first_triangle: u32,
    pub second_triangle: u32,
    pub first_edge: u8,
    pub second_edge: u8,
    pub cost: f32,
}

fn half_surface_area(vertices: &[V3], indices: &[u32]) -> f32 {
    let (lo, hi) = bounds_of(indices.iter().map(|&i| &vertices[i as usize]));
    area(lo, hi)
}

fn quad_candidate_cost(vertices: &[V3], first: [u32; 3], second: [u32; 3]) -> f32 {
    let first_area = half_surface_area(vertices, &first);
    let second_area = half_surface_area(vertices, &second);
    let union = half_surface_area(vertices, &[first[0], first[1], first[2], second[0], second[1], second[2]]);
    (union - second_area) + (union - first_area)
}

/// Shared-edge quad candidates from the sorted half-edge array. CK's strict
/// inner bound never compares the final sorted half-edge, and each half-edge
/// keeps only its first matching partner, which matters on non-manifold edges.
pub(crate) fn quad_candidates(vertices: &[V3], triangles: &[[u32; 3]], vertex_map: &[u32], layers: &[u32]) -> Vec<QuadCandidate> {
    let mapped: Vec<[u32; 3]> = triangles.iter().map(|t| t.map(|i| vertex_map[i as usize])).collect();
    // (va, vb, triangle, edge)
    let mut half_edges: Vec<(u32, u32, u32, u8)> = Vec::with_capacity(mapped.len() * 3);
    for (triangle, t) in mapped.iter().enumerate() {
        for edge in 0..3 {
            let (start, end) = (t[edge], t[(edge + 1) % 3]);
            half_edges.push((start.min(end), start.max(end), triangle as u32, edge as u8));
        }
    }
    havok_quicksort(&mut half_edges, |h| (h.0, h.1), |a: &(u32, u32), b: &(u32, u32)| a < b);

    let mut candidates = Vec::new();
    let n = half_edges.len();
    for first_index in 0..n.saturating_sub(1) {
        let first = half_edges[first_index];
        let first_triangle = mapped[first.2 as usize];
        let first_edge = first.3 as usize;
        for &second in &half_edges[first_index + 1..n - 1] {
            if (first.0, first.1) != (second.0, second.1) {
                break;
            }
            if first.2 == second.2 || layers[first.2 as usize] != layers[second.2 as usize] {
                continue;
            }
            let second_triangle = mapped[second.2 as usize];
            let second_edge = second.3 as usize;
            if first_triangle[first_edge] != second_triangle[(second_edge + 1) % 3]
                || second_triangle[second_edge] != first_triangle[(first_edge + 1) % 3]
                || first_triangle[(first_edge + 2) % 3] == second_triangle[(second_edge + 2) % 3]
            {
                continue;
            }
            candidates.push(QuadCandidate {
                first_triangle: first.2,
                second_triangle: second.2,
                first_edge: first.3,
                second_edge: second.3,
                cost: quad_candidate_cost(vertices, first_triangle, second_triangle),
            });
            break;
        }
    }
    candidates
}

pub(crate) fn select_quad_pairs(mut candidates: Vec<QuadCandidate>, triangle_count: usize) -> Vec<QuadCandidate> {
    havok_quicksort(&mut candidates, |c| c.cost, |a: &f32, b: &f32| a < b);
    let mut used = vec![false; triangle_count];
    let mut selected = Vec::new();
    for candidate in candidates {
        let (first, second) = (candidate.first_triangle as usize, candidate.second_triangle as usize);
        if used[first] || used[second] {
            continue;
        }
        used[first] = true;
        used[second] = true;
        selected.push(candidate);
    }
    selected
}

fn paired_triangles(selected: &[QuadCandidate], triangle_count: usize) -> Vec<bool> {
    let mut used = vec![false; triangle_count];
    for candidate in selected {
        used[candidate.first_triangle as usize] = true;
        used[candidate.second_triangle as usize] = true;
    }
    used
}

/// Havok's unstable sort of primitive data values: returns
/// `(input -> output, output -> input)` position maps.
pub(crate) fn data_sort_order<T: PartialOrd + Copy>(values: &[T]) -> (Vec<usize>, Vec<usize>) {
    let mut indices: Vec<usize> = (0..values.len()).collect();
    havok_quicksort(&mut indices, |&i| values[i], |a: &T, b: &T| a < b);
    let input_to_output = indices;
    let mut output_to_input = vec![0; input_to_output.len()];
    for (input, &output) in input_to_output.iter().enumerate() {
        output_to_input[output] = input;
    }
    (input_to_output, output_to_input)
}

pub(crate) fn expand_data_runs(runs: &[(u16, u8, u8)], primitive_count: usize) -> Result<Vec<u16>> {
    let mut values: Vec<Option<u16>> = vec![None; primitive_count];
    for &(value, index, count) in runs {
        let (index, count) = (index as usize, count as usize);
        if index + count > primitive_count {
            return Err(PrevisError::invalid("primitive data run exceeds the section"));
        }
        for slot in &mut values[index..index + count] {
            if slot.replace(value).is_some() {
                return Err(PrevisError::invalid("primitive data runs overlap"));
            }
        }
    }
    values
        .into_iter()
        .map(|v| v.ok_or_else(|| PrevisError::invalid("primitive data runs do not cover the section")))
        .collect()
}

pub(crate) fn compress_data_runs(values: &[u16]) -> Vec<(u16, u8, u8)> {
    let mut runs = Vec::new();
    let Some(&first_value) = values.first() else { return runs };
    let (mut first, mut value) = (0, first_value);
    for (index, &candidate) in values.iter().enumerate().skip(1) {
        if candidate == value {
            continue;
        }
        runs.push((value, first as u8, (index - first) as u8));
        first = index;
        value = candidate;
    }
    runs.push((value, first as u8, (values.len() - first) as u8));
    runs
}

fn candidate_primitive(candidate: &QuadCandidate, triangles: &[[u32; 3]]) -> [u32; 4] {
    let first = triangles[candidate.first_triangle as usize];
    let second = triangles[candidate.second_triangle as usize];
    let (first_edge, second_edge) = (candidate.first_edge as usize, candidate.second_edge as usize);
    [first[(first_edge + 1) % 3], first[(first_edge + 2) % 3], second[(second_edge + 1) % 3], second[(second_edge + 2) % 3]]
}

/// Accepted quads in selection order, then unpaired triangles in input order.
pub(crate) fn primitive_sequences(expanded: &ExpandedLeaves, vertex_map: &[u32], selected: &[QuadCandidate]) -> Vec<[usize; 4]> {
    let mapped: Vec<[u32; 3]> = expanded.triangles.iter().map(|t| t.map(|i| vertex_map[i as usize])).collect();
    let source = |i: u32| expanded.vertex_sources[i as usize] as usize;
    let used = paired_triangles(selected, mapped.len());
    let mut primitives: Vec<[usize; 4]> = Vec::with_capacity(mapped.len() - selected.len());
    primitives.extend(selected.iter().map(|c| candidate_primitive(c, &mapped).map(source)));
    for (t, _) in mapped.iter().zip(&used).filter(|(_, used)| !**used) {
        let [a, b, c] = t.map(source);
        primitives.push([a, b, c, c]);
    }
    primitives
}

pub(crate) fn primitive_data_values(triangle_values: &[u16], selected: &[QuadCandidate]) -> Result<Vec<u16>> {
    let used = paired_triangles(selected, triangle_values.len());
    let mut values = Vec::with_capacity(triangle_values.len() - selected.len());
    for c in selected {
        let (first, second) = (triangle_values[c.first_triangle as usize], triangle_values[c.second_triangle as usize]);
        if first != second {
            return Err(PrevisError::invalid("paired triangles have different primitive data"));
        }
        values.push(first);
    }
    values.extend(triangle_values.iter().zip(&used).filter(|(_, used)| !**used).map(|(&v, _)| v));
    Ok(values)
}

// ---------------------------------------------------------------------------
// Source packfile parsing
// ---------------------------------------------------------------------------

/// A packfile's `__data__` section with its fixup maps and object classes.
pub(crate) struct Packfile<'a> {
    pub blob: &'a [u8],
    pub pointer_size: usize,
    pub data: usize,
    pub objects: Vec<(usize, String)>,
    pub local: HashMap<usize, usize>,
    pub global: HashMap<usize, usize>,
}

impl<'a> Packfile<'a> {
    pub fn parse(blob: &'a [u8]) -> Result<Self> {
        if blob.get(..8) != Some(&HKX_MAGIC[..]) {
            return Err(PrevisError::invalid("missing Havok packfile magic"));
        }
        let parsed = parse_packfile(blob).map_err(|e| PrevisError::invalid(format!("Havok packfile: {e}")))?;
        let pointer_size = parsed.header.pointer_size as usize;
        if !matches!(pointer_size, 4 | 8) {
            return Err(PrevisError::unsupported(format!("Havok pointer size {pointer_size}")));
        }
        let data = parsed
            .section("__data__")
            .ok_or_else(|| PrevisError::invalid("Havok packfile has no data section"))?
            .offset;
        let names: HashMap<usize, &str> = parsed.classnames.iter().map(|c| (c.position, c.name.as_str())).collect();
        let objects = parsed
            .virtual_fixups
            .iter()
            .map(|f| (f.source as usize, names.get(&(f.classname_offset as usize)).copied().unwrap_or("").to_string()))
            .collect();
        let local = parsed.local_fixups.iter().map(|f| (f.source as usize, f.target as usize)).collect();
        let global = parsed
            .global_fixups
            .iter()
            .filter(|f| f.section == 2)
            .map(|f| (f.source as usize, f.target as usize))
            .collect();
        Ok(Packfile {
            blob,
            pointer_size,
            data,
            objects,
            local,
            global,
        })
    }

    pub fn object(&self, class: &str) -> Option<usize> {
        self.objects.iter().find(|(_, c)| c == class).map(|(o, _)| *o)
    }

    pub fn class_at(&self, relative: usize) -> Option<&str> {
        self.objects.iter().find(|(o, _)| *o == relative).map(|(_, c)| c.as_str())
    }

    fn bytes(&self, relative: usize, len: usize) -> Result<&[u8]> {
        self.blob
            .get(self.data + relative..self.data + relative + len)
            .ok_or_else(|| PrevisError::invalid("Havok packfile read is out of bounds"))
    }

    pub fn u8(&self, relative: usize) -> Result<u8> {
        Ok(self.bytes(relative, 1)?[0])
    }

    pub fn u16(&self, relative: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(relative, 2)?.try_into().unwrap()))
    }

    pub fn u32(&self, relative: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(relative, 4)?.try_into().unwrap()))
    }

    pub fn f32(&self, relative: usize) -> Result<f32> {
        Ok(f32::from_bits(self.u32(relative)?))
    }

    pub fn v3(&self, relative: usize) -> Result<V3> {
        Ok([self.f32(relative)?, self.f32(relative + 4)?, self.f32(relative + 8)?])
    }

    /// `(target, count)` of the array header at `field`.
    pub fn array(&self, field: usize) -> Result<(usize, usize)> {
        let count = (self.u32(field + self.pointer_size)? & 0x3FFF_FFFF) as usize;
        if count == 0 {
            return Ok((0, 0));
        }
        let target = *self
            .local
            .get(&field)
            .ok_or_else(|| PrevisError::invalid(format!("Havok array at {field:#x} has no fixup")))?;
        Ok((target, count))
    }

    pub fn body_array(&self) -> Result<(usize, usize)> {
        let system = self
            .object("hknpPhysicsSystemData")
            .ok_or_else(|| PrevisError::invalid("packfile has no hknpPhysicsSystemData"))?;
        self.array(system + if self.pointer_size == 8 { 0x40 } else { 0x2C })
    }

    pub fn body_filter(&self, body: usize) -> Result<u32> {
        self.u32(body + if self.pointer_size == 8 { 0x14 } else { 0x10 })
    }

    pub fn body_motion(&self, body: usize) -> Result<u32> {
        self.u32(body + if self.pointer_size == 8 { 0x0C } else { 0x08 })
    }
}

/// The single compressed-mesh body of a physics system.
pub fn parse_compressed_mesh(blob: &[u8]) -> Result<CompressedMesh> {
    let pf = Packfile::parse(blob)?;
    let p8 = pf.pointer_size == 8;
    let need = |class: &str| pf.object(class).ok_or_else(|| PrevisError::invalid(format!("packfile has no {class}")));
    let data_rel = need("hknpCompressedMeshShapeData")?;
    let shape_rel = need("hknpCompressedMeshShape")?;
    let material_rel = need("hknpBSMaterialProperties")?;

    let (bodies, body_count) = pf.body_array()?;
    let mesh_bodies: Vec<usize> =
        (0..body_count).map(|i| bodies + i * 0x60).filter(|b| pf.global.get(b) == Some(&shape_rel)).collect();
    let [body] = mesh_bodies[..] else {
        return Err(PrevisError::unsupported("expected one body referencing the compressed mesh"));
    };
    let mut tail = [0u8; 0x40];
    tail.copy_from_slice(pf.bytes(body + 0x20, 0x40)?);

    let (entries, entry_count) = pf.array(material_rel + if p8 { 0x10 } else { 0x08 })?;
    let (entry_size, filter_offset) = if p8 { (0x18, 0x10) } else { (0x10, 0x08) };
    let material_entries = (0..entry_count)
        .map(|i| {
            let e = entries + i * entry_size + filter_offset;
            Ok((pf.u32(e)?, pf.u32(e + 4)?))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(CompressedMesh {
        collision_filter_info: pf.body_filter(body)?,
        body_cinfo_tail: Some(tail),
        material_entries,
        ..read_compressed_mesh(&pf, shape_rel, data_rel)?
    })
}

/// An `hknpCompressedMeshShape` and its data; the body and material fields stay empty.
fn read_compressed_mesh(pf: &Packfile, shape_rel: usize, data_rel: usize) -> Result<CompressedMesh> {
    let p8 = pf.pointer_size == 8;
    let bitfield = |field: usize, bits: usize| -> Result<(Vec<u32>, u32)> {
        let (target, count) = pf.array(shape_rel + field)?;
        let words = (0..count).map(|i| pf.u32(target + i * 4)).collect::<Result<_>>()?;
        Ok((words, pf.u32(shape_rel + bits)?))
    };
    let ((quad_words, quad_bits), (interior_words, interior_bits)) = if p8 {
        (bitfield(0x68, 0x78)?, bitfield(0x80, 0x90)?)
    } else {
        (bitfield(0x64, 0x70)?, bitfield(0x74, 0x80)?)
    };

    let offsets: [usize; 6] = if p8 { [0x50, 0x60, 0x70, 0x80, 0x90, 0xA0] } else { [0x4C, 0x58, 0x64, 0x70, 0x7C, 0x88] };
    let (master, master_count) = pf.array(data_rel + 0x10)?;
    let (sections_at, section_count) = pf.array(data_rel + offsets[0])?;
    let (primitives_at, _) = pf.array(data_rel + offsets[1])?;
    let (shared_at, shared_count) = pf.array(data_rel + offsets[2])?;
    let (packed_at, packed_count) = pf.array(data_rel + offsets[3])?;
    let (shared_vertices_at, shared_vertex_count) = pf.array(data_rel + offsets[4])?;
    let (runs_at, run_count) = pf.array(data_rel + offsets[5])?;

    let shared_indices = (0..shared_count).map(|i| pf.u16(shared_at + i * 2)).collect::<Result<Vec<_>>>()?;
    let packed = (0..packed_count).map(|i| pf.u32(packed_at + i * 4)).collect::<Result<Vec<_>>>()?;
    let shared_vertices = (0..shared_vertex_count)
        .map(|i| Ok(u64::from_le_bytes(pf.bytes(shared_vertices_at + i * 8, 8)?.try_into().unwrap())))
        .collect::<Result<Vec<_>>>()?;
    let runs = (0..run_count)
        .map(|i| Ok((pf.u16(runs_at + i * 4)?, pf.u8(runs_at + i * 4 + 2)?, pf.u8(runs_at + i * 4 + 3)?)))
        .collect::<Result<Vec<_>>>()?;
    let master_tree_nodes = (0..master_count)
        .map(|i| Ok(pf.bytes(master + i * 5, 5)?.try_into().unwrap()))
        .collect::<Result<Vec<[u8; 5]>>>()?;

    let convex_radius = pf.f32(shape_rel + 0x14)?;
    let object_min = pf.v3(data_rel + 0x20)?;
    let object_max = pf.v3(data_rel + 0x30)?;
    let decoded_shared: Vec<V3> = shared_vertices.iter().map(|&p| decode_vertex_21_21_22(object_min, object_max, p)).collect();

    let mut sections = Vec::with_capacity(section_count);
    for section_index in 0..section_count {
        let s = sections_at + section_index * 0x60;
        let base = pf.v3(s + 0x30)?;
        let scale = pf.v3(s + 0x3C)?;
        let first_vertex = pf.u32(s + 0x48)? as usize;
        let vertex_range = pf.u32(s + 0x4C)?;
        let primitive_range = pf.u32(s + 0x50)?;
        let run_range = pf.u32(s + 0x54)?;
        let packed_count = match pf.u8(s + 0x58)? {
            0 => (vertex_range & 0xFF) as usize,
            n => n as usize,
        };
        let first_shared = (vertex_range >> 8) as usize;
        let shared_count = pf.u8(s + 0x59)? as usize;
        let page = pf.u8(s + 0x5C)?;
        let page_base = (page as usize) << 16;
        let (first_primitive, primitive_count) = ((primitive_range >> 8) as usize, (primitive_range & 0xFF) as usize);
        let (first_run, section_run_count) = ((run_range >> 8) as usize, (run_range & 0xFF) as usize);
        let section_shared: Vec<u16> = shared_indices
            .get(first_shared..first_shared + shared_count)
            .ok_or_else(|| PrevisError::invalid("section shared indices are out of bounds"))?
            .to_vec();
        let primitives = (first_primitive..first_primitive + primitive_count)
            .map(|i| Ok(pf.bytes(primitives_at + i * 4, 4)?.try_into().unwrap()))
            .collect::<Result<Vec<[u8; 4]>>>()?;
        let (tree_at, tree_count) = pf.array(s)?;
        let tree_nodes = (0..tree_count)
            .map(|i| Ok(pf.bytes(tree_at + i * 4, 4)?.try_into().unwrap()))
            .collect::<Result<Vec<[u8; 4]>>>()?;
        let tree_minimum = pf.v3(s + 0x10)?;
        let tree_maximum = pf.v3(s + 0x20)?;
        let tree_bounds = static_aabb4_node_bounds(tree_minimum, tree_maximum, &tree_nodes)?;
        let section_packed: Vec<u32> = packed
            .get(first_vertex..first_vertex + packed_count)
            .ok_or_else(|| PrevisError::invalid("section packed vertices are out of bounds"))?
            .to_vec();
        let mut vertices: Vec<V3> = section_packed.iter().map(|&p| decode_vertex_11_11_10(base, scale, p)).collect();

        let mut custom_primitives = Vec::new();
        let mut normal_shared: rustc_hash::FxHashSet<usize> = Default::default();
        for (primitive_index, p) in primitives.iter().enumerate() {
            if *p == DEAD_PRIMITIVE {
                continue;
            }
            if p[1] == p[2] && p[2] == p[3] {
                let header_index = (p[0] as usize)
                    .checked_sub(packed_count)
                    .filter(|&h| h + 1 < section_shared.len())
                    .ok_or_else(|| PrevisError::invalid("custom primitive header is out of bounds"))?;
                let header = section_shared[header_index];
                let tag_count = (header >> 6 & 3) as usize;
                let tag_end = header_index + 2 + tag_count;
                if tag_end > section_shared.len() {
                    return Err(PrevisError::invalid("custom primitive tags are out of bounds"));
                }
                let tree_node_index = p[1] as usize;
                let node_data = tree_nodes
                    .get(tree_node_index)
                    .ok_or_else(|| PrevisError::invalid("custom primitive tree node is out of bounds"))?[3];
                if node_data & 1 != 0 || (node_data >> 1) as usize != primitive_index {
                    return Err(PrevisError::invalid("custom primitive/tree leaf mismatch"));
                }
                let first_shared_vertex = section_shared[header_index + 1] as usize;
                let tags = section_shared[header_index + 2..tag_end].to_vec();
                let compression_mode = (header >> 4 & 3) as u8;
                let vertex_count = (header >> 8) as usize;
                custom_primitives.push(CustomPrimitive {
                    primitive_index,
                    tree_node_index,
                    header,
                    custom_type: (header & 0xF) as u8,
                    compression_mode,
                    vertex_count,
                    first_shared_vertex,
                    convex_radius: tags.first().map_or(convex_radius, |&t| decode_radius_tag(t)),
                    tags,
                    vertices: decode_custom_vertices(
                        compression_mode,
                        vertex_count,
                        page_base + first_shared_vertex,
                        &shared_vertices,
                        (object_min, object_max),
                        tree_bounds[tree_node_index],
                    )?,
                });
                continue;
            }
            for &v in p {
                if v as usize >= packed_count {
                    normal_shared.insert(v as usize - packed_count);
                }
            }
        }
        for (offset, &shared_index) in section_shared.iter().enumerate() {
            if !normal_shared.contains(&offset) {
                vertices.push([0.0; 3]);
                continue;
            }
            vertices.push(
                *decoded_shared
                    .get(page_base + shared_index as usize)
                    .ok_or_else(|| PrevisError::invalid("section shared vertex index is out of bounds"))?,
            );
        }
        sections.push(Section {
            tree_minimum,
            tree_maximum,
            tree_minimum_w: pf.f32(s + 0x1C)?,
            tree_maximum_w: pf.f32(s + 0x2C)?,
            base,
            scale,
            packed_vertices: section_packed,
            vertices,
            primitives,
            primitive_data_runs: runs
                .get(first_run..first_run + section_run_count)
                .ok_or_else(|| PrevisError::invalid("section data runs are out of bounds"))?
                .to_vec(),
            tree_nodes,
            shared_vertex_indices: section_shared,
            leaf_index: pf.u16(s + 0x5A)?,
            page,
            section_flags: pf.u8(s + 0x5D)?,
            custom_primitives,
        });
    }

    Ok(CompressedMesh {
        object_aabb_min: object_min,
        object_aabb_max: object_max,
        object_aabb_min_w: pf.f32(data_rel + 0x2C)?,
        object_aabb_max_w: pf.f32(data_rel + 0x3C)?,
        quad_is_flat_words: quad_words,
        quad_is_flat_num_bits: quad_bits,
        triangle_is_interior_words: interior_words,
        triangle_is_interior_num_bits: interior_bits,
        sections,
        convex_radius,
        shape_user_data: pf.u32(shape_rel + 0x18)?,
        shared_vertices,
        master_tree_nodes,
        key_count: Some(pf.u32(data_rel + 0x40)?),
        ..CompressedMesh::empty()
    })
}

const DIRECT_CONVEX_SHAPES: [&str; 4] = ["hknpSphereShape", "hknpCapsuleShape", "hknpConvexShape", "hknpConvexPolytopeShape"];
/// `hkRefCountedProperties` key of `hknpBSMaterialProperties`.
const BS_MATERIAL_PROPERTIES_KEY: u16 = 0xF601;
/// `CanAddToCombinedShape`'s limit on a convex shape's `calcSize()`.
const COMBINED_SHAPE_SIZE_LIMIT: usize = 0x800;

/// A compound instance's full transform (`hknpShapeInstance::getFullTransform`):
/// Havok column `j` is `rotation[..][j]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartTransform {
    pub rotation: Mat3,
    pub translation: V3,
}

/// One shape `CombinedData::AddShape` reaches from a body: the body's own
/// shape, or a child of its compound.
#[derive(Clone, Debug, PartialEq)]
pub struct SourcePart {
    pub instance: Option<PartTransform>,
    /// An `hknpScaledConvexShape`'s scale, which CK applies in vertex mode.
    pub wrapper_scale: Option<V3>,
    /// Triangles and custom primitives with materials resolved as
    /// `CombinedData::GetMaterial` resolves them.
    pub mesh: CompressedMesh,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceBody {
    /// The body shape's local AABB.
    pub aabb: Bounds,
    /// CK centers the cell on the union of every body's `calcAabb`: a convex
    /// body bounds its transformed vertices, any other transforms `aabb` as a box.
    pub convex: bool,
    pub parts: Vec<SourcePart>,
}

/// A collision source in the form `GenerateCombinedShape` consumes. CK
/// places every body at its collision node and ignores the body's own
/// position and orientation (on CELL 1294F4, 181 instances of bodies stored
/// off their node land on CK's vertices only that way).
#[derive(Clone, Debug, PartialEq)]
pub struct PrecombineSource {
    pub bodies: Vec<SourceBody>,
    /// `CanAddToCombinedShape`: false when a convex shape exceeds CK's size
    /// limit, which makes CK clone every collision of the base instead.
    pub fits_combined_shape: bool,
}

impl PrecombineSource {
    /// One body holding `mesh` directly.
    pub fn from_mesh(mesh: CompressedMesh) -> Self {
        PrecombineSource {
            bodies: vec![SourceBody {
                aabb: (mesh.object_aabb_min, mesh.object_aabb_max),
                convex: false,
                parts: vec![SourcePart { instance: None, wrapper_scale: None, mesh }],
            }],
            fits_combined_shape: true,
        }
    }
}

/// The `(filter, material)` entries of the `hknpBSMaterialProperties` in a
/// shape's property set; empty when it has none.
fn material_properties(pf: &Packfile, shape: usize) -> Result<Vec<(u32, u32)>> {
    let Some(&properties) = pf.global.get(&(shape + 0x20)) else { return Ok(Vec::new()) };
    let (entries, count) = pf.array(properties)?;
    for entry in (0..count).map(|i| entries + i * 0x10) {
        if pf.u16(entry + 8)? != BS_MATERIAL_PROPERTIES_KEY {
            continue;
        }
        let Some(&material) = pf.global.get(&entry) else { continue };
        let (items, item_count) = pf.array(material + 0x10)?;
        return (0..item_count)
            .map(|i| {
                let item = items + i * 0x18 + 0x10;
                Ok((pf.u32(item)?, pf.u32(item + 4)?))
            })
            .collect();
    }
    Ok(Vec::new())
}

/// `CombinedData::GetMaterial`: the parent's material entry for `tag` (a zero
/// material falls back to the parent's user data), or with no material
/// properties the body's filter and the child's user data.
fn combined_material(pf: &Packfile, parent: usize, child: usize, tag: u16, filter: u32) -> Result<(u32, u32)> {
    let entries = material_properties(pf, parent)?;
    if entries.is_empty() {
        return Ok((filter, pf.u32(child + 0x18)?));
    }
    let &(entry_filter, material) =
        entries.get(tag as usize).ok_or_else(|| PrevisError::invalid(format!("shape tag {tag} has no collision material")))?;
    Ok((entry_filter, if material == 0 { pf.u32(parent + 0x18)? } else { material }))
}

/// A synthetic one-section mesh holding one custom primitive.
fn custom_mesh(custom: CustomPrimitive, bounds: Bounds, material: (u32, u32)) -> CompressedMesh {
    CompressedMesh {
        object_aabb_min: bounds.0,
        object_aabb_max: bounds.1,
        quad_is_flat_words: vec![0],
        quad_is_flat_num_bits: 1,
        triangle_is_interior_words: vec![0],
        triangle_is_interior_num_bits: 2,
        sections: vec![Section {
            tree_minimum: bounds.0,
            tree_maximum: bounds.1,
            primitives: vec![[0; 4]],
            primitive_data_runs: vec![(0, 0, 1)],
            tree_nodes: vec![[0; 4]],
            section_flags: 1,
            custom_primitives: vec![custom],
            ..Section::default()
        }],
        material_entries: vec![material],
        ..CompressedMesh::empty()
    }
}

/// `CombinedData::AddShape` over one pointer-8 physics system.
struct SourceWalker<'a> {
    pf: &'a Packfile<'a>,
    parts: Vec<SourcePart>,
    fits_combined_shape: bool,
}

impl SourceWalker<'_> {
    fn add_shape(&mut self, shape: usize, instance: Option<PartTransform>, filter: u32, material: (u32, u32)) -> Result<()> {
        let pf = self.pf;
        match pf.class_at(shape).unwrap_or("") {
            "hknpDynamicCompoundShape" => {
                if instance.is_some() {
                    return Err(PrevisError::unsupported("merging nested compound collision is not verified"));
                }
                let (instances, count) = pf.array(shape + 0x60)?;
                for at in (0..count).map(|i| instances + i * 0x80) {
                    if pf.u8(at + 0x60)? != 0 {
                        continue;
                    }
                    if pf.v3(at + 0x40)? != [1.0; 3] {
                        return Err(PrevisError::unsupported("merging scaled compound instances is not verified"));
                    }
                    let child = *pf
                        .global
                        .get(&(at + 0x50))
                        .ok_or_else(|| PrevisError::invalid("compound instance has no shape"))?;
                    let mut rotation = [[0.0; 3]; 3];
                    for (column, offset) in [0x00, 0x10, 0x20].into_iter().enumerate() {
                        for (row, value) in pf.v3(at + offset)?.into_iter().enumerate() {
                            rotation[row][column] = value;
                        }
                    }
                    let transform = PartTransform { rotation, translation: pf.v3(at + 0x30)? };
                    let material = combined_material(pf, shape, child, pf.u16(at + 0x58)?, filter)?;
                    self.add_shape(child, Some(transform), filter, material)?;
                }
            }
            "hknpCompressedMeshShape" => {
                let data = *pf
                    .global
                    .get(&(shape + 0x60))
                    .ok_or_else(|| PrevisError::invalid("compressed mesh shape has no data"))?;
                let mut mesh = read_compressed_mesh(pf, shape, data)?;
                let user_data = mesh.shape_user_data;
                let entries = material_properties(pf, shape)?;
                mesh.material_entries = if entries.is_empty() {
                    for section in &mut mesh.sections {
                        section.primitive_data_runs = compress_data_runs(&vec![0; section.primitives.len()]);
                    }
                    vec![(filter, user_data)]
                } else {
                    entries.into_iter().map(|(f, m)| (f, if m == 0 { user_data } else { m })).collect()
                };
                self.parts.push(SourcePart { instance, wrapper_scale: None, mesh });
            }
            _ => {
                let part = self.convex_part(shape, instance, material)?;
                self.parts.push(part);
            }
        }
        Ok(())
    }

    /// A convex shape, or an `hknpScaledConvexShape` around one, as the custom
    /// primitive CK makes of it.
    fn convex_part(&mut self, shape: usize, instance: Option<PartTransform>, material: (u32, u32)) -> Result<SourcePart> {
        let pf = self.pf;
        let (convex, wrapper) = if pf.class_at(shape) == Some("hknpScaledConvexShape") {
            let child = *pf
                .global
                .get(&(shape + 0x30))
                .ok_or_else(|| PrevisError::invalid("scaled convex shape has no child"))?;
            (child, Some((pf.v3(shape + 0x40)?, pf.v3(shape + 0x50)?)))
        } else {
            (shape, None)
        };
        let class = pf.class_at(convex).unwrap_or("");
        if !DIRECT_CONVEX_SHAPES.contains(&class) {
            return Err(PrevisError::unsupported(format!("merging {class} collision is not verified")));
        }
        let count = pf.u16(convex + 0x30)? as usize;
        if matches!(class, "hknpConvexShape" | "hknpConvexPolytopeShape") && ((count >> 2) + 1) << 6 > COMBINED_SHAPE_SIZE_LIMIT {
            self.fits_combined_shape = false;
        }
        let (support, custom_type): (Vec<V3>, u8) = if class == "hknpCapsuleShape" {
            (vec![pf.v3(convex + 0x50)?, pf.v3(convex + 0x60)?], 1)
        } else {
            let delta = pf.u16(convex + 0x32)? as usize;
            let vertices = (0..count).map(|i| pf.v3(convex + 0x30 + delta + i * 0x10)).collect::<Result<Vec<_>>>()?;
            if class == "hknpSphereShape" {
                (vertices.into_iter().take(1).collect(), 0)
            } else {
                (vertices, 2)
            }
        };
        if support.is_empty() {
            return Err(PrevisError::invalid("convex shape has no support vertices"));
        }
        let radius = pf.f32(convex + 0x14)?;
        if !radius.is_finite() || radius < 0.0 {
            return Err(PrevisError::invalid(format!("convex shape has radius {radius}")));
        }
        // `calcAabb` sees the wrapper's translation, which the combined shape drops.
        let (lo, hi) = match wrapper {
            Some((scale, translation)) => {
                let placed: Vec<V3> = support.iter().map(|v| [0, 1, 2].map(|a| v[a] * scale[a] + translation[a])).collect();
                bounds_of(placed.iter())
            }
            None => bounds_of(support.iter()),
        };
        let custom = CustomPrimitive {
            primitive_index: 0,
            tree_node_index: 0,
            header: (support.len() as u16) << 8 | 1 << 6 | 1 << 4 | custom_type as u16,
            custom_type,
            compression_mode: 1,
            vertex_count: support.len(),
            first_shared_vertex: 0,
            tags: vec![encode_radius_tag(radius)],
            convex_radius: radius,
            vertices: support,
        };
        Ok(SourcePart {
            instance,
            wrapper_scale: wrapper.map(|w| w.0),
            mesh: custom_mesh(custom, (lo.map(|v| v - radius), hi.map(|v| v + radius)), material),
        })
    }
}

/// CK 1.11.240 merges a physics system into the cell's `_Physics.NIF` only
/// when every body is static on layer 1; any other system is cloned.
pub fn is_static_layer_one_system(blob: &[u8]) -> Result<bool> {
    let pf = Packfile::parse(blob)?;
    let (bodies, body_count) = pf.body_array()?;
    for body in (0..body_count).map(|i| bodies + i * 0x60) {
        if pf.body_filter(body)? & 0x7F != 1 || pf.body_motion(body)? != 0x7FFF_FFFF {
            return Ok(false);
        }
    }
    Ok(body_count > 0)
}

/// Every body of a pointer-8 physics system as `GenerateCombinedShape`
/// passes it to `CombinedData::AddShape`, with the body's filter and its
/// shape's user data as the starting material.
pub fn parse_precombine_source(blob: &[u8]) -> Result<PrecombineSource> {
    let pf = Packfile::parse(blob)?;
    if pf.pointer_size != 8 {
        return Err(PrevisError::unsupported("merging pointer-4 collision is not verified"));
    }
    let (bodies, body_count) = pf.body_array()?;
    let mut walker = SourceWalker { pf: &pf, parts: Vec::new(), fits_combined_shape: true };
    let mut source_bodies = Vec::with_capacity(body_count);
    for body in (0..body_count).map(|i| bodies + i * 0x60) {
        let shape = *pf.global.get(&body).ok_or_else(|| PrevisError::invalid("physics body has no shape"))?;
        let filter = pf.body_filter(body)?;
        walker.add_shape(shape, None, filter, (filter, pf.u32(shape + 0x18)?))?;
        let parts = std::mem::take(&mut walker.parts);
        let class = pf.class_at(shape);
        let aabb = match class {
            Some("hknpDynamicCompoundShape") => (pf.v3(shape + 0x80)?, pf.v3(shape + 0x90)?),
            _ => (parts[0].mesh.object_aabb_min, parts[0].mesh.object_aabb_max),
        };
        let convex = !matches!(class, Some("hknpDynamicCompoundShape" | "hknpCompressedMeshShape"));
        source_bodies.push(SourceBody { aabb, convex, parts });
    }
    if source_bodies.is_empty() {
        return Err(PrevisError::invalid("physics system has no bodies"));
    }
    Ok(PrecombineSource { bodies: source_bodies, fits_combined_shape: walker.fits_combined_shape })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quicksort_swaps_equal_keys_like_havok() {
        // Middle pivot with equal keys: CK's order, not a stable sort's.
        let mut values = vec![(1.0f32, 'a'), (1.0, 'b'), (1.0, 'c')];
        havok_quicksort(&mut values, |v| v.0, |a: &f32, b: &f32| a < b);
        assert_eq!(values.iter().map(|v| v.1).collect::<String>(), "cba");
    }

    #[test]
    fn identity_rotation_survives_the_quaternion_round_trip() {
        let identity = crate::f32ops::Transform::IDENTITY.rotation;
        assert_eq!(physics_rotation(&identity), identity);
    }

    #[test]
    fn physics_rotation_matches_the_prototype() {
        let rotation = physics_rotation(&crate::f32ops::euler_rotation([0.3, -0.4, 0.7]));
        assert_eq!(
            rotation.map(|row| row.map(f32::to_bits)),
            [
                [1060394983, 1058530992, 1053254103],
                [3207861806, 1059590988, 1049320645],
                [3183531960, 3203297063, 1063338661],
            ]
        );
    }

    #[test]
    fn vertex_codecs_match_the_prototype() {
        let (lo, hi) = ([-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]);
        let packed = encode_vertex_21_21_22(lo, hi, [0.25, -1.5, 2.75]);
        assert_eq!(packed, 17678124423000096767);
        assert_eq!(decode_vertex_21_21_22(lo, hi, packed).map(f32::to_bits), [1048575976, 3217031166, 1076887550]);
        assert_eq!(encode_vertex_11_11_10(lo, hi, [0.25, -1.5, 2.75]), 4110943487);
        assert_eq!(encode_radius_tag(0.01), 0x3C24);
    }

    #[test]
    fn weld_keeps_the_first_sorted_duplicate_as_representative() {
        let vertices = [[1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
        let mapping = weld_mapping(&vertices);
        assert_eq!(mapping[0], mapping[2]);
        assert_ne!(mapping[0], mapping[3]);
    }

    #[test]
    fn degenerate_triangles_are_invalid() {
        // Squared cross lengths straddle the 1e-7 tolerance (CELL 1294F4 slivers).
        let sliver = |height: f32| [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, height, 0.0]];
        let identity = [0, 1, 2];
        assert_eq!(valid_triangles(&sliver(3.0e-4), &[identity], &identity), vec![false]);
        assert_eq!(valid_triangles(&sliver(4.0e-4), &[identity], &identity), vec![true]);
        let square = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 0.0]];
        let welded = weld_mapping(&square);
        assert_eq!(valid_triangles(&square, &[[0, 1, 2], [0, 2, 3]], &welded), vec![true, false]);
    }

    #[test]
    fn a_half_edge_keeps_only_its_first_partner() {
        // A square doubled in the source (a quad plus two triangles over the
        // same diagonal): four opposite pairings share the diagonal, but each
        // half-edge records only its first partner in sorted order.
        let corners = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let leaves = [[3, 0, 2, 3], [0, 1, 2, 3], [0, 2, 3, 0], [2, 0, 1, 2]];
        let vertices: Vec<V3> = leaves.iter().flat_map(|t| t.map(|i| corners[i])).collect();
        let triangles: Vec<[u32; 3]> = (0..leaves.len() as u32).map(|t| [4 * t, 4 * t + 1, 4 * t + 2]).collect();
        let welded = weld_mapping(&vertices);
        assert_eq!(quad_candidates(&vertices, &triangles, &welded, &[0; 4]).len(), 3);
    }

    #[test]
    fn bad_primitives_lose_degenerate_triangles() {
        let mut section = Section {
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.5, 0.0, 0.0], [2.0, 2.0, 0.0], [0.25, 0.0, 0.0]],
            primitives: vec![[0, 1, 2, 3], [0, 1, 2, 5], [0, 4, 1, 2], [0, 4, 1, 1], [0, 4, 1, 6], [9, 3, 3, 3]],
            ..Section::default()
        };
        mark_bad_primitives(&mut section);
        assert_eq!(section.primitives, vec![[0, 1, 2, 3], [0, 1, 2, 2], [0, 1, 2, 2], DEAD_PRIMITIVE, DEAD_PRIMITIVE, [9, 3, 3, 3]]);
    }
}
