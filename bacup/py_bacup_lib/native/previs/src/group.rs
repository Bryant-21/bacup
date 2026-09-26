//! Precombined group NIF writer (`Meshes\PreCombined\<plugin>\<cell>_<key>_OC.NIF`).
//!
//! Each retained source shape keeps its geometry in the plugin PSG; the group
//! NIF holds one `BSTriShape` per compatible bucket with a
//! `BSPackedCombinedSharedGeomDataExtra` listing every instance transform.

use std::sync::{Arc, OnceLock};

use indexmap::IndexMap;
use nif_core_native::model::{NifBlock, NifFile, NifValue};
use rustc_hash::FxHashSet;

use crate::assets::AssetResolver;
use crate::crc;
use crate::error::{PrevisError, Result};
use crate::f32ops::{Transform, Vec3, add, mul};
use crate::havok_mesh::{PrecombineSource, is_static_layer_one_system, parse_precombine_source};
use crate::nif::{BoundRow, block_transform, bound_rows, bounding_sphere, decode_positions, geometry_buffer, integer};
use crate::shader::{self, Alpha, AlphaSource, ExpandedShader, HIDE_ON_LOCAL_MAP, MATERIAL_ALPHA_BITS, TREE_ANIM, get_bare};
use crate::collision_scale;
use crate::swap::SwapLayers;

/// CK's third export string is an uninitialised pointer fragment; these are
/// the payloads the verified captures carry for STAT and SCOL groups.
pub const EXPORT_INFO_STAT: [u8; 10] = [0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0xF6, 0x7F, 0x00];
pub const EXPORT_INFO_SCOL: [u8; 10] = [0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0xF7, 0x7F, 0x00];

const MARKER_NAME_PARTS: [&str; 4] = ["EditorMarker", "LRTMarker", "AnimInteractionMarker", "FurnitureMarker"];
const RETAINED_SHAPE_TYPES: [&str; 3] = ["BSTriShape", "BSMeshLODTriShape", "BSSubIndexTriShape"];
const NAMED_GROUP_BLOCKS: [&str; 7] = [
    "BSFadeNode",
    "BSXFlags",
    "NiNode",
    "BSTriShape",
    "BSMeshLODTriShape",
    "BSPackedCombinedSharedGeomDataExtra",
    "NiAlphaProperty",
];
const MAX_BUCKET_VERTICES: u32 = 0xFFFE;
const BASE_FLAG_HIDE_FROM_LOCAL_MAP: u32 = 0x200;
/// A model with this BSXFlags bit keeps its collision cloned even when static.
const BSX_KEEPS_COLLISION_CLONED: u64 = 0x40;

fn combined_descriptor(source: u64) -> Option<u64> {
    match source {
        0x0000_9000_0002_0003 => Some(0x0040_9000_0004_0005),
        0x0001_B000_0043_0205 => Some(0x0041_B000_0065_0407),
        0x0003_B000_0543_0206 => Some(0x0043_B000_0765_0408),
        _ => None,
    }
}

/// Why [`build_group`] would reject a model's shapes, if it would: checked
/// while planning, so such a base stays uncombined instead of failing its
/// CELL after geometry offsets are assigned.
pub fn unbuildable_shape(model: &SourceModel) -> Option<String> {
    model.shapes.iter().find_map(|shape| {
        if shape.vertex_count == 0 || shape.vertex_count >= 0xFFFF {
            return Some("a source instance must contain 1..65534 vertices".to_string());
        }
        combined_descriptor(shape.vertex_desc)
            .is_none()
            .then(|| format!("unverified source vertex descriptor {:#x}", shape.vertex_desc))
    })
}

/// One retained source shape, with its expanded shader and geometry.
pub struct RetainedShape {
    pub block: usize,
    pub vertex_desc: u64,
    pub vertex_count: u32,
    pub triangle_count: u32,
    pub lod_sizes: Option<[u32; 3]>,
    pub geometry: Vec<u8>,
    pub transform: Transform,
    pub bound: (Vec3, f32),
    pub shader_block: usize,
    pub texture_set_block: Option<usize>,
    /// Shared with every identical expansion by [`crate::precombine::SourceCache`].
    pub shader: Arc<ExpandedShader>,
    pub alpha_block: Option<usize>,
    pub source_alpha: Option<Alpha>,
}

impl RetainedShape {
    /// Decoded from the geometry buffer, which is the cached models' only copy.
    pub fn positions(&self) -> Vec<Vec3> {
        decode_positions(self.vertex_desc, self.vertex_count as usize, &self.geometry).expect("positions are validated at load")
    }
}

/// A scene-graph node carrying a `bhkNPCollisionObject`.
pub struct CollisionNode {
    /// `None` for the model root, which CK renames `CLONE <EditorID>`.
    pub name: Option<String>,
    /// Transform below the model root (the root's own is replaced by the placement).
    pub local: Transform,
    pub body_id: u64,
}

/// Source collision: every collision node shares one `bhkPhysicsSystem`.
/// CK merges a static system into the cell's `_Physics.NIF`; any other is
/// cloned under the group's `HavokRoot` with its packfile copied unchanged.
pub struct Collision {
    pub nodes: Vec<CollisionNode>,
    pub physics_block: usize,
    pub flags: u64,
    pub packfile: Vec<u8>,
    unscalable: OnceLock<Option<String>>,
    /// The system as `GenerateCombinedShape` consumes it, when it is merged:
    /// the bodies at each collision node (by index), in body order. A static
    /// system outside the reproduced shapes stays cloned.
    pub merged: Option<Vec<(usize, Arc<PrecombineSource>)>>,
}

impl Collision {
    /// Why the cloned packfile cannot be pre-scaled for a scaled placement, if so.
    pub fn unscalable(&self) -> Option<&str> {
        self.unscalable
            .get_or_init(|| collision_scale::scale_packfile(&self.packfile, 2.0).err().map(|e| e.to_string()))
            .as_deref()
    }
}

/// A source model prepared for combining: its NIF and retained shapes.
pub struct SourceModel {
    pub path: String,
    /// Only the source blocks a group copies (shaders, alpha, physics); the
    /// parsed NIF is dropped after load so cached models stay small.
    blocks: Vec<(usize, NifBlock)>,
    pub bound_rows: Vec<BoundRow>,
    pub shapes: Vec<RetainedShape>,
    pub collision: Option<Collision>,
}

fn block_name(block: &NifBlock) -> &str {
    match block.get_field("Name") {
        Some(NifValue::String(s)) => s,
        _ => "",
    }
}

fn reference(value: Option<&NifValue>) -> i32 {
    match value {
        Some(NifValue::Ref(r)) => *r,
        Some(NifValue::Int(r)) => *r as i32,
        _ => -1,
    }
}

fn alpha_fields(block: &NifBlock) -> Result<Alpha> {
    let flags = integer(block, "Flags")?;
    let threshold = integer(block, "Threshold")?;
    Ok(Alpha {
        flags: flags as u16,
        threshold: threshold as u8,
    })
}

