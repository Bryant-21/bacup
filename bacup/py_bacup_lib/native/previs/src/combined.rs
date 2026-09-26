//! Previs scene for a CELL whose references are precombined: every group
//! shape contributes its PSG geometry, expanded through the PCD transforms.

use nif_core_native::model::{NifBlock, NifFile, NifValue};
use rustc_hash::FxHashSet;

use crate::assets::AssetResolver;
use crate::error::{PrevisError, Result};
use crate::f32ops::{Transform, Vec3, add, dot, mul};
use crate::nif::{float, integer, matrix, shader_state, vec3};
use crate::scene::{
    Geometry, INSTANCE_TARGET, INSTANCE_TARGET_OCCLUDER, MODEL_ID_BASE, Scene, compact_geometry, instance,
    snapped_volume,
};

const POSITION_PRECISION_MASK: u64 = 0x0040_1000_0000_0000;

pub struct PsgEntry {
    pub vertex_desc: u64,
    pub vertex_count: usize,
    pub index_count: usize,
    pub data: Vec<u8>,
}

/// Shared-geometry rows by data offset, read as group shapes reach them.
pub trait PsgSource {
    /// `None` when no row starts at `offset`.
    fn entry(&mut self, offset: u32) -> Result<Option<&PsgEntry>>;
}

fn positions(entry: &PsgEntry) -> Result<Vec<Vec3>> {
    let stride = ((entry.vertex_desc & 0xF) * 4) as usize;
    if stride == 0 {
        return Err(PrevisError::invalid("PSG vertex descriptor has a zero stride"));
    }
    let precision = entry.vertex_desc & POSITION_PRECISION_MASK;
    let full = precision == 0 || precision == POSITION_PRECISION_MASK;
    Ok((0..entry.vertex_count)
        .map(|index| {
            let base = index * stride;
            let d = &entry.data;
            if full {
                [0, 4, 8].map(|o| f32::from_le_bytes(d[base + o..base + o + 4].try_into().unwrap()))
            } else {
                [0, 2, 4].map(|o| half::f16::from_bits(u16::from_le_bytes([d[base + o], d[base + o + 1]])).to_f32())
            }
        })
        .collect())
}

fn indices(entry: &PsgEntry) -> Vec<u16> {
    let offset = entry.vertex_count * ((entry.vertex_desc & 0xF) * 4) as usize;
    (0..entry.index_count)
        .map(|i| u16::from_le_bytes([entry.data[offset + i * 2], entry.data[offset + i * 2 + 1]]))
        .collect()
}

struct ShapePair {
    shape: usize,
    packed: usize,
    shader: usize,
}

/// Group shapes in hierarchy preorder, each with its PCD and shader.
fn shape_pairs(nif: &NifFile) -> Result<Vec<ShapePair>> {
    let schema = &*nif_core_native::schema::SCHEMA;
    let mut visited = FxHashSet::default();
    let mut pairs = Vec::new();
    fn visit(
        nif: &NifFile,
        schema: &nif_core_native::schema::NifSchema,
        id: usize,
        visited: &mut FxHashSet<usize>,
        pairs: &mut Vec<ShapePair>,
    ) -> Result<()> {
        if !visited.insert(id) {
            return Ok(());
        }
        let block = &nif.blocks[id];
        let children: Vec<usize> = block
            .get_refs(schema)
            .into_iter()
            .filter(|&r| r >= 0 && (r as usize) < nif.blocks.len())
            .map(|r| r as usize)
            .collect();
        if matches!(block.type_name.as_str(), "BSTriShape" | "BSMeshLODTriShape") {
            let owned: Vec<usize> = children.into_iter().filter(|&c| visited.insert(c)).collect();
            let of = |name: &str| owned.iter().copied().filter(|&c| nif.blocks[c].type_name == name).collect::<Vec<_>>();
            let packed = of("BSPackedCombinedSharedGeomDataExtra");
            let shaders: Vec<usize> = owned
                .iter()
                .copied()
                .filter(|&c| nif.blocks[c].type_name.ends_with("ShaderProperty"))
                .collect();
            if packed.len() != 1 || shaders.len() != 1 || of("NiAlphaProperty").len() > 1 {
                return Err(PrevisError::unsupported(format!(
                    "group shape {id} is outside the PCD + shader subset"
                )));
            }
            pairs.push(ShapePair {
                shape: id,
                packed: packed[0],
                shader: shaders[0],
            });
            return Ok(());
        }
        for child in children {
            visit(nif, schema, child, visited, pairs)?;
        }
        Ok(())
    }
    let roots: Vec<usize> = nif
        .header
        .footer_roots
        .iter()
        .filter(|&&r| r >= 0 && (r as usize) < nif.blocks.len())
        .map(|&r| r as usize)
        .collect();
    for root in if roots.is_empty() { vec![0] } else { roots } {
        visit(nif, schema, root, &mut visited, &mut pairs)?;
    }
    Ok(pairs)
}

