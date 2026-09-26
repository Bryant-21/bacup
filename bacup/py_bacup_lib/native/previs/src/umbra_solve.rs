//! Clean-room visibility solve: turns a previs scene into the cell/portal
//! graph an Umbra tome encodes.
//!
//! Umbra culls at runtime by walking portals from the camera's cell, so the
//! solve only has to describe empty space. It is conservative by
//! construction: every opening between cells gets a portal at least as large
//! as the opening, and every target is listed in each cell it can be seen from
//! (and never in none). Nothing here reproduces the CK optimizer.
//!
//! The solve reads a scene as an [`Occupancy`]: solid voxels on the world
//! lattice, which a planner fills triangle by triangle without keeping any.

use rayon::prelude::*;
use rustc_hash::FxHashMap;
use serde::Serialize;
use std::sync::Mutex;
use std::time::Instant;

use crate::combined::CombinedRow;
use crate::f32ops::Vec3;
use crate::scene::{
    Geometry, INSTANCE_GATE, INSTANCE_OCCLUDER, INSTANCE_TARGET, Instance, LandSurface, Scene, SceneSink, instance,
    land_instance, visible_lands,
};

#[derive(Clone, Copy, Debug)]
pub struct SolveParams {
    /// Tile split grid (retail `T 512`); tome bounds are snapped to it.
    pub tile_size: f32,
    /// Cells are labelled within blocks of this edge, so a doorway or a leak
    /// inside a block cannot join two spaces into one cell whose portals
    /// bypass it. At 512 the Whitespring bunker saw three times what CK's
    /// tome shows; at 128 about as much. Must divide `tile_size`.
    pub block_size: f32,
    /// Occluder voxel edge; gaps narrower than about two voxels close.
    pub voxel_size: f32,
    /// Boxes with solid voxels wider than this split on the tile grid before
    /// the per-tile cell budget applies.
    pub max_voxel_tile: f32,
    /// A voxelized tile with more merged cells than this is split further
    /// (retail leaf tiles hold 50-90 cells, never over 95).
    pub max_tile_cells: u32,
    /// Cell-tree boxes this small stop splitting and take their majority
    /// cell. At one voxel the tree is exact in power-of-two tiles; at 128 a
    /// thin wall inside a block hands open space on one side to the cell on
    /// the other, 0.3% of the Whitespring bunker, for about 20% less tome.
    pub min_tree_leaf: f32,
    /// Sealed empty pockets with fewer voxels than this (a 3-voxel cube:
    /// hollows inside walls and props) cannot hold the camera and count as
    /// solid instead of becoming cells.
    pub min_cell_voxels: usize,
    /// Objects are listed in every cell with a voxel this close to their
    /// bounds. A surface's own voxels are solid, so it is seen from the empty
    /// voxels next to them (every CK listing is within 32 units of its cell).
    pub list_margin: f32,
    /// Flush neighbouring cells merge when the opening between them covers
    /// at least this fraction of the smaller cell's face on the plane between.
    pub merge_open_fraction: f32,
    /// ...and when the two fill at least this fraction of their union's box.
    pub merge_fill: f32,
    /// Merged cells grow no longer than this on any axis.
    pub max_merged_cell: f32,
}

impl Default for SolveParams {
    fn default() -> Self {
        Self {
            tile_size: 512.0,
            block_size: 128.0,
            voxel_size: 16.0,
            max_voxel_tile: 4096.0,
            max_tile_cells: 96,
            min_tree_leaf: 16.0,
            min_cell_voxels: 27,
            list_margin: 32.0,
            merge_open_fraction: 0.5,
            merge_fill: 0.9,
            max_merged_cell: 512.0,
        }
    }
}