fn binary_data(block: &NifBlock) -> Option<Vec<u8>> {
    let Some(NifValue::Struct(fields)) = block.get_field("Binary Data") else {
        return None;
    };
    match fields.get("Data")? {
        NifValue::Array(items) => items.iter().map(|v| u8::try_from(v.as_i64()).ok()).collect(),
        NifValue::Bytes(bytes) => Some(bytes.clone()),
        _ => None,
    }
}

/// Class names listed in a Havok packfile's `__classnames__` section.
fn packfile_classes(blob: &[u8]) -> Option<Vec<String>> {
    let u32_at = |o: usize| blob.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let sections = u32_at(0x14)?;
    let header = (0..sections).map(|i| 0x40 + i * 0x40).find(|&h| blob.get(h..h + 15) == Some(b"__classnames__\0"))?;
    let start = u32_at(header + 0x14)?;
    let end = start + u32_at(header + 0x18)?;
    let mut classes = Vec::new();
    let mut cursor = start;
    while cursor + 5 < end && blob[cursor + 4] == 0x09 {
        let name_start = cursor + 5;
        let length = blob.get(name_start..end)?.iter().position(|&b| b == 0)?;
        classes.push(String::from_utf8_lossy(&blob[name_start..name_start + length]).into_owned());
        cursor = name_start + length + 1;
    }
    Some(classes)
}

fn is_constraint_class(name: &str) -> bool {
    name.ends_with("ConstraintData")
        || name.ends_with("ConstraintMotor")
        || matches!(name, "hkpBallSocketChainData" | "hkpPoweredChainData" | "hkpStiffSpringChainData")
}

/// CK 1.11.137 sets `SET_LOCAL` only for unconstrained capsule systems.
fn clone_collision_flags(classes: &[String]) -> u64 {
    let capsule = classes.iter().any(|c| c == "hknpCapsuleShape");
    if capsule && !classes.iter().any(|c| is_constraint_class(c)) { 0x88 } else { 0x80 }
}

fn children_of(block: &NifBlock) -> Vec<usize> {
    match block.get_field("Children") {
        Some(NifValue::Array(items)) => items
            .iter()
            .map(|v| reference(Some(v)))
            .filter(|&r| r >= 0)
            .map(|r| r as usize)
            .collect(),
        _ => Vec::new(),
    }
}

fn collision_of(nif: &NifFile) -> Result<Option<Collision>> {
    let mut objects = 0;
    for block in &nif.blocks {
        match block.type_name.as_str() {
            "bhkNPCollisionObject" => objects += 1,
            "bhkPhysicsSystem" => {}
            other if other.starts_with("bhk") || other.starts_with("hk") => {
                return Err(PrevisError::unsupported(format!("{other} collision is not yet supported")));
            }
            _ => {}
        }
    }
    if objects == 0 {
        return Ok(None);
    }
    let root = nif.header.footer_roots.first().copied().unwrap_or(0).max(0) as usize;
    let mut nodes = Vec::new();
    let mut physics = None;
    let mut visited = FxHashSet::default();
    let mut stack = vec![(root, Transform::IDENTITY)];
    while let Some((id, parent)) = stack.pop() {
        if id >= nif.blocks.len() || !visited.insert(id) {
            continue;
        }
        let block = &nif.blocks[id];
        let local = if id == root { Transform::IDENTITY } else { parent.compose(&block_transform(block)?) };
        let object = reference(block.get_field("Collision Object"));
        if object >= 0 {
            if local.scale != 1.0 {
                return Err(PrevisError::unsupported("collision below a scaled node is not yet supported"));
            }
            let collision = &nif.blocks[object as usize];
            let data = reference(collision.get_field("Data"));
            if collision.type_name != "bhkNPCollisionObject"
                || reference(collision.get_field("Target")) != id as i32
                || data < 0
                || nif.blocks[data as usize].type_name != "bhkPhysicsSystem"
            {
                return Err(PrevisError::unsupported("collision object layout is not yet supported"));
            }
            if *physics.get_or_insert(data as usize) != data as usize {
                return Err(PrevisError::unsupported("collision across several physics systems is not yet supported"));
            }
            nodes.push(CollisionNode {
                name: (id != root).then(|| block_name(block).to_string()),
                local,
                body_id: integer(collision, "Body ID")? as u64,
            });
        }
        for child in children_of(block).into_iter().rev() {
            stack.push((child, local));
        }
    }
    if nodes.len() != objects {
        return Err(PrevisError::unsupported("collision object outside the scene graph"));
    }
    let physics_block = physics.unwrap();
    let packfile = binary_data(&nif.blocks[physics_block])
        .ok_or_else(|| PrevisError::invalid("bhkPhysicsSystem has no packfile"))?;
    let classes =
        packfile_classes(&packfile).ok_or_else(|| PrevisError::invalid("bhkPhysicsSystem packfile is unreadable"))?;
    let bsx_flags = nif.blocks.iter().find(|b| b.type_name == "BSXFlags").map_or(Ok(0), |b| integer(b, "Integer Data"))?;
    let merged = (bsx_flags & BSX_KEEPS_COLLISION_CLONED == 0 && is_static_layer_one_system(&packfile).unwrap_or(false))
        .then(|| parse_precombine_source(&packfile).ok())
        .flatten()
        .and_then(|source| bodies_by_node(source, &nodes));
    Ok(Some(Collision {
        nodes,
        physics_block,
        flags: clone_collision_flags(&classes),
        packfile,
        unscalable: OnceLock::new(),
        merged,
    }))
}

/// A single collision node carries every body; several nodes each carry the
/// body their collision object names, and every body needs its own node.
fn bodies_by_node(source: PrecombineSource, nodes: &[CollisionNode]) -> Option<Vec<(usize, Arc<PrecombineSource>)>> {
    if nodes.len() == 1 {
        return Some(vec![(0, Arc::new(source))]);
    }
    if nodes.len() != source.bodies.len() {
        return None;
    }
    let fits_combined_shape = source.fits_combined_shape;
    source
        .bodies
        .into_iter()
        .enumerate()
        .map(|(index, body)| {
            let node = nodes.iter().position(|n| n.body_id == index as u64)?;
            Some((node, Arc::new(PrecombineSource { bodies: vec![body], fits_combined_shape })))
        })
        .collect()
}

/// Rejects scene-graph content whose combine behaviour is not verified.
fn reject_unsupported_blocks(nif: &NifFile) -> Result<()> {
    for block in &nif.blocks {
        let name = block.type_name.as_str();
        if name.contains("Particle")
            || name.contains("Switch")
            || name.contains("Billboard")
            || name.contains("Controller")
            || name.contains("Interpolator")
        {
            return Err(PrevisError::unsupported(format!("model contains {name}")));
        }
    }
    Ok(())
}