fn structs(value: Option<&NifValue>) -> Result<&[NifValue]> {
    match value {
        Some(NifValue::Array(items)) => Ok(items),
        _ => Err(PrevisError::invalid("PCD table is missing")),
    }
}

fn member<'a>(value: &'a NifValue, name: &str) -> Result<&'a NifValue> {
    match value {
        NifValue::Struct(fields) => fields
            .get(name)
            .ok_or_else(|| PrevisError::invalid(format!("PCD entry has no {name}"))),
        _ => Err(PrevisError::invalid("PCD entry is not a struct")),
    }
}

fn member_u64(value: &NifValue, name: &str) -> Result<u64> {
    match member(value, name)? {
        NifValue::UInt(v) => Ok(*v),
        NifValue::Int(v) if *v >= 0 => Ok(*v as u64),
        _ => Err(PrevisError::invalid(format!("PCD {name} is not an integer"))),
    }
}

fn member_transform(value: &NifValue) -> Result<Transform> {
    let invalid = || PrevisError::invalid("PCD transform is malformed");
    Ok(Transform {
        rotation: matrix(member(value, "Rotation")?).ok_or_else(invalid)?,
        translation: vec3(member(value, "Translation")?).ok_or_else(invalid)?,
        scale: float(member(value, "Scale")?).ok_or_else(invalid)?,
    })
}

/// CK re-derives the shape origin from the PCD instance bounds in double
/// precision, rounding only the final centre.
fn shape_translation(object_data: &[NifValue]) -> Result<Vec3> {
    let mut bounds = Vec::new();
    for data in object_data {
        for combined in structs(Some(member(data, "Combined")?))? {
            let sphere = member(combined, "Bounding Sphere")?;
            let center = vec3(member(sphere, "Center")?).ok_or_else(|| PrevisError::invalid("PCD bound centre"))?;
            let radius = float(member(sphere, "Radius")?).ok_or_else(|| PrevisError::invalid("PCD bound radius"))?;
            bounds.push(([center[0] as f64, center[1] as f64, center[2] as f64], radius as f64));
        }
    }
    let (&(mut center, mut radius), rest) = bounds
        .split_first()
        .ok_or_else(|| PrevisError::invalid("PCD object has no combined instances"))?;
    for &(other_center, other_radius) in rest {
        let delta = [0, 1, 2].map(|a| other_center[a] - center[a]);
        let distance = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
        if radius >= distance + other_radius {
            continue;
        }
        if other_radius >= distance + radius {
            center = other_center;
            radius = other_radius;
            continue;
        }
        let merged = (distance + radius + other_radius) * 0.5;
        let fraction = (merged - radius) / distance;
        center = [0, 1, 2].map(|a| center[a] + fraction * delta[a]);
        radius = merged;
    }
    Ok(center.map(|v| v as f32))
}

fn apply_inverse(point: &Vec3, rotation: &[[f32; 3]; 3], translation: &Vec3, scale: f32) -> Vec3 {
    let delta = [0, 1, 2].map(|i| add(point[i], -translation[i]));
    let inverse_scale = (1.0f64 / scale as f64) as f32;
    [0, 1, 2].map(|i| mul(dot(&[rotation[0][i], rotation[1][i], rotation[2][i]], &delta), inverse_scale))
}

