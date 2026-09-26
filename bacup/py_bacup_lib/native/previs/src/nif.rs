//! Direct-previs shape extraction from Fallout 4 NIFs.

use nif_core_native::model::{NifBlock, NifFile, NifValue};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::assets::AssetResolver;
use crate::error::{PrevisError, Result};
use crate::f32ops::{Mat3, Transform, Vec3};
use crate::scene::DirectShape;

const POSITION_PRECISION_MASK: u64 = 0x0040_1000_0000_0000;
const SHAPE_TYPES: [&str; 3] = ["BSTriShape", "BSMeshLODTriShape", "BSSubIndexTriShape"];
const LIGHTING_SHADER: &str = "BSLightingShaderProperty";
const ZBUFFER_WRITE_BIT: u64 = 1;

struct ShapeRow {
    shape_id: usize,
    shader_id: Option<usize>,
    parent_ids: Vec<usize>,
    is_root: bool,
}

fn block_type<'a>(nif: &'a NifFile, id: usize) -> &'a str {
    nif.blocks[id].type_name.as_str()
}

/// Preorder walk over Ref/Ptr links from the footer roots, visiting each
/// block once, exactly like the hierarchy the research tooling traversed.
fn shape_rows(nif: &NifFile) -> Result<Vec<ShapeRow>> {
    let schema = &*nif_core_native::schema::SCHEMA;
    let mut roots: Vec<usize> = nif
        .header
        .footer_roots
        .iter()
        .filter(|&&root| root >= 0 && (root as usize) < nif.blocks.len())
        .map(|&root| root as usize)
        .collect();
    if roots.is_empty() && !nif.blocks.is_empty() {
        roots.push(0);
    }
    let parents = scene_parents(nif);
    let mut visited = FxHashSet::default();
    let mut rows = Vec::new();

    fn visit(
        nif: &NifFile,
        schema: &nif_core_native::schema::NifSchema,
        id: usize,
        parents: &FxHashMap<usize, usize>,
        root_id: usize,
        visited: &mut FxHashSet<usize>,
        rows: &mut Vec<ShapeRow>,
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
        if SHAPE_TYPES.contains(&block.type_name.as_str()) {
            // The hierarchy claims the shape's whole subtree, so a block shared
            // with a later shape (for example a reused shader) is not its child.
            let mut shaders = Vec::new();
            for &child in &children {
                if claim(nif, schema, child, visited) && block_type(nif, child).ends_with("ShaderProperty") {
                    shaders.push(child);
                }
            }
            if shaders.len() > 1 {
                return Err(PrevisError::unsupported(format!(
                    "shape block {id} has more than one shader property"
                )));
            }
            rows.push(ShapeRow {
                shape_id: id,
                shader_id: shaders.first().copied(),
                parent_ids: scene_ancestors(parents, id, root_id),
                is_root: id == root_id,
            });
            return Ok(());
        }
        for child in children {
            visit(nif, schema, child, parents, root_id, visited, rows)?;
        }
        Ok(())
    }

    fn claim(
        nif: &NifFile,
        schema: &nif_core_native::schema::NifSchema,
        id: usize,
        visited: &mut FxHashSet<usize>,
    ) -> bool {
        if !visited.insert(id) {
            return false;
        }
        for child in nif.blocks[id].get_refs(schema) {
            if child >= 0 && (child as usize) < nif.blocks.len() {
                claim(nif, schema, child as usize, visited);
            }
        }
        true
    }

    for root in roots {
        visit(nif, schema, root, &parents, root, &mut visited, &mut rows)?;
    }
    Ok(rows)
}

/// Each block's scene-graph parent from the nodes' `Children` lists. The
/// Ref/Ptr walk can reach a node through an animation controller's targets
/// first, and a controller has no transform to contribute.
fn scene_parents(nif: &NifFile) -> FxHashMap<usize, usize> {
    let mut parents = FxHashMap::default();
    for (id, block) in nif.blocks.iter().enumerate() {
        let Some(NifValue::Array(children)) = block.get_field("Children") else {
            continue;
        };
        for child in children {
            if let NifValue::Ref(r) = child
                && *r >= 0
                && (*r as usize) < nif.blocks.len()
            {
                parents.insert(*r as usize, id);
            }
        }
    }
    parents
}