/// Retained shapes in scene-graph preorder, skipping marker subtrees, and
/// whether a marker subtree held any shape.
fn retained_shape_blocks(nif: &NifFile) -> (Vec<usize>, bool) {
    let schema = &*nif_core_native::schema::SCHEMA;
    let mut roots: Vec<usize> = nif
        .header
        .footer_roots
        .iter()
        .filter(|&&r| r >= 0 && (r as usize) < nif.blocks.len())
        .map(|&r| r as usize)
        .collect();
    if roots.is_empty() && !nif.blocks.is_empty() {
        roots.push(0);
    }
    // Build the whole hierarchy first so a shared block belongs to its first parent.
    let mut visited = FxHashSet::default();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); nif.blocks.len()];
    let mut stack_roots = Vec::new();
    fn build(
        nif: &NifFile,
        schema: &nif_core_native::schema::NifSchema,
        id: usize,
        visited: &mut FxHashSet<usize>,
        children: &mut Vec<Vec<usize>>,
    ) -> bool {
        if !visited.insert(id) {
            return false;
        }
        for r in nif.blocks[id].get_refs(schema) {
            if r >= 0 && (r as usize) < nif.blocks.len() && build(nif, schema, r as usize, visited, children) {
                children[id].push(r as usize);
            }
        }
        true
    }
    for root in roots {
        if build(nif, schema, root, &mut visited, &mut children) {
            stack_roots.push(root);
        }
    }
    let mut shapes = Vec::new();
    let mut culled = false;
    fn visit(nif: &NifFile, id: usize, children: &[Vec<usize>], in_marker: bool, shapes: &mut Vec<usize>, culled: &mut bool) {
        let block = &nif.blocks[id];
        let in_marker = in_marker || MARKER_NAME_PARTS.iter().any(|part| block_name(block).contains(part));
        if RETAINED_SHAPE_TYPES.contains(&block.type_name.as_str()) {
            if in_marker {
                *culled = true;
            } else {
                shapes.push(id);
            }
        }
        for &child in &children[id] {
            visit(nif, child, children, in_marker, shapes, culled);
        }
    }
    for root in stack_roots {
        visit(nif, root, &children, false, &mut shapes, &mut culled);
    }
    (shapes, culled)
}

impl SourceModel {
    pub fn load(
        path: &str,
        bytes: &[u8],
        assets: &AssetResolver,
        swap: SwapLayers,
        color_index: Option<f32>,
    ) -> Result<SourceModel> {
        let nif = NifFile::from_bytes_raw_arrays(bytes, None)
            .map_err(|e| PrevisError::invalid(format!("{path}: NIF parse failed: {e}")))?;
        reject_unsupported_blocks(&nif)?;
        // CK combines a model whose only geometry is marker-culled (the SCOL
        // pivot dummy) and emits nothing for it.
        let (shape_ids, marker_geometry) = retained_shape_blocks(&nif);
        if shape_ids.is_empty() && !marker_geometry {
            return Err(PrevisError::unsupported(format!("{path}: no retained supported shapes")));
        }
        let mut shapes = Vec::with_capacity(shape_ids.len());
        for id in shape_ids {
            let block = &nif.blocks[id];
            let shader_block = reference(block.get_field("Shader Property"));
            if shader_block < 0 {
                return Err(PrevisError::unsupported(format!("{path}: retained shape {id} has no shader")));
            }
            let shader_block = shader_block as usize;
            let texture_set = reference(get_bare(&nif.blocks[shader_block].fields, "Texture Set"));
            let texture_set_block = (texture_set >= 0).then(|| &nif.blocks[texture_set as usize]);
            let shader = shader::expand_shader(&nif.blocks[shader_block], texture_set_block, assets, swap, color_index)
                .map_err(|e| e.with_context(path))?;
            // Retail CDX files never index slots 8/9, so their kind is unknown.
            if shader.textures.iter().flatten().skip(8).any(|t| !t.is_empty()) {
                return Err(PrevisError::unsupported(format!(
                    "{path}: texture slot 8/9 (displacement) has no verified CDX kind"
                )));
            }
            let alpha_id = reference(block.get_field("Alpha Property"));
            let source_alpha = if alpha_id >= 0 {
                let alpha_block = &nif.blocks[alpha_id as usize];
                if alpha_block.type_name != "NiAlphaProperty" {
                    return Err(PrevisError::unsupported(format!("{path}: unsupported alpha property")));
                }
                Some(alpha_fields(alpha_block)?)
            } else {
                None
            };
            let (vertex_desc, vertex_count, triangle_count, geometry) = geometry_buffer(block)?;
            decode_positions(vertex_desc, vertex_count, &geometry)?;
            // CK combines a BSSubIndexTriShape's geometry like a plain BSTriShape.
            let lod_sizes = if block.type_name == "BSMeshLODTriShape" {
                Some([
                    integer(block, "LOD0 Size")? as u32,
                    integer(block, "LOD1 Size")? as u32,
                    integer(block, "LOD2 Size")? as u32,
                ])
            } else {
                None
            };
            shapes.push(RetainedShape {
                block: id,
                vertex_desc,
                vertex_count: vertex_count as u32,
                triangle_count: triangle_count as u32,
                lod_sizes,
                geometry,
                transform: block_transform(block)?,
                bound: bounding_sphere(block)?,
                shader_block,
                texture_set_block: (texture_set >= 0).then_some(texture_set as usize),
                shader: Arc::new(shader),
                alpha_block: (alpha_id >= 0).then_some(alpha_id as usize),
                source_alpha,
            });
        }
        let collision = collision_of(&nif).map_err(|e| e.with_context(path))?;
        let shader_blocks: FxHashSet<usize> = shapes.iter().map(|s| s.shader_block).collect();
        let mut kept: Vec<usize> = shapes.iter().flat_map(|s| [Some(s.shader_block), s.alpha_block]).flatten().collect();
        kept.extend(collision.as_ref().map(|c| c.physics_block));
        kept.sort_unstable();
        kept.dedup();
        let blocks = kept
            .into_iter()
            .map(|id| {
                let source = &nif.blocks[id];
                let mut block = NifBlock::new(source.block_id, source.type_name.clone());
                // A group writes a shader's expanded fields, and the packfile
                // is kept once as bytes in `Collision::packfile`.
                if !shader_blocks.contains(&id) {
                    block.fields = source.fields.clone();
                    block.fields.shift_remove("Binary Data");
                }
                (id, block)
            })
            .collect();
        Ok(SourceModel {
            path: path.to_string(),
            blocks,
            bound_rows: bound_rows(&nif).map_err(|e| e.with_context(path))?,
            shapes,
            collision,
        })
    }

    fn block(&self, id: usize) -> &NifBlock {
        &self.blocks[self.blocks.binary_search_by_key(&id, |(k, _)| *k).expect("retained source block")].1
    }
}

/// One source model placed in a group: its instance transforms (already
/// composed with any SCOL component placement) in CK order.
pub struct GroupSource<'a> {
    pub model: &'a SourceModel,
    pub instances: Vec<Transform>,
    pub base_record_flags: u32,
    pub leaf: Option<(f32, f32)>,
    /// Names the root collision clone (`CLONE <EditorID>`).
    pub editor_id: &'a str,
    /// PSG data offset of each retained shape, in `model.shapes` order.
    pub data_offsets: Vec<u32>,
    /// The collision goes to the CELL's `_Physics.NIF` rather than a clone.
    pub merges_collision: bool,
}

pub struct GroupRequest<'a> {
    pub root_name: String,
    pub sources: Vec<GroupSource<'a>>,
    pub psg_file_name: String,
    pub export_info: [u8; 10],
}

