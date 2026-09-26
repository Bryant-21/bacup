//! Bethesda Umbra scene (format 15) construction and serialization.

use rustc_hash::FxHashMap;

use crate::combined::CombinedRow;
use crate::error::{PrevisError, Result};
use crate::f32ops::{FLOAT_MAX, Transform, Vec3, add, fma_rounded, mul};

pub const SCENE_MAGIC: u32 = 0xBEEF_CAFE;
pub const SCENE_VERSION: u32 = 15;
pub const MODEL_ID_BASE: u32 = 0xFD00_0000;
const DIRECT_OCCLUDER_DIAMETER_THRESHOLD: f32 = 128.0;
const SMALL_MODEL_BOUND_THRESHOLD: f64 = 100.0;
const SMALL_MODEL_DISTANCE: f32 = 4096.0;
const IDENTITY_MATRIX: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];

pub const INSTANCE_OCCLUDER: u32 = 1;
pub const INSTANCE_TARGET: u32 = 4;
pub const INSTANCE_TARGET_OCCLUDER: u32 = 5;
/// A door: blocks the view while closed (an Umbra gate).
pub const INSTANCE_GATE: u32 = 0x10;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    pub object_id: u32,
    pub model_index: u32,
    pub transform: [f32; 12],
    pub flags: u32,
    pub vector: Vec3,
    pub bounds: [f32; 6],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub geometries: Vec<Geometry>,
    pub instances: Vec<Instance>,
    pub volumes: Vec<[f32; 12]>,
}