impl SolveParams {
    /// Block edge in voxels.
    fn block_voxels(&self) -> usize {
        (self.block_size / self.voxel_size).round().max(1.0) as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    fn overlaps(&self, other: &Aabb) -> bool {
        (0..3).all(|a| self.min[a] <= other.max[a] && other.min[a] <= self.max[a])
    }

    fn grown(&self, by: f32) -> Aabb {
        Aabb { min: self.min.map(|v| v - by), max: self.max.map(|v| v + by) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PortalTarget {
    /// A cell of another tile, reached across a tile face.
    Tile { tile: u32, cell: u32 },
    /// Leaves the tome.
    Outside,
}

/// An axis-aligned quad on one face of a cell's tile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Portal {
    /// 0–5: axis is `face >> 1`, the low bit selects the max side.
    pub face: u8,
    /// Plane position along the face axis.
    pub plane: f32,
    /// Quad extent on the other two axes, in increasing axis order.
    pub extent: [[f32; 2]; 2],
    pub target: PortalTarget,
}

/// A way through a door to `cell` of the same tile, open only while the
/// door is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GatePortal {
    /// Index into `VisModel::gates`.
    pub gate: u32,
    pub cell: u32,
    /// The door's voxels between the two cells.
    pub bounds: Aabb,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub bounds: Aabb,
    pub portals: Vec<Portal>,
    pub gates: Vec<GatePortal>,
    /// Indices into `VisModel::objects`.
    pub objects: Vec<u32>,
}

/// Point-to-cell lookup inside a tile: leaves are `None` for solid space.
#[derive(Clone, Debug, PartialEq)]
pub enum KdNode {
    Split { axis: u8, value: f32, low: Box<KdNode>, high: Box<KdNode> },
    Leaf { cell: Option<u32> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub bounds: Aabb,
    pub tree: KdNode,
    pub cells: Vec<Cell>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    /// Bethesda target ID (`Instance::object_id`).
    pub user_id: u32,
    pub bounds: Aabb,
    /// The runtime culls the object beyond this distance (`f32::MAX`: never).
    pub max_distance: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisModel {
    pub bounds: Aabb,
    /// Tile hierarchy over `bounds`; its leaves hold indices into `tiles`.
    pub top: KdNode,
    pub tiles: Vec<Tile>,
    pub objects: Vec<Object>,
    /// Gate (door) IDs.
    pub gates: Vec<u32>,
}

const NO_GATE: u32 = u32::MAX;

fn transform_point(t: &[f32; 12], p: &Vec3) -> Vec3 {
    std::array::from_fn(|a| t[a * 4] * p[0] + t[a * 4 + 1] * p[1] + t[a * 4 + 2] * p[2] + t[a * 4 + 3])
}

fn triangle_bounds(tri: &[Vec3; 3]) -> Aabb {
    Aabb {
        min: std::array::from_fn(|a| tri[0][a].min(tri[1][a]).min(tri[2][a])),
        max: std::array::from_fn(|a| tri[0][a].max(tri[1][a]).max(tri[2][a])),
    }
}

fn sub(a: &Vec3, b: &Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: &Vec3, b: &Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: &Vec3, b: &Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Separating-axis triangle/box overlap test (box given by centre and half size).
fn triangle_overlaps_box(tri: &[Vec3; 3], centre: &Vec3, half: &Vec3) -> bool {
    let v = [sub(&tri[0], centre), sub(&tri[1], centre), sub(&tri[2], centre)];
    let edges = [sub(&v[1], &v[0]), sub(&v[2], &v[1]), sub(&v[0], &v[2])];
    let separated = |axis: &Vec3| {
        let p = [dot(&v[0], axis), dot(&v[1], axis), dot(&v[2], axis)];
        let radius = half[0] * axis[0].abs() + half[1] * axis[1].abs() + half[2] * axis[2].abs();
        p[0].min(p[1]).min(p[2]) > radius || p[0].max(p[1]).max(p[2]) < -radius
    };
    let units = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for unit in &units {
        for edge in &edges {
            let axis = cross(unit, edge);
            if axis != [0.0; 3] && separated(&axis) {
                return false;
            }
        }
    }
    if units.iter().any(separated) {
        return false;
    }
    let normal = cross(&edges[0], &edges[1]);
    normal == [0.0; 3] || !separated(&normal)
}

/// Solid occluder voxels on the world lattice, where voxel `i` spans
/// `[i * voxel, (i + 1) * voxel)` on each axis, with everything else the
/// solve reads from a scene: its targets, its gates and the Z range its
/// geometry spans.
///
/// The lattice is fixed before any geometry is read, so a planner rasterizes
/// each triangle as it arrives and keeps none. Voxels are stored one bit each
/// per touched block of `block`³ voxels. A voxel is solid when an occluder
/// triangle touches it, by the separating-axis test on the voxel's box.
pub struct Occupancy {
    voxel: f32,
    /// Block edge in voxels (a power of two).
    block: usize,
    /// Bit words per block.
    words: usize,
    slots: FxHashMap<[i32; 3], u32>,
    bits: Vec<u64>,
    /// The last block looked up, which the next voxel usually shares.
    recent: Option<([i32; 3], u32)>,
    /// Voxel range rasterized on each axis (all of it when `None`).
    footprint: Option<[[i32; 2]; 3]>,
    /// Voxels a door's triangles touch, with the door's gate index; they are
    /// solid too, so a closed door separates cells.
    gate_voxels: Vec<([i32; 3], u32)>,
    pub objects: Vec<Object>,
    /// Gate (door) IDs, indexed by gate.
    pub gates: Vec<u32>,
    /// Z extent of every occluder triangle, inside the footprint or not.
    z_range: [f32; 2],
    scratch: Vec<Vec3>,
}

impl Occupancy {
    /// An empty occupancy on `params`' lattice. With a `footprint` (a world
    /// box, unbounded axes infinite) only voxels inside it are kept: an
    /// exterior cluster's tome never reaches beyond its CELLs, and geometry
    /// out of the camera's reach does not occlude.
    pub fn new(params: &SolveParams, footprint: Option<Aabb>) -> Self {
        let block = params.block_voxels();
        assert!(block.is_power_of_two(), "block_size / voxel_size must be a power of two");
        let voxel = params.voxel_size;
        let index = |v: f32| (v as f64 / voxel as f64).floor() as i32;
        Occupancy {
            voxel,
            block,
            words: (block * block * block).div_ceil(64),
            slots: FxHashMap::default(),
            bits: Vec::new(),
            recent: None,
            footprint: footprint.map(|f| std::array::from_fn(|a| [index(f.min[a]), (f.max[a] as f64 / voxel as f64).ceil() as i32])),
            gate_voxels: Vec::new(),
            objects: Vec::new(),
            gates: Vec::new(),
            z_range: [f32::INFINITY, f32::NEG_INFINITY],
            scratch: Vec::new(),
        }
    }

    /// Every instance of `scene` in order, as a planner streams them.
    pub fn from_scene(scene: &Scene, params: &SolveParams) -> Self {
        let mut occupancy = Occupancy::new(params, None);
        for instance in &scene.instances {
            if let Some(geometry) = scene.geometries.get(instance.model_index as usize) {
                occupancy.add_instance(instance, geometry);
            }
        }
        occupancy.seal();
        occupancy
    }

    /// Rasterizes an occluder or gate instance and records a target's box.
    pub fn add_instance(&mut self, instance: &Instance, geometry: &Geometry) {
        if instance.flags & (INSTANCE_OCCLUDER | INSTANCE_GATE) != 0 {
            let gate = if instance.flags & INSTANCE_GATE != 0 {
                self.gates.push(instance.object_id);
                self.gates.len() as u32 - 1
            } else {
                NO_GATE
            };
            let mut world = std::mem::take(&mut self.scratch);
            world.clear();
            world.extend(geometry.vertices.iter().map(|v| transform_point(&instance.transform, v)));
            for t in &geometry.triangles {
                let (Some(a), Some(b), Some(c)) = (world.get(t[0] as usize), world.get(t[1] as usize), world.get(t[2] as usize))
                else {
                    continue;
                };
                self.add_triangle(&[*a, *b, *c], gate);
            }
            self.scratch = world;
        }
        if instance.flags & INSTANCE_TARGET != 0 && !geometry.vertices.is_empty() {
            let mut bounds = Aabb { min: [f32::INFINITY; 3], max: [f32::NEG_INFINITY; 3] };
            for v in &geometry.vertices {
                let p = transform_point(&instance.transform, v);
                for a in 0..3 {
                    bounds.min[a] = bounds.min[a].min(p[a]);
                    bounds.max[a] = bounds.max[a].max(p[a]);
                }
            }
            // CK marks small models with the vector `(0, 0, distance)`, else `(0, 0, -1)`.
            let max_distance = if instance.vector[2] > 0.0 { instance.vector[2] } else { f32::MAX };
            self.objects.push(Object { user_id: instance.object_id, bounds, max_distance });
        }
    }

    fn add_triangle(&mut self, tri: &[Vec3; 3], gate: u32) {
        let bounds = triangle_bounds(tri);
        self.z_range = [self.z_range[0].min(bounds.min[2]), self.z_range[1].max(bounds.max[2])];
        if !(0..3).all(|a| bounds.min[a].is_finite() && bounds.max[a].is_finite()) {
            return;
        }
        let voxel = self.voxel;
        let index = |v: f32| (v as f64 / voxel as f64).floor() as i32;
        let mut lo: [i32; 3] = std::array::from_fn(|a| index(bounds.min[a]));
        let mut hi: [i32; 3] = std::array::from_fn(|a| index(bounds.max[a]) + 1);
        if let Some(footprint) = self.footprint {
            for a in 0..3 {
                lo[a] = lo[a].max(footprint[a][0]);
                hi[a] = hi[a].min(footprint[a][1]);
            }
        }
        if (0..3).any(|a| lo[a] >= hi[a]) {
            return;
        }
        let normal = cross(&sub(&tri[1], &tri[0]), &sub(&tri[2], &tri[0]));
        // Walk columns along the dominant normal axis and test only the voxels
        // the triangle's plane crosses: cost follows area, not bounding volume.
        let axis = (0..3).max_by(|&a, &b| normal[a].abs().total_cmp(&normal[b].abs())).unwrap();
        if normal[axis] == 0.0 {
            return;
        }
        let [ua, va] = other_axes(axis);
        let offset = dot(&normal, &tri[0]);
        let plane_at = |u: f32, v: f32| (offset - normal[ua] * u - normal[va] * v) / normal[axis];
        let half = voxel * 0.5;
        for v in lo[va]..hi[va] {
            for u in lo[ua]..hi[ua] {
                let (u0, v0) = (u as f32 * voxel, v as f32 * voxel);
                let corners = [plane_at(u0, v0), plane_at(u0 + voxel, v0), plane_at(u0, v0 + voxel), plane_at(u0 + voxel, v0 + voxel)];
                let (low, high) = corners.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), &c| (l.min(c), h.max(c)));
                let from = ((low / voxel).floor() - 1.0).max(lo[axis] as f32) as i32;
                let to = ((high / voxel).floor() + 2.0).min(hi[axis] as f32).max(from as f32) as i32;
                for w in from..to {
                    let mut at = [0; 3];
                    at[axis] = w;
                    at[ua] = u;
                    at[va] = v;
                    if gate == NO_GATE && self.is_solid(at) {
                        continue;
                    }
                    let centre = at.map(|i| i as f32 * voxel + half);
                    if triangle_overlaps_box(tri, &centre, &[half; 3]) {
                        self.set_solid(at);
                        if gate != NO_GATE {
                            self.gate_voxels.push((at, gate));
                        }
                    }
                }
            }
        }
    }

    /// Block coordinate of voxel `at` and the voxel's bit in that block.
    fn locate(&self, at: [i32; 3]) -> ([i32; 3], usize) {
        let shift = self.block.trailing_zeros();
        let mask = self.block as i32 - 1;
        let local = at.map(|v| (v & mask) as usize);
        (at.map(|v| v >> shift), (local[2] * self.block + local[1]) * self.block + local[0])
    }

    fn slot(&mut self, block: [i32; 3], create: bool) -> Option<u32> {
        if let Some((recent, slot)) = self.recent
            && recent == block
        {
            return Some(slot);
        }
        let slot = match self.slots.get(&block) {
            Some(&slot) => slot,
            None if create => {
                let slot = (self.bits.len() / self.words) as u32;
                self.bits.resize(self.bits.len() + self.words, 0);
                self.slots.insert(block, slot);
                slot
            }
            None => return None,
        };
        self.recent = Some((block, slot));
        Some(slot)
    }

    fn is_solid(&mut self, at: [i32; 3]) -> bool {
        let (block, bit) = self.locate(at);
        self.slot(block, false).is_some_and(|slot| self.bits[slot as usize * self.words + bit / 64] >> (bit % 64) & 1 != 0)
    }

    fn set_solid(&mut self, at: [i32; 3]) {
        let (block, bit) = self.locate(at);
        let slot = self.slot(block, true).unwrap();
        self.bits[slot as usize * self.words + bit / 64] |= 1 << (bit % 64);
    }

    /// Solid bits of a touched block, bit `(z * block + y) * block + x`.
    fn block_bits(&self, block: [i32; 3]) -> Option<&[u64]> {
        let slot = *self.slots.get(&block)? as usize;
        Some(&self.bits[slot * self.words..(slot + 1) * self.words])
    }

    /// Adds `other`'s solid voxels and Z range.
    fn merge(&mut self, other: &Occupancy) {
        for (block, slot) in &other.slots {
            let source = &other.bits[*slot as usize * other.words..(*slot as usize + 1) * other.words];
            let target = self.slot(*block, true).unwrap() as usize * self.words;
            for (word, bits) in self.bits[target..target + self.words].iter_mut().zip(source) {
                *word |= bits;
            }
        }
        self.z_range = [self.z_range[0].min(other.z_range[0]), self.z_range[1].max(other.z_range[1])];
    }

    /// Orders the gate voxels; call once every instance is in.
    pub fn seal(&mut self) {
        self.gate_voxels.sort_unstable();
        self.gate_voxels.dedup();
    }

    /// Touched blocks lying inside `bounds`, whose faces are block planes.
    fn blocks_within(&self, bounds: &Aabb) -> Vec<[i32; 3]> {
        let edge = self.voxel * self.block as f32;
        let lo: [i32; 3] = std::array::from_fn(|a| (bounds.min[a] / edge).round() as i32);
        let hi: [i32; 3] = std::array::from_fn(|a| (bounds.max[a] / edge).round() as i32);
        self.slots.keys().copied().filter(|b| (0..3).all(|a| lo[a] <= b[a] && b[a] < hi[a])).collect()
    }

    /// The number of cells labelling the empty voxels of a touched block gives.
    fn block_cells(&self, block: [i32; 3], min_cell_voxels: usize) -> u32 {
        let mut cells = Block::from_bits(self.block_bits(block).unwrap(), self.block);
        label_block(&mut cells, self.block, [self.block; 3], 0, min_cell_voxels, &mut Vec::new(), &mut Vec::new())
    }

    /// Solid and gate voxels of every touched block, in world voxel
    /// coordinates, for comparisons and tests.
    pub fn voxels(&self) -> (Vec<[i32; 3]>, Vec<([i32; 3], u32)>) {
        let b = self.block as i32;
        let mut solid = Vec::new();
        for (block, slot) in &self.slots {
            let bits = &self.bits[*slot as usize * self.words..(*slot as usize + 1) * self.words];
            for bit in 0..self.block.pow(3) {
                if bits[bit / 64] >> (bit % 64) & 1 != 0 {
                    let local = [bit % self.block, bit / self.block % self.block, bit / (self.block * self.block)];
                    solid.push(std::array::from_fn(|a| block[a] * b + local[a] as i32));
                }
            }
        }
        solid.sort_unstable();
        (solid, self.gate_voxels.clone())
    }
}

/// Streams a planned scene into an [`Occupancy`]. Reference models are
/// pooled like a scene pools them, so every instance of a model is
/// rasterized from the first reference's copy, exactly as a scene file
/// would present it to the solve.
pub struct OccupancySink {
    occupancy: Occupancy,
    params: SolveParams,
    footprint: Option<Aabb>,
    /// First-seen geometry of each model, by local index.
    pool: FxHashMap<String, Vec<Option<Geometry>>>,
    /// Group shapes of the CELL being read.
    pending: Occupancy,
    pending_rows: usize,
    pending_targets: bool,
    /// Targets of committed group shapes, which follow every other target.
    combined_objects: Vec<Object>,
    has_target: bool,
    has_geometry: bool,
    timing: OccupancyTiming,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct OccupancyTiming {
    pub land_seconds: f64,
    pub direct_seconds: f64,
    pub combined_seconds: f64,
    pub commit_seconds: f64,
    pub instances: usize,
    pub triangles_submitted: usize,
}

impl OccupancySink {
    pub fn new(params: &SolveParams, footprint: Option<Aabb>) -> Self {
        OccupancySink {
            occupancy: Occupancy::new(params, footprint),
            params: *params,
            footprint,
            pool: FxHashMap::default(),
            pending: Occupancy::new(params, footprint),
            pending_rows: 0,
            pending_targets: false,
            combined_objects: Vec::new(),
            has_target: false,
            has_geometry: false,
            timing: OccupancyTiming::default(),
        }
    }

    pub fn timing(&self) -> OccupancyTiming { self.timing }

    pub fn finish(self) -> Occupancy {
        let mut occupancy = self.occupancy;
        occupancy.objects.extend(self.combined_objects);
        occupancy.seal();
        occupancy
    }

    fn take_pending(&mut self) -> Occupancy {
        self.pending_rows = 0;
        self.pending_targets = false;
        std::mem::replace(&mut self.pending, Occupancy::new(&self.params, self.footprint))
    }
}

impl SceneSink for OccupancySink {
    fn lands(&mut self, lands: &[LandSurface]) {
        let started = Instant::now();
        for (index, land) in visible_lands(lands).into_iter().enumerate() {
            let (geometry, instance) = land_instance(land, index as u32);
            self.timing.instances += 1;
            self.timing.triangles_submitted += geometry.triangles.len();
            self.occupancy.add_instance(&instance, &geometry);
            self.has_target = true;
            self.has_geometry = true;
        }
        self.timing.land_seconds += started.elapsed().as_secs_f64();
    }

    fn pooled(&mut self, model_key: &str, direct: Scene) {
        let started = Instant::now();
        self.has_geometry |= !direct.geometries.is_empty();
        if !self.pool.contains_key(model_key) {
            self.pool.insert(model_key.to_string(), Vec::new());
        }
        let models = self.pool.get_mut(model_key).unwrap();
        for (local, geometry) in direct.geometries.into_iter().enumerate() {
            if models.len() <= local {
                models.resize_with(local + 1, || None);
            }
            models[local].get_or_insert(geometry);
        }
        for instance in &direct.instances {
            if let Some(Some(geometry)) = models.get(instance.model_index as usize) {
                self.timing.instances += 1;
                self.timing.triangles_submitted += geometry.triangles.len();
                self.occupancy.add_instance(instance, geometry);
            }
            self.has_target |= instance.flags & INSTANCE_TARGET != 0;
        }
        self.timing.direct_seconds += started.elapsed().as_secs_f64();
    }

    fn combined(&mut self, (geometry, transform, flags): CombinedRow) {
        let started = Instant::now();
        self.timing.instances += 1;
        self.timing.triangles_submitted += geometry.triangles.len();
        // Until the CELL commits, a target's user ID is its row index.
        let instance = instance(self.pending_rows as u32, 0, transform, flags, &geometry);
        self.pending.add_instance(&instance, &geometry);
        self.pending_rows += 1;
        self.pending_targets |= flags & INSTANCE_TARGET != 0;
        self.timing.combined_seconds += started.elapsed().as_secs_f64();
    }

    fn commit_combined(&mut self, occluders_only: bool, object_id: &dyn Fn(usize) -> u32) {
        let started = Instant::now();
        let targets = self.pending_targets;
        let pending = self.take_pending();
        if !occluders_only {
            self.has_target |= targets;
            self.combined_objects.extend(
                pending.objects.iter().map(|o| Object { user_id: object_id(o.user_id as usize), ..o.clone() }),
            );
        }
        self.occupancy.merge(&pending);
        self.has_geometry = true;
        self.timing.commit_seconds += started.elapsed().as_secs_f64();
    }

    fn discard_combined(&mut self) {
        self.take_pending();
    }

    fn has_target(&self) -> bool {
        self.has_target
    }

    fn has_geometry(&self) -> bool {
        self.has_geometry
    }
}

/// The voxels of one leaf tile, stored per `block`-voxel cube: only blocks
/// holding a solid voxel have voxel data; every other block is uniform air,
/// one cell (`air`). `solid` marks voxels touched by occluder surfaces,
/// `cell` labels each empty voxel with its connected component, mapped
/// through `merged` once the tile's cells are merged.
struct Grid {
    origin: Vec3,
    dims: [usize; 3],
    voxel: f32,
    block: usize,
    block_dims: [usize; 3],
    blocks: Vec<Option<Block>>,
    air: Vec<u32>,
    merged: Option<Vec<u32>>,
    /// Voxels a door's triangles touch, with the door's gate index.
    gate_voxels: Vec<([usize; 3], u32)>,
}

/// Voxel data of one touched block, indexed `(z * block + y) * block + x`.
struct Block {
    solid: Vec<bool>,
    cell: Vec<u32>,
}

impl Block {
    fn from_bits(bits: &[u64], block: usize) -> Self {
        let volume = block * block * block;
        Block { solid: (0..volume).map(|bit| bits[bit / 64] >> (bit % 64) & 1 != 0).collect(), cell: vec![NO_CELL; volume] }
    }
}

const NO_CELL: u32 = u32::MAX;

impl Grid {
    fn new(origin: Vec3, dims: [usize; 3], voxel: f32, block: usize) -> Self {
        let block_dims: [usize; 3] = std::array::from_fn(|a| dims[a].div_ceil(block));
        let count = block_dims[0] * block_dims[1] * block_dims[2];
        Grid {
            origin,
            dims,
            voxel,
            block,
            block_dims,
            blocks: std::iter::repeat_with(|| None).take(count).collect(),
            air: vec![NO_CELL; count],
            merged: None,
            gate_voxels: Vec::new(),
        }
    }

    /// Label of in-block voxel `local` of a touched block.
    fn block_cell(&self, block: &Block, local: usize) -> u32 {
        let cell = block.cell[local];
        if cell == NO_CELL {
            return NO_CELL;
        }
        self.merged.as_ref().map_or(cell, |merged| merged[cell as usize])
    }

    /// Relabels every cell `c` as `merged[c]` (air blocks at once, touched
    /// blocks as they are read).
    fn merge(&mut self, merged: Vec<u32>) {
        for (index, air) in self.air.iter_mut().enumerate() {
            if self.blocks[index].is_none() {
                *air = merged[*air as usize];
            }
        }
        self.merged = Some(merged);
    }

    /// Blocks are a power of two wide, so voxel addressing is shifts and masks.
    fn shift_mask(&self) -> (u32, usize) {
        (self.block.trailing_zeros(), self.block - 1)
    }

    fn block_index(&self, bx: usize, by: usize, bz: usize) -> usize {
        (bz * self.block_dims[1] + by) * self.block_dims[0] + bx
    }

    /// Block index and in-block voxel index of voxel `(x, y, z)`.
    fn locate(&self, x: usize, y: usize, z: usize) -> (usize, usize) {
        let (shift, mask) = self.shift_mask();
        let index = self.block_index(x >> shift, y >> shift, z >> shift);
        (index, ((((z & mask) << shift) | (y & mask)) << shift) | (x & mask))
    }

    fn cell_at(&self, x: usize, y: usize, z: usize) -> u32 {
        let (index, local) = self.locate(x, y, z);
        match &self.blocks[index] {
            Some(block) => self.block_cell(block, local),
            None => self.air[index],
        }
    }

    /// Voxel range of block `(bx, by, bz)`, clipped to the grid.
    fn block_range(&self, bx: usize, by: usize, bz: usize) -> ([usize; 3], [usize; 3]) {
        let lo = [bx * self.block, by * self.block, bz * self.block];
        let hi = std::array::from_fn(|a| (lo[a] + self.block).min(self.dims[a]));
        (lo, hi)
    }

    /// Calls `visit(cell, voxels)` for the cells in voxel range `[lo, hi)`:
    /// once per air block, once per voxel of a touched block. Stops when
    /// `visit` returns false.
    fn scan(&self, lo: [usize; 3], hi: [usize; 3], mut visit: impl FnMut(u32, usize) -> bool) {
        let b = self.block;
        let (shift, mask) = self.shift_mask();
        for bz in lo[2] / b..hi[2].div_ceil(b) {
            for by in lo[1] / b..hi[1].div_ceil(b) {
                for bx in lo[0] / b..hi[0].div_ceil(b) {
                    let (block_lo, block_hi) = self.block_range(bx, by, bz);
                    let from: [usize; 3] = std::array::from_fn(|a| block_lo[a].max(lo[a]));
                    let to: [usize; 3] = std::array::from_fn(|a| block_hi[a].min(hi[a]));
                    let index = self.block_index(bx, by, bz);
                    match &self.blocks[index] {
                        None => {
                            let voxels = (to[0] - from[0]) * (to[1] - from[1]) * (to[2] - from[2]);
                            if !visit(self.air[index], voxels) {
                                return;
                            }
                        }
                        Some(block) => {
                            for z in from[2]..to[2] {
                                for y in from[1]..to[1] {
                                    for x in from[0]..to[0] {
                                        let local = ((((z & mask) << shift) | (y & mask)) << shift) | (x & mask);
                                        if !visit(self.block_cell(block, local), 1) {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Voxel index range covering `bounds`, clamped to the grid.
    fn range(&self, bounds: &Aabb) -> Option<([usize; 3], [usize; 3])> {
        let (mut lo, mut hi) = ([0; 3], [0; 3]);
        for a in 0..3 {
            let low = ((bounds.min[a] - self.origin[a]) / self.voxel).floor();
            let high = ((bounds.max[a] - self.origin[a]) / self.voxel).floor();
            if high < 0.0 || low >= self.dims[a] as f32 {
                return None;
            }
            lo[a] = low.max(0.0) as usize;
            hi[a] = (high as usize + 1).min(self.dims[a]);
        }
        Some((lo, hi))
    }
}

/// Labels the 6-connected empty voxels of one block (its first `size`
/// voxels on each axis) from label `next` on; a sealed pocket smaller than
/// `min_cell_voxels` becomes solid. Returns the next free label.
fn label_block(
    block: &mut Block,
    b: usize,
    size: [usize; 3],
    mut next: u32,
    min_cell_voxels: usize,
    stack: &mut Vec<[usize; 3]>,
    members: &mut Vec<usize>,
) -> u32 {
    for z in 0..size[2] {
        for y in 0..size[1] {
            for x in 0..size[0] {
                let start = (z * b + y) * b + x;
                if block.solid[start] || block.cell[start] != NO_CELL {
                    continue;
                }
                block.cell[start] = next;
                stack.push([x, y, z]);
                members.clear();
                members.push(start);
                let mut sealed = true;
                while let Some(p) = stack.pop() {
                    for axis in 0..3 {
                        for step in [-1isize, 1] {
                            let q = p[axis] as isize + step;
                            if q < 0 || q >= size[axis] as isize {
                                sealed = false;
                                continue;
                            }
                            let mut n = p;
                            n[axis] = q as usize;
                            let local = (n[2] * b + n[1]) * b + n[0];
                            if !block.solid[local] && block.cell[local] == NO_CELL {
                                block.cell[local] = next;
                                stack.push(n);
                                members.push(local);
                            }
                        }
                    }
                }
                // A component reaching the block face may continue in the
                // next block, however small its part here.
                if sealed && members.len() < min_cell_voxels {
                    // A sealed pocket too small to hold the camera: treat as solid.
                    for &local in members.iter() {
                        block.solid[local] = true;
                        block.cell[local] = NO_CELL;
                    }
                    continue;
                }
                next += 1;
            }
        }
    }
    next
}

/// Labels 6-connected empty voxels block by block (cells never cross
/// block boundaries), numbering cells in block order. Air blocks are one
/// cell each. Returns the cell count.
fn label_cells(grid: &mut Grid, min_cell_voxels: usize) -> u32 {
    let b = grid.block;
    let mut next = 0u32;
    let mut stack = Vec::new();
    let mut members = Vec::new();
    for bz in 0..grid.block_dims[2] {
        for by in 0..grid.block_dims[1] {
            for bx in 0..grid.block_dims[0] {
                let index = grid.block_index(bx, by, bz);
                let (lo, hi) = grid.block_range(bx, by, bz);
                let size = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
                match grid.blocks[index].as_mut() {
                    None => {
                        grid.air[index] = next;
                        next += 1;
                    }
                    Some(block) => next = label_block(block, b, size, next, min_cell_voxels, &mut stack, &mut members),
                }
            }
        }
    }
    next
}

/// The cells a camera in each voxel may start its walk from, voxel
/// `(z * dims[1] + y) * dims[0] + x`: an empty voxel's own cell; for a solid
/// voxel, the cells of its empty face neighbours, since the camera can stand
/// in the open part of a surface voxel (left unconstrained, the tree may hand
/// it to a pocket beside the space the camera is really in); for solid
/// voxels away from empty space, any cell.
struct CameraCells {
    origin: Vec3,
    dims: [usize; 3],
    voxel: f32,
    spans: Vec<(u32, u8)>,
    choices: Vec<u32>,
}

impl CameraCells {
    fn new(grid: &Grid) -> Self {
        let [nx, ny, nz] = grid.dims;
        let mut cells = Vec::with_capacity(nx * ny * nz);
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    cells.push(grid.cell_at(x, y, z));
                }
            }
        }
        let index = |x: usize, y: usize, z: usize| (z * ny + y) * nx + x;
        let mut spans = Vec::with_capacity(cells.len());
        let mut choices = Vec::with_capacity(cells.len());
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let start = choices.len() as u32;
                    let cell = cells[index(x, y, z)];
                    if cell != NO_CELL {
                        choices.push(cell);
                    } else {
                        for axis in 0..3 {
                            let at = [x, y, z];
                            for step in [at[axis].checked_sub(1), Some(at[axis] + 1).filter(|&q| q < grid.dims[axis])] {
                                let Some(q) = step else { continue };
                                let mut n = at;
                                n[axis] = q;
                                let neighbour = cells[index(n[0], n[1], n[2])];
                                if neighbour != NO_CELL && !choices[start as usize..].contains(&neighbour) {
                                    choices.push(neighbour);
                                }
                            }
                        }
                    }
                    spans.push((start, (choices.len() - start as usize) as u8));
                }
            }
        }
        CameraCells { origin: grid.origin, dims: grid.dims, voxel: grid.voxel, spans, choices }
    }

    /// Calls `visit` with each voxel's choices in `[lo, hi)` until it returns false.
    fn scan(&self, lo: [usize; 3], hi: [usize; 3], mut visit: impl FnMut(&[u32]) -> bool) {
        for z in lo[2]..hi[2] {
            for y in lo[1]..hi[1] {
                let row = (z * self.dims[1] + y) * self.dims[0];
                for &(start, len) in &self.spans[row + lo[0]..row + hi[0]] {
                    if !visit(&self.choices[start as usize..start as usize + len as usize]) {
                        return;
                    }
                }
            }
        }
    }
}

/// Splits `[lo, hi)` at box midpoints, which the tome stores implicitly,
/// until every leaf has a cell all its voxels accept; a leaf of voxels that
/// accept any cell is empty. Boxes no wider than `min_leaf` stop splitting
/// and take the cell most of their voxels accept.
fn build_tree(grid: &CameraCells, lo: Vec3, hi: Vec3, min_leaf: f32) -> KdNode {
    let range = |a: usize| {
        let start = ((lo[a] - grid.origin[a]) / grid.voxel).floor().max(0.0) as usize;
        let end = ((hi[a] - grid.origin[a]) / grid.voxel).ceil().min(grid.dims[a] as f32) as usize;
        let start = start.min(grid.dims[a] - 1);
        (start, end.max(start + 1).min(grid.dims[a]))
    };
    let [(x0, x1), (y0, y1), (z0, z1)] = [range(0), range(1), range(2)];
    let (from, to) = ([x0, y0, z0], [x1, y1, z1]);
    let mut common: Option<Vec<u32>> = None;
    grid.scan(from, to, |choices| {
        if choices.is_empty() {
            return true;
        }
        let common = common.get_or_insert_with(|| choices.to_vec());
        common.retain(|c| choices.contains(c));
        !common.is_empty()
    });
    let mixed = common.as_ref().is_some_and(Vec::is_empty);
    let first = common.and_then(|c| c.first().copied());
    let extent = |a: usize| hi[a] - lo[a];
    let axis = (0..3).max_by(|&a, &b| extent(a).total_cmp(&extent(b)).then(b.cmp(&a))).unwrap();
    if !mixed {
        return KdNode::Leaf { cell: first };
    }
    if extent(axis) <= min_leaf {
        let mut votes: FxHashMap<u32, usize> = FxHashMap::default();
        grid.scan(from, to, |choices| {
            for &cell in choices {
                *votes.entry(cell).or_default() += 1;
            }
            true
        });
        let majority = votes.into_iter().max_by_key(|&(cell, n)| (n, std::cmp::Reverse(cell))).map(|(c, _)| c);
        return KdNode::Leaf { cell: majority };
    }
    let value = (lo[axis] + hi[axis]) * 0.5;
    let (mut low_hi, mut high_lo) = (hi, lo);
    low_hi[axis] = value;
    high_lo[axis] = value;
    KdNode::Split {
        axis: axis as u8,
        value,
        low: Box::new(build_tree(grid, lo, low_hi, min_leaf)),
        high: Box::new(build_tree(grid, high_lo, hi, min_leaf)),
    }
}

/// The union of a scene's view volumes.
pub fn scene_volume(volumes: &[[f32; 12]]) -> Option<Aabb> {
    let mut volumes = volumes.iter();
    let first = volumes.next()?;
    let mut bounds = Aabb { min: [first[0], first[1], first[2]], max: [first[3], first[4], first[5]] };
    for v in volumes {
        for a in 0..3 {
            bounds.min[a] = bounds.min[a].min(v[a]);
            bounds.max[a] = bounds.max[a].max(v[a + 3]);
        }
    }
    Some(bounds)
}

/// Grows the planner's Z range (whose top is fixed at 8192) to cover all
/// scene geometry (`z_range`) on the 128 grid; X/Y stay on the cell grid.
fn covering_volume(mut volume: Aabb, z_range: [f32; 2]) -> Aabb {
    volume.min[2] = volume.min[2].min(z_range[0]);
    volume.max[2] = volume.max[2].max(z_range[1]);
    volume.min[2] = (volume.min[2] / 128.0).floor() * 128.0;
    volume.max[2] = (volume.max[2] / 128.0).ceil() * 128.0;
    volume
}

/// Snaps `volume` outward to the world tile grid, as CK's tome bounds are:
/// tiles then line up with the world lattice and every tile spans a power
/// of two of voxels (see [`split_plane`]).
fn tile_aligned(volume: Aabb, tile: f32) -> Aabb {
    Aabb { min: volume.min.map(|v| (v / tile).floor() * tile), max: volume.max.map(|v| (v / tile).ceil() * tile) }
}

/// Voxel index box `[lo, hi)` of each cell.
fn cell_voxel_boxes(grid: &Grid, cells: u32) -> Vec<([usize; 3], [usize; 3])> {
    let mut boxes = vec![([usize::MAX; 3], [0; 3]); cells as usize];
    let mut grow = |cell: u32, lo: [usize; 3], hi: [usize; 3]| {
        let target = &mut boxes[cell as usize];
        for a in 0..3 {
            target.0[a] = target.0[a].min(lo[a]);
            target.1[a] = target.1[a].max(hi[a]);
        }
    };
    for bz in 0..grid.block_dims[2] {
        for by in 0..grid.block_dims[1] {
            for bx in 0..grid.block_dims[0] {
                let index = grid.block_index(bx, by, bz);
                let (lo, hi) = grid.block_range(bx, by, bz);
                let Some(block) = &grid.blocks[index] else {
                    grow(grid.air[index], lo, hi);
                    continue;
                };
                for z in lo[2]..hi[2] {
                    for y in lo[1]..hi[1] {
                        for x in lo[0]..hi[0] {
                            let (_, local) = grid.locate(x, y, z);
                            let cell = grid.block_cell(block, local);
                            if cell != NO_CELL {
                                grow(cell, [x, y, z], [x + 1, y + 1, z + 1]);
                            }
                        }
                    }
                }
            }
        }
    }
    boxes
}

/// Bounds of each cell's voxels, clipped to the tile.
fn cell_bounds(grid: &Grid, cells: u32, tile: &Aabb) -> Vec<Aabb> {
    cell_voxel_boxes(grid, cells)
        .into_iter()
        .map(|(lo, hi)| Aabb {
            min: std::array::from_fn(|a| grid.origin[a] + lo[a] as f32 * grid.voxel),
            max: std::array::from_fn(|a| (grid.origin[a] + hi[a] as f32 * grid.voxel).min(tile.max[a])),
        })
        .collect()
}

/// Voxel coordinates of layer `layer` on axis `axis`, with (u, v) the other axes in order.
fn face_voxel(axis: usize, layer: usize, u: usize, v: usize) -> [usize; 3] {
    match axis {
        0 => [layer, u, v],
        1 => [u, layer, v],
        _ => [u, v, layer],
    }
}

fn other_axes(axis: usize) -> [usize; 2] {
    match axis {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    }
}

/// The cell ids on each boundary face of a voxel tile (indexed `face = axis * 2 + side`,
/// `v * dims[u axis] + u`): all a finished tile needs to link to its neighbours.
struct Faces {
    origin: Vec3,
    dims: [usize; 3],
    voxel: f32,
    cells: [Vec<u32>; 6],
}

/// A finished leaf tile. Its voxel grid is dropped as soon as the tile is
/// built, so a solve never holds more than the tiles being built.
struct Leaf {
    tile: Tile,
    /// `None` for an empty tile (one cell covering every face).
    faces: Option<Faces>,
    /// `(object, cells it touches)` for every object overlapping the tile.
    objects: Vec<(u32, Vec<u32>)>,
}

/// Marks in-tile portal targets until the tile's final index is known.
const THIS_TILE: u32 = u32::MAX;

const EPSILON: f32 = 1e-3;

/// Inputs shared by the whole tile split.
struct Split<'a> {
    occupancy: &'a Occupancy,
    volume: Aabb,
    params: &'a SolveParams,
    work: Mutex<SplitWork>,
}

#[derive(Debug, Default, Serialize)]
pub struct SplitWork {
    pub count_blocks_seconds: f64,
    pub counted_blocks: usize,
    pub grid_seconds: f64,
    pub merge_seconds: f64,
    pub rejected_probe_seconds: f64,
    pub probes: usize,
    pub rejected_probes: usize,
    pub labelled_blocks: usize,
    pub merge_passes: usize,
    pub merge_candidates: usize,
    pub merge_adjacency_seconds: f64,
    pub merge_shared_seconds: f64,
    pub merge_filter_seconds: f64,
    pub merge_sort_seconds: f64,
}

#[derive(Debug, Default, Serialize)]
pub struct SolveTiming {
    pub split_seconds: f64,
    pub assign_seconds: f64,
    pub portals_seconds: f64,
    pub collapse_seconds: f64,
    pub finish_seconds: f64,
    pub objects: usize,
    pub occupied_blocks: usize,
    pub tiles: usize,
    pub cells_before_collapse: usize,
    pub cells_after_collapse: usize,
    pub split_work: SplitWork,
}

impl Split<'_> {
    fn leaf(&self, leaf: Leaf) -> (KdNode, Vec<Leaf>) {
        (KdNode::Leaf { cell: Some(0) }, vec![leaf])
    }

    fn block_edge(&self) -> f32 {
        self.params.block_size
    }
}

fn empty_leaf(bounds: Aabb, objects: &[Object], params: &SolveParams) -> Leaf {
    Leaf {
        tile: Tile {
            bounds,
            tree: KdNode::Leaf { cell: Some(0) },
            cells: vec![Cell { bounds, portals: Vec::new(), gates: Vec::new(), objects: Vec::new() }],
        },
        faces: None,
        objects: objects
            .iter()
            .enumerate()
            .filter(|(_, o)| bounds.overlaps(&o.bounds.grown(params.list_margin)))
            .map(|(i, _)| (i as u32, vec![0]))
            .collect(),
    }
}

/// A leaf tile of `grid`, whose `count` cells are already merged.
fn voxel_leaf(bounds: Aabb, grid: Grid, count: u32, objects: &[Object], params: &SolveParams) -> Leaf {
    let grid = &grid;
    let mut cells: Vec<Cell> = cell_bounds(grid, count, &bounds)
        .into_iter()
        .map(|bounds| Cell { bounds, portals: Vec::new(), gates: Vec::new(), objects: Vec::new() })
        .collect();
    for (cell, gate) in gate_portals(grid, &bounds) {
        cells[cell as usize].gates.push(gate);
    }
    let gate_voxels: rustc_hash::FxHashSet<[usize; 3]> = grid.gate_voxels.iter().map(|&(at, _)| at).collect();
    for (axis, plane, low, high, quad) in block_portals(grid, &bounds) {
        let extent = [[quad[0], quad[1]], [quad[2], quad[3]]];
        cells[low as usize].portals.push(Portal {
            face: (axis * 2 + 1) as u8,
            plane,
            extent,
            target: PortalTarget::Tile { tile: THIS_TILE, cell: high },
        });
        cells[high as usize].portals.push(Portal {
            face: (axis * 2) as u8,
            plane,
            extent,
            target: PortalTarget::Tile { tile: THIS_TILE, cell: low },
        });
    }
    let faces = Faces {
        origin: grid.origin,
        dims: grid.dims,
        voxel: grid.voxel,
        cells: std::array::from_fn(|face| {
            let (axis, side) = (face / 2, face % 2);
            let [ua, va] = other_axes(axis);
            let layer = if side == 1 { grid.dims[axis] - 1 } else { 0 };
            let inward = if side == 1 { layer.checked_sub(1) } else { Some(1).filter(|&l| l < grid.dims[axis]) };
            let mut out = Vec::with_capacity(grid.dims[ua] * grid.dims[va]);
            for v in 0..grid.dims[va] {
                for u in 0..grid.dims[ua] {
                    let [x, y, z] = face_voxel(axis, layer, u, v);
                    let mut cell = grid.cell_at(x, y, z);
                    // Gate portals only join cells of one tile, so a door on the
                    // tile face would close its doorway for good: leave it open.
                    if let Some(inward) = inward.filter(|_| cell == NO_CELL && gate_voxels.contains(&[x, y, z])) {
                        let [x, y, z] = face_voxel(axis, inward, u, v);
                        cell = grid.cell_at(x, y, z);
                    }
                    out.push(cell);
                }
            }
            out
        }),
    };
    let mut touched = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        let reach = object.bounds.grown(params.list_margin);
        let Some((lo, hi)) = bounds.overlaps(&reach).then(|| grid.range(&reach)).flatten() else { continue };
        let mut seen = vec![false; count as usize];
        grid.scan(lo, hi, |cell, _| {
            if cell != NO_CELL {
                seen[cell as usize] = true;
            }
            true
        });
        let hit: Vec<u32> = (0..count).filter(|&c| seen[c as usize]).collect();
        if !hit.is_empty() {
            touched.push((index as u32, hit));
        }
    }
    Leaf {
        tile: Tile { bounds, tree: build_tree(&CameraCells::new(grid), bounds.min, bounds.max, params.min_tree_leaf), cells },
        faces: Some(faces),
        objects: touched,
    }
}

/// Gate portals between the cells on the two sides of each door: the cells
/// of the empty voxels at both ends of a straight run of one door's voxels.
/// Each portal's box holds the runs joining its two cells (clipped to the
/// tile). Returned as `(from, portal)`, in both directions.
fn gate_portals(grid: &Grid, tile: &Aabb) -> Vec<(u32, GatePortal)> {
    let mut gate_of: FxHashMap<[usize; 3], u32> = FxHashMap::default();
    for &(at, gate) in &grid.gate_voxels {
        gate_of.entry(at).or_insert(gate);
    }
    let mut runs = std::collections::BTreeMap::new();
    for &(start, gate) in &grid.gate_voxels {
        if gate_of[&start] != gate {
            continue;
        }
        for axis in 0..3 {
            let offset = |at: [usize; 3], by: isize| {
                let q = at[axis].checked_add_signed(by).filter(|&q| q < grid.dims[axis])?;
                let mut n = at;
                n[axis] = q;
                Some(n)
            };
            let before = offset(start, -1);
            if before.is_some_and(|b| gate_of.get(&b) == Some(&gate)) {
                continue;
            }
            let mut end = start;
            while let Some(next) = offset(end, 1).filter(|n| gate_of.get(n) == Some(&gate)) {
                end = next;
            }
            let (Some(before), Some(after)) = (before, offset(end, 1)) else { continue };
            let (a, b) = (grid.cell_at(before[0], before[1], before[2]), grid.cell_at(after[0], after[1], after[2]));
            if a == NO_CELL || b == NO_CELL || a == b {
                continue;
            }
            let hi = end.map(|v| v + 1);
            let (lo, top) = runs.entry((gate, a.min(b), a.max(b))).or_insert((start, hi));
            for k in 0..3 {
                lo[k] = lo[k].min(start[k]);
                top[k] = top[k].max(hi[k]);
            }
        }
    }
    let mut out = Vec::with_capacity(runs.len() * 2);
    for ((gate, a, b), (lo, hi)) in runs {
        let bounds = Aabb {
            min: std::array::from_fn(|k| grid.origin[k] + lo[k] as f32 * grid.voxel),
            max: std::array::from_fn(|k| (grid.origin[k] + hi[k] as f32 * grid.voxel).min(tile.max[k])),
        };
        out.push((a, GatePortal { gate, cell: b, bounds }));
        out.push((b, GatePortal { gate, cell: a, bounds }));
    }
    out
}

/// World block coordinate of the block whose min corner is `corner`.
fn block_at(corner: &Vec3, edge: f32) -> [i32; 3] {
    corner.map(|v| (v / edge).round() as i32)
}

/// Splits `bounds` at tile-grid lines until each box holds no solid voxel or
/// is small enough for [`region`] (or is one tile wide). `blocks` are the
/// touched blocks inside `bounds`; every split plane is a block plane, so
/// each goes to exactly one side.
fn split_tiles(bounds: Aabb, blocks: Vec<[i32; 3]>, split: &Split) -> (KdNode, Vec<Leaf>) {
    let params = split.params;
    if blocks.is_empty() {
        return split.leaf(empty_leaf(bounds, &split.occupancy.objects, params));
    }
    let extent = |a: usize| bounds.max[a] - bounds.min[a];
    let longest = (0..3).max_by(|&a, &b| extent(a).total_cmp(&extent(b))).unwrap();
    let splittable = extent(longest) > params.tile_size + EPSILON;
    let margin = splittable.then(|| empty_margin(&bounds, &blocks, split)).flatten();
    if margin.is_none() && (!splittable || extent(longest) <= params.max_voxel_tile + EPSILON) {
        return region(bounds, &blocks, split);
    }
    let (axis, plane) = margin.unwrap_or_else(|| (longest, split_plane(&bounds, longest, params.tile_size)));
    let (mut low, mut high) = (bounds, bounds);
    low.max[axis] = plane;
    high.min[axis] = plane;
    let cut = (plane / split.block_edge()).round() as i32;
    let (low_blocks, high_blocks): (Vec<[i32; 3]>, Vec<[i32; 3]>) = blocks.into_iter().partition(|b| b[axis] < cut);
    let ((low_tree, mut leaves), (mut high_tree, high)) =
        rayon::join(|| split_tiles(low, low_blocks, split), || split_tiles(high, high_blocks, split));
    offset_leaves(&mut high_tree, leaves.len() as u32);
    leaves.extend(high);
    let tree = KdNode::Split { axis: axis as u8, value: plane, low: Box::new(low_tree), high: Box::new(high_tree) };
    (tree, leaves)
}

/// The tile-grid plane that cuts the largest slab without a solid voxel off
/// `bounds`, so empty air (mostly above terrain) becomes one empty tile
/// instead of being voxelized.
fn empty_margin(bounds: &Aabb, blocks: &[[i32; 3]], split: &Split) -> Option<(usize, f32)> {
    let edge = split.block_edge();
    let mut used = Aabb { min: bounds.max, max: bounds.min };
    for block in blocks {
        for a in 0..3 {
            used.min[a] = used.min[a].min(block[a] as f32 * edge);
            used.max[a] = used.max[a].max((block[a] + 1) as f32 * edge);
        }
    }
    let (volume, size) = (&split.volume, split.params.tile_size);
    let mut best: Option<(f32, usize, f32)> = None;
    for a in 0..3 {
        let low = volume.min[a] + ((used.min[a] - volume.min[a]) / size).floor() * size;
        let high = volume.min[a] + ((used.max[a] - volume.min[a]) / size).ceil() * size;
        for (plane, removed) in [(low, low - bounds.min[a]), (high, bounds.max[a] - high)] {
            let inside = plane > bounds.min[a] + EPSILON && plane < bounds.max[a] - EPSILON;
            if inside && removed > EPSILON && best.is_none_or(|(most, _, _)| removed > most) {
                best = Some((removed, a, plane));
            }
        }
    }
    best.map(|(_, axis, plane)| (axis, plane))
}

/// The tile-grid plane on `axis` that leaves a power-of-two number of tiles
/// below it: half of them when the count already is one, else the largest
/// power of two under it. A tile tree splits only at midpoints of the tile
/// box, so a cell wall lands exactly on its voxel plane only in a tile that
/// spans a power of two of voxels; anywhere else the tree places it up to
/// half a leaf away and the runtime starts its portal walk from the wrong
/// cell (culling nearly everything in view).
fn split_plane(bounds: &Aabb, axis: usize, size: f32) -> f32 {
    let tiles = ((bounds.max[axis] - bounds.min[axis]) / size - EPSILON).ceil() as u32;
    let below = if tiles.is_power_of_two() { tiles / 2 } else { 1 << tiles.ilog2() };
    bounds.min[axis] + below as f32 * size
}

fn spans_power_of_two_voxels(extent: f32, voxel: f32) -> bool {
    ((extent / voxel).round() as u32).is_power_of_two()
}

/// Per-block cell count and solid flag of a region, in block order
/// `(z * dims[1] + y) * dims[0] + x`: all the region's split needs. Labels
/// themselves are rebuilt per leaf tile and dropped with it.
struct Region {
    dims: [usize; 3],
    count: Vec<u32>,
    solid: Vec<bool>,
}

/// Labels each touched block of `bounds` once to learn its cell count, then
/// splits the box like [`split_region`].
fn region(bounds: Aabb, blocks: &[[i32; 3]], split: &Split) -> (KdNode, Vec<Leaf>) {
    let started = Instant::now();
    let edge = split.block_edge();
    let origin = block_at(&bounds.min, edge);
    let dims: [usize; 3] = std::array::from_fn(|a| ((bounds.max[a] - bounds.min[a]) / edge).round() as usize);
    let index = |b: &[i32; 3]| {
        let at: [usize; 3] = std::array::from_fn(|a| (b[a] - origin[a]) as usize);
        (at[2] * dims[1] + at[1]) * dims[0] + at[0]
    };
    let counted: Vec<(usize, u32)> = blocks
        .par_iter()
        .map(|b| (index(b), split.occupancy.block_cells(*b, split.params.min_cell_voxels)))
        .collect();
    let volume = dims[0] * dims[1] * dims[2];
    let mut region = Region { dims, count: vec![1; volume], solid: vec![false; volume] };
    for (at, count) in counted {
        region.count[at] = count;
        region.solid[at] = true;
    }
    {
        let mut work = split.work.lock().unwrap();
        work.count_blocks_seconds += started.elapsed().as_secs_f64();
        work.counted_blocks += blocks.len();
    }
    split_region(&region, [0; 3], dims, bounds, split)
}

/// Splits the block range `[lo, hi)` of a region (world box `bounds`): a
/// sub-box with no solid voxel is an empty tile, one whose merged cells fit
/// the cell budget becomes a tile of its own. Retail tiles are sized the same
/// way: 1024 to 4096 wide, never over 95 cells.
fn split_region(region: &Region, lo: [usize; 3], hi: [usize; 3], bounds: Aabb, split: &Split) -> (KdNode, Vec<Leaf>) {
    let params = split.params;
    let extent = |a: usize| bounds.max[a] - bounds.min[a];
    let longest = (0..3).max_by(|&a, &b| extent(a).total_cmp(&extent(b))).unwrap();
    let splittable = extent(longest) > params.tile_size + EPSILON;
    let (mut cells, mut solid) = (0usize, false);
    for z in lo[2]..hi[2] {
        for y in lo[1]..hi[1] {
            for x in lo[0]..hi[0] {
                let index = (z * region.dims[1] + y) * region.dims[0] + x;
                cells += region.count[index] as usize;
                solid |= region.solid[index];
            }
        }
    }
    if !solid {
        return split.leaf(empty_leaf(bounds, &split.occupancy.objects, params));
    }
    // Block cells only merge, so `cells` bounds the tile's count from above.
    // Merged cells are at most `max_merged_cell` long, so a box of mostly
    // empty space holds at least `fewest` of them: past the budget it cannot fit.
    let budget = params.max_tile_cells as usize;
    let fewest: f32 = (0..3).map(|a| (extent(a) / params.max_merged_cell - EPSILON).ceil()).product();
    // A voxel tile must span a power of two of voxels on every axis.
    let uneven = (0..3)
        .filter(|&a| extent(a) > params.tile_size + EPSILON && !spans_power_of_two_voxels(extent(a), params.voxel_size))
        .max_by(|&a, &b| extent(a).total_cmp(&extent(b)));
    if uneven.is_none() && (!splittable || cells <= budget || fewest <= budget as f32) {
        let started = Instant::now();
        let (grid, count, mut work) = merged_grid(split.occupancy, bounds, params);
        let accepted = !splittable || count as usize <= budget;
        work.rejected_probes = usize::from(!accepted);
        work.rejected_probe_seconds = if accepted { 0.0 } else { started.elapsed().as_secs_f64() };
        {
            let mut total = split.work.lock().unwrap();
            total.grid_seconds += work.grid_seconds;
            total.merge_seconds += work.merge_seconds;
            total.rejected_probe_seconds += work.rejected_probe_seconds;
            total.probes += 1;
            total.rejected_probes += work.rejected_probes;
            total.labelled_blocks += work.labelled_blocks;
            total.merge_passes += work.merge_passes;
            total.merge_candidates += work.merge_candidates;
            total.merge_adjacency_seconds += work.merge_adjacency_seconds;
            total.merge_shared_seconds += work.merge_shared_seconds;
            total.merge_filter_seconds += work.merge_filter_seconds;
            total.merge_sort_seconds += work.merge_sort_seconds;
        }
        if !splittable || count as usize <= budget {
            return split.leaf(voxel_leaf(bounds, grid, count, &split.occupancy.objects, params));
        }
    }
    let axis = uneven.filter(|_| cells <= budget).unwrap_or(longest);
    let plane = split_plane(&bounds, axis, params.tile_size);
    let cut = lo[axis] + ((plane - bounds.min[axis]) / split.block_edge()).round() as usize;
    let (mut low, mut high) = (bounds, bounds);
    low.max[axis] = plane;
    high.min[axis] = plane;
    let (mut low_hi, mut high_lo) = (hi, lo);
    low_hi[axis] = cut;
    high_lo[axis] = cut;
    let ((low_tree, mut tiles), (mut high_tree, high_tiles)) = rayon::join(
        || split_region(region, lo, low_hi, low, split),
        || split_region(region, high_lo, hi, high, split),
    );
    offset_leaves(&mut high_tree, tiles.len() as u32);
    tiles.extend(high_tiles);
    let tree = KdNode::Split { axis: axis as u8, value: plane, low: Box::new(low_tree), high: Box::new(high_tree) };
    (tree, tiles)
}

/// The labelled voxel grid of a block-aligned box with its cells merged, and
/// the merged cell count.
fn merged_grid(occupancy: &Occupancy, bounds: Aabb, params: &SolveParams) -> (Grid, u32, SplitWork) {
    let started = Instant::now();
    let (mut grid, count) = leaf_grid(occupancy, bounds, params);
    let mut work = SplitWork { grid_seconds: started.elapsed().as_secs_f64(), ..SplitWork::default() };
    work.labelled_blocks = grid.blocks.iter().filter(|b| b.is_some()).count();
    let started = Instant::now();
    let (merged, count) = merge_cells(&grid, count, params, &mut work);
    grid.merge(merged);
    work.merge_seconds = started.elapsed().as_secs_f64();
    (grid, count, work)
}

/// The labelled voxel grid of one leaf tile (a block-aligned box), with its
/// cell count: cells are numbered in block order.
fn leaf_grid(occupancy: &Occupancy, bounds: Aabb, params: &SolveParams) -> (Grid, u32) {
    let (voxel, block) = (params.voxel_size, params.block_voxels());
    let dims: [usize; 3] = std::array::from_fn(|a| ((bounds.max[a] - bounds.min[a]) / voxel).round() as usize);
    let mut grid = Grid::new(bounds.min, dims, voxel, block);
    let origin = block_at(&bounds.min, params.block_size);
    for bz in 0..grid.block_dims[2] {
        for by in 0..grid.block_dims[1] {
            for bx in 0..grid.block_dims[0] {
                let world = [origin[0] + bx as i32, origin[1] + by as i32, origin[2] + bz as i32];
                if let Some(bits) = occupancy.block_bits(world) {
                    let index = grid.block_index(bx, by, bz);
                    grid.blocks[index] = Some(Block::from_bits(bits, block));
                }
            }
        }
    }
    let first: [i32; 3] = std::array::from_fn(|a| origin[a] * block as i32);
    let inside = |at: &[i32; 3]| (0..3).all(|a| first[a] <= at[a] && ((at[a] - first[a]) as usize) < dims[a]);
    let start = occupancy.gate_voxels.partition_point(|(at, _)| at[0] < first[0]);
    grid.gate_voxels = occupancy.gate_voxels[start..]
        .iter()
        .take_while(|(at, _)| ((at[0] - first[0]) as usize) < dims[0])
        .filter(|(at, _)| inside(at))
        .map(|&(at, gate)| (std::array::from_fn(|a| (at[a] - first[a]) as usize), gate))
        .collect();
    let count = label_cells(&mut grid, params.min_cell_voxels);
    (grid, count)
}

fn offset_leaves(node: &mut KdNode, by: u32) {
    match node {
        KdNode::Leaf { cell } => *cell = cell.map(|c| c + by),
        KdNode::Split { low, high, .. } => {
            offset_leaves(low, by);
            offset_leaves(high, by);
        }
    }
}

/// Calls `visit(axis, layer, low cell, high cell, u, v)` for the open faces
/// between different cells across the grid's internal block planes (`u` and
/// `v` are voxel ranges on the other two axes): once for a whole face between
/// two air blocks, else once per voxel face.
fn block_faces(grid: &Grid, mut visit: impl FnMut(usize, usize, u32, u32, [usize; 2], [usize; 2])) {
    let b = grid.block;
    for axis in 0..3 {
        let [ua, va] = other_axes(axis);
        for layer in (b..grid.dims[axis]).step_by(b) {
            for bv in 0..grid.block_dims[va] {
                for bu in 0..grid.block_dims[ua] {
                    let block_at = |layer_block: usize| {
                        let mut at = [0; 3];
                        at[axis] = layer_block;
                        at[ua] = bu;
                        at[va] = bv;
                        grid.block_index(at[0], at[1], at[2])
                    };
                    let (low_block, high_block) = (block_at(layer / b - 1), block_at(layer / b));
                    let u = [bu * b, ((bu + 1) * b).min(grid.dims[ua])];
                    let v = [bv * b, ((bv + 1) * b).min(grid.dims[va])];
                    if grid.blocks[low_block].is_none() && grid.blocks[high_block].is_none() {
                        let (low, high) = (grid.air[low_block], grid.air[high_block]);
                        if low != high {
                            visit(axis, layer, low, high, u, v);
                        }
                        continue;
                    }
                    for vv in v[0]..v[1] {
                        for uu in u[0]..u[1] {
                            let [lx, ly, lz] = face_voxel(axis, layer - 1, uu, vv);
                            let [hx, hy, hz] = face_voxel(axis, layer, uu, vv);
                            let (low, high) = (grid.cell_at(lx, ly, lz), grid.cell_at(hx, hy, hz));
                            if low != NO_CELL && high != NO_CELL && low != high {
                                visit(axis, layer, low, high, [uu, uu + 1], [vv, vv + 1]);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Portal quads between cells of one grid across its internal block planes,
/// as `(axis, plane, low cell, high cell, [u0, u1, v0, v1])`.
fn block_portals(grid: &Grid, bounds: &Aabb) -> Vec<(usize, f32, u32, u32, [f32; 4])> {
    let mut quads: std::collections::BTreeMap<(usize, usize, u32, u32), [f32; 4]> = std::collections::BTreeMap::new();
    block_faces(grid, |axis, layer, low, high, u, v| {
        let [ua, va] = other_axes(axis);
        let quad = [
            grid.origin[ua] + u[0] as f32 * grid.voxel,
            (grid.origin[ua] + u[1] as f32 * grid.voxel).min(bounds.max[ua]),
            grid.origin[va] + v[0] as f32 * grid.voxel,
            (grid.origin[va] + v[1] as f32 * grid.voxel).min(bounds.max[va]),
        ];
        quads.entry((axis, layer, low, high)).and_modify(|q| grow(q, &quad)).or_insert(quad);
    });
    quads
        .into_iter()
        .map(|((axis, layer, low, high), quad)| (axis, grid.origin[axis] + layer as f32 * grid.voxel, low, high, quad))
        .collect()
}

fn find_root(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        parent[i as usize] = parent[parent[i as usize] as usize];
        i = parent[i as usize];
    }
    i
}

/// Merges neighbouring cells that meet flush on a block plane and nearly
/// fill their union's box (`merge_fill`), with an opening across at least
/// `merge_open_fraction` of the smaller face and a union at most
/// `max_merged_cell` long. The portal such a union drops barely clips the
/// view (everything seen past it is seen through the union's own portals),
/// so it culls about as well with far fewer cells and portals; open air
/// collapses into large boxes. Returns each cell's new label and the new count.
fn merge_cells(grid: &Grid, count: u32, params: &SolveParams, work: &mut SplitWork) -> (Vec<u32>, u32) {
    let started = Instant::now();
    let mut boxes = cell_voxel_boxes(grid, count);
    let mut open: FxHashMap<(usize, usize, u32, u32), usize> = FxHashMap::default();
    block_faces(grid, |axis, layer, low, high, u, v| {
        *open.entry((axis, layer, low, high)).or_default() += (u[1] - u[0]) * (v[1] - v[0]);
    });
    let open: Vec<_> = open.into_iter().collect();
    let longest = (params.max_merged_cell / grid.voxel).round() as usize;
    let mut parent: Vec<u32> = (0..count).collect();
    let mut shared: FxHashMap<(u32, u32, usize, usize), usize> = FxHashMap::default();
    let mut candidates: Vec<(f32, (u32, u32, usize, usize))> = Vec::new();
    let mut merged = vec![false; count as usize];
    work.merge_adjacency_seconds = started.elapsed().as_secs_f64();
    loop {
        work.merge_passes += 1;
        let started = Instant::now();
        shared.clear();
        for &((axis, layer, low, high), faces) in &open {
            let (low, high) = (find_root(&mut parent, low), find_root(&mut parent, high));
            if low != high {
                *shared.entry((low, high, axis, layer)).or_default() += faces;
            }
        }
        work.merge_shared_seconds += started.elapsed().as_secs_f64();
        let started = Instant::now();
        candidates.clear();
        candidates.extend(shared
            .iter()
            .filter_map(|(&(low, high, axis, layer), &faces)| {
                let ((low_min, low_max), (high_min, high_max)) = (boxes[low as usize], boxes[high as usize]);
                let [ua, va] = other_axes(axis);
                let flush = low_max[axis] == layer && high_min[axis] == layer;
                let volume = |lo: [usize; 3], hi: [usize; 3]| (0..3).map(|a| (hi[a] - lo[a]) as f32).product::<f32>();
                let union_min: [usize; 3] = std::array::from_fn(|a| low_min[a].min(high_min[a]));
                let union_max: [usize; 3] = std::array::from_fn(|a| low_max[a].max(high_max[a]));
                let fill = (volume(low_min, low_max) + volume(high_min, high_max)) / volume(union_min, union_max);
                let face = |lo: [usize; 3], hi: [usize; 3]| (hi[ua] - lo[ua]) * (hi[va] - lo[va]);
                let open = faces as f32 / face(low_min, low_max).min(face(high_min, high_max)) as f32;
                let short = (0..3).all(|a| union_max[a] - union_min[a] <= longest);
                (flush && fill >= params.merge_fill && short && open >= params.merge_open_fraction)
                    .then_some((open, (low, high, axis, layer)))
            }));
        work.merge_candidates += candidates.len();
        work.merge_filter_seconds += started.elapsed().as_secs_f64();
        if candidates.is_empty() {
            break;
        }
        let started = Instant::now();
        // The full tie-break key fixes merge order independently of hash iteration.
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        work.merge_sort_seconds += started.elapsed().as_secs_f64();
        // A cell merges once per pass, so every box a candidate was checked
        // against is still current when it is applied.
        merged.fill(false);
        for &(_, (low, high, _, _)) in &candidates {
            if merged[low as usize] || merged[high as usize] {
                continue;
            }
            merged[low as usize] = true;
            merged[high as usize] = true;
            parent[high as usize] = low;
            let (high_min, high_max) = boxes[high as usize];
            let target = &mut boxes[low as usize];
            for a in 0..3 {
                target.0[a] = target.0[a].min(high_min[a]);
                target.1[a] = target.1[a].max(high_max[a]);
            }
        }
    }
    let mut label = vec![u32::MAX; count as usize];
    let mut next = 0;
    let merged = (0..count)
        .map(|cell| {
            let root = find_root(&mut parent, cell) as usize;
            if label[root] == u32::MAX {
                label[root] = next;
                next += 1;
            }
            label[root]
        })
        .collect();
    (merged, next)
}

/// Open face footprint of each cell of a tile on one face, clipped to `rect`
/// (`[u0, u1, v0, v1]` in world units on the face's other two axes).
fn face_footprint(faces: Option<&Faces>, bounds: &Aabb, axis: usize, side: usize, rect: [f32; 4]) -> FaceCells {
    let Some(faces) = faces else { return FaceCells::Whole };
    let [ua, va] = other_axes(axis);
    let layer = &faces.cells[axis * 2 + side];
    let lower = |a: usize, value: f32| (((value - faces.origin[a]) / faces.voxel).round() as isize).max(0);
    let upper = |a: usize, value: f32| (((value - faces.origin[a]) / faces.voxel).ceil() as isize).min(faces.dims[a] as isize);
    let (u0, u1) = (lower(ua, rect[0]), upper(ua, rect[1]));
    let (v0, v1) = (lower(va, rect[2]), upper(va, rect[3]));
    let mut voxels = Vec::new();
    for v in v0..v1 {
        for u in u0..u1 {
            let cell = layer[v as usize * faces.dims[ua] + u as usize];
            if cell == NO_CELL {
                continue;
            }
            let start_u = faces.origin[ua] + u as f32 * faces.voxel;
            let start_v = faces.origin[va] + v as f32 * faces.voxel;
            let quad = [
                start_u.max(rect[0]),
                (start_u + faces.voxel).min(bounds.max[ua]).min(rect[1]),
                start_v.max(rect[2]),
                (start_v + faces.voxel).min(bounds.max[va]).min(rect[3]),
            ];
            voxels.push((cell, quad));
        }
    }
    FaceCells::Voxels(voxels)
}

enum FaceCells {
    /// An empty tile: its single cell covers the whole face.
    Whole,
    /// `(cell, world quad)` for each open face voxel.
    Voxels(Vec<(u32, [f32; 4])>),
}

fn grow(quad: &mut [f32; 4], other: &[f32; 4]) {
    quad[0] = quad[0].min(other[0]);
    quad[1] = quad[1].max(other[1]);
    quad[2] = quad[2].min(other[2]);
    quad[3] = quad[3].max(other[3]);
}

/// Portal quads between two touching faces, keyed by `(low cell, high cell)`.
fn pair_quads(low: &FaceCells, high: &FaceCells, rect: [f32; 4]) -> Vec<((u32, u32), [f32; 4])> {
    let mut quads: FxHashMap<(u32, u32), [f32; 4]> = FxHashMap::default();
    let mut add = |key: (u32, u32), quad: &[f32; 4]| {
        quads.entry(key).and_modify(|q| grow(q, quad)).or_insert(*quad);
    };
    match (low, high) {
        (FaceCells::Whole, FaceCells::Whole) => add((0, 0), &rect),
        (FaceCells::Voxels(cells), FaceCells::Whole) => cells.iter().for_each(|(c, q)| add((*c, 0), q)),
        (FaceCells::Whole, FaceCells::Voxels(cells)) => cells.iter().for_each(|(c, q)| add((0, *c), q)),
        (FaceCells::Voxels(a), FaceCells::Voxels(b)) => {
            // Both grids share the global voxel lattice, so equal quad corners are the same voxel.
            let corner = |q: &[f32; 4]| [(q[0] * 16.0).round() as i64, (q[2] * 16.0).round() as i64];
            let by_position: FxHashMap<[i64; 2], u32> = b.iter().map(|(c, q)| (corner(q), *c)).collect();
            for (cell, quad) in a {
                if let Some(&other) = by_position.get(&corner(quad)) {
                    add((*cell, other), quad);
                }
            }
        }
    }
    let mut quads: Vec<_> = quads.into_iter().collect();
    quads.sort_by_key(|(key, _)| *key);
    quads
}


pub enum Solved {
    Model(VisModel),
    NoViewVolume,
}

/// Where a camera can be. Cells no camera reaches from these through portals
/// and gates are sealed off (under terrain, inside closed shells): each
/// tile's sealed cells collapse into one cell, which sees exactly what they
/// saw together. With neither, nothing collapses.
#[derive(Clone, Debug, Default)]
pub struct Reach {
    /// The open sky: every cell with a portal out of the tome's top face.
    pub sky: bool,
    /// Camera positions, such as CK's navmesh seeds.
    pub points: Vec<Vec3>,
}

/// The cell (or tile, in the tile hierarchy) containing `point`.
fn locate(node: &KdNode, point: &Vec3) -> Option<u32> {
    match node {
        KdNode::Leaf { cell } => *cell,
        KdNode::Split { axis, value, low, high } => locate(if point[*axis as usize] < *value { low } else { high }, point),
    }
}

/// Renumbers a tile tree's cells through `map`, joining split nodes whose two
/// sides end up the same cell.
fn remap_tree(node: &mut KdNode, map: &[u32]) {
    match node {
        KdNode::Leaf { cell } => *cell = cell.map(|c| map[c as usize]),
        KdNode::Split { low, high, .. } => {
            remap_tree(low, map);
            remap_tree(high, map);
            if let (KdNode::Leaf { cell: a }, KdNode::Leaf { cell: b }) = (&**low, &**high)
                && a == b
            {
                *node = KdNode::Leaf { cell: *a };
            }
        }
    }
}

/// Collapses each tile's cells that no camera in `reach` gets to into one.
fn collapse_sealed(tiles: &mut [Tile], top: &KdNode, bounds: &Aabb, reach: &Reach) {
    let mut stack: Vec<(usize, u32)> = Vec::new();
    if reach.sky {
        for (t, tile) in tiles.iter().enumerate() {
            for (c, cell) in tile.cells.iter().enumerate() {
                if cell.portals.iter().any(|p| p.face == 5 && p.target == PortalTarget::Outside) {
                    stack.push((t, c as u32));
                }
            }
        }
    }
    for point in &reach.points {
        let inside = (0..3).all(|a| bounds.min[a] <= point[a] && point[a] < bounds.max[a]);
        let Some(tile) = locate(top, point).filter(|_| inside) else { continue };
        if let Some(cell) = locate(&tiles[tile as usize].tree, point) {
            stack.push((tile as usize, cell));
        }
    }
    if stack.is_empty() {
        return;
    }
    let mut reached: Vec<Vec<bool>> = tiles.iter().map(|t| vec![false; t.cells.len()]).collect();
    while let Some((t, c)) = stack.pop() {
        if std::mem::replace(&mut reached[t][c as usize], true) {
            continue;
        }
        let cell = &tiles[t].cells[c as usize];
        stack.extend(cell.portals.iter().filter_map(|p| match p.target {
            PortalTarget::Tile { tile, cell } => Some((tile as usize, cell)),
            PortalTarget::Outside => None,
        }));
        stack.extend(cell.gates.iter().map(|g| (t, g.cell)));
    }
    // Reached cells keep their order; the sealed rest become one cell after them.
    let remap: Vec<Vec<u32>> = reached
        .iter()
        .map(|flags| {
            let sealed = flags.iter().filter(|&&r| r).count() as u32;
            let mut next = 0;
            flags
                .iter()
                .map(|&r| {
                    if !r {
                        return sealed;
                    }
                    next += 1;
                    next - 1
                })
                .collect()
        })
        .collect();
    tiles.par_iter_mut().enumerate().for_each(|(t, tile)| {
        let map = &remap[t];
        let count = map.iter().max().map_or(0, |m| *m as usize + 1);
        let mut cells: Vec<Option<Cell>> = (0..count).map(|_| None).collect();
        for (index, mut cell) in std::mem::take(&mut tile.cells).into_iter().enumerate() {
            for portal in &mut cell.portals {
                if let PortalTarget::Tile { tile, cell } = &mut portal.target {
                    *cell = remap[*tile as usize][*cell as usize];
                }
            }
            for gate in &mut cell.gates {
                gate.cell = map[gate.cell as usize];
            }
            match &mut cells[map[index] as usize] {
                slot @ None => *slot = Some(cell),
                Some(into) => {
                    for a in 0..3 {
                        into.bounds.min[a] = into.bounds.min[a].min(cell.bounds.min[a]);
                        into.bounds.max[a] = into.bounds.max[a].max(cell.bounds.max[a]);
                    }
                    into.portals.extend(cell.portals);
                    into.gates.extend(cell.gates);
                    into.objects.extend(cell.objects);
                }
            }
        }
        tile.cells = cells.into_iter().map(Option::unwrap).collect();
        let sealed = reached[t].iter().filter(|&&r| r).count();
        if let Some(cell) = tile.cells.get_mut(sealed) {
            // Its sealed parts' portals to each other are gone; the rest keep
            // one quad per face and target, covering all of theirs.
            let own = PortalTarget::Tile { tile: t as u32, cell: sealed as u32 };
            let mut quads: std::collections::BTreeMap<(u8, PortalTarget), Portal> = std::collections::BTreeMap::new();
            for portal in cell.portals.drain(..).filter(|p| p.target != own) {
                quads
                    .entry((portal.face, portal.target))
                    .and_modify(|q| {
                        for a in 0..2 {
                            q.extent[a][0] = q.extent[a][0].min(portal.extent[a][0]);
                            q.extent[a][1] = q.extent[a][1].max(portal.extent[a][1]);
                        }
                    })
                    .or_insert(portal);
            }
            cell.portals = quads.into_values().collect();
            cell.gates.retain(|g| g.cell != sealed as u32);
            cell.objects.sort_unstable();
            cell.objects.dedup();
        }
        remap_tree(&mut tile.tree, map);
    });
}

/// Solves one scene. Returns `None` when it has no view volume.
pub fn solve(scene: &Scene, params: &SolveParams) -> Option<VisModel> {
    match solve_bounded(scene, params) {
        Solved::Model(model) => Some(model),
        Solved::NoViewVolume => None,
    }
}

/// Solves a whole scene, read the way a planner streams one.
pub fn solve_bounded(scene: &Scene, params: &SolveParams) -> Solved {
    match scene_volume(&scene.volumes) {
        Some(volume) => solve_occupancy(Occupancy::from_scene(scene, params), volume, params, &Reach::default()),
        None => Solved::NoViewVolume,
    }
}

/// The tome's bounds: the planner's view volume grown on Z to cover the
/// occupancy's occluders and targets and snapped to the tile grid; `None`
/// when it is empty.
pub fn tome_bounds(occupancy: &Occupancy, volume: Aabb, params: &SolveParams) -> Option<Aabb> {
    let z_range = occupancy.objects.iter().fold(occupancy.z_range, |[low, high], o| [low.min(o.bounds.min[2]), high.max(o.bounds.max[2])]);
    let volume = covering_volume(volume, z_range);
    (0..3).all(|a| volume.min[a] < volume.max[a]).then(|| tile_aligned(volume, params.tile_size))
}

/// Solves an occupancy inside the planner's view volume (see [`tome_bounds`]),
/// collapsing the cells sealed off from `reach`.
pub fn solve_occupancy(occupancy: Occupancy, volume: Aabb, params: &SolveParams, reach: &Reach) -> Solved {
    solve_occupancy_profiled(occupancy, volume, params, reach).0
}

pub fn solve_occupancy_profiled(occupancy: Occupancy, volume: Aabb, params: &SolveParams, reach: &Reach) -> (Solved, SolveTiming) {
    let mut timing = SolveTiming {
        objects: occupancy.objects.len(),
        occupied_blocks: occupancy.slots.len(),
        ..SolveTiming::default()
    };
    assert!(
        occupancy.voxel == params.voxel_size && occupancy.block == params.block_voxels(),
        "the occupancy was built on another lattice"
    );
    let Some(volume) = tome_bounds(&occupancy, volume, params) else {
        return (Solved::NoViewVolume, timing);
    };
    let started = Instant::now();
    let blocks = occupancy.blocks_within(&volume);
    let split = Split { occupancy: &occupancy, volume, params, work: Mutex::default() };
    let (top, leaves) = split_tiles(volume, blocks, &split);
    timing.split_seconds = started.elapsed().as_secs_f64();
    timing.tiles = leaves.len();
    let started = Instant::now();
    let mut tiles = Vec::with_capacity(leaves.len());
    let mut faces = Vec::with_capacity(leaves.len());
    let mut listed = vec![false; occupancy.objects.len()];
    for (index, leaf) in leaves.into_iter().enumerate() {
        let mut tile = leaf.tile;
        for cell in &mut tile.cells {
            for portal in &mut cell.portals {
                if let PortalTarget::Tile { tile, .. } = &mut portal.target
                    && *tile == THIS_TILE
                {
                    *tile = index as u32;
                }
            }
        }
        for (object, cells) in leaf.objects {
            for cell in cells {
                tile.cells[cell as usize].objects.push(object);
                listed[object as usize] = true;
            }
        }
        tiles.push(tile);
        faces.push(leaf.faces);
    }

    timing.assign_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let key = |v: f32| (v * 16.0).round() as i64;
    let bounds: Vec<Aabb> = tiles.iter().map(|t| t.bounds).collect();
    let mut by_min_plane: FxHashMap<(usize, i64), Vec<usize>> = FxHashMap::default();
    for (index, b) in bounds.iter().enumerate() {
        for axis in 0..3 {
            by_min_plane.entry((axis, key(b.min[axis]))).or_default().push(index);
        }
    }
    // Each tile emits its outside portals and both sides of the links to its
    // max-side neighbours, so every pair is visited exactly once.
    let links: Vec<Vec<(usize, u32, Portal)>> = (0..bounds.len())
        .into_par_iter()
        .map(|index| {
            let mut out = Vec::new();
            let tile_bounds = bounds[index];
            for axis in 0..3 {
                let [ua, va] = other_axes(axis);
                let whole = [tile_bounds.min[ua], tile_bounds.max[ua], tile_bounds.min[va], tile_bounds.max[va]];
                // Faces on the tome boundary lead outside.
                for side in 0..2 {
                    let on_boundary = if side == 1 {
                        tile_bounds.max[axis] >= volume.max[axis] - EPSILON
                    } else {
                        tile_bounds.min[axis] <= volume.min[axis] + EPSILON
                    };
                    if !on_boundary {
                        continue;
                    }
                    let plane = if side == 1 { tile_bounds.max[axis] } else { tile_bounds.min[axis] };
                    let footprint = face_footprint(faces[index].as_ref(), &tile_bounds, axis, side, whole);
                    for ((cell, _), quad) in pair_quads(&footprint, &FaceCells::Whole, whole) {
                        out.push((index, cell, Portal {
                            face: (axis * 2 + side) as u8,
                            plane,
                            extent: [[quad[0], quad[1]], [quad[2], quad[3]]],
                            target: PortalTarget::Outside,
                        }));
                    }
                }
                let Some(neighbours) = by_min_plane.get(&(axis, key(tile_bounds.max[axis]))) else { continue };
                for &other in neighbours {
                    let other_bounds = bounds[other];
                    let rect = [
                        tile_bounds.min[ua].max(other_bounds.min[ua]),
                        tile_bounds.max[ua].min(other_bounds.max[ua]),
                        tile_bounds.min[va].max(other_bounds.min[va]),
                        tile_bounds.max[va].min(other_bounds.max[va]),
                    ];
                    if rect[1] - rect[0] <= EPSILON || rect[3] - rect[2] <= EPSILON {
                        continue;
                    }
                    let low = face_footprint(faces[index].as_ref(), &tile_bounds, axis, 1, rect);
                    let high = face_footprint(faces[other].as_ref(), &other_bounds, axis, 0, rect);
                    let plane = tile_bounds.max[axis];
                    for ((low_cell, high_cell), quad) in pair_quads(&low, &high, rect) {
                        let extent = [[quad[0], quad[1]], [quad[2], quad[3]]];
                        out.push((index, low_cell, Portal {
                            face: (axis * 2 + 1) as u8,
                            plane,
                            extent,
                            target: PortalTarget::Tile { tile: other as u32, cell: high_cell },
                        }));
                        out.push((other, high_cell, Portal {
                            face: (axis * 2) as u8,
                            plane,
                            extent,
                            target: PortalTarget::Tile { tile: index as u32, cell: low_cell },
                        }));
                    }
                }
            }
            out
        })
        .collect();
    drop(faces);
    for (tile, cell, portal) in links.into_iter().flatten() {
        tiles[tile].cells[cell as usize].portals.push(portal);
    }
    timing.portals_seconds = started.elapsed().as_secs_f64();
    timing.cells_before_collapse = tiles.iter().map(|tile| tile.cells.len()).sum();
    let started = Instant::now();
    collapse_sealed(&mut tiles, &top, &volume, reach);
    timing.collapse_seconds = started.elapsed().as_secs_f64();
    timing.cells_after_collapse = tiles.iter().map(|tile| tile.cells.len()).sum();
    let started = Instant::now();
    tiles.par_iter_mut().for_each(|tile| {
        for cell in &mut tile.cells {
            cell.portals.sort_by_key(|p| (p.face, p.target));
        }
    });
    for (object, _) in listed.iter().enumerate().filter(|(_, l)| !**l) {
        for tile in &mut tiles {
            for cell in &mut tile.cells {
                cell.objects.push(object as u32);
            }
        }
    }
    for tile in &mut tiles {
        for cell in &mut tile.cells {
            cell.objects.sort_unstable();
        }
    }
    timing.finish_seconds = started.elapsed().as_secs_f64();
    timing.split_work = split.work.into_inner().unwrap();
    (Solved::Model(VisModel { bounds: volume, top, tiles, objects: occupancy.objects, gates: occupancy.gates }), timing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Geometry, INSTANCE_GATE, INSTANCE_TARGET_OCCLUDER, Instance, MODEL_ID_BASE, SceneBuilder, TeeSink};

    const IDENTITY: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];

    fn volume(min: Vec3, max: Vec3) -> [f32; 12] {
        [min[0], min[1], min[2], max[0], max[1], max[2], -1.0, 1.0, 1.0, 1.0, -1.0, 0.0]
    }

    /// Two triangles spanning an axis-aligned rectangle at `x`, minus an optional hole.
    fn wall_x(x: f32, y: [f32; 2], z: [f32; 2]) -> Geometry {
        Geometry {
            vertices: vec![[x, y[0], z[0]], [x, y[1], z[0]], [x, y[1], z[1]], [x, y[0], z[1]]],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        }
    }

    fn instance(object_id: u32, model_index: u32, flags: u32) -> Instance {
        Instance { object_id, model_index, transform: IDENTITY, flags, vector: [0.0; 3], bounds: [0.0; 6] }
    }

    fn cube(min: Vec3, size: f32) -> Geometry {
        let [x, y, z] = min;
        let s = size;
        Geometry {
            vertices: vec![
                [x, y, z], [x + s, y, z], [x + s, y + s, z], [x, y + s, z],
                [x, y, z + s], [x + s, y, z + s], [x + s, y + s, z + s], [x, y + s, z + s],
            ],
            triangles: vec![
                [0, 1, 2], [0, 2, 3], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4],
                [2, 3, 7], [2, 7, 6], [1, 2, 6], [1, 6, 5], [0, 3, 7], [0, 7, 4],
            ],
        }
    }

    /// The `(tile, cell)` the model's trees place `p` in.
    fn locate(model: &VisModel, p: Vec3) -> (u32, Option<u32>) {
        let walk = |mut node: &KdNode| loop {
            match node {
                KdNode::Leaf { cell } => return *cell,
                KdNode::Split { axis, value, low, high } => node = if p[*axis as usize] < *value { low } else { high },
            }
        };
        let tile = walk(&model.top).unwrap();
        (tile, walk(&model.tiles[tile as usize].tree))
    }

    /// Whether the portal walk (through doors too, with `gates`) gets from `from` to `to`.
    fn reaches(model: &VisModel, from: (u32, u32), to: (u32, u32), gates: bool) -> bool {
        let mut seen = std::collections::HashSet::from([from]);
        let mut stack = vec![from];
        while let Some((tile, cell)) = stack.pop() {
            let cell = &model.tiles[tile as usize].cells[cell as usize];
            let doors = cell.gates.iter().filter(|_| gates).map(|g| (tile, g.cell));
            let next = cell.portals.iter().filter_map(|p| match p.target {
                PortalTarget::Tile { tile, cell } => Some((tile, cell)),
                PortalTarget::Outside => None,
            });
            for n in next.chain(doors) {
                if seen.insert(n) {
                    stack.push(n);
                }
            }
        }
        seen.contains(&to)
    }

    /// Every portal and gate portal has a twin leading back through the same quad or box.
    fn assert_reciprocal(model: &VisModel) {
        for (t, tile) in model.tiles.iter().enumerate() {
            for (c, cell) in tile.cells.iter().enumerate() {
                let here = PortalTarget::Tile { tile: t as u32, cell: c as u32 };
                for p in &cell.portals {
                    let PortalTarget::Tile { tile, cell } = p.target else { continue };
                    let back = &model.tiles[tile as usize].cells[cell as usize];
                    assert!(
                        back.portals.iter().any(|q| q.target == here
                            && q.face == p.face ^ 1
                            && q.plane == p.plane
                            && q.extent == p.extent),
                        "{p:?} from tile {t} cell {c} has no twin"
                    );
                }
                for g in &cell.gates {
                    let back = &tile.cells[g.cell as usize];
                    assert!(back.gates.iter().any(|h| h.cell == c as u32 && h.gate == g.gate && h.bounds == g.bounds));
                }
            }
        }
    }

    #[test]
    fn empty_single_tile_is_one_cell_with_six_outside_portals() {
        let scene = Scene { volumes: vec![volume([0.0; 3], [512.0; 3])], ..Scene::default() };
        let model = solve(&scene, &SolveParams::default()).unwrap();
        assert_eq!(model.tiles.len(), 1);
        let tile = &model.tiles[0];
        assert_eq!(tile.cells.len(), 1);
        assert_eq!(tile.tree, KdNode::Leaf { cell: Some(0) });
        let faces: Vec<u8> = tile.cells[0].portals.iter().map(|p| p.face).collect();
        assert_eq!(faces, [0, 1, 2, 3, 4, 5]);
        assert!(tile.cells[0].portals.iter().all(|p| p.target == PortalTarget::Outside
            && p.extent == [[0.0, 512.0], [0.0, 512.0]]));
    }

    #[test]
    fn a_wall_separates_cells_and_a_hole_in_it_is_their_portal() {
        let solid = Scene {
            geometries: vec![wall_x(256.0, [0.0, 512.0], [0.0, 512.0])],
            instances: vec![instance(1, 0, INSTANCE_OCCLUDER)],
            volumes: vec![volume([0.0; 3], [512.0; 3])],
        };
        let model = solve(&solid, &SolveParams::default()).unwrap();
        let (west, east) = (locate(&model, [100.0, 256.0, 256.0]), locate(&model, [400.0, 256.0, 256.0]));
        let (west, east) = ((west.0, west.1.unwrap()), (east.0, east.1.unwrap()));
        assert!(!reaches(&model, west, east, true));

        // Same wall as four panels leaving a 128-unit square hole in the middle:
        // a doorway, which keeps the rooms apart as cells but links them.
        let panels = [
            wall_x(256.0, [0.0, 512.0], [0.0, 192.0]),
            wall_x(256.0, [0.0, 512.0], [320.0, 512.0]),
            wall_x(256.0, [0.0, 192.0], [192.0, 320.0]),
            wall_x(256.0, [320.0, 512.0], [192.0, 320.0]),
        ];
        let holed = Scene {
            geometries: panels.to_vec(),
            instances: (0..4).map(|i| instance(1, i, INSTANCE_OCCLUDER)).collect(),
            volumes: vec![volume([0.0; 3], [512.0; 3])],
        };
        let model = solve(&holed, &SolveParams::default()).unwrap();
        let (west, east) = (locate(&model, [100.0, 100.0, 100.0]), locate(&model, [400.0, 100.0, 100.0]));
        let (west, east) = ((west.0, west.1.unwrap()), (east.0, east.1.unwrap()));
        assert_ne!(west, east);
        assert!(reaches(&model, west, east, false));
        let through_wall: Vec<&Portal> = model.tiles[0]
            .cells
            .iter()
            .flat_map(|c| &c.portals)
            .filter(|p| p.face >> 1 == 0 && p.plane == 256.0)
            .collect();
        assert!(!through_wall.is_empty());
        for p in through_wall {
            assert!(p.extent.iter().all(|e| e[0] >= 192.0 && e[1] <= 320.0), "{p:?} is not in the hole");
        }
    }

    #[test]
    fn neighbouring_tiles_get_reciprocal_portals() {
        // A small occluder, and a cell budget of one, force the volume into two tiles.
        let scene = Scene {
            geometries: vec![cube([100.0; 3], 20.0)],
            instances: vec![instance(1, 0, INSTANCE_OCCLUDER)],
            volumes: vec![volume([0.0; 3], [1024.0, 512.0, 512.0])],
        };
        let one_cell = SolveParams { max_tile_cells: 1, ..SolveParams::default() };
        let model = solve(&scene, &one_cell).unwrap();
        assert_eq!(model.tiles.len(), 2);
        assert_eq!(model.tiles[1].tree, KdNode::Leaf { cell: Some(0) });
        assert!(matches!(&model.top, KdNode::Split { axis: 0, value, low, high }
            if *value == 512.0 && **low == KdNode::Leaf { cell: Some(0) } && **high == KdNode::Leaf { cell: Some(1) }));
        let east: Vec<&Portal> = model.tiles[0]
            .cells
            .iter()
            .flat_map(|c| &c.portals)
            .filter(|p| p.target == PortalTarget::Tile { tile: 1, cell: 0 })
            .collect();
        assert!(!east.is_empty() && east.iter().all(|p| p.face == 1 && p.plane == 512.0));
        assert_reciprocal(&model);

        // An occluder straddling the tile plane.
        let straddling = Scene { geometries: vec![cube([500.0, 100.0, 100.0], 20.0)], ..scene };
        let model = solve(&straddling, &one_cell).unwrap();
        assert!(model.tiles.len() > 1);
        assert_reciprocal(&model);
    }

    #[test]
    fn open_air_merges_into_few_boxes() {
        // A small occluder in one corner of a 512 tile of 64 blocks.
        let scene = Scene {
            geometries: vec![cube([40.0; 3], 20.0)],
            instances: vec![instance(1, 0, INSTANCE_OCCLUDER)],
            volumes: vec![volume([0.0; 3], [512.0; 3])],
        };
        let model = solve(&scene, &SolveParams::default()).unwrap();
        let cells = &model.tiles[0].cells;
        assert!(cells.len() <= 8, "{} cells", cells.len());
        assert_reciprocal(&model);
    }

    #[test]
    fn a_small_space_split_by_a_block_plane_stays_a_cell() {
        // A sealed box whose inside (2 x 2 x 6 voxels) the z = 128 block
        // plane cuts into two 12-voxel parts, each under `min_cell_voxels`.
        let geometry = Geometry {
            vertices: cube([96.0, 96.0, 64.0], 64.0).vertices.iter().map(|&[x, y, z]| [x, y, 64.0 + (z - 64.0) * 2.0]).collect(),
            triangles: cube([0.0; 3], 1.0).triangles,
        };
        let scene = Scene {
            geometries: vec![geometry],
            instances: vec![instance(1, 0, INSTANCE_OCCLUDER)],
            volumes: vec![volume([0.0; 3], [512.0; 3])],
        };
        let model = solve(&scene, &SolveParams::default()).unwrap();
        let inside = |b: &Aabb| b.min[0] >= 96.0 && b.max[0] <= 160.0 && b.min[2] >= 64.0 && b.max[2] <= 192.0;
        let cells: Vec<&Cell> = model.tiles[0].cells.iter().filter(|c| inside(&c.bounds)).collect();
        assert!(cells.iter().any(|c| c.bounds.min[2] < 128.0) && cells.iter().any(|c| c.bounds.max[2] > 128.0), "{cells:?}");
    }

    #[test]
    fn a_target_flush_with_a_wall_is_listed_beside_it() {
        // The plate lies in the solid voxels of the wall at x = 256; a second
        // wall at x = 128 closes off a room it cannot be seen from.
        let plate = Geometry {
            vertices: vec![[262.0, 240.0, 240.0], [262.0, 272.0, 240.0], [262.0, 272.0, 272.0]],
            triangles: vec![[0, 1, 2]],
        };
        let scene = Scene {
            geometries: vec![wall_x(256.0, [0.0, 512.0], [0.0, 512.0]), wall_x(128.0, [0.0, 512.0], [0.0, 512.0]), plate],
            instances: vec![
                instance(1, 0, INSTANCE_OCCLUDER),
                instance(2, 1, INSTANCE_OCCLUDER),
                instance(0x1234, 2, INSTANCE_TARGET),
            ],
            volumes: vec![volume([0.0; 3], [512.0; 3])],
        };
        let model = solve(&scene, &SolveParams::default()).unwrap();
        let listed = |p: Vec3| {
            let (tile, cell) = locate(&model, p);
            model.tiles[tile as usize].cells[cell.unwrap() as usize].objects.contains(&0)
        };
        assert!(listed([300.0, 256.0, 256.0]));
        assert!(!listed([50.0, 256.0, 256.0]));
    }

    #[test]
    fn streamed_occupancy_matches_its_scene_and_every_touched_voxel() {
        let params = SolveParams::default();
        let (angle_sin, angle_cos) = 0.5f32.sin_cos();
        let turned = [angle_cos, -angle_sin, 0.0, -37.5, angle_sin, angle_cos, 0.0, 210.25, 0.0, 0.0, 1.0, -3.0];
        let moved = [1.0, 0.0, 0.0, 200.0, 0.0, 1.0, 0.0, -20.0, 0.0, 0.0, 1.0, 64.0];
        let sliver = Geometry { vertices: vec![[3.0, 5.0, 7.0], [250.0, 40.0, 180.0], [60.0, 230.0, 90.5]], triangles: vec![[0, 1, 2]] };
        let reference = |object_id, geometry: Geometry, transform, flags| Scene {
            geometries: vec![geometry],
            instances: vec![Instance { transform, ..instance(object_id, 0, flags) }],
            volumes: Vec::new(),
        };
        let mut streamed = OccupancySink::new(&params, None);
        let mut builder = SceneBuilder::default();
        let mut sink = TeeSink(&mut streamed, &mut builder);
        sink.pooled("box", reference(1, cube([0.0; 3], 40.0), IDENTITY, INSTANCE_TARGET_OCCLUDER));
        // A later reference of a model is drawn with the first one's geometry.
        sink.pooled("box", reference(2, cube([0.0; 3], 99.0), moved, INSTANCE_TARGET_OCCLUDER));
        sink.pooled("sliver", reference(3, sliver, turned, INSTANCE_TARGET_OCCLUDER));
        sink.pooled("door", reference(0xFE00_0001, wall_x(130.5, [-20.0, 60.0], [0.0, 90.0]), turned, INSTANCE_GATE));
        sink.combined((cube([300.0; 3], 50.0), IDENTITY, INSTANCE_TARGET_OCCLUDER));
        sink.discard_combined();
        sink.combined((wall_x(-60.25, [0.0, 300.0], [-40.0, 200.0]), moved, INSTANCE_TARGET_OCCLUDER));
        sink.combined((cube([10.0, 400.0, 5.0], 30.0), IDENTITY, INSTANCE_TARGET));
        sink.commit_combined(false, &|index| MODEL_ID_BASE + index as u32);
        sink.combined((cube([150.0, -90.0, 0.0], 45.0), turned, INSTANCE_TARGET_OCCLUDER));
        sink.commit_combined(true, &|index| MODEL_ID_BASE + 100 + index as u32);
        let streamed = streamed.finish();
        let scene = builder.finish(volume([0.0; 3], [512.0; 3]));
        let read = Occupancy::from_scene(&scene, &params);
        assert_eq!(streamed.voxels(), read.voxels());
        assert_eq!(streamed.objects, read.objects);
        assert_eq!(streamed.gates, read.gates);
        assert_eq!(streamed.z_range, read.z_range);

        let mut triangles = Vec::new();
        for instance in scene.instances.iter().filter(|i| i.flags & (INSTANCE_OCCLUDER | INSTANCE_GATE) != 0) {
            let geometry = &scene.geometries[instance.model_index as usize];
            let gate = read.gates.iter().position(|&g| g == instance.object_id && instance.flags & INSTANCE_GATE != 0);
            for t in &geometry.triangles {
                let tri = t.map(|v| transform_point(&instance.transform, &geometry.vertices[v as usize]));
                triangles.push((tri, triangle_bounds(&tri), gate.map_or(NO_GATE, |g| g as u32)));
            }
        }
        let (mut solid, mut gate_voxels) = (Vec::new(), Vec::new());
        let (voxel, half) = (params.voxel_size, params.voxel_size / 2.0);
        for z in -8..20 {
            for y in -12..36 {
                for x in -12..30 {
                    let at = [x, y, z];
                    let centre = at.map(|i| i as f32 * voxel + half);
                    // Voxel `i` spans `[i * voxel, (i + 1) * voxel)`: touching only its closed low face does not count.
                    let spans = |b: &Aabb| (0..3).all(|a| at[a] as f32 * voxel <= b.max[a] && (at[a] + 1) as f32 * voxel > b.min[a]);
                    let touching: Vec<u32> = triangles
                        .iter()
                        .filter(|(tri, bounds, _)| spans(bounds) && triangle_overlaps_box(tri, &centre, &[half; 3]))
                        .map(|t| t.2)
                        .collect();
                    if !touching.is_empty() {
                        solid.push([x, y, z]);
                    }
                    gate_voxels.extend(touching.into_iter().filter(|&g| g != NO_GATE).map(|g| ([x, y, z], g)));
                }
            }
        }
        solid.sort_unstable();
        gate_voxels.sort_unstable();
        gate_voxels.dedup();
        assert!(!gate_voxels.is_empty());
        assert_eq!(read.voxels(), (solid, gate_voxels));
    }

    #[test]
    fn a_camera_in_a_surface_voxel_starts_in_a_cell_beside_it() {
        // Boxes and a wall leaving small pockets next to open space.
        let geometries = vec![
            cube([100.0; 3], 40.0),
            cube([150.0, 120.0, 90.0], 30.0),
            cube([96.0, 300.0, 40.0], 20.0),
            wall_x(300.0, [0.0, 512.0], [0.0, 200.0]),
        ];
        let scene = Scene {
            instances: (0..geometries.len() as u32).map(|i| instance(1, i, INSTANCE_OCCLUDER)).collect(),
            geometries,
            volumes: vec![volume([0.0; 3], [512.0; 3])],
        };
        let bounds = Aabb { min: [0.0; 3], max: [512.0; 3] };
        let params = SolveParams::default();
        let (sub, _) = leaf_grid(&Occupancy::from_scene(&scene, &params), bounds, &params);
        let cameras = CameraCells::new(&sub);
        let tree = build_tree(&cameras, bounds.min, bounds.max, params.min_tree_leaf);
        let mut surface = 0;
        for (index, &(start, len)) in cameras.spans.iter().enumerate() {
            let choices = &cameras.choices[start as usize..start as usize + len as usize];
            if choices.is_empty() {
                continue;
            }
            let at = [index % sub.dims[0], index / sub.dims[0] % sub.dims[1], index / (sub.dims[0] * sub.dims[1])];
            surface += usize::from(sub.cell_at(at[0], at[1], at[2]) == NO_CELL);
            let centre: Vec3 = std::array::from_fn(|a| (at[a] as f32 + 0.5) * sub.voxel);
            let mut node = &tree;
            let cell = loop {
                match node {
                    KdNode::Leaf { cell } => break cell.unwrap(),
                    KdNode::Split { axis, value, low, high } => node = if centre[*axis as usize] < *value { low } else { high },
                }
            };
            assert!(choices.contains(&cell), "voxel {at:?} starts in cell {cell}, not one of {choices:?}");
        }
        assert!(surface > 0);
    }

    #[test]
    fn a_closed_door_parts_cells_that_a_gate_portal_joins() {
        let mut geometries = vec![
            wall_x(256.0, [0.0, 512.0], [192.0, 512.0]),
            wall_x(256.0, [0.0, 192.0], [0.0, 192.0]),
            wall_x(256.0, [320.0, 512.0], [0.0, 192.0]),
        ];
        let mut instances: Vec<Instance> = (0..3).map(|i| instance(1, i, INSTANCE_OCCLUDER)).collect();
        // The door fills the doorway (y 192..320, z 0..192).
        geometries.push(wall_x(256.0, [192.0, 320.0], [0.0, 192.0]));
        instances.push(instance(0xFE00_0ABC, 3, INSTANCE_GATE));
        let scene = Scene { geometries, instances, volumes: vec![volume([0.0; 3], [512.0; 3])] };
        let model = solve(&scene, &SolveParams::default()).unwrap();
        assert_eq!(model.gates, vec![0xFE00_0ABC]);
        let (west, east) = (locate(&model, [100.0, 256.0, 100.0]), locate(&model, [400.0, 256.0, 100.0]));
        let (west, east) = ((west.0, west.1.unwrap()), (east.0, east.1.unwrap()));
        assert!(!reaches(&model, west, east, false), "only the door joins the rooms");
        assert!(reaches(&model, west, east, true));
        let gates: Vec<&GatePortal> = model.tiles.iter().flat_map(|t| &t.cells).flat_map(|c| &c.gates).collect();
        assert!(!gates.is_empty());
        for g in gates {
            assert_eq!(g.gate, 0);
            let b = g.bounds;
            assert!(b.min[0] >= 224.0 && b.max[0] <= 288.0 && b.min[1] >= 176.0 && b.max[1] <= 336.0, "{b:?}");
        }
        assert_reciprocal(&model);
    }

    #[test]
    fn empty_volume_stays_one_tile_however_large() {
        let scene = Scene { volumes: vec![volume([0.0; 3], [4096.0, 4096.0, 1024.0])], ..Scene::default() };
        let model = solve(&scene, &SolveParams::default()).unwrap();
        assert_eq!(model.tiles.len(), 1);
        assert_eq!(model.tiles[0].cells[0].portals.len(), 6);
    }

    #[test]
    fn a_footprint_keeps_only_its_voxels_but_every_target() {
        let footprint = Aabb { min: [f32::NEG_INFINITY, f32::NEG_INFINITY, 0.0], max: [f32::INFINITY, f32::INFINITY, 64.0] };
        let mut occupancy = Occupancy::new(&SolveParams::default(), Some(footprint));
        let wall = wall_x(8.0, [0.0, 32.0], [-200.0, 200.0]);
        occupancy.add_instance(&instance(1, 0, INSTANCE_TARGET_OCCLUDER), &wall);
        let solid_at = |occupancy: &mut Occupancy, z: i32| occupancy.is_solid([0, 1, z]);
        assert!((0..4).all(|z| solid_at(&mut occupancy, z)), "inside the footprint");
        assert!(!solid_at(&mut occupancy, -1) && !solid_at(&mut occupancy, 4), "above and below it");
        assert_eq!((occupancy.objects[0].bounds.min[2], occupancy.objects[0].bounds.max[2]), (-200.0, 200.0));
    }

    #[test]
    fn every_target_is_listed_even_inside_solid_space() {
        // A small target buried inside a closed occluder box.
        let scene = Scene {
            geometries: vec![cube([200.0; 3], 100.0), cube([240.0; 3], 8.0)],
            instances: vec![instance(1, 0, INSTANCE_OCCLUDER), instance(0x1234, 1, INSTANCE_TARGET)],
            volumes: vec![volume([0.0; 3], [512.0; 3])],
        };
        let model = solve(&scene, &SolveParams::default()).unwrap();
        assert_eq!(model.objects.len(), 1);
        assert_eq!(model.objects[0].user_id, 0x1234);
        assert!(model.tiles[0].cells.iter().any(|c| c.objects.contains(&0)));
    }

    #[test]
    fn cells_sealed_from_the_camera_collapse_into_one_per_tile() {
        // A closed box, its inside sealed off from the sky above.
        let scene = Scene {
            geometries: vec![cube([256.0; 3], 1536.0)],
            instances: vec![instance(1, 0, INSTANCE_TARGET_OCCLUDER)],
            volumes: vec![volume([0.0; 3], [2048.0; 3])],
        };
        let params = SolveParams::default();
        let solve_with = |reach: &Reach| {
            let occupancy = Occupancy::from_scene(&scene, &params);
            let Solved::Model(model) = solve_occupancy(occupancy, scene_volume(&scene.volumes).unwrap(), &params, reach) else {
                panic!("no model");
            };
            model
        };
        let inside = |c: &Cell| (0..3).all(|a| c.bounds.min[a] >= 256.0 && c.bounds.max[a] <= 1792.0);
        let most_inside = |m: &VisModel| m.tiles.iter().map(|t| t.cells.iter().filter(|c| inside(c)).count()).max().unwrap();
        let cells = |m: &VisModel| m.tiles.iter().map(|t| t.cells.len()).sum::<usize>();

        let open = solve_with(&Reach::default());
        let sky = solve_with(&Reach { sky: true, points: Vec::new() });
        assert!(most_inside(&open) > 1);
        assert_eq!(most_inside(&sky), 1);
        assert!(cells(&sky) < cells(&open));
        assert_reciprocal(&sky);

        // A camera seed inside the box reaches it, so nothing is sealed.
        let seeded = solve_with(&Reach { sky: true, points: vec![[1024.0; 3]] });
        assert_eq!(cells(&seeded), cells(&open));
    }

    #[test]
    fn kd_tree_maps_points_to_the_right_cell() {
        let scene = Scene {
            geometries: vec![wall_x(256.0, [0.0, 512.0], [0.0, 512.0])],
            instances: vec![instance(1, 0, INSTANCE_OCCLUDER)],
            volumes: vec![volume([0.0; 3], [512.0; 3])],
        };
        let model = solve(&scene, &SolveParams::default()).unwrap();
        let tile = &model.tiles[0];
        let find = |p: Vec3| {
            let mut node = &tile.tree;
            loop {
                match node {
                    KdNode::Leaf { cell } => return *cell,
                    KdNode::Split { axis, value, low, high } => {
                        node = if p[*axis as usize] < *value { low } else { high };
                    }
                }
            }
        };
        let (west, east) = (find([100.0, 100.0, 100.0]), find([400.0, 100.0, 100.0]));
        assert!(west.is_some() && east.is_some() && west != east);
        // A camera in the wall's voxel stands in the room beside it.
        assert_eq!(find([260.0, 100.0, 100.0]), east);
        assert!(tile.cells[west.unwrap() as usize].bounds.max[0] <= 256.0);
    }

    #[test]
    fn every_point_resolves_to_a_cell_that_contains_it() {
        // 3.75 tiles wide with an occluder in every block: before tiles were
        // power-of-two sized this stayed one 1920-wide tile whose midpoint
        // tree put the block-plane cell walls up to 64 units off.
        let geometries: Vec<Geometry> = (0..4).map(|i| cube([100.0 + 512.0 * i as f32, 200.0, 200.0], 40.0)).collect();
        let scene = Scene {
            instances: (0..4).map(|i| instance(i + 1, i, INSTANCE_OCCLUDER)).collect(),
            geometries,
            volumes: vec![volume([0.0; 3], [1920.0, 512.0, 512.0])],
        };
        let params = SolveParams::default();
        let model = solve(&scene, &params).unwrap();
        assert_eq!(model.bounds.max[0], 2048.0, "the partial tile is padded to a power of two of voxels");
        for tile in model.tiles.iter().filter(|t| t.cells.len() > 1) {
            for a in 0..3 {
                assert!(spans_power_of_two_voxels(tile.bounds.max[a] - tile.bounds.min[a], params.voxel_size), "{:?}", tile.bounds);
            }
        }
        let walk = |mut node: &KdNode, p: Vec3| loop {
            match node {
                KdNode::Leaf { cell } => return *cell,
                KdNode::Split { axis, value, low, high } => node = if p[*axis as usize] < *value { low } else { high },
            }
        };
        let step = params.voxel_size;
        let (mut x, mut checked) = (step / 2.0, 0);
        while x < model.bounds.max[0] {
            let p = [x, 300.0, 300.0];
            let tile = &model.tiles[walk(&model.top, p).unwrap() as usize];
            if let Some(cell) = walk(&tile.tree, p) {
                let b = tile.cells[cell as usize].bounds;
                assert!((0..3).all(|a| b.min[a] <= p[a] && p[a] <= b.max[a]), "{p:?} maps to cell {b:?}");
                checked += 1;
            }
            x += step;
        }
        assert!(checked > 100);
    }
}