#[derive(Clone)]
struct Row<'a> {
    source_index: usize,
    shape: &'a RetainedShape,
    shader: ExpandedShader,
    alpha: Option<Alpha>,
    instances: Vec<Transform>,
    data_offset: u32,
    leaf: Option<(f32, f32)>,
}

impl Row<'_> {
    /// CK 1.11.240 output shares a bucket between BSTriShape and
    /// BSMeshLODTriShape rows, so the shape type is not part of the key.
    fn key(&self) -> (u64, u32, u64, Option<[u32; 6]>, Option<(u32, u32)>) {
        let flags = (self.shader.flags1 as u64 | ((self.shader.flags2 as u64) << 32)) & !0x0000_0200_0000_0000;
        (
            self.shape.vertex_desc,
            self.shader.material_crc32,
            flags,
            self.alpha.map(|a| a.compatibility_key()),
            self.leaf.map(|(a, f)| (a.to_bits(), f.to_bits())),
        )
    }
}

/// CK 1.11.240 output: a BGSM decides whether the shape keeps an alpha
/// property at all, and its enable bits and threshold; the source property's
/// remaining bits survive.
fn combined_alpha(material: AlphaSource, source: Option<Alpha>) -> Option<Alpha> {
    match (material, source) {
        (AlphaSource::Source, source) => source,
        (AlphaSource::Material(None), _) => None,
        (AlphaSource::Material(Some(material)), Some(source)) => Some(Alpha {
            flags: source.flags & !MATERIAL_ALPHA_BITS | material.flags & MATERIAL_ALPHA_BITS,
            threshold: material.threshold,
        }),
        (AlphaSource::Material(material), None) => material,
    }
}

/// Splits rows into shape buckets that each stay below the u16 vertex limit.
fn buckets<'a>(rows: &[Row<'a>]) -> Result<Vec<Vec<Row<'a>>>> {
    let mut buckets: Vec<Vec<Row<'a>>> = Vec::new();
    for row in rows {
        let key = row.key();
        let source_vertices = row.shape.vertex_count;
        if source_vertices == 0 || source_vertices >= 0xFFFF {
            return Err(PrevisError::unsupported("a source instance must contain 1..65534 vertices"));
        }
        let mut first = 0;
        while first < row.instances.len() {
            let mut target = None;
            let mut available = 0usize;
            for (index, bucket) in buckets.iter().enumerate().rev() {
                if bucket[0].key() != key {
                    continue;
                }
                let current: u32 = bucket.iter().map(|r| r.shape.vertex_count * r.instances.len() as u32).sum();
                available = ((MAX_BUCKET_VERTICES - current) / source_vertices) as usize;
                if available > 0 {
                    target = Some(index);
                    break;
                }
            }
            let target = match target {
                Some(index) => index,
                None => {
                    buckets.push(Vec::new());
                    available = (MAX_BUCKET_VERTICES / source_vertices) as usize;
                    buckets.len() - 1
                }
            };
            let count = available.min(row.instances.len() - first);
            let mut split = row.clone();
            split.instances = row.instances[first..first + count].to_vec();
            buckets[target].push(split);
            first += count;
        }
    }
    Ok(buckets)
}

fn f(value: f32) -> NifValue {
    NifValue::Float(value as f64)
}

fn u(value: u64) -> NifValue {
    NifValue::UInt(value)
}

fn vector(v: Vec3) -> NifValue {
    NifValue::Struct(IndexMap::from([
        ("x".to_string(), f(v[0])),
        ("y".to_string(), f(v[1])),
        ("z".to_string(), f(v[2])),
    ]))
}

fn rotation(m: &[[f32; 3]; 3]) -> NifValue {
    NifValue::Struct(IndexMap::from([
        ("m11".to_string(), f(m[0][0])),
        ("m21".to_string(), f(m[0][1])),
        ("m31".to_string(), f(m[0][2])),
        ("m12".to_string(), f(m[1][0])),
        ("m22".to_string(), f(m[1][1])),
        ("m32".to_string(), f(m[1][2])),
        ("m13".to_string(), f(m[2][0])),
        ("m23".to_string(), f(m[2][1])),
        ("m33".to_string(), f(m[2][2])),
    ]))
}

fn transform_value(t: &Transform) -> NifValue {
    NifValue::Struct(IndexMap::from([
        ("Rotation".to_string(), rotation(&t.rotation)),
        ("Translation".to_string(), vector(t.translation)),
        ("Scale".to_string(), f(t.scale)),
    ]))
}

fn sphere(center: Vec3, radius: f32) -> NifValue {
    NifValue::Struct(IndexMap::from([
        ("Center".to_string(), vector(center)),
        ("Radius".to_string(), f(radius)),
    ]))
}