impl Scene {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.encoded_size_hint());
        fn u32s(out: &mut Vec<u8>, values: &[u32]) {
            for value in values {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        fn f32s(out: &mut Vec<u8>, values: &[f32]) {
            for value in values {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        u32s(&mut out, &[SCENE_MAGIC, SCENE_VERSION, 6, 21]);
        f32s(&mut out, &[0.0; 21]);
        for geometry in &self.geometries {
            u32s(&mut out, &[0, geometry.vertices.len() as u32, 1, 12]);
            for vertex in &geometry.vertices {
                f32s(&mut out, vertex);
            }
            u32s(&mut out, &[geometry.triangles.len() as u32]);
            for triangle in &geometry.triangles {
                u32s(&mut out, triangle);
            }
            u32s(&mut out, &[0]);
        }
        for instance in &self.instances {
            u32s(&mut out, &[1, instance.object_id, instance.model_index]);
            f32s(&mut out, &instance.transform);
            u32s(&mut out, &[instance.flags, 0]);
            f32s(&mut out, &instance.vector);
            f32s(&mut out, &instance.bounds);
        }
        u32s(&mut out, &[4, self.volumes.len() as u32]);
        for volume in &self.volumes {
            f32s(&mut out, volume);
        }
        out
    }

    /// Inverse of [`Scene::encode`].
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let truncated = || PrevisError::invalid("scene is truncated");
        let mut at = 0usize;
        let mut u32_at = || -> Result<u32> {
            let word = bytes.get(at..at + 4).ok_or_else(truncated)?;
            at += 4;
            Ok(u32::from_le_bytes(word.try_into().unwrap()))
        };
        if u32_at()? != SCENE_MAGIC || u32_at()? != SCENE_VERSION {
            return Err(PrevisError::invalid("not a format-15 previs scene"));
        }
        let _ = (u32_at()?, u32_at()?);
        for _ in 0..21 {
            u32_at()?;
        }
        let f32_at = |u32_at: &mut dyn FnMut() -> Result<u32>| u32_at().map(f32::from_bits);
        let mut scene = Scene::default();
        loop {
            match u32_at()? {
                0 => {
                    let count = u32_at()? as usize;
                    let _ = (u32_at()?, u32_at()?);
                    let mut vertices = Vec::with_capacity(count.min(bytes.len() / 12));
                    for _ in 0..count {
                        vertices.push([f32_at(&mut u32_at)?, f32_at(&mut u32_at)?, f32_at(&mut u32_at)?]);
                    }
                    let count = u32_at()? as usize;
                    let mut triangles = Vec::with_capacity(count.min(bytes.len() / 12));
                    for _ in 0..count {
                        triangles.push([u32_at()?, u32_at()?, u32_at()?]);
                    }
                    u32_at()?;
                    scene.geometries.push(Geometry { vertices, triangles });
                }
                1 => {
                    let (object_id, model_index) = (u32_at()?, u32_at()?);
                    let mut transform = [0.0; 12];
                    for value in &mut transform {
                        *value = f32_at(&mut u32_at)?;
                    }
                    let flags = u32_at()?;
                    u32_at()?;
                    let vector = [f32_at(&mut u32_at)?, f32_at(&mut u32_at)?, f32_at(&mut u32_at)?];
                    let mut bounds = [0.0; 6];
                    for value in &mut bounds {
                        *value = f32_at(&mut u32_at)?;
                    }
                    scene.instances.push(Instance { object_id, model_index, transform, flags, vector, bounds });
                }
                4 => {
                    let count = u32_at()? as usize;
                    for _ in 0..count {
                        let mut volume = [0.0; 12];
                        for value in &mut volume {
                            *value = f32_at(&mut u32_at)?;
                        }
                        scene.volumes.push(volume);
                    }
                    return Ok(scene);
                }
                tag => return Err(PrevisError::invalid(format!("unknown scene tag {tag}"))),
            }
        }
    }

    fn encoded_size_hint(&self) -> usize {
        let geometry: usize = self
            .geometries
            .iter()
            .map(|g| 24 + g.vertices.len() * 12 + g.triangles.len() * 12)
            .sum();
        100 + geometry + self.instances.len() * 108 + self.volumes.len() * 48
    }
}

/// Deduplicates exact XYZ positions in first triangle-reference order.
pub fn compact_geometry(positions: &[Vec3], indices: &[u16]) -> (Vec<Vec3>, Vec<u32>) {
    let mut compacted = Vec::new();
    let mut remapped = Vec::with_capacity(indices.len());
    let mut vertex_map: FxHashMap<[u32; 3], u32> = FxHashMap::default();
    for &index in indices {
        let position = positions[index as usize];
        let key = [position[0].to_bits(), position[1].to_bits(), position[2].to_bits()];
        let compact_index = *vertex_map.entry(key).or_insert_with(|| {
            compacted.push(position);
            (compacted.len() - 1) as u32
        });
        remapped.push(compact_index);
    }
    (compacted, remapped)
}

pub fn merge_geometry(rows: &[(&[Vec3], &[u32])]) -> Geometry {
    let mut geometry = Geometry::default();
    for (positions, indices) in rows {
        let base = geometry.vertices.len() as u32;
        geometry.vertices.extend_from_slice(positions);
        geometry.triangles.extend(
            indices
                .chunks_exact(3)
                .map(|t| [base + t[0], base + t[1], base + t[2]]),
        );
    }
    geometry
}

pub fn snapped_volume(world_vertices: &[Vec3]) -> [f32; 12] {
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for point in world_vertices {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(point[axis]);
            maximum[axis] = maximum[axis].max(point[axis]);
        }
    }
    let floor = |v: f32| ((v as f64 / 128.0).floor() * 128.0) as f32;
    let ceil = |v: f32| ((v as f64 / 128.0).ceil() * 128.0) as f32;
    [
        floor(minimum[0]),
        floor(minimum[1]),
        floor(minimum[2]),
        ceil(maximum[0]),
        ceil(maximum[1]),
        ceil(maximum[2]),
        -1.0,
        1.0,
        1.0,
        1.0,
        -1.0,
        0.0,
    ]
}

pub fn exterior_volume(root_x: i32, root_y: i32, minimum_z: f32) -> [f32; 12] {
    [
        ((root_x - 1) * 4096) as f32,
        ((root_y - 1) * 4096) as f32,
        minimum_z,
        ((root_x + 2) * 4096) as f32,
        ((root_y + 2) * 4096) as f32,
        8192.0,
        -1.0,
        1.0,
        1.0,
        1.0,
        -1.0,
        0.0,
    ]
}