/// Parents of `id` nearest first, stopping below `root_id`.
fn scene_ancestors(parents: &FxHashMap<usize, usize>, id: usize, root_id: usize) -> Vec<usize> {
    let mut chain = Vec::new();
    let mut current = id;
    while let Some(&parent) = parents.get(&current) {
        if parent == root_id || chain.len() > parents.len() {
            break;
        }
        chain.push(parent);
        current = parent;
    }
    chain
}

pub(crate) fn field<'a>(block: &'a NifBlock, name: &str) -> Result<&'a NifValue> {
    block.get_field(name).ok_or_else(|| {
        PrevisError::invalid(format!("{} block {} has no {name}", block.type_name, block.block_id))
    })
}

pub(crate) fn float(value: &NifValue) -> Option<f32> {
    match value {
        NifValue::Float(v) => Some(*v as f32),
        NifValue::Int(v) => Some(*v as f32),
        NifValue::UInt(v) => Some(*v as f32),
        _ => None,
    }
}

pub(crate) fn integer(block: &NifBlock, name: &str) -> Result<u64> {
    match field(block, name)? {
        NifValue::Int(v) if *v >= 0 => Ok(*v as u64),
        NifValue::UInt(v) => Ok(*v),
        other => Err(PrevisError::invalid(format!(
            "{} block {} field {name} is not an unsigned integer: {other:?}",
            block.type_name, block.block_id
        ))),
    }
}

pub(crate) fn vec3(value: &NifValue) -> Option<Vec3> {
    match value {
        NifValue::Vec3(v) => Some(*v),
        NifValue::Struct(fields) => Some([
            float(fields.get("x")?)?,
            float(fields.get("y")?)?,
            float(fields.get("z")?)?,
        ]),
        _ => None,
    }
}

/// Rows are `(m11, m21, m31)`, `(m12, m22, m32)`, `(m13, m23, m33)`.
pub(crate) fn matrix(value: &NifValue) -> Option<Mat3> {
    match value {
        NifValue::Matrix33(m) => Some(*m),
        NifValue::Struct(fields) => {
            let get = |name: &str| fields.get(name).and_then(float);
            Some([
                [get("m11")?, get("m21")?, get("m31")?],
                [get("m12")?, get("m22")?, get("m32")?],
                [get("m13")?, get("m23")?, get("m33")?],
            ])
        }
        _ => None,
    }
}

pub(crate) fn block_transform(block: &NifBlock) -> Result<Transform> {
    let rotation = matrix(field(block, "Rotation")?)
        .ok_or_else(|| PrevisError::invalid(format!("block {} Rotation", block.block_id)))?;
    let translation = vec3(field(block, "Translation")?)
        .ok_or_else(|| PrevisError::invalid(format!("block {} Translation", block.block_id)))?;
    let scale = float(field(block, "Scale")?)
        .ok_or_else(|| PrevisError::invalid(format!("block {} Scale", block.block_id)))?;
    Ok(Transform {
        rotation,
        translation,
        scale,
    })
}

pub(crate) fn bounding_sphere(block: &NifBlock) -> Result<(Vec3, f32)> {
    let NifValue::Struct(sphere) = field(block, "Bounding Sphere")? else {
        return Err(PrevisError::invalid(format!(
            "direct shape {} has no structured bounding sphere",
            block.block_id
        )));
    };
    let center = sphere.get("Center").and_then(vec3);
    let radius = sphere.get("Radius").and_then(float);
    match (center, radius) {
        (Some(center), Some(radius)) => Ok((center, radius)),
        _ => Err(PrevisError::invalid(format!(
            "direct shape {} bounding sphere is incomplete",
            block.block_id
        ))),
    }
}

fn has_flag(value: Option<&NifValue>, name: &str, bit: u64) -> bool {
    match value {
        Some(NifValue::Array(items)) => items
            .iter()
            .any(|item| matches!(item, NifValue::String(s) if s == name)),
        Some(NifValue::UInt(v)) => v & bit != 0,
        Some(NifValue::Int(v)) => (*v as u64) & bit != 0,
        _ => false,
    }
}