fn map(entries: Vec<(&str, NifValue)>) -> IndexMap<String, NifValue> {
    entries.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

struct BucketSpec {
    shape_type: String,
    shape: IndexMap<String, NifValue>,
    packed: IndexMap<String, NifValue>,
    vertex_desc: u64,
    triangles: u32,
    vertices: u32,
}

fn bucket_spec(rows: &[Row], filename_hash: u32) -> Result<BucketSpec> {
    let source_descriptor = rows[0].shape.vertex_desc;
    let descriptor = combined_descriptor(source_descriptor).ok_or_else(|| {
        PrevisError::unsupported(format!("unverified source vertex descriptor {source_descriptor:#x}"))
    })?;
    let row_transforms: Vec<Vec<Transform>> = rows
        .iter()
        .map(|row| row.instances.iter().map(|i| i.compose(&row.shape.transform)).collect())
        .collect();
    let row_positions: Vec<Vec<Vec3>> = rows.iter().map(|row| row.shape.positions()).collect();
    // Placed twice (bounds, then radius) rather than held: a bucket can place
    // thousands of instances.
    let world = || {
        row_transforms.iter().zip(&row_positions).flat_map(|(transforms, positions)| {
            transforms.iter().flat_map(move |transform| positions.iter().map(move |position| transform.apply(position)))
        })
    };
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for point in world() {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(point[axis]);
            maximum[axis] = maximum[axis].max(point[axis]);
        }
    }
    let center = [0, 1, 2].map(|axis| mul(add(minimum[axis], maximum[axis]), 0.5));
    let radius_squared = world()
        .map(|p| {
            let d = [0, 1, 2].map(|axis| add(p[axis], -center[axis]));
            add(add(mul(d[1], d[1]), mul(d[0], d[0])), mul(d[2], d[2]))
        })
        .fold(f32::NEG_INFINITY, f32::max);
    let radius = (radius_squared as f64).sqrt() as f32;

    // A BSTriShape row in a LOD bucket counts wholly towards LOD0.
    let lod = rows.iter().any(|r| r.shape.lod_sizes.is_some());
    let triangles: u32 = rows.iter().map(|r| r.shape.triangle_count * r.instances.len() as u32).sum();
    let vertices: u32 = rows.iter().map(|r| r.shape.vertex_count * r.instances.len() as u32).sum();
    let mut shape = map(vec![
        ("Name", NifValue::String(String::new())),
        ("Flags", u(526 | if lod { 0x1000 } else { 0 })),
        ("Translation", vector(center)),
        ("Rotation", rotation(&Transform::IDENTITY.rotation)),
        ("Scale", f(1.0)),
        ("Bounding Sphere", sphere([0.0; 3], radius)),
        ("Vertex Desc", u(descriptor)),
        ("Num Triangles", u(triangles as u64)),
        ("Num Vertices", u(vertices as u64)),
        ("Data Size", u(0)),
    ]);
    if lod {
        for level in 0..3 {
            let total: u32 = rows
                .iter()
                .map(|r| r.shape.lod_sizes.unwrap_or([r.shape.triangle_count, 0, 0])[level] * r.instances.len() as u32)
                .sum();
            shape.insert(format!("LOD{level} Size"), u(total as u64));
        }
    }

    let mut objects = Vec::new();
    let mut object_data = Vec::new();
    for (row, transforms) in rows.iter().zip(&row_transforms) {
        let (bound_center, bound_radius) = row.shape.bound;
        let combined: Vec<NifValue> = transforms
            .iter()
            .map(|t| {
                NifValue::Struct(map(vec![
                    ("Grayscale to Palette Scale", f(row.shader.grayscale)),
                    ("Transform", transform_value(t)),
                    ("Bounding Sphere", sphere(t.apply(&bound_center), mul(bound_radius, t.scale.abs()))),
                ]))
            })
            .collect();
        objects.push(NifValue::Struct(map(vec![
            ("Filename Hash", u(filename_hash as u64)),
            ("Data Offset", u(row.data_offset as u64)),
        ])));
        let [lod0, lod1, lod2] = row.shape.lod_sizes.unwrap_or([row.shape.triangle_count, 0, 0]);
        let (offset1, offset2) = match row.shape.lod_sizes {
            Some(_) => (lod0 * 3, (lod0 + lod1) * 3),
            None => (0, 0),
        };
        object_data.push(NifValue::Struct(map(vec![
            ("Num Verts", u(row.shape.vertex_count as u64)),
            ("LOD Levels", u(3)),
            ("Tri Count LOD0", u(lod0 as u64)),
            ("Tri Offset LOD0", u(0)),
            ("Tri Count LOD1", u(lod1 as u64)),
            ("Tri Offset LOD1", u(offset1 as u64)),
            ("Tri Count LOD2", u(lod2 as u64)),
            ("Tri Offset LOD2", u(offset2 as u64)),
            ("Num Combined", u(combined.len() as u64)),
            ("Combined", NifValue::Array(combined)),
            ("Vertex Desc", u(source_descriptor)),
        ])));
    }
    let (amplitude, frequency) = if rows[0].shader.flags2 & TREE_ANIM != 0 {
        rows[0]
            .leaf
            .ok_or_else(|| PrevisError::unsupported("tree-animation rows require leaf amplitude and frequency"))?
    } else {
        (0.0, 0.0)
    };
    let packed = map(vec![
        ("Name", NifValue::String("PCD".into())),
        ("Vertex Desc", u(descriptor)),
        ("Num Vertices", u(vertices as u64)),
        ("Num Triangles", u(triangles as u64)),
        ("Unknown Flags 1", u(amplitude.to_bits() as u64)),
        ("Unknown Flags 2", u(frequency.to_bits() as u64)),
        ("Num Data", u(rows.len() as u64)),
        ("Object", NifValue::Array(objects)),
        ("Object Data", NifValue::Array(object_data)),
    ]);
    Ok(BucketSpec {
        shape_type: if lod { "BSMeshLODTriShape" } else { "BSTriShape" }.to_string(),
        shape,
        packed,
        vertex_desc: descriptor,
        triangles,
        vertices,
    })
}

fn attach_child(nif: &mut NifFile, parent: usize, child: usize) {
    let block = &mut nif.blocks[parent];
    if let Some(NifValue::Array(children)) = block.fields.get_mut("Children") {
        children.push(NifValue::Ref(child as i32));
    }
    let count = block.fields.get("Num Children").map(NifValue::as_i64).unwrap_or(0);
    block.set_field("Num Children", NifValue::UInt(count as u64 + 1));
}

fn set_fields(nif: &mut NifFile, block: usize, fields: IndexMap<String, NifValue>) {
    for (name, value) in fields {
        shader::set_bare(&mut nif.blocks[block].fields, &name, value);
    }
}

/// Adds a schema-defaulted block, then applies fields by suffix-less name.
fn add_node(nif: &mut NifFile, type_name: &str, fields: IndexMap<String, NifValue>) -> usize {
    let id = nif.add_block(type_name, None);
    set_fields(nif, id, fields);
    id
}

/// Copies a source block into the target with fresh (unserialised) state.
fn copy_block(nif: &mut NifFile, source: &NifBlock) -> usize {
    let id = nif.add_block(source.type_name.clone(), None);
    nif.blocks[id].fields = source.fields.clone();
    id
}