fn shape_geometry(
    shape: &NifBlock,
    packed: &NifBlock,
    psg: &mut dyn PsgSource,
    world: &mut Vec<Vec3>,
) -> Result<(Geometry, [f32; 12])> {
    let objects = structs(packed.get_field("Object"))?;
    let object_data = structs(packed.get_field("Object Data"))?;
    if integer(packed, "Num Data")? as usize != objects.len() || objects.len() != object_data.len() {
        return Err(PrevisError::invalid("PCD object tables have inconsistent lengths"));
    }
    let rotation = matrix(shape.get_field("Rotation").ok_or_else(|| PrevisError::invalid("shape rotation"))?)
        .ok_or_else(|| PrevisError::invalid("shape rotation"))?;
    let scale = float(shape.get_field("Scale").ok_or_else(|| PrevisError::invalid("shape scale"))?)
        .ok_or_else(|| PrevisError::invalid("shape scale"))?;
    if scale == 0.0 {
        return Err(PrevisError::invalid("combined shape has zero scale"));
    }
    let translation = shape_translation(object_data)?;
    let lod = shape.type_name == "BSMeshLODTriShape";
    let mut geometry = Geometry::default();
    for (object, data) in objects.iter().zip(object_data).rev() {
        let offset = member_u64(object, "Data Offset")? as u32;
        let entry = psg
            .entry(offset)?
            .ok_or_else(|| PrevisError::invalid(format!("PCD references missing PSG offset {offset}")))?;
        if member_u64(data, "Vertex Desc")? != entry.vertex_desc || member_u64(data, "Num Verts")? as usize != entry.vertex_count {
            return Err(PrevisError::invalid("PCD object metadata does not match its PSG entry"));
        }
        let all = indices(entry);
        let (first, count) = if lod {
            (0, entry.index_count)
        } else {
            (member_u64(data, "Tri Offset LOD0")? as usize, member_u64(data, "Tri Count LOD0")? as usize * 3)
        };
        let range = all
            .get(first..first + count)
            .filter(|r| r.iter().all(|&i| (i as usize) < entry.vertex_count))
            .ok_or_else(|| PrevisError::invalid("PCD LOD0 index range is invalid"))?;
        let (compact, remapped) = compact_geometry(&positions(entry)?, range);
        for combined in structs(Some(member(data, "Combined")?))? {
            let transform = member_transform(member(combined, "Transform")?)?;
            let base = geometry.vertices.len() as u32;
            for point in &compact {
                let placed = transform.apply(point);
                world.push(placed);
                geometry.vertices.push(apply_inverse(&placed, &rotation, &translation, scale));
            }
            geometry
                .triangles
                .extend(remapped.chunks_exact(3).map(|t| [base + t[0], base + t[1], base + t[2]]));
        }
    }
    if geometry.vertices.len() > integer(packed, "Num Vertices")? as usize {
        return Err(PrevisError::invalid("compacted vertex count exceeds the shape vertex count"));
    }
    if geometry.triangles.len() != integer(packed, "Num Triangles")? as usize {
        return Err(PrevisError::invalid("PCD combined triangle count does not match the shape"));
    }
    let matrix = [
        rotation[0][0], rotation[0][1], rotation[0][2], translation[0],
        rotation[1][0], rotation[1][1], rotation[1][2], translation[1],
        rotation[2][0], rotation[2][1], rotation[2][2], translation[2],
    ];
    Ok((geometry, matrix))
}

/// One group shape: local geometry, its scene matrix and instance flags.
pub type CombinedRow = (Geometry, [f32; 12], u32);

/// Error when a CELL's groups hold no usable shape at all.
pub fn no_combined_geometry() -> PrevisError {
    PrevisError::invalid("no supported combined geometry found")
}

/// The group shapes of one group NIF in hierarchy preorder, each passed to
/// `emit` with the world-space vertices it places. Across a CELL's groups in
/// XCRI mesh-ID order, the running shape index is the index in CK's combined
/// object IDs.
pub fn group_rows(
    nif: &NifFile,
    psg: &mut dyn PsgSource,
    assets: &AssetResolver,
    emit: &mut dyn FnMut(CombinedRow, &[Vec3]),
) -> Result<()> {
    let mut world = Vec::new();
    for pair in shape_pairs(nif)? {
        world.clear();
        let (geometry, transform) = shape_geometry(&nif.blocks[pair.shape], &nif.blocks[pair.packed], psg, &mut world)?;
        let shader = &nif.blocks[pair.shader];
        let (non_occluder, zbuffer_write) = shader_state(Some(shader), assets)?;
        let lighting = shader.type_name == "BSLightingShaderProperty";
        let occludes = !non_occluder && (!lighting || zbuffer_write);
        emit((geometry, transform, if occludes { INSTANCE_TARGET_OCCLUDER } else { INSTANCE_TARGET }), &world);
    }
    Ok(())
}

/// Builds the scene from the plugin shared geometry and the CELL's group NIFs
/// in XCRI mesh-ID order.
pub fn combined_scene(psg: &mut dyn PsgSource, groups: &[NifFile], assets: &AssetResolver) -> Result<Scene> {
    let mut scene = Scene::default();
    let mut world = Vec::new();
    for nif in groups {
        group_rows(nif, psg, assets, &mut |(geometry, transform, flags), placed| {
            let index = scene.instances.len() as u32;
            scene.instances.push(instance(MODEL_ID_BASE + index, index, transform, flags, &geometry));
            scene.geometries.push(geometry);
            world.extend_from_slice(placed);
        })?;
    }
    if scene.instances.is_empty() || world.is_empty() {
        return Err(no_combined_geometry());
    }
    scene.volumes.push(snapped_volume(&world));
    Ok(scene)
}