/// Builds one instance record, including CK's small-model bound shortcut.
///
/// The Z maximum deliberately reuses the X column and the minimum is written
/// as `(x, z, y)`; both quirks are what CK serializes.
pub fn instance(
    object_id: u32,
    model_index: u32,
    transform: [f32; 12],
    flags: u32,
    geometry: &Geometry,
) -> Instance {
    let mut vector = [0.0, 0.0, -1.0];
    let mut bounds = [FLOAT_MAX, FLOAT_MAX, FLOAT_MAX, -FLOAT_MAX, -FLOAT_MAX, -FLOAT_MAX];
    if !geometry.vertices.is_empty() {
        let mut minimum = [f32::INFINITY; 3];
        let mut maximum = [f32::NEG_INFINITY; 3];
        for point in &geometry.vertices {
            for axis in 0..3 {
                minimum[axis] = minimum[axis].min(point[axis]);
            }
            maximum[0] = maximum[0].max(point[0]);
            maximum[1] = maximum[1].max(point[1]);
        }
        maximum[2] = maximum[0];
        let extent = (0..3)
            .map(|axis| maximum[axis] as f64 - minimum[axis] as f64)
            .fold(f64::NEG_INFINITY, f64::max);
        if extent < SMALL_MODEL_BOUND_THRESHOLD {
            let mut transformed_minimum = [f32::INFINITY; 3];
            let mut transformed_maximum = [f32::NEG_INFINITY; 3];
            for mask in 0..8 {
                let corner = [
                    if mask & 1 != 0 { maximum[0] } else { minimum[0] },
                    if mask & 2 != 0 { maximum[1] } else { minimum[1] },
                    if mask & 4 != 0 { maximum[2] } else { minimum[2] },
                ];
                for axis in 0..3 {
                    let value = add(
                        add(
                            add(
                                mul(transform[axis * 4], corner[0]),
                                mul(transform[axis * 4 + 1], corner[1]),
                            ),
                            mul(transform[axis * 4 + 2], corner[2]),
                        ),
                        transform[axis * 4 + 3],
                    );
                    transformed_minimum[axis] = transformed_minimum[axis].min(value);
                    transformed_maximum[axis] = transformed_maximum[axis].max(value);
                }
            }
            vector = [0.0, 0.0, SMALL_MODEL_DISTANCE];
            bounds = [
                transformed_minimum[0],
                transformed_minimum[2],
                transformed_minimum[1],
                transformed_maximum[0],
                transformed_maximum[1],
                transformed_maximum[2],
            ];
        }
    }
    Instance {
        object_id,
        model_index,
        transform,
        flags,
        vector,
        bounds,
    }
}

/// Terrain surface for one CELL from its 33x33 height grid.
pub fn land_geometry(cell_x: i32, cell_y: i32, heights: &[[f32; 33]; 33], hide_flags: u8) -> Geometry {
    let vertex = |x: usize, y: usize| -> Vec3 {
        [
            (cell_x as f64 * 4096.0 + x as f64 * 128.0) as f32,
            (cell_y as f64 * 4096.0 + y as f64 * 128.0) as f32,
            heights[y][x],
        ]
    };
    let mut geometry = Geometry::default();
    let mut vertex_map: FxHashMap<[u32; 3], u32> = FxHashMap::default();
    let mut emit = |geometry: &mut Geometry, triangle: [Vec3; 3]| {
        let mut remapped = [0u32; 3];
        for (slot, position) in triangle.iter().enumerate() {
            let key = [position[0].to_bits(), position[1].to_bits(), position[2].to_bits()];
            remapped[slot] = *vertex_map.entry(key).or_insert_with(|| {
                geometry.vertices.push(*position);
                (geometry.vertices.len() - 1) as u32
            });
        }
        geometry.triangles.push(remapped);
    };
    for quadrant_y in 0..2 {
        for quadrant_x in 0..2 {
            let quadrant = quadrant_y * 2 + quadrant_x;
            if hide_flags & (1 << quadrant) != 0 {
                continue;
            }
            for y in quadrant_y * 16..quadrant_y * 16 + 16 {
                for x in quadrant_x * 16..quadrant_x * 16 + 16 {
                    let bottom_left = vertex(x, y);
                    let bottom_right = vertex(x + 1, y);
                    let top_left = vertex(x, y + 1);
                    let top_right = vertex(x + 1, y + 1);
                    if (x + y) & 1 != 0 {
                        emit(&mut geometry, [top_left, bottom_left, bottom_right]);
                        emit(&mut geometry, [bottom_right, top_right, top_left]);
                    } else {
                        emit(&mut geometry, [top_right, top_left, bottom_left]);
                        emit(&mut geometry, [bottom_left, bottom_right, top_right]);
                    }
                }
            }
        }
    }
    geometry
}