pub fn build_group(request: &GroupRequest) -> Result<Vec<u8>> {
    if request.sources.is_empty() {
        return Err(PrevisError::invalid("a group requires at least one source"));
    }
    let mut rows = Vec::new();
    for (source_index, source) in request.sources.iter().enumerate() {
        if source.instances.is_empty() || source.data_offsets.len() != source.model.shapes.len() {
            return Err(PrevisError::invalid("group source has no instances or mismatched offsets"));
        }
        for (shape, &data_offset) in source.model.shapes.iter().zip(&source.data_offsets) {
            let mut expanded = ExpandedShader::clone(&shape.shader);
            if source.base_record_flags & BASE_FLAG_HIDE_FROM_LOCAL_MAP != 0 {
                expanded.flags2 |= HIDE_ON_LOCAL_MAP;
                shader::set_bare(&mut expanded.fields, "Shader Flags 2", NifValue::UInt(expanded.flags2 as u64));
            }
            let alpha = combined_alpha(expanded.alpha, shape.source_alpha);
            rows.push(Row {
                source_index,
                shape,
                shader: expanded,
                alpha,
                instances: source.instances.clone(),
                data_offset,
                leaf: source.leaf,
            });
        }
    }
    let buckets = buckets(&rows)?;
    let has_lod = buckets.iter().flatten().any(|r| r.shape.lod_sizes.is_some());
    let filename_hash = crc::geometry_filename_hash(&request.psg_file_name);

    let mut nif = NifFile::new("fo4");
    set_fields(
        &mut nif,
        0,
        map(vec![
            ("Name", NifValue::String(request.root_name.clone())),
            ("Flags", u(16398 | if has_lod { 0x1000 } else { 0 })),
            ("Scale", f(1.0)),
        ]),
    );
    // CK keeps BSX and HavokRoot only for collision it clones.
    let has_collision = request.sources.iter().any(|s| s.model.collision.is_some() && !s.merges_collision);
    if has_collision {
        let flags = add_node(
            &mut nif,
            "BSXFlags",
            map(vec![("Name", NifValue::String("BSX".into())), ("Integer Data", u(2))]),
        );
        set_fields(
            &mut nif,
            0,
            map(vec![
                ("Num Extra Data List", u(1)),
                ("Extra Data List", NifValue::Array(vec![NifValue::Ref(flags as i32)])),
            ]),
        );
    }
    let render = add_node(
        &mut nif,
        "NiNode",
        map(vec![
            ("Name", NifValue::String(String::new())),
            ("Flags", u(14 | if has_lod { 0x1000 } else { 0 })),
            ("Scale", f(1.0)),
        ]),
    );
    attach_child(&mut nif, 0, render);

    let mut lod_specs = Vec::new();
    let mut texture_sets: Vec<((usize, usize), usize)> = Vec::new();
    for bucket in &buckets {
        let first = &bucket[0];
        let spec = bucket_spec(bucket, filename_hash)?;
        let model = request.sources[first.source_index].model;
        let target_shape = add_node(&mut nif, &spec.shape_type, spec.shape);
        attach_child(&mut nif, render, target_shape);
        let target_packed = add_node(&mut nif, "BSPackedCombinedSharedGeomDataExtra", spec.packed);
        if spec.shape_type == "BSMeshLODTriShape" {
            lod_specs.push((target_shape, spec.vertex_desc, spec.triangles, spec.vertices));
        }

        // CK clones the source shader (a lighting texture set is replaced below).
        let target_shader = nif.add_block(model.block(first.shape.shader_block).type_name.clone(), None);
        let mut shader_fields = first.shader.fields.clone();
        if let Some(textures) = &first.shader.textures {
            let texture_key = (first.source_index, first.shape.shader_block);
            let target_texture_set = match texture_sets.iter().find(|(k, _)| *k == texture_key) {
                Some(&(_, id)) => id,
                None => {
                    let slots = textures.iter().map(|t| NifValue::String(t.clone())).collect();
                    let id = add_node(
                        &mut nif,
                        "BSShaderTextureSet",
                        map(vec![("Num Textures", u(textures.len() as u64)), ("Textures", NifValue::Array(slots))]),
                    );
                    texture_sets.push((texture_key, id));
                    id
                }
            };
            shader::set_bare(&mut shader_fields, "Texture Set", NifValue::Ref(target_texture_set as i32));
        }

        let target_alpha = match (first.shape.alpha_block, first.alpha) {
            (Some(source_alpha), Some(alpha)) => {
                let id = copy_block(&mut nif, model.block(source_alpha));
                set_fields(
                    &mut nif,
                    id,
                    map(vec![("Flags", u(alpha.flags as u64)), ("Threshold", u(alpha.threshold as u64))]),
                );
                id as i32
            }
            (None, Some(alpha)) => add_node(
                &mut nif,
                "NiAlphaProperty",
                map(vec![("Flags", u(alpha.flags as u64)), ("Threshold", u(alpha.threshold as u64))]),
            ) as i32,
            _ => -1,
        };
        set_fields(
            &mut nif,
            target_shape,
            map(vec![
                ("Num Extra Data List", u(1)),
                ("Extra Data List", NifValue::Array(vec![NifValue::Ref(target_packed as i32)])),
                ("Shader Property", NifValue::Ref(target_shader as i32)),
                ("Alpha Property", NifValue::Ref(target_alpha)),
            ]),
        );
        nif.blocks[target_shader].fields = shader_fields;
    }
    if has_collision {
        attach_collision_clones(&mut nif, &request.sources)?;
    }
    nif.rebuild_header();
    let bytes = nif
        .to_bytes()
        .map_err(|e| PrevisError::invalid(format!("group NIF serialization failed: {e}")))?;
    let bytes = strip_combined_lod_geometry(bytes, &lod_specs)?;
    canonicalize(bytes, &request.root_name, &request.export_info)
}

/// Retail layout: one `HavokRoot` holding, per placed instance, a flat node
/// for every source collision node at its world transform, all sharing one
/// copy of the source physics system.
fn attach_collision_clones(nif: &mut NifFile, sources: &[GroupSource]) -> Result<()> {
    let havok_root = add_node(
        nif,
        "NiNode",
        map(vec![
            ("Name", NifValue::String("HavokRoot".into())),
            ("Flags", u(14)),
            ("Scale", f(1.0)),
        ]),
    );
    attach_child(nif, 0, havok_root);
    for source in sources {
        let Some(collision) = source.model.collision.as_ref().filter(|_| !source.merges_collision) else { continue };
        let mut scaled: Vec<(u32, NifValue)> = Vec::new();
        for instance in &source.instances {
            let binary = if instance.scale == 1.0 {
                None
            } else {
                let key = instance.scale.to_bits();
                if let Some((_, value)) = scaled.iter().find(|(k, _)| *k == key) {
                    Some(value.clone())
                } else {
                    let blob = collision_scale::scale_packfile(&collision.packfile, instance.scale)
                        .map_err(|e| e.with_context(&source.model.path))?;
                    let value = binary_data_value(&blob);
                    scaled.push((key, value.clone()));
                    Some(value)
                }
            };
            let mut physics = None;
            for node in &collision.nodes {
                let name = node.name.clone().unwrap_or_else(|| format!("CLONE {}", source.editor_id));
                let target = add_node(
                    nif,
                    "NiNode",
                    map(vec![("Name", NifValue::String(name)), ("Flags", u(14))]),
                );
                let world = instance.compose(&node.local);
                set_fields(
                    nif,
                    target,
                    map(vec![
                        ("Rotation", rotation(&world.rotation)),
                        ("Translation", vector(world.translation)),
                        ("Scale", f(world.scale)),
                    ]),
                );
                attach_child(nif, havok_root, target);
                let object = add_node(
                    nif,
                    "bhkNPCollisionObject",
                    map(vec![
                        ("Target", NifValue::Ref(target as i32)),
                        ("Flags", u(collision.flags)),
                        ("Body ID", u(node.body_id)),
                    ]),
                );
                let data = *physics.get_or_insert_with(|| {
                    let id = copy_block(nif, source.model.block(collision.physics_block));
                    let value = binary.clone().unwrap_or_else(|| binary_data_value(&collision.packfile));
                    nif.blocks[id].set_field("Binary Data", value);
                    id
                });
                set_fields(nif, object, map(vec![("Data", NifValue::Ref(data as i32))]));
                set_fields(nif, target, map(vec![("Collision Object", NifValue::Ref(object as i32))]));
            }
        }
    }
    Ok(())
}

fn binary_data_value(blob: &[u8]) -> NifValue {
    NifValue::Struct(IndexMap::from([
        ("Data Size".to_string(), NifValue::UInt(blob.len() as u64)),
        ("Data".to_string(), NifValue::Bytes(blob.to_vec())),
    ]))
}

/// Offsets of the serialized header regions and blocks of an FO4 NIF.
struct Layout {
    export_info: (usize, usize),
    block_sizes_offset: usize,
    strings: (usize, usize),
    string_values: Vec<Vec<u8>>,
    blocks: Vec<(String, usize, usize)>,
}