fn string_field(block: &NifBlock, name: &str) -> String {
    match block.get_field(name) {
        Some(NifValue::String(s)) => s.trim_end_matches('\0').to_string(),
        _ => String::new(),
    }
}

/// `(non_occluder, zbuffer_write)` as resolved from the shader's material.
pub(crate) fn shader_state(shader: Option<&NifBlock>, assets: &AssetResolver) -> Result<(bool, bool)> {
    let Some(shader) = shader else {
        return Ok((false, false));
    };
    let material = string_field(shader, "Name");
    if material.is_empty() {
        let flags = shader.get_field("Shader Flags 2");
        return Ok((false, has_flag(flags, "ZBuffer_Write", ZBUFFER_WRITE_BIT)));
    }
    let relative = material_relative_path(&material);
    let bytes = assets.read(&relative)?.ok_or_else(|| {
        PrevisError::unsupported(format!("direct-shape material not found: {relative}"))
    })?;
    let lower = relative.to_ascii_lowercase();
    let header = if lower.ends_with(".bgsm") {
        materials_native::bgsm::parse(&bytes)
            .map_err(|e| PrevisError::invalid(format!("{relative}: {e}")))?
            .header
    } else if lower.ends_with(".bgem") {
        materials_native::bgem::parse(&bytes)
            .map_err(|e| PrevisError::invalid(format!("{relative}: {e}")))?
            .header
    } else {
        return Err(PrevisError::unsupported(format!("unsupported direct-shape material: {relative}")));
    };
    Ok((header.non_occluder, header.zbuffer_write))
}

/// Strips absolute CK build prefixes through the embedded `materials/` segment.
pub fn material_relative_path(shader_name: &str) -> String {
    let normalized = shader_name.replace('\\', "/");
    let marker = "materials/";
    let tail = match normalized.to_ascii_lowercase().find(marker) {
        Some(offset) => normalized[offset + marker.len()..].to_string(),
        None => normalized,
    };
    format!("materials/{tail}")
}

/// A packed tri shape's vertex data followed by its triangles, from a NIF read
/// with `NifFile::from_bytes_raw_arrays`.
pub(crate) fn geometry_buffer(block: &NifBlock) -> Result<(u64, usize, usize, Vec<u8>)> {
    let descriptor = integer(block, "Vertex Desc")?;
    let triangle_count = integer(block, "Num Triangles")? as usize;
    let vertex_count = integer(block, "Num Vertices")? as usize;
    let data_size = integer(block, "Data Size")? as usize;
    let (Some(NifValue::Bytes(vertices)), Some(NifValue::Bytes(triangles))) =
        (block.get_field("Vertex Data"), block.get_field("Triangles"))
    else {
        return Err(PrevisError::invalid(format!(
            "packed tri shape {} has an invalid data size",
            block.block_id
        )));
    };
    if data_size == 0 || vertices.len() + triangles.len() != data_size {
        return Err(PrevisError::invalid(format!(
            "packed tri shape {} has an invalid data size",
            block.block_id
        )));
    }
    let stride = ((descriptor & 0xF) * 4) as usize;
    if vertex_count * stride + triangle_count * 6 != data_size {
        return Err(PrevisError::invalid(format!(
            "packed tri shape {} geometry size is inconsistent",
            block.block_id
        )));
    }
    Ok((descriptor, vertex_count, triangle_count, [vertices.as_slice(), triangles].concat()))
}

pub(crate) fn decode_positions(descriptor: u64, vertex_count: usize, data: &[u8]) -> Result<Vec<Vec3>> {
    let stride = ((descriptor & 0xF) * 4) as usize;
    if stride == 0 {
        return Err(PrevisError::invalid("vertex descriptor has a zero stride"));
    }
    let precision = descriptor & POSITION_PRECISION_MASK;
    let full = precision == 0 || precision == POSITION_PRECISION_MASK;
    let position_size = if full { 12 } else { 6 };
    if position_size > stride {
        return Err(PrevisError::invalid("vertex stride is smaller than its position"));
    }
    Ok((0..vertex_count)
        .map(|index| {
            let base = index * stride;
            if full {
                [0, 4, 8].map(|o| f32::from_le_bytes(data[base + o..base + o + 4].try_into().unwrap()))
            } else {
                [0, 2, 4].map(|o| {
                    half::f16::from_bits(u16::from_le_bytes([data[base + o], data[base + o + 1]])).to_f32()
                })
            }
        })
        .collect())
}