pub fn flat_heights(height: f32) -> [[f32; 33]; 33] {
    [[height; 33]; 33]
}

/// Integrates VHGT row-delta encoding in CK's float32 order.
pub fn vhgt_heights(base: f32, deltas: &[[i8; 33]; 33]) -> [[f32; 33]; 33] {
    let mut heights = [[0.0f32; 33]; 33];
    let base_height = mul(base, 8.0);
    for y in 0..33 {
        let mut current = if y == 0 { base_height } else { heights[y - 1][0] };
        for x in 0..33 {
            current = add(current, mul(deltas[y][x] as f32, 8.0));
            heights[y][x] = current;
        }
    }
    heights
}

/// Minimum height in double precision; used only for the task volume floor.
pub fn vhgt_minimum_height(base: f32, deltas: &[[i8; 33]; 33]) -> f64 {
    let base = base as f64 * 8.0;
    let mut previous_first = base;
    let mut minimum = f64::INFINITY;
    for (y, row) in deltas.iter().enumerate() {
        let mut current = if y == 0 { base } else { previous_first };
        for (x, &delta) in row.iter().enumerate() {
            current += delta as f64 * 8.0;
            if x == 0 {
                previous_first = current;
            }
            minimum = minimum.min(current);
        }
    }
    minimum
}

/// One retained shape of a direct (non-precombined) reference model.
#[derive(Clone, Debug)]
pub struct DirectShape {
    /// Decoded vertex positions in shape-local space.
    pub positions: Vec<Vec3>,
    pub indices: Vec<u16>,
    /// Shape transform (omitted when the shape is the root) followed by its
    /// ancestors nearest-first, excluding the root node.
    pub transforms: Vec<Transform>,
    pub bound_center: Vec3,
    pub bound_radius: f32,
    pub occludes: bool,
}