fn layout(data: &[u8]) -> Result<Layout> {
    let err = || PrevisError::invalid("generated NIF header is truncated");
    let u32_at = |o: usize| -> Result<u32> {
        Ok(u32::from_le_bytes(data.get(o..o + 4).ok_or_else(err)?.try_into().unwrap()))
    };
    let mut cursor = data.iter().position(|&b| b == b'\n').ok_or_else(err)? + 1;
    cursor += 4 + 1 + 4;
    let block_count = u32_at(cursor)? as usize;
    cursor += 8;
    let export_start = cursor;
    for _ in 0..4 {
        cursor += 1 + *data.get(cursor).ok_or_else(err)? as usize;
    }
    let export_end = cursor;
    let type_count = u16::from_le_bytes(data.get(cursor..cursor + 2).ok_or_else(err)?.try_into().unwrap()) as usize;
    cursor += 2;
    let mut type_names = Vec::with_capacity(type_count);
    for _ in 0..type_count {
        let length = u32_at(cursor)? as usize;
        type_names.push(String::from_utf8_lossy(data.get(cursor + 4..cursor + 4 + length).ok_or_else(err)?).into_owned());
        cursor += 4 + length;
    }
    let type_indices: Vec<usize> = (0..block_count)
        .map(|i| u16::from_le_bytes([data[cursor + i * 2], data[cursor + i * 2 + 1]]) as usize)
        .collect();
    cursor += block_count * 2;
    let block_sizes_offset = cursor;
    let sizes: Vec<usize> = (0..block_count).map(|i| u32_at(cursor + i * 4).map(|v| v as usize)).collect::<Result<_>>()?;
    cursor += block_count * 4;
    let string_count = u32_at(cursor)? as usize;
    cursor += 8;
    let strings_start = cursor;
    let mut string_values = Vec::with_capacity(string_count);
    for _ in 0..string_count {
        let length = u32_at(cursor)? as usize;
        string_values.push(data.get(cursor + 4..cursor + 4 + length).ok_or_else(err)?.to_vec());
        cursor += 4 + length;
    }
    let strings_end = cursor;
    let group_count = u32_at(cursor)? as usize;
    cursor += 4 + group_count * 4;
    let mut blocks = Vec::with_capacity(block_count);
    for (index, size) in type_indices.iter().zip(sizes) {
        let name = type_names.get(*index).ok_or_else(err)?.clone();
        blocks.push((name, cursor, size));
        cursor += size;
    }
    Ok(Layout {
        export_info: (export_start, export_end),
        block_sizes_offset,
        strings: (strings_start, strings_end),
        string_values,
        blocks,
    })
}

/// The toolkit writer serializes placeholder geometry for LOD shapes; CK
/// keeps the counts but writes a zero data size and no geometry.
fn strip_combined_lod_geometry(mut data: Vec<u8>, specs: &[(usize, u64, u32, u32)]) -> Result<Vec<u8>> {
    if specs.is_empty() {
        return Ok(data);
    }
    let layout = layout(&data)?;
    for &(block_id, descriptor, triangles, vertices) in specs.iter().rev() {
        let (ref name, offset, size) = layout.blocks[block_id];
        if name != "BSMeshLODTriShape" {
            return Err(PrevisError::invalid(format!("block {block_id} is not BSMeshLODTriShape")));
        }
        let geometry_size = vertices as usize * ((descriptor & 0xF) * 4) as usize + triangles as usize * 6;
        let metadata = offset + size - 12 - geometry_size - 18;
        let mut expected = Vec::with_capacity(18);
        expected.extend_from_slice(&descriptor.to_le_bytes());
        expected.extend_from_slice(&triangles.to_le_bytes());
        expected.extend_from_slice(&(vertices as u16).to_le_bytes());
        expected.extend_from_slice(&(geometry_size as u32).to_le_bytes());
        if data[metadata..metadata + 18] != expected[..] {
            return Err(PrevisError::invalid(format!("unexpected LOD geometry metadata in block {block_id}")));
        }
        data[metadata + 14..metadata + 18].copy_from_slice(&0u32.to_le_bytes());
        data.drain(metadata + 18..metadata + 18 + geometry_size);
        let size_offset = layout.block_sizes_offset + block_id * 4;
        let new_size = size as u32 - geometry_size as u32;
        data[size_offset..size_offset + 4].copy_from_slice(&new_size.to_le_bytes());
    }
    Ok(data)
}