pub(crate) fn decode_indices(descriptor: u64, vertex_count: usize, triangle_count: usize, data: &[u8]) -> Vec<u16> {
    let offset = vertex_count * ((descriptor & 0xF) * 4) as usize;
    (0..triangle_count * 3)
        .map(|i| u16::from_le_bytes([data[offset + i * 2], data[offset + i * 2 + 1]]))
        .collect()
}

/// World-space bounding spheres of a model's shapes placed by `placement`,
/// merged the way CK derives a reference's runtime bound.
pub fn model_bound(nif: &NifFile, placement: &Transform) -> Result<(Vec3, f32)> {
    placed_bound(&bound_rows(nif)?, placement)
}

/// A shape's local bounding sphere and its transform chain up to the model
/// root, which is all `placed_bound` needs from the NIF.
pub struct BoundRow {
    center: Vec3,
    radius: f32,
    chain: Vec<Transform>,
}

pub fn bound_rows(nif: &NifFile) -> Result<Vec<BoundRow>> {
    let mut rows = Vec::new();
    for row in shape_rows(nif)? {
        let block = &nif.blocks[row.shape_id];
        let mut chain = Vec::with_capacity(row.parent_ids.len() + 1);
        if !row.is_root {
            chain.push(block_transform(block)?);
        }
        for &parent in &row.parent_ids {
            chain.push(block_transform(&nif.blocks[parent])?);
        }
        let (center, radius) = bounding_sphere(block)?;
        rows.push(BoundRow { center, radius, chain });
    }
    Ok(rows)
}

pub fn placed_bound(rows: &[BoundRow], placement: &Transform) -> Result<(Vec3, f32)> {
    let mut bounds = Vec::with_capacity(rows.len());
    for row in rows {
        let mut transforms = row.chain.clone();
        transforms.push(*placement);
        let mut radius = row.radius;
        for transform in &transforms {
            radius = transform.scale.abs() * radius;
        }
        bounds.push((crate::f32ops::apply_chain(&row.center, &transforms), radius));
    }
    crate::scene::merged_bound(&bounds)
}

/// Reads every retained direct shape of a model NIF.
pub fn direct_shapes(bytes: &[u8], assets: &AssetResolver) -> Result<Vec<DirectShape>> {
    let nif = NifFile::from_bytes_raw_arrays(bytes, None)
        .map_err(|e| PrevisError::invalid(format!("NIF parse failed: {e}")))?;
    let mut shapes = Vec::new();
    for row in shape_rows(&nif)? {
        let block = &nif.blocks[row.shape_id];
        let mut transforms = Vec::with_capacity(row.parent_ids.len() + 1);
        if !row.is_root {
            transforms.push(block_transform(block)?);
        }
        for &parent in &row.parent_ids {
            transforms.push(block_transform(&nif.blocks[parent])?);
        }
        let (bound_center, bound_radius) = bounding_sphere(block)?;
        let shader = row.shader_id.map(|id| &nif.blocks[id]);
        let (non_occluder, zbuffer_write) = shader_state(shader, assets)?;
        let is_lighting = shader.is_some_and(|s| s.type_name == LIGHTING_SHADER);
        let occludes = !non_occluder && (!is_lighting || zbuffer_write);
        let (descriptor, vertex_count, triangle_count, data) = geometry_buffer(block)?;
        shapes.push(DirectShape {
            positions: decode_positions(descriptor, vertex_count, &data)?,
            indices: decode_indices(descriptor, vertex_count, triangle_count, &data),
            transforms,
            bound_center,
            bound_radius,
            occludes,
        });
    }
    Ok(shapes)
}