/// Sequentially merged minimal enclosing sphere, in CK's float32 order.
pub fn merged_bound(bounds: &[(Vec3, f32)]) -> Result<(Vec3, f32)> {
    let (mut center, mut radius) = *bounds
        .first()
        .ok_or_else(|| PrevisError::invalid("direct model has no shape bounds"))?;
    for &(other_center, other_radius) in &bounds[1..] {
        let delta = [
            add(other_center[0], -center[0]),
            add(other_center[1], -center[1]),
            add(other_center[2], -center[2]),
        ];
        let distance_squared = add(
            add(mul(delta[0], delta[0]), mul(delta[1], delta[1])),
            mul(delta[2], delta[2]),
        );
        let distance = (distance_squared as f64).sqrt() as f32;
        if radius >= add(distance, other_radius) {
            continue;
        }
        if other_radius >= add(distance, radius) {
            center = other_center;
            radius = other_radius;
            continue;
        }
        let merged_radius = mul(add(add(distance, radius), other_radius), 0.5);
        let fraction = ((merged_radius as f64 - radius as f64) / distance as f64) as f32;
        center = [
            fma_rounded(fraction, delta[0], center[0]),
            fma_rounded(fraction, delta[1], center[1]),
            fma_rounded(fraction, delta[2], center[2]),
        ];
        radius = merged_radius;
    }
    Ok((center, radius))
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DirectFormFlags {
    pub non_occluder: bool,
    pub is_water: bool,
}

/// Builds the one- or two-model scene of a single direct reference.
///
/// Returns the scene plus the reference's world-space vertices, which drive
/// the interior volume.
pub fn direct_scene(
    shapes: &[DirectShape],
    object_id: u32,
    reference: Option<&Transform>,
    form: DirectFormFlags,
) -> Result<(Scene, Vec<Vec3>)> {
    if shapes.is_empty() {
        return Err(PrevisError::unsupported("model has no supported direct geometry"));
    }
    let inverse = reference.map(Transform::inverse).transpose().map_err(PrevisError::invalid)?;
    let mut rows: Vec<(Vec<Vec3>, Vec<u32>, bool)> = Vec::with_capacity(shapes.len());
    let mut world_vertices = Vec::new();
    let mut bounds = Vec::with_capacity(shapes.len());
    for shape in shapes {
        let mut radius = shape.bound_radius;
        for transform in &shape.transforms {
            radius = mul(transform.scale.abs(), radius);
        }
        bounds.push((
            crate::f32ops::apply_chain(&shape.bound_center, &shape.transforms),
            radius,
        ));
        let (positions, indices) = compact_geometry(&shape.positions, &shape.indices);
        let model_positions: Vec<Vec3> = positions
            .iter()
            .map(|p| crate::f32ops::apply_chain(p, &shape.transforms))
            .collect();
        let stored = match (reference, &inverse) {
            (Some(reference), Some(inverse)) => {
                let world: Vec<Vec3> = model_positions.iter().map(|p| reference.apply(p)).collect();
                let stored = world.iter().map(|p| inverse.apply(p)).collect();
                world_vertices.extend(world);
                stored
            }
            _ => {
                world_vertices.extend_from_slice(&model_positions);
                model_positions
            }
        };
        rows.push((stored, indices, shape.occludes));
    }

    let (_, mut bound_radius) = merged_bound(&bounds)?;
    if let Some(reference) = reference {
        bound_radius = mul(reference.scale.abs(), bound_radius);
    }
    let force_target_only = form.non_occluder
        || add(bound_radius, bound_radius) < DIRECT_OCCLUDER_DIAMETER_THRESHOLD
        || form.is_water;
    let all_rows: Vec<(&[Vec3], &[u32])> =
        rows.iter().map(|(p, i, _)| (p.as_slice(), i.as_slice())).collect();
    let occluder_rows: Vec<(&[Vec3], &[u32])> = if force_target_only {
        Vec::new()
    } else {
        rows.iter()
            .filter(|(_, _, occludes)| *occludes)
            .map(|(p, i, _)| (p.as_slice(), i.as_slice()))
            .collect()
    };
    let transform = reference.map_or(IDENTITY_MATRIX, Transform::scene_matrix);
    let mut scene = Scene::default();
    scene.geometries.push(merge_geometry(&all_rows));
    if !occluder_rows.is_empty() && occluder_rows.len() != all_rows.len() {
        scene.geometries.push(merge_geometry(&occluder_rows));
        scene.instances.push(instance(object_id, 0, transform, INSTANCE_TARGET, &scene.geometries[0]));
        scene.instances.push(instance(
            (object_id & 0x00FF_FFFF) | 0xFE00_0000,
            1,
            transform,
            INSTANCE_OCCLUDER,
            &scene.geometries[1],
        ));
    } else {
        let flags = if occluder_rows.is_empty() {
            INSTANCE_TARGET
        } else {
            INSTANCE_TARGET_OCCLUDER
        };
        scene.instances.push(instance(object_id, 0, transform, flags, &scene.geometries[0]));
    }
    scene.volumes.push(snapped_volume(&world_vertices));
    Ok((scene, world_vertices))
}

#[derive(Clone, Debug)]
pub struct LandSurface {
    pub x: i32,
    pub y: i32,
    /// Umbra target ID: the LAND FormID, or the child CELL for inherited LAND.
    pub target_id: u32,
    pub heights: [[f32; 33]; 33],
    pub hide_flags: u8,
}

/// One direct reference placed in an exterior cluster, with its pooling key.
pub struct ExteriorDirect {
    pub model_key: String,
    pub scene: Scene,
}

/// Combines LAND surfaces and direct-reference scenes into one cluster scene.
///
/// `directs` must already be in CK's global reverse traversal order.
pub fn exterior_scene(lands: &[LandSurface], directs: Vec<ExteriorDirect>, volume: [f32; 12]) -> Result<Scene> {
    let mut builder = PooledScene::with_lands(lands);
    if builder.scene.geometries.is_empty() && directs.is_empty() {
        return Err(PrevisError::unsupported("exterior scene has no LAND or direct geometry"));
    }
    for direct in directs {
        builder.push(direct);
    }
    Ok(builder.finish(volume))
}

/// The LAND surfaces a scene draws (any quadrant shown), sorted by grid coordinate.
pub fn visible_lands(lands: &[LandSurface]) -> Vec<&LandSurface> {
    let mut lands: Vec<&LandSurface> = lands.iter().filter(|l| l.hide_flags & 0x0F != 0x0F).collect();
    lands.sort_by_key(|land| (land.x, land.y));
    lands
}

/// A visible LAND surface's geometry and its instance (model index `index`).
pub fn land_instance(land: &LandSurface, index: u32) -> (Geometry, Instance) {
    let geometry = land_geometry(land.x, land.y, &land.heights, land.hide_flags);
    let instance = instance(land.target_id, index, IDENTITY_MATRIX, INSTANCE_TARGET_OCCLUDER, &geometry);
    (geometry, instance)
}

/// Incremental form of [`exterior_scene`]: each direct scene is pooled as it
/// arrives, so a cluster never holds more than one copy of a model's geometry.
#[derive(Default)]
pub struct PooledScene {
    pub scene: Scene,
    shared: FxHashMap<(String, usize), u32>,
}

impl PooledScene {
    /// Starts with the visible LAND surfaces, sorted by grid coordinate.
    pub fn with_lands(lands: &[LandSurface]) -> Self {
        let mut scene = Scene::default();
        let (geometries, instances): (Vec<_>, Vec<_>) =
            visible_lands(lands).into_iter().enumerate().map(|(index, land)| land_instance(land, index as u32)).unzip();
        scene.geometries = geometries;
        scene.instances = instances;
        PooledScene {
            scene,
            shared: FxHashMap::default(),
        }
    }

    /// Appends one direct scene, sharing geometry per `(model_key, local index)`.
    pub fn push(&mut self, direct: ExteriorDirect) {
        let mut local = Vec::with_capacity(direct.scene.geometries.len());
        for (local_index, geometry) in direct.scene.geometries.into_iter().enumerate() {
            let key = (direct.model_key.clone(), local_index);
            let model_index = match self.shared.get(&key) {
                Some(&index) => index,
                None => {
                    let index = self.scene.geometries.len() as u32;
                    self.scene.geometries.push(geometry);
                    self.shared.insert(key, index);
                    index
                }
            };
            local.push(model_index);
        }
        for mut instance in direct.scene.instances {
            instance.model_index = local[instance.model_index as usize];
            self.scene.instances.push(instance);
        }
    }

    pub fn finish(mut self, volume: [f32; 12]) -> Scene {
        self.scene.volumes.push(volume);
        self.scene
    }
}

/// Receives a planned scene piece by piece: the planner streams every model
/// into a sink as soon as it is built, so no stage but a [`SceneBuilder`]
/// ever holds the whole scene.
///
/// LAND comes first; reference models follow in scene order. Group shapes of
/// one precombined CELL wait until their CELL is committed (every group read)
/// or discarded (the CELL falls back to its source references), and follow
/// all reference models in the scene, in commit order.
pub trait SceneSink {
    fn lands(&mut self, lands: &[LandSurface]);
    /// One reference's scene; models are shared per `(model_key, local
    /// index)`, the first reference's geometry serving every later one.
    fn pooled(&mut self, model_key: &str, direct: Scene);
    fn combined(&mut self, row: CombinedRow);
    /// Keeps the pending rows, the `index`-th surviving one as object
    /// `object_id(index)`; with `occluders_only`, targets lose that role and
    /// rows that do not occlude are dropped.
    fn commit_combined(&mut self, occluders_only: bool, object_id: &dyn Fn(usize) -> u32);
    fn discard_combined(&mut self);
    fn has_target(&self) -> bool;
    /// Whether LAND, a reference model or a committed CELL's groups arrived.
    fn has_geometry(&self) -> bool;
}

/// The sink that assembles the input scene CK's optimizer reads (format 15),
/// for scene files, parity checks and ground truth.
#[derive(Default)]
pub struct SceneBuilder {
    pooled: PooledScene,
    pending: Vec<CombinedRow>,
    combined: Vec<(u32, CombinedRow)>,
    committed_cells: usize,
}

impl SceneBuilder {
    pub fn finish(self, volume: [f32; 12]) -> Scene {
        let mut scene = self.pooled.finish(volume);
        for (object_id, (geometry, transform, flags)) in self.combined {
            let model_index = scene.geometries.len() as u32;
            scene.instances.push(instance(object_id, model_index, transform, flags, &geometry));
            scene.geometries.push(geometry);
        }
        scene
    }
}

impl SceneSink for SceneBuilder {
    fn lands(&mut self, lands: &[LandSurface]) {
        self.pooled = PooledScene::with_lands(lands);
    }

    fn pooled(&mut self, model_key: &str, direct: Scene) {
        self.pooled.push(ExteriorDirect { model_key: model_key.to_string(), scene: direct });
    }

    fn combined(&mut self, row: CombinedRow) {
        self.pending.push(row);
    }

    fn commit_combined(&mut self, occluders_only: bool, object_id: &dyn Fn(usize) -> u32) {
        let mut rows = std::mem::take(&mut self.pending);
        if occluders_only {
            rows.retain_mut(|row| {
                row.2 &= INSTANCE_OCCLUDER;
                row.2 != 0
            });
        }
        self.combined.extend(rows.into_iter().enumerate().map(|(index, row)| (object_id(index), row)));
        self.committed_cells += 1;
    }

    fn discard_combined(&mut self) {
        self.pending.clear();
    }

    fn has_target(&self) -> bool {
        self.pooled.scene.instances.iter().any(|i| i.flags & INSTANCE_TARGET != 0)
            || self.combined.iter().any(|(_, row)| row.2 & INSTANCE_TARGET != 0)
    }

    fn has_geometry(&self) -> bool {
        !self.pooled.scene.geometries.is_empty() || self.committed_cells > 0
    }
}

/// Feeds one planned scene to two sinks, for example an occupancy for the
/// solve and a [`SceneBuilder`] for a debug scene file.
pub struct TeeSink<'a>(pub &'a mut dyn SceneSink, pub &'a mut dyn SceneSink);

impl SceneSink for TeeSink<'_> {
    fn lands(&mut self, lands: &[LandSurface]) {
        self.0.lands(lands);
        self.1.lands(lands);
    }

    fn pooled(&mut self, model_key: &str, direct: Scene) {
        self.0.pooled(model_key, direct.clone());
        self.1.pooled(model_key, direct);
    }

    fn combined(&mut self, row: CombinedRow) {
        self.0.combined(row.clone());
        self.1.combined(row);
    }

    fn commit_combined(&mut self, occluders_only: bool, object_id: &dyn Fn(usize) -> u32) {
        self.0.commit_combined(occluders_only, object_id);
        self.1.commit_combined(occluders_only, object_id);
    }

    fn discard_combined(&mut self) {
        self.0.discard_combined();
        self.1.discard_combined();
    }

    fn has_target(&self) -> bool {
        self.0.has_target()
    }

    fn has_geometry(&self) -> bool {
        self.0.has_geometry()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_land_has_expected_topology() {
        let geometry = land_geometry(0, 0, &flat_heights(-2048.0), 0);
        assert_eq!(geometry.vertices.len(), 33 * 33);
        assert_eq!(geometry.triangles.len(), 32 * 32 * 2);
    }

    #[test]
    fn hidden_quadrant_drops_its_triangles() {
        let geometry = land_geometry(0, 0, &flat_heights(0.0), 0b0001);
        assert_eq!(geometry.triangles.len(), 32 * 32 * 2 - 16 * 16 * 2);
    }

    #[test]
    fn scene_encoding_has_header_and_volume() {
        let scene = Scene {
            volumes: vec![exterior_volume(0, 0, -128.0)],
            ..Scene::default()
        };
        let bytes = scene.encode();
        assert_eq!(&bytes[..4], &SCENE_MAGIC.to_le_bytes());
        assert_eq!(bytes.len(), 16 + 21 * 4 + 8 + 48);
    }

    #[test]
    fn scene_decode_inverts_encode() {
        let scene = Scene {
            geometries: vec![Geometry { vertices: vec![[0.0, 1.0, 2.0], [3.0, 4.0, 5.0], [6.0, 7.0, 8.0]], triangles: vec![[0, 1, 2]] }],
            instances: vec![Instance {
                object_id: 0x0100_0801,
                model_index: 0,
                transform: IDENTITY_MATRIX,
                flags: INSTANCE_TARGET_OCCLUDER,
                vector: [1.0, 2.0, 3.0],
                bounds: [0.0, 1.0, 2.0, 6.0, 7.0, 8.0],
            }],
            volumes: vec![exterior_volume(0, 0, -128.0)],
        };
        let bytes = scene.encode();
        assert_eq!(Scene::decode(&bytes).unwrap(), scene);
        assert!(Scene::decode(&bytes[..bytes.len() - 4]).is_err());
    }
}