/// Rewrites the two serializer details that differ from CK: the export-info
/// payload and the root name interned before the empty string.
fn canonicalize(data: Vec<u8>, root_name: &str, export_info: &[u8]) -> Result<Vec<u8>> {
    let layout = layout(&data)?;
    if data[layout.export_info.0..layout.export_info.1] != [0, 0, 0, 0] {
        return Err(PrevisError::invalid("unexpected toolkit NIF export info"));
    }
    let root_index = layout
        .string_values
        .iter()
        .position(|s| s == root_name.as_bytes())
        .ok_or_else(|| PrevisError::invalid("group NIF string table is missing the root name"))? as i32;
    let mut data = data;
    let swap = |data: &mut Vec<u8>, offset: usize| {
        let value = i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        let replacement = if value == 0 {
            root_index
        } else if value == root_index {
            0
        } else {
            return;
        };
        data[offset..offset + 4].copy_from_slice(&replacement.to_le_bytes());
    };
    for (name, offset, _) in &layout.blocks {
        if NAMED_GROUP_BLOCKS.contains(&name.as_str()) {
            swap(&mut data, *offset);
        }
        if name == "BSLightingShaderProperty" {
            swap(&mut data, offset + 60);
        }
    }
    let mut strings = layout.string_values.clone();
    strings.swap(0, root_index as usize);
    let mut out = Vec::with_capacity(data.len() + export_info.len());
    out.extend_from_slice(&data[..layout.export_info.0]);
    out.extend_from_slice(export_info);
    out.extend_from_slice(&data[layout.export_info.1..layout.strings.0]);
    for s in &strings {
        out.extend_from_slice(&(s.len() as u32).to_le_bytes());
        out.extend_from_slice(s);
    }
    out.extend_from_slice(&data[layout.strings.1..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packfile(classes: &[&str]) -> Vec<u8> {
        let mut names = Vec::new();
        for class in classes {
            names.extend_from_slice(&[0, 0, 0, 0, 0x09]);
            names.extend_from_slice(class.as_bytes());
            names.push(0);
        }
        let mut blob = vec![0u8; 0x80];
        blob[0x14..0x18].copy_from_slice(&1u32.to_le_bytes());
        blob[0x40..0x4F].copy_from_slice(b"__classnames__\0");
        blob[0x54..0x58].copy_from_slice(&0x80u32.to_le_bytes());
        blob[0x58..0x5C].copy_from_slice(&(names.len() as u32).to_le_bytes());
        blob.extend_from_slice(&names);
        blob
    }

    fn with_collision(nif: &mut NifFile, node: usize, physics: usize, body: u64) {
        let object = add_node(
            nif,
            "bhkNPCollisionObject",
            map(vec![
                ("Target", NifValue::Ref(node as i32)),
                ("Data", NifValue::Ref(physics as i32)),
                ("Body ID", u(body)),
            ]),
        );
        set_fields(nif, node, map(vec![("Collision Object", NifValue::Ref(object as i32))]));
    }

    #[test]
    fn collision_nodes_share_one_physics_system_below_the_root() {
        let mut nif = NifFile::new("fo4");
        let blob = packfile(&["hknpPhysicsSystemData", "hknpCapsuleShape"]);
        let physics = nif.add_block("bhkPhysicsSystem", None);
        nif.blocks[physics].set_field(
            "Binary Data",
            NifValue::Struct(IndexMap::from([
                ("Data Size".to_string(), u(blob.len() as u64)),
                ("Data".to_string(), NifValue::Array(blob.iter().map(|&b| u(b as u64)).collect())),
            ])),
        );
        let helper = add_node(
            &mut nif,
            "NiNode",
            map(vec![
                ("Name", NifValue::String("C_Help".into())),
                ("Translation", vector([1.0, 2.0, 3.0])),
                ("Scale", f(1.0)),
            ]),
        );
        attach_child(&mut nif, 0, helper);
        with_collision(&mut nif, 0, physics, 0);
        with_collision(&mut nif, helper, physics, 1);

        let collision = collision_of(&nif).unwrap().unwrap();
        assert_eq!(collision.physics_block, physics);
        assert_eq!(collision.flags, 0x88);
        let nodes: Vec<_> = collision.nodes.iter().map(|n| (n.name.clone(), n.local.translation, n.body_id)).collect();
        assert_eq!(nodes, [(None, [0.0; 3], 0), (Some("C_Help".into()), [1.0, 2.0, 3.0], 1)]);
    }

    #[test]
    fn marker_only_models_report_culled_geometry() {
        let mut nif = NifFile::new("fo4");
        let marker = add_node(&mut nif, "BSTriShape", map(vec![("Name", NifValue::String("EditorMarker".into()))]));
        attach_child(&mut nif, 0, marker);
        assert_eq!(retained_shape_blocks(&nif), (vec![], true));
        let mesh = add_node(&mut nif, "BSTriShape", map(vec![("Name", NifValue::String("Mesh".into()))]));
        attach_child(&mut nif, 0, mesh);
        assert_eq!(retained_shape_blocks(&nif), (vec![mesh], true));
    }

    #[test]
    fn material_alpha_decides_presence_enables_and_threshold() {
        let alpha = |flags: u16, threshold: u8| Alpha { flags, threshold };
        let material = |a| AlphaSource::Material(Some(a));
        assert_eq!(combined_alpha(AlphaSource::Material(None), Some(alpha(0x12EC, 128))), None);
        assert_eq!(combined_alpha(material(alpha(0x02EC, 128)), Some(alpha(0x10ED, 0))), Some(alpha(0x12EC, 128)));
        assert_eq!(combined_alpha(material(alpha(0x82EC, 6)), None), Some(alpha(0x82EC, 6)));
        assert_eq!(combined_alpha(material(alpha(0x00ED, 66)), Some(alpha(0x92EC, 1))), Some(alpha(0x10ED, 66)));
        assert_eq!(combined_alpha(AlphaSource::Source, Some(alpha(0x10ED, 30))), Some(alpha(0x10ED, 30)));
    }

    fn retained(vertex_count: u32, triangle_count: u32, lod_sizes: Option<[u32; 3]>) -> RetainedShape {
        RetainedShape {
            block: 0,
            vertex_desc: 0x0001_B000_0043_0205,
            vertex_count,
            triangle_count,
            lod_sizes,
            // Stride 20: all-zero positions.
            geometry: vec![0; vertex_count as usize * 20],
            transform: Transform::IDENTITY,
            bound: ([0.0; 3], 1.0),
            shader_block: 0,
            texture_set_block: None,
            shader: Arc::new(ExpandedShader {
                fields: IndexMap::new(),
                source_shader_type: 0,
                shader_type: 0,
                flags1: 0,
                flags2: 0,
                textures: None,
                material_crc32: 7,
                alpha: AlphaSource::Source,
                grayscale: 1.0,
            }),
            alpha_block: None,
            source_alpha: None,
        }
    }

    #[test]
    fn plain_rows_join_a_lod_bucket_in_lod0() {
        let lod = retained(80, 88, Some([0, 0, 88]));
        let plain = retained(120, 132, None);
        fn row(shape: &RetainedShape, count: usize) -> Row<'_> {
            Row {
                source_index: 0,
                shape,
                shader: ExpandedShader::clone(&shape.shader),
                alpha: None,
                instances: vec![Transform::IDENTITY; count],
                data_offset: 0,
                leaf: None,
            }
        }
        let buckets = buckets(&[row(&lod, 2), row(&plain, 1)]).unwrap();
        assert_eq!(buckets.len(), 1);
        let spec = bucket_spec(&buckets[0], 0).unwrap();
        assert_eq!(spec.shape_type, "BSMeshLODTriShape");
        let size = |level: &str| spec.shape.get(level).map(NifValue::as_i64);
        assert_eq!((size("LOD0 Size"), size("LOD1 Size"), size("LOD2 Size")), (Some(132), Some(0), Some(176)));
        assert_eq!(spec.vertices, 280);
    }

    #[test]
    fn static_layer_one_collision_merges_unless_bsx_keeps_it_cloned() {
        let routed = |collision_filter_info: u32, bsx: u64| {
            let mesh = crate::havok_mesh::CompressedMesh {
                sections: vec![crate::havok_mesh::Section {
                    packed_vertices: vec![0, 0x7FF, 0x3FF_FFFF],
                    primitives: vec![[0, 1, 2, 2]],
                    primitive_data_runs: vec![(0, 0, 1)],
                    tree_nodes: vec![[0; 4]],
                    ..Default::default()
                }],
                quad_is_flat_words: vec![0],
                quad_is_flat_num_bits: 1,
                triangle_is_interior_words: vec![0],
                triangle_is_interior_num_bits: 2,
                collision_filter_info,
                ..crate::havok_mesh::CompressedMesh::empty()
            };
            let blob = crate::combined_physics::serialize_packfile(&mesh, 8).unwrap();
            let mut nif = NifFile::new("fo4");
            let physics = nif.add_block("bhkPhysicsSystem", None);
            nif.blocks[physics].set_field("Binary Data", binary_data_value(&blob));
            add_node(&mut nif, "BSXFlags", map(vec![("Name", NifValue::String("BSX".into())), ("Integer Data", u(bsx))]));
            with_collision(&mut nif, 0, physics, 0);
            collision_of(&nif).unwrap().unwrap().merged.is_some()
        };
        assert!(routed(1, 0x82));
        assert!(!routed(1, 0xC2));
        assert!(!routed(26, 0x82));
    }

    #[test]
    fn constrained_or_capsule_free_systems_keep_sync_only() {
        let names = |c: &[&str]| c.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(clone_collision_flags(&names(&["hknpCompressedMeshShape"])), 0x80);
        assert_eq!(clone_collision_flags(&names(&["hknpCapsuleShape", "hkpRagdollConstraintData"])), 0x80);
        assert_eq!(packfile_classes(&packfile(&["hkA", "hkB"])).unwrap(), ["hkA", "hkB"]);
    }
}
