//! CK's merged precombine collision (`%08X_Physics.NIF`): every mergeable
//! source collision of a cell rebuilt as one static `hknpCompressedMeshShape`,
//! as `CombinedData::GenerateCombinedShape` does. Reproduces
//! `tools/re/ck_havok_packfile32.py` (`build_controlled_precombine_meshes`,
//! `build_ck_packfile32`) and the Physics.NIF container CK writes around it.

use std::collections::BTreeSet;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::error::{PrevisError, Result};
use crate::f32ops::{Mat3, Transform};
use crate::havok_mesh::{
    Bounds, CompressedMesh, CustomPrimitive, ExpandedLeaves, GAME_TO_HAVOK_SCALE, HAVOK_TO_GAME_SCALE, HKX_MAGIC, Packfile,
    PartTransform, PrecombineSource, Section, SourcePart, Tree, V3, bounds_of, bounds_of_indices, compress_data_runs, data_sort_order,
    decode_custom_vertices,
    decode_vertex_11_11_10, decode_vertex_21_21_22, encode_custom_vertices, encode_radius_tag, encode_vertex_11_11_10,
    encode_vertex_21_21_22, expand_data_runs, expand_leaves, first_max, first_min, flag_interior_triangle_words,
    flat_convex_quad_words, mark_bad_primitives, partition_sections, physics_rotation, primitive_data_values, primitive_sequences,
    quad_candidates, refit_for_leaf_bounds, sah_tree_for_bounds, select_quad_pairs, static_aabb4_node_bounds,
    static_aabb4_nodes, static_aabb5_nodes, valid_triangles, weld_mapping,
};

/// Base, maximum and scale of a section without packed vertices.
const EMPTY_PACKED_BASE: f32 = f32::from_bits(0x7F7F_FFEE);
const DEFAULT_MATERIAL_CRC: u32 = 0x1DD9_C611;
/// Export info CK 1.11.240 writes into precombine Physics.NIFs.
pub const PHYSICS_EXPORT_INFO: [u8; 10] = [0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0xFA, 0x7F, 0x00];

/// One source collision placed in the cell: the world transform of its
/// collision node (NIF-row rotation, game-unit translation, uniform scale).
#[derive(Clone)]
pub struct PhysicsInstance {
    pub source: Arc<PrecombineSource>,
    pub transform: Transform,
    /// The reference's scale and a SCOL component's (1 for a STAT): CK
    /// rescales convex surfaces by each in turn, not by their product.
    pub reference_scale: f32,
    pub component_scale: f32,
}

pub struct CombinedPhysics {
    pub mesh: CompressedMesh,
    pub center: V3,
    /// Physics.NIF root translation: the Havok-space center in game units.
    pub root_translation: V3,
    pub candidate_count: usize,
    pub selected_pair_count: usize,
}

pub fn physics_root_name(cell_form_id: u32) -> String {
    format!("{:08X}_Physics", cell_form_id & 0x00FF_FFFF)
}

pub fn physics_file_name(cell_form_id: u32) -> String {
    format!("{}.NIF", physics_root_name(cell_form_id))
}

fn is_unrotated_unit_scale(transform: &Transform) -> bool {
    transform.rotation == Transform::IDENTITY.rotation && transform.scale == 1.0
}

fn transformed_shape_bounds(aabb: Bounds, transform: &Transform, rotation: &Mat3, translation: V3) -> Bounds {
    if is_unrotated_unit_scale(transform) {
        return ([0, 1, 2].map(|a| aabb.0[a] + translation[a]), [0, 1, 2].map(|a| aabb.1[a] + translation[a]));
    }
    let scaled_min = aabb.0.map(|v| v * transform.scale);
    let scaled_max = aabb.1.map(|v| v * transform.scale);
    let center = [0, 1, 2].map(|a| (scaled_min[a] + scaled_max[a]) * 0.5);
    let half = [0, 1, 2].map(|a| (scaled_max[a] - scaled_min[a]) * 0.5);
    let mut minimum = [0.0; 3];
    let mut maximum = [0.0; 3];
    for (axis, row) in rotation.iter().enumerate() {
        let transformed_center = center[0] * row[0] + center[1] * row[1] + center[2] * row[2] + translation[axis];
        let extent = (half[0] * row[0]).abs() + (half[1] * row[1]).abs() + (half[2] * row[2]).abs();
        minimum[axis] = transformed_center - extent;
        maximum[axis] = transformed_center + extent;
    }
    (minimum, maximum)
}

/// `hknpConvexShape::calcAabb`: the rotated vertices' bounds, translated, then
/// grown by the radius (CELL 1294F4 is centered only with these bounds).
fn convex_bounds(part: &SourcePart, instance: &PhysicsInstance, rotation: &Mat3, translation: V3) -> Bounds {
    let custom = &part.mesh.sections[0].custom_primitives[0];
    let (vertex_scale, radius) = custom_scale(custom, part.wrapper_scale, instance);
    let (lo, hi) = bounds_of(transform_custom_vertices(&custom.vertices, rotation, vertex_scale).iter());
    ([0, 1, 2].map(|a| (lo[a] + translation[a]) - radius), [0, 1, 2].map(|a| (hi[a] + translation[a]) + radius))
}

/// The rotation-matrix padding lanes CK's SIMD code carries into AABB W.
fn w_coefficients(rotation: &Mat3) -> (f32, f32) {
    (rotation[1][2], rotation[0][2])
}

fn center_w(aabb: Bounds, first: f32, second: f32) -> f32 {
    let (lo, hi) = aabb;
    let corners = [first * lo[0] + second * lo[1], first * lo[0] + second * hi[1], first * hi[0] + second * lo[1], first * hi[0] + second * hi[1]];
    let minimum = corners[1..].iter().fold(corners[0], |m, &c| first_min(m, c));
    let maximum = corners[1..].iter().fold(corners[0], |m, &c| first_max(m, c));
    (minimum + maximum) * 0.5
}

/// Compound children get their body's W residue; theirs is not recovered.
fn centered_w_values(aabb: Bounds, vertices: &[V3], rotation: &Mat3, scale: f32) -> Vec<f32> {
    let (first, second) = w_coefficients(rotation);
    let center = center_w(aabb, first, second);
    vertices.iter().map(|v| ((first * v[0] + second * v[1]) - center) * scale).collect()
}

fn rotate(vertex: V3, matrix: &Mat3) -> V3 {
    matrix.map(|row| vertex[1] * row[1] + vertex[0] * row[0] + vertex[2] * row[2])
}

/// `hkTransform::setMul` of a body's rotation (no translation) with a
/// compound instance's transform.
fn compose_instance(body: &Mat3, instance: &PartTransform) -> PartTransform {
    let column = |j: usize| rotate([instance.rotation[0][j], instance.rotation[1][j], instance.rotation[2][j]], body);
    let columns = [column(0), column(1), column(2)];
    PartTransform {
        rotation: std::array::from_fn(|i| std::array::from_fn(|j| columns[j][i])),
        translation: rotate(instance.translation, body),
    }
}

/// Custom-primitive support vertices are scaled before they are rotated.
fn transform_custom_vertices(vertices: &[V3], rotation: &Mat3, scale: V3) -> Vec<V3> {
    vertices.iter().map(|v| rotate([0, 1, 2].map(|a| v[a] * scale[a]), rotation)).collect()
}

/// `hknpShapeUtil::calcScalingParameters` in surface mode: a per-axis vertex
/// scale that grows the shape by `scale` without scaling its convex radius.
fn surface_scale(vertices: &[V3], radius: f32, scale: V3) -> V3 {
    let scale = scale.map(|s| first_max(s, 0.001));
    if radius.abs() <= 1.0e-5 || scale.iter().all(|s| (s - 1.0).abs() <= f32::EPSILON) {
        return scale;
    }
    let (lo, hi) = bounds_of(vertices.iter());
    let half = [0, 1, 2].map(|a| (hi[a] - lo[a]) * 0.5);
    let surface = [0, 1, 2].map(|a| (radius + half[a]) * scale[a].abs());
    let kept_radius = if surface.iter().all(|&s| radius < s) { radius } else { first_min(first_min(surface[0], surface[1]), surface[2]) };
    [0, 1, 2].map(|a| {
        let adjust = if half[a] == 0.0 { 0.0 } else { first_min((scale[a].abs() * radius - kept_radius) / half[a], 100.0) };
        first_max(scale[a] + adjust, 0.001)
    })
}

/// The per-axis vertex scale and convex radius CK gives a placed custom
/// primitive. A convex surface is rescaled by the SCOL component's scale and
/// then, from that result, by the reference's (CELL 1294F4: 136 of 138
/// doubly scaled SCOL convexes land on CK's vertices only that way); its
/// radius stays unscaled.
fn custom_scale(custom: &CustomPrimitive, wrapper_scale: Option<V3>, instance: &PhysicsInstance) -> (V3, f32) {
    let scale = instance.transform.scale;
    let rounded = matches!(custom.custom_type, 0 | 1);
    if let Some(wrapper) = wrapper_scale {
        let vertex_scale = wrapper.map(|w| first_max(w * scale, 0.001));
        return if rounded {
            ([vertex_scale[0]; 3], custom.convex_radius * vertex_scale[0])
        } else {
            (vertex_scale, custom.convex_radius)
        };
    }
    if rounded {
        return ([scale; 3], scale.abs() * custom.convex_radius);
    }
    let component = surface_scale(&custom.vertices, custom.convex_radius, [instance.component_scale; 3]);
    if instance.reference_scale == 1.0 {
        return (component, custom.convex_radius);
    }
    let vertex_scale = component.map(|c| c * instance.reference_scale);
    (surface_scale(&custom.vertices, custom.convex_radius, vertex_scale), custom.convex_radius)
}

struct BuildCustom {
    vertices: Vec<V3>,
    custom_type: u8,
    tags: Vec<u16>,
    convex_radius: f32,
    material: u16,
    minimum_w: f32,
    maximum_w: f32,
}

fn custom_bounds(primitive: &BuildCustom) -> Bounds {
    let (lo, hi) = bounds_of(primitive.vertices.iter());
    (lo.map(|v| v - primitive.convex_radius), hi.map(|v| v + primitive.convex_radius))
}

#[derive(Default)]
struct MaterialTable {
    entries: Vec<(u32, u32)>,
    index: FxHashMap<(u32, u32), u16>,
}

impl MaterialTable {
    fn remap(&mut self, source: &CompressedMesh, source_index: u16) -> Result<u16> {
        if source.material_entries.is_empty() {
            if source_index != 0 {
                return Err(PrevisError::invalid("collision has primitive material data but no material table"));
            }
            return Ok(0);
        }
        let material = *source
            .material_entries
            .get(source_index as usize)
            .ok_or_else(|| PrevisError::invalid("collision primitive material index is out of bounds"))?;
        if let Some(&index) = self.index.get(&material) {
            return Ok(index);
        }
        let index = u16::try_from(self.entries.len()).map_err(|_| PrevisError::unsupported("more than 65536 collision materials"))?;
        self.index.insert(material, index);
        self.entries.push(material);
        Ok(index)
    }
}

fn min_max(mut values: impl Iterator<Item = f32>) -> (f32, f32) {
    let first = values.next().unwrap_or(0.0);
    values.fold((first, first), |(lo, hi), v| (first_min(lo, v), first_max(hi, v)))
}

/// `GenerateCombinedShape` over `instances` in CK body order (the reverse of
/// the cell's reference order).
pub fn build_combined_physics(instances: &[PhysicsInstance]) -> Result<CombinedPhysics> {
    if instances.is_empty() {
        return Err(PrevisError::invalid("precombine collision requires at least one source instance"));
    }
    let rotations: Vec<Mat3> = instances.iter().map(|i| physics_rotation(&i.transform.rotation)).collect();
    let body_positions: Vec<V3> = instances.iter().map(|i| i.transform.translation.map(|v| v * GAME_TO_HAVOK_SCALE)).collect();
    let mut transformed: Vec<Bounds> = Vec::new();
    for ((instance, rotation), &position) in instances.iter().zip(&rotations).zip(&body_positions) {
        for body in &instance.source.bodies {
            transformed.push(match body.convex {
                true => convex_bounds(&body.parts[0], instance, rotation, position),
                false => transformed_shape_bounds(body.aabb, &instance.transform, rotation, position),
            });
        }
    }
    let aggregate_min = bounds_of(transformed.iter().map(|b| &b.0)).0;
    let aggregate_max = bounds_of(transformed.iter().map(|b| &b.1)).1;
    let center: V3 = [0, 1, 2].map(|a| (aggregate_min[a] + aggregate_max[a]) * 0.5);

    let mut materials = MaterialTable::default();
    let mut centered: Vec<V3> = Vec::new();
    let mut w_values: Vec<f32> = Vec::new();
    let mut expanded = ExpandedLeaves {
        vertices: Vec::new(),
        triangles: Vec::new(),
        vertex_sources: Vec::new(),
        triangle_sources: Vec::new(),
    };
    let mut triangle_materials: Vec<u32> = Vec::new();
    let mut customs: Vec<BuildCustom> = Vec::new();
    let single_instance = instances.len() == 1;
    for ((instance, rotation), &body_position) in instances.iter().zip(&rotations).zip(&body_positions) {
        let scale = instance.transform.scale;
        let scaled_rotation = rotation.map(|row| row.map(|c| c * scale));
        let relative_position: V3 = [0, 1, 2].map(|a| body_position[a] - center[a]);
        let candidate_translation = if single_instance { [0.0; 3] } else { body_position };
        let (first_w, second_w) = w_coefficients(rotation);
        for body in &instance.source.bodies {
            let body_center_w = center_w(body.aabb, first_w, second_w);
            for part in &body.parts {
                let source = &part.mesh;
                let placed = part.instance.map(|child| compose_instance(&scaled_rotation, &child));
                let matrix = placed.map_or(scaled_rotation, |p| p.rotation);
                let offset = |base: V3| match &placed {
                    Some(p) => [0, 1, 2].map(|a| p.translation[a] + base[a]),
                    None => base,
                };
                let part_position = offset(relative_position);
                let source_vertices: Vec<V3> = source.sections.iter().flat_map(|s| s.vertices.iter().copied()).collect();
                let transformed_vertices: Vec<V3> = source_vertices.iter().map(|&v| rotate(v, &matrix)).collect();
                let vertex_offset = centered.len();
                centered.extend(transformed_vertices.iter().map(|v| [0, 1, 2].map(|a| v[a] + part_position[a])));
                w_values.extend(centered_w_values(body.aabb, &source_vertices, rotation, scale));

                let leaves = expand_leaves(source, &transformed_vertices, scale < 0.0)?;
                let candidate_position = offset(candidate_translation);
                let expanded_offset = expanded.vertices.len();
                expanded.vertices.extend(leaves.vertices.iter().map(|v| [0, 1, 2].map(|a| v[a] + candidate_position[a])));
                let (expanded_offset, vertex_offset) = (index_u32(expanded_offset)?, index_u32(vertex_offset)?);
                expanded.triangles.extend(leaves.triangles.iter().map(|t| t.map(|i| expanded_offset + i)));
                expanded.vertex_sources.extend(leaves.vertex_sources.iter().map(|&i| vertex_offset + i));

                let mut source_materials = Vec::new();
                for section in &source.sections {
                    source_materials.extend(expand_data_runs(&section.primitive_data_runs, section.primitives.len())?);
                }
                for &primitive in &leaves.triangle_sources {
                    triangle_materials.push(materials.remap(source, source_materials[primitive])? as u32);
                }

                // A scaled placement's convex transform is orthonormalized by CK;
                // the unscaled rotation stands in for that.
                let custom_rotation = match part.instance {
                    None => *rotation,
                    Some(_) if scale == 1.0 => matrix,
                    Some(child) => compose_instance(rotation, &child).rotation,
                };
                let mut primitive_offset = 0;
                for section in &source.sections {
                    for custom in &section.custom_primitives {
                        let material = materials.remap(source, source_materials[primitive_offset + custom.primitive_index])?;
                        let (vertex_scale, radius) = custom_scale(custom, part.wrapper_scale, instance);
                        let vertices = transform_custom_vertices(&custom.vertices, &custom_rotation, vertex_scale)
                            .into_iter()
                            .map(|v| [0, 1, 2].map(|a| v[a] + part_position[a]))
                            .collect();
                        let tags = if radius != 0.0 || matches!(custom.custom_type, 0 | 1) {
                            vec![encode_radius_tag(radius)]
                        } else {
                            Vec::new()
                        };
                        let (support_min, support_max) =
                            min_max(custom.vertices.iter().map(|v| ((first_w * v[0] + second_w * v[1]) - body_center_w) * scale));
                        let radius_w = radius * (first_w.abs() + second_w.abs());
                        customs.push(BuildCustom {
                            vertices,
                            custom_type: custom.custom_type,
                            tags,
                            convex_radius: radius,
                            material,
                            minimum_w: support_min - radius_w,
                            maximum_w: support_max + radius_w,
                        });
                    }
                    primitive_offset += section.primitives.len();
                }
            }
        }
    }
    let paired = pair_leaves(expanded, triangle_materials)?;
    let sequences = paired.sequences;

    if sequences.is_empty() && customs.is_empty() {
        return Err(PrevisError::invalid("precombine collision has no primitives"));
    }
    let mut leaf_bounds: Vec<Bounds> = Vec::with_capacity(sequences.len() + customs.len());
    leaf_bounds.extend(sequences.iter().map(|p| bounds_of_indices(&centered, p)));
    leaf_bounds.extend(customs.iter().map(custom_bounds));
    let (tree, root) = sah_tree_for_bounds(&leaf_bounds);
    drop(leaf_bounds);
    let material_entries = materials.entries;
    let distinct_vertices = || sequences.iter().flatten().copied().collect::<FxHashSet<usize>>().len();
    let mesh = if !customs.is_empty() || sequences.len() >= 128 || distinct_vertices() >= 256 {
        build_sectioned(&centered, &w_values, &sequences, &paired.materials, &customs, tree, root, material_entries)?
    } else {
        build_single_section(&centered, &w_values, &sequences, &paired.materials, &tree, root, material_entries)?
    };
    Ok(CombinedPhysics {
        mesh,
        center,
        root_translation: center.map(|v| v * HAVOK_TO_GAME_SCALE),
        candidate_count: paired.candidate_count,
        selected_pair_count: paired.selected_pair_count,
    })
}

fn index_u32(index: usize) -> Result<u32> {
    u32::try_from(index).map_err(|_| PrevisError::unsupported("precombine collision exceeds 2^32 leaf vertices"))
}

struct PairedLeaves {
    /// Source vertex indices of each primitive: accepted quads, then unpaired triangles.
    sequences: Vec<[usize; 4]>,
    materials: Vec<u16>,
    candidate_count: usize,
    selected_pair_count: usize,
}

/// `buildStep12`'s weld, degenerate-triangle removal and quad pairing. Only
/// the primitives outlive it: the leaf stream is the largest allocation of a
/// CELL's collision build.
fn pair_leaves(mut expanded: ExpandedLeaves, mut triangle_materials: Vec<u32>) -> Result<PairedLeaves> {
    let vertex_map = weld_mapping(&expanded.vertices);
    let valid = valid_triangles(&expanded.vertices, &expanded.triangles, &vertex_map);
    let mut kept = valid.iter();
    expanded.triangles.retain(|_| *kept.next().unwrap());
    let mut kept = valid.iter();
    triangle_materials.retain(|_| *kept.next().unwrap());
    drop(valid);
    let candidates = quad_candidates(&expanded.vertices, &expanded.triangles, &vertex_map, &triangle_materials);
    let candidate_count = candidates.len();
    let selected = select_quad_pairs(candidates, expanded.triangles.len());
    let sequences = primitive_sequences(&expanded, &vertex_map, &selected);
    drop((expanded, vertex_map));
    let triangle_values: Vec<u16> = triangle_materials.iter().map(|&m| m as u16).collect();
    let materials = primitive_data_values(&triangle_values, &selected)?;
    Ok(PairedLeaves { sequences, materials, candidate_count, selected_pair_count: selected.len() })
}

struct PackedFrame {
    base: V3,
    maximum: V3,
    scale: V3,
}

fn packed_frame(centered: &[V3], packed_order: &[usize]) -> PackedFrame {
    if packed_order.is_empty() {
        return PackedFrame {
            base: [EMPTY_PACKED_BASE; 3],
            maximum: [-EMPTY_PACKED_BASE; 3],
            scale: [f32::NEG_INFINITY; 3],
        };
    }
    let (base, maximum) = bounds_of_indices(centered, packed_order);
    PackedFrame {
        base,
        maximum,
        scale: vertex_scale(base, maximum),
    }
}

fn vertex_scale(minimum: V3, maximum: V3) -> V3 {
    [(0, 2047.0f32), (1, 2047.0), (2, 1023.0)].map(|(a, limit)| (maximum[a] - minimum[a]) / limit)
}

fn as_u8(value: usize, what: &str) -> Result<u8> {
    u8::try_from(value).map_err(|_| PrevisError::unsupported(format!("{what} exceeds CK's byte-sized section limit")))
}

/// The shape-key domain and key count of a sectioned mesh, taken before CK's
/// bad-primitive pass, and its flat-quad storage, taken after it.
fn key_domains(sections: &mut [Section]) -> (Vec<u32>, u32, u32, u32) {
    let last = sections.last().expect("mesh has sections");
    let quad_bits = (sections.len() - 1) * 128 + last.primitives.len();
    let last_primitive = last.primitives.last().expect("section has primitives");
    let interior_bits = quad_bits * 2 - usize::from(last_primitive[2] == last_primitive[3]);
    let key_count = shape_key_count(sections);
    sections.iter_mut().for_each(mark_bad_primitives);
    let mut quad_words = vec![0u32; quad_bits.div_ceil(32)];
    for (section_index, section) in sections.iter().enumerate() {
        let local = flat_convex_quad_words(section);
        for primitive in 0..section.primitives.len() {
            if local[primitive >> 5] & 1 << (primitive & 31) != 0 {
                let key = section_index * 128 + primitive;
                quad_words[key >> 5] |= 1 << (key & 31);
            }
        }
    }
    (quad_words, quad_bits as u32, interior_bits as u32, key_count)
}

fn assign_leaf_indices(sections: &mut [Section], master: &[[u8; 5]]) -> Result<()> {
    let mut leaf_nodes = FxHashMap::default();
    for (node_index, node) in master.iter().enumerate() {
        let data = (node[3] as usize) << 8 | node[4] as usize;
        if data & 0x8000 == 0 {
            leaf_nodes.insert(data, node_index);
        }
    }
    for (index, section) in sections.iter_mut().enumerate() {
        let node = *leaf_nodes.get(&index).ok_or_else(|| PrevisError::invalid("section is missing from the master tree"))?;
        section.leaf_index = u16::try_from(node).map_err(|_| PrevisError::unsupported("master tree exceeds 65536 nodes"))?;
    }
    Ok(())
}

fn finish_mesh(mut mesh: CompressedMesh) -> Result<CompressedMesh> {
    mesh.triangle_is_interior_words = flag_interior_triangle_words(&mesh)?;
    Ok(mesh)
}

/// One section's slice of the shared stream: page-relative shared-vertex
/// indices and custom-primitive metadata, in leaf order.
struct SectionStream {
    page: u8,
    stream: Vec<u16>,
    vertex_positions: FxHashMap<usize, usize>,
    custom_metadata: FxHashMap<usize, usize>,
}

struct SharedLayout {
    sections: Vec<SectionStream>,
    /// Encoded shared vertices and page padding; custom words are filled in
    /// once their leaf bounds are known.
    words: Vec<Option<u64>>,
    custom_first_word: FxHashMap<usize, usize>,
}

/// Shared vertices are numbered in first-use order. A section whose new
/// words would cross a 64K page boundary starts the next page instead: the
/// gap is zero-filled and vertices from earlier pages are written again.
fn layout_shared_stream(
    section_payloads: &[Vec<usize>],
    sequences: &[[usize; 4]],
    shared: &FxHashSet<usize>,
    customs: &[BuildCustom],
    encode: impl Fn(usize) -> u64,
) -> Result<SharedLayout> {
    let standard_count = sequences.len();
    let custom_words = |payload: usize| customs[payload - standard_count].vertices.len().div_ceil(2);
    let mut layout = SharedLayout {
        sections: Vec::with_capacity(section_payloads.len()),
        words: Vec::new(),
        custom_first_word: FxHashMap::default(),
    };
    let mut page = 0usize;
    let mut page_words: FxHashMap<usize, usize> = FxHashMap::default();
    for payloads in section_payloads {
        let mut pending = FxHashSet::default();
        let mut new_words = 0;
        for &payload in payloads {
            if payload >= standard_count {
                new_words += custom_words(payload);
                continue;
            }
            for vertex in sequences[payload] {
                if shared.contains(&vertex) && !page_words.contains_key(&vertex) && pending.insert(vertex) {
                    new_words += 1;
                }
            }
        }
        let page_end = (page + 1) << 16;
        if layout.words.len() + new_words > page_end {
            layout.words.resize(page_end, Some(0));
            page += 1;
            page_words.clear();
        }
        let page_base = page << 16;
        let mut section = SectionStream {
            page: u8::try_from(page).map_err(|_| PrevisError::unsupported("shared vertex stream exceeds 256 pages"))?,
            stream: Vec::new(),
            vertex_positions: FxHashMap::default(),
            custom_metadata: FxHashMap::default(),
        };
        for &payload in payloads {
            if payload < standard_count {
                for vertex in sequences[payload] {
                    if !shared.contains(&vertex) || section.vertex_positions.contains_key(&vertex) {
                        continue;
                    }
                    let global = *page_words.entry(vertex).or_insert_with(|| {
                        layout.words.push(Some(encode(vertex)));
                        layout.words.len() - 1
                    });
                    section.vertex_positions.insert(vertex, section.stream.len());
                    section.stream.push((global - page_base) as u16);
                }
                continue;
            }
            let custom = &customs[payload - standard_count];
            let first = layout.words.len();
            layout.words.resize(first + custom_words(payload), None);
            layout.custom_first_word.insert(payload, first);
            section.custom_metadata.insert(payload, section.stream.len());
            section.stream.push((custom.vertices.len() as u16) << 8 | (custom.tags.len() as u16) << 6 | 1 << 4 | custom.custom_type as u16);
            section.stream.push((first - page_base) as u16);
            section.stream.extend_from_slice(&custom.tags);
        }
        layout.sections.push(section);
    }
    Ok(layout)
}

/// Meshes past one section's limits (or with custom primitives): the SAH tree
/// is split into sections under a master tree and vertices used by several
/// sections move to the shared stream.
#[allow(clippy::too_many_arguments)]
fn build_sectioned(
    centered: &[V3],
    w_values: &[f32],
    sequences: &[[usize; 4]],
    materials: &[u16],
    customs: &[BuildCustom],
    tree: Tree,
    root: usize,
    material_entries: Vec<(u32, u32)>,
) -> Result<CompressedMesh> {
    let standard_count = sequences.len();
    // A custom primitive's header, first-vertex word and tags are metadata
    // tokens numbered after the vertices.
    let mut custom_tokens = Vec::with_capacity(customs.len());
    let mut tag_token = centered.len();
    for custom in customs {
        let count = 2 + custom.tags.len();
        custom_tokens.push(tag_token..tag_token + count);
        tag_token += count;
    }
    let partition = partition_sections(&tree, root, |payload, tokens| match payload.checked_sub(standard_count) {
        None => tokens.extend(sequences[payload]),
        Some(custom) => tokens.extend(custom_tokens[custom].clone()),
    })?;
    let (object_min, object_max) = (tree.nodes[root].min, tree.nodes[root].max);
    drop(tree);

    let shared = shared_vertices(sequences, &partition.payloads, standard_count);
    let mut layout = layout_shared_stream(&partition.payloads, sequences, &shared, customs, |v| {
        encode_vertex_21_21_22(object_min, object_max, centered[v])
    })?;
    let decoded_shared: FxHashMap<usize, V3> =
        shared.iter().map(|&v| (v, decode_vertex_21_21_22(object_min, object_max, encode_vertex_21_21_22(object_min, object_max, centered[v])))).collect();

    let mut sections = Vec::with_capacity(partition.payloads.len());
    for (((section_tree, section_root), payloads), section_stream) in partition.sections.into_iter().zip(&partition.payloads).zip(&layout.sections) {
        let mut seen = FxHashSet::default();
        let packed_order: Vec<usize> = payloads
            .iter()
            .filter(|&&p| p < standard_count)
            .flat_map(|&p| sequences[p])
            .filter(|v| !shared.contains(v) && seen.insert(*v))
            .collect();
        let frame = packed_frame(centered, &packed_order);
        let packed_vertices: Vec<u32> =
            packed_order.iter().map(|&v| encode_vertex_11_11_10(frame.base, frame.maximum, centered[v])).collect();
        let decoded_packed: FxHashMap<usize, V3> = packed_order
            .iter()
            .zip(&packed_vertices)
            .map(|(&v, &p)| (v, decode_vertex_11_11_10(frame.base, frame.scale, p)))
            .collect();
        let stream = &section_stream.stream;
        if packed_order.len() + stream.len() > 0xFF {
            return Err(PrevisError::unsupported("section vertex/metadata stream exceeds CK's byte limit"));
        }
        let mut source_to_section: FxHashMap<usize, usize> = packed_order.iter().enumerate().map(|(i, &v)| (v, i)).collect();
        source_to_section.extend(section_stream.vertex_positions.iter().map(|(&v, &p)| (v, packed_order.len() + p)));
        let quantized = |v: usize| decoded_shared.get(&v).or_else(|| decoded_packed.get(&v)).copied().unwrap_or(centered[v]);

        let quantized_bounds: Vec<Option<Bounds>> = payloads
            .iter()
            .map(|&p| (p < standard_count).then(|| bounds_of(sequences[p].iter().map(|&v| quantized(v)).collect::<Vec<_>>().iter())))
            .collect();
        let (tree_min, tree_max) = (section_tree.nodes[section_root].min, section_tree.nodes[section_root].max);
        let mut refit = section_tree;
        refit_for_leaf_bounds(&mut refit, section_root, &quantized_bounds);

        let local_materials: Vec<u16> =
            payloads.iter().map(|&p| if p < standard_count { materials[p] } else { customs[p - standard_count].material }).collect();
        let (local_to_output, output_to_local) = data_sort_order(&local_materials);
        let output_materials: Vec<u16> = output_to_local.iter().map(|&i| local_materials[i]).collect();
        let tree_nodes = static_aabb4_nodes(&refit, section_root, Some(local_to_output.as_slice()));
        drop(refit);
        let tree_bounds = static_aabb4_node_bounds(tree_min, tree_max, &tree_nodes)?;
        let node_by_output: FxHashMap<usize, usize> =
            tree_nodes.iter().enumerate().filter(|(_, n)| n[3] & 1 == 0).map(|(i, n)| ((n[3] >> 1) as usize, i)).collect();

        let mut input_primitives: Vec<[u8; 4]> = Vec::with_capacity(payloads.len());
        let mut custom_records = Vec::new();
        for (local_index, &payload) in payloads.iter().enumerate() {
            if payload < standard_count {
                let mut primitive = [0u8; 4];
                for (slot, vertex) in primitive.iter_mut().zip(sequences[payload]) {
                    *slot = as_u8(source_to_section[&vertex], "section vertex index")?;
                }
                input_primitives.push(primitive);
                continue;
            }
            let custom = &customs[payload - standard_count];
            let output_index = local_to_output[local_index];
            let node_index = node_by_output[&output_index];
            let metadata_offset = section_stream.custom_metadata[&payload];
            let first_word = layout.custom_first_word[&payload];
            let node = as_u8(node_index, "section tree")?;
            input_primitives.push([as_u8(packed_order.len() + metadata_offset, "custom primitive header")?, node, node, node]);
            let leaf = tree_bounds[node_index];
            let encoded = encode_custom_vertices(1, &custom.vertices, (object_min, object_max), leaf)?;
            for (offset, &word) in encoded.iter().enumerate() {
                layout.words[first_word + offset] = Some(word);
            }
            custom_records.push(CustomPrimitive {
                primitive_index: output_index,
                tree_node_index: node_index,
                header: stream[metadata_offset],
                custom_type: custom.custom_type,
                compression_mode: 1,
                vertex_count: custom.vertices.len(),
                first_shared_vertex: stream[metadata_offset + 1] as usize,
                tags: custom.tags.clone(),
                convex_radius: custom.convex_radius,
                vertices: decode_custom_vertices(1, custom.vertices.len(), 0, &encoded, (object_min, object_max), leaf)?,
            });
        }
        let mut output_primitives = vec![[0u8; 4]; input_primitives.len()];
        for (local_index, &output_index) in local_to_output.iter().enumerate() {
            output_primitives[output_index] = input_primitives[local_index];
        }

        let mut section_vertices = vec![[0.0f32; 3]; packed_order.len() + stream.len()];
        for (&source, &local) in &source_to_section {
            section_vertices[local] = quantized(source);
        }
        let used: BTreeSet<usize> = payloads.iter().filter(|&&p| p < standard_count).flat_map(|&p| sequences[p]).collect();
        let section_customs: Vec<&BuildCustom> = payloads.iter().filter(|&&p| p >= standard_count).map(|&p| &customs[p - standard_count]).collect();
        let (minimum_w, maximum_w) = min_max(
            used.iter()
                .map(|&v| w_values[v])
                .chain(section_customs.iter().map(|c| c.minimum_w))
                .chain(section_customs.iter().map(|c| c.maximum_w)),
        );
        custom_records.sort_by_key(|c| c.primitive_index);
        sections.push(Section {
            tree_minimum: tree_min,
            tree_maximum: tree_max,
            tree_minimum_w: minimum_w,
            tree_maximum_w: maximum_w,
            base: frame.base,
            scale: frame.scale,
            packed_vertices,
            vertices: section_vertices,
            primitives: output_primitives,
            primitive_data_runs: compress_data_runs(&output_materials),
            tree_nodes,
            shared_vertex_indices: stream.clone(),
            leaf_index: 0,
            page: section_stream.page,
            section_flags: u8::from(!custom_records.is_empty()),
            custom_primitives: custom_records,
        });
    }

    let shared_vertices = layout
        .words
        .into_iter()
        .map(|w| w.ok_or_else(|| PrevisError::invalid("custom/shared vertex layout left unfilled stream entries")))
        .collect::<Result<Vec<u64>>>()?;
    let (master, master_root) = &partition.master;
    let master_tree_nodes = static_aabb5_nodes(master, *master_root)?;
    assign_leaf_indices(&mut sections, &master_tree_nodes)?;
    let (quad_words, quad_bits, interior_bits, key_count) = key_domains(&mut sections);
    let used: BTreeSet<usize> = sequences.iter().flatten().copied().collect();
    let (object_min_w, object_max_w) = min_max(
        used.iter().map(|&v| w_values[v]).chain(customs.iter().map(|c| c.minimum_w)).chain(customs.iter().map(|c| c.maximum_w)),
    );
    finish_mesh(CompressedMesh {
        object_aabb_min: object_min,
        object_aabb_max: object_max,
        object_aabb_min_w: object_min_w,
        object_aabb_max_w: object_max_w,
        quad_is_flat_words: quad_words,
        quad_is_flat_num_bits: quad_bits,
        triangle_is_interior_words: Vec::new(),
        triangle_is_interior_num_bits: interior_bits,
        sections,
        material_entries,
        shared_vertices,
        master_tree_nodes,
        key_count: Some(key_count),
        ..CompressedMesh::empty()
    })
}

/// Vertices referenced by standard primitives of more than one section.
fn shared_vertices(sequences: &[[usize; 4]], section_payloads: &[Vec<usize>], standard_count: usize) -> FxHashSet<usize> {
    let mut first_section: FxHashMap<usize, usize> = FxHashMap::default();
    let mut shared = FxHashSet::default();
    for (section, payloads) in section_payloads.iter().enumerate() {
        for &payload in payloads.iter().filter(|&&p| p < standard_count) {
            for vertex in sequences[payload] {
                if *first_section.entry(vertex).or_insert(section) != section {
                    shared.insert(vertex);
                }
            }
        }
    }
    shared
}

fn build_single_section(
    centered: &[V3],
    w_values: &[f32],
    sequences: &[[usize; 4]],
    materials: &[u16],
    tree: &Tree,
    root: usize,
    material_entries: Vec<(u32, u32)>,
) -> Result<CompressedMesh> {
    let sah_order = tree.leaf_payloads(root);
    let sah_materials: Vec<u16> = sah_order.iter().map(|&p| materials[p]).collect();
    let (local_to_output, output_to_local) = data_sort_order(&sah_materials);
    let output_order: Vec<usize> = output_to_local.iter().map(|&i| sah_order[i]).collect();
    let mut primitive_to_output = vec![0; sequences.len()];
    for (local, &primitive) in sah_order.iter().enumerate() {
        primitive_to_output[primitive] = local_to_output[local];
    }
    let output_sequences: Vec<[usize; 4]> = output_order.iter().map(|&p| sequences[p]).collect();
    let output_materials: Vec<u16> = output_order.iter().map(|&p| materials[p]).collect();

    let mut vertex_order = Vec::new();
    let mut seen = FxHashSet::default();
    for &primitive in &sah_order {
        for vertex in output_sequences[primitive_to_output[primitive]] {
            if seen.insert(vertex) {
                vertex_order.push(vertex);
            }
        }
    }
    let source_to_output: FxHashMap<usize, usize> = vertex_order.iter().enumerate().map(|(i, &v)| (v, i)).collect();
    if vertex_order.len() > 0xFF {
        return Err(PrevisError::unsupported("single-section collision exceeds 255 vertices"));
    }
    let primitives: Vec<[u8; 4]> = output_sequences.iter().map(|s| s.map(|v| source_to_output[&v] as u8)).collect();

    let (domain_min, domain_max) = (tree.nodes[root].min, tree.nodes[root].max);
    let scale = vertex_scale(domain_min, domain_max);
    let packed_vertices: Vec<u32> = vertex_order.iter().map(|&v| encode_vertex_11_11_10(domain_min, domain_max, centered[v])).collect();
    let decoded: Vec<V3> = packed_vertices.iter().map(|&p| decode_vertex_11_11_10(domain_min, scale, p)).collect();
    let quantized: FxHashMap<usize, V3> = vertex_order.iter().copied().zip(decoded.iter().copied()).collect();
    let leaf_bounds: Vec<Option<Bounds>> =
        sequences.iter().map(|s| Some(bounds_of(s.iter().map(|v| quantized[v]).collect::<Vec<_>>().iter()))).collect();
    let mut refit = tree.clone();
    refit_for_leaf_bounds(&mut refit, root, &leaf_bounds);
    let tree_nodes = static_aabb4_nodes(&refit, root, Some(primitive_to_output.as_slice()));

    let (minimum_w, maximum_w) = min_max(vertex_order.iter().map(|&v| w_values[v]));
    let mut section = Section {
        tree_minimum: domain_min,
        tree_maximum: domain_max,
        tree_minimum_w: minimum_w,
        tree_maximum_w: maximum_w,
        base: domain_min,
        scale,
        packed_vertices,
        vertices: decoded,
        primitives,
        primitive_data_runs: compress_data_runs(&output_materials),
        tree_nodes,
        ..Section::default()
    };
    let last = section.primitives[section.primitives.len() - 1];
    let interior_bits = section.primitives.len() * 2 - usize::from(last[2] == last[3]);
    let key_count = shape_key_count(std::slice::from_ref(&section));
    mark_bad_primitives(&mut section);
    let quad_words = flat_convex_quad_words(&section);
    let primitive_count = section.primitives.len() as u32;
    finish_mesh(CompressedMesh {
        object_aabb_min: domain_min,
        object_aabb_max: domain_max,
        object_aabb_min_w: minimum_w,
        object_aabb_max_w: maximum_w,
        quad_is_flat_words: quad_words,
        quad_is_flat_num_bits: primitive_count,
        triangle_is_interior_words: Vec::new(),
        triangle_is_interior_num_bits: interior_bits as u32,
        sections: vec![section],
        material_entries,
        key_count: Some(key_count),
        ..CompressedMesh::empty()
    })
}

// ---------------------------------------------------------------------------
// Packfile serialization
// ---------------------------------------------------------------------------

const CLASS_ENTRIES: [(u32, &str); 9] = [
    (0x33D4_2383, "hkClass"),
    (0xB0EF_A719, "hkClassMember"),
    (0x8A36_09CF, "hkClassEnum"),
    (0xCE6F_8A6C, "hkClassEnumItem"),
    (0xB857_718B, "hknpPhysicsSystemData"),
    (0x5F60_D536, "hknpCompressedMeshShape"),
    (0x7C57_4867, "hkRefCountedProperties"),
    (0xA3E4_7A9A, "hknpBSMaterialProperties"),
    (0xA2BD_FC59, "hknpCompressedMeshShapeData"),
];
const CONTENTS_VERSION: &[u8; 16] = b"hk_2014.1.0-r1\0\xff";
const BODY_PROPERTIES_32: [u8; 0x50] = hex_bytes(concat!(
    "00000000000000000000000000ff003f",
    "003fcd3e01024c3deeff7f7f0000803f",
    "0000803f000000000000000000000000",
    "a0400000000000000000000000000000",
    "00000000000000000000000000000000",
));
const BODY_PROPERTIES_64: [u8; 0x50] = hex_bytes(concat!(
    "00000000000000000000000000000000",
    "00ff003f003fcd3e01024c3deeff7f7f",
    "0000803f0000803f0000000000000000",
    "0000000000000000a040000000000000",
    "00000000000000000000000000000000",
));
const SIMD_LOW: [u8; 4] = [0xEE, 0xFF, 0x7F, 0x7F];
const SIMD_HIGH: [u8; 4] = [0xEE, 0xFF, 0x7F, 0xFF];

const fn hex_bytes<const N: usize>(text: &str) -> [u8; N] {
    const fn nibble(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => panic!("hex digit"),
        }
    }
    let bytes = text.as_bytes();
    assert!(bytes.len() == N * 2);
    let mut out = [0u8; N];
    let mut i = 0;
    while i < N {
        out[i] = nibble(bytes[2 * i]) << 4 | nibble(bytes[2 * i + 1]);
        i += 1;
    }
    out
}

/// Field offsets of the objects the combined shape uses, per pointer size.
struct Layout {
    pointer_size: usize,
    physics_system_size: usize,
    physics_arrays: [usize; 6],
    body_filter: usize,
    shape_size: usize,
    shape_empty_arrays: [usize; 2],
    shape_second_sentinel: usize,
    shape_quad: (usize, usize),
    shape_interior: (usize, usize),
    ref_key: usize,
    material_array: usize,
    material_entry_size: usize,
    material_entry_filter: usize,
    data_size: usize,
    data_arrays: [usize; 7],
}

const LAYOUT_32: Layout = Layout {
    pointer_size: 4,
    physics_system_size: 0x60,
    physics_arrays: [0x08, 0x14, 0x20, 0x2C, 0x38, 0x44],
    body_filter: 0x10,
    shape_size: 0x90,
    shape_empty_arrays: [0x38, 0x44],
    shape_second_sentinel: 0x50,
    shape_quad: (0x64, 0x70),
    shape_interior: (0x74, 0x80),
    ref_key: 0x14,
    material_array: 0x08,
    material_entry_size: 0x10,
    material_entry_filter: 0x08,
    data_size: 0xB0,
    // sections, primitives, shared indices, packed, shared vertices, runs, SIMD nodes
    data_arrays: [0x4C, 0x58, 0x64, 0x70, 0x7C, 0x88, 0xA4],
};

const LAYOUT_64: Layout = Layout {
    pointer_size: 8,
    physics_system_size: 0x80,
    physics_arrays: [0x10, 0x20, 0x30, 0x40, 0x50, 0x60],
    body_filter: 0x14,
    shape_size: 0xA0,
    shape_empty_arrays: [0x38, 0x48],
    shape_second_sentinel: 0x58,
    shape_quad: (0x68, 0x78),
    shape_interior: (0x80, 0x90),
    ref_key: 0x18,
    material_array: 0x10,
    material_entry_size: 0x18,
    material_entry_filter: 0x10,
    data_size: 0xC8,
    data_arrays: [0x50, 0x60, 0x70, 0x80, 0x90, 0xA0, 0xB8],
};

impl Layout {
    fn for_pointer_size(pointer_size: usize) -> Result<&'static Layout> {
        match pointer_size {
            4 => Ok(&LAYOUT_32),
            8 => Ok(&LAYOUT_64),
            other => Err(PrevisError::unsupported(format!("Havok pointer size {other}"))),
        }
    }

    fn put_array(&self, buffer: &mut [u8], offset: usize, count: usize) {
        let count = count as u32;
        put_u32(buffer, offset + self.pointer_size, count);
        put_u32(buffer, offset + self.pointer_size + 4, count | 0x8000_0000);
    }
}

fn put_u32(buffer: &mut [u8], offset: usize, value: u32) {
    buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_f32(buffer: &mut [u8], offset: usize, value: f32) {
    put_u32(buffer, offset, value.to_bits());
}

fn put_v3(buffer: &mut [u8], offset: usize, value: V3) {
    for (axis, v) in value.iter().enumerate() {
        put_f32(buffer, offset + axis * 4, *v);
    }
}

fn align16(data: &mut Vec<u8>) {
    data.resize(data.len().next_multiple_of(16), 0);
}

fn classnames() -> (Vec<u8>, FxHashMap<&'static str, u32>) {
    let mut data = Vec::new();
    let mut offsets = FxHashMap::default();
    for (signature, name) in CLASS_ENTRIES {
        data.extend_from_slice(&signature.to_le_bytes());
        data.push(9);
        offsets.insert(name, data.len() as u32);
        data.extend_from_slice(name.as_bytes());
        data.push(0);
    }
    data.resize(data.len().next_multiple_of(16), 0xFF);
    (data, offsets)
}

fn section_header(name: &str, start: usize, local: usize, global: usize, virtual_: usize, exports: usize) -> [u8; 0x40] {
    let mut header = [0u8; 0x40];
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[0x13] = 0xFF;
    put_u32(&mut header, 0x14, start as u32);
    for (offset, absolute) in (0x18..0x30).step_by(4).zip([local, global, virtual_, exports, exports, exports]) {
        put_u32(&mut header, offset, (absolute - start) as u32);
    }
    header[0x30..].fill(0xFF);
    header
}

fn file_header(pointer_size: usize, class_name_offset: u32) -> [u8; 0x40] {
    let mut header = [0u8; 0x40];
    header[..8].copy_from_slice(&HKX_MAGIC);
    put_u32(&mut header, 0x0C, 11);
    header[0x10..0x14].copy_from_slice(&[pointer_size as u8, 1, 0, 1]);
    put_u32(&mut header, 0x14, 3);
    put_u32(&mut header, 0x18, 2);
    put_u32(&mut header, 0x24, class_name_offset);
    header[0x28..0x38].copy_from_slice(CONTENTS_VERSION);
    put_u32(&mut header, 0x3C, 21);
    header
}

fn simd_tree_nodes() -> Vec<u8> {
    let mut data = Vec::with_capacity(224);
    for _ in 0..2 {
        for index in 0..6 {
            for _ in 0..4 {
                data.extend_from_slice(if index % 2 == 0 { &SIMD_LOW } else { &SIMD_HIGH });
            }
        }
        data.extend_from_slice(&[0u8; 16]);
    }
    data
}

fn fixup_table<const N: usize>(rows: &[[usize; N]]) -> Vec<u8> {
    let mut data: Vec<u8> = rows.iter().flatten().flat_map(|&v| (v as u32).to_le_bytes()).collect();
    data.resize(data.len().next_multiple_of(16), 0xFF);
    data
}

fn shape_key_bits(max_key: u32) -> u32 {
    (32 - (max_key + 1).leading_zeros()).max(1)
}

/// Keys of the mesh: one per triangle, two per quad.
fn shape_key_count(sections: &[Section]) -> u32 {
    sections.iter().flat_map(|s| &s.primitives).map(|p| if p[2] == p[3] { 1 } else { 2 }).sum()
}

/// CK 1.11.240 points a section at the first earlier copy of its run sequence
/// instead of appending it again. The copy must end before the last run already
/// in the table; a match that ends exactly there is never taken.
fn data_run_table(sections: &[Section]) -> (Vec<(u16, u8, u8)>, Vec<usize>) {
    let mut table: Vec<(u16, u8, u8)> = Vec::new();
    let mut positions_by_run: FxHashMap<(u16, u8, u8), Vec<usize>> = FxHashMap::default();
    let mut first_runs = Vec::with_capacity(sections.len());
    for section in sections {
        let runs = &section.primitive_data_runs;
        let end = table.len();
        let reused = runs.first().and_then(|first| {
            positions_by_run.get(first)?.iter().copied().take_while(|&p| p + runs.len() < end).find(|&p| table[p..p + runs.len()] == runs[..])
        });
        first_runs.push(reused.unwrap_or(end));
        if reused.is_none() {
            for (offset, &run) in runs.iter().enumerate() {
                positions_by_run.entry(run).or_default().push(end + offset);
            }
            table.extend_from_slice(runs);
        }
    }
    (table, first_runs)
}

/// One static body with the compressed mesh, in CK's object and fixup order.
pub fn serialize_packfile(mesh: &CompressedMesh, pointer_size: usize) -> Result<Vec<u8>> {
    let layout = Layout::for_pointer_size(pointer_size)?;
    if mesh.sections.is_empty() {
        return Err(PrevisError::invalid("compressed mesh has no sections"));
    }
    if mesh.sections.iter().any(|s| {
        s.primitives.len() > 127 || s.packed_vertices.len() + s.shared_vertex_indices.len() > 255 || s.shared_vertex_indices.len() > 255
    }) {
        return Err(PrevisError::invalid("a section exceeds CK's byte-sized limits"));
    }
    let primitive_count: usize = mesh.sections.iter().map(|s| s.primitives.len()).sum();
    if primitive_count == 0 {
        return Err(PrevisError::invalid("compressed mesh has no primitives"));
    }
    let packed_vertex_count: usize = mesh.sections.iter().map(|s| s.packed_vertices.len()).sum();
    let shared_index_count: usize = mesh.sections.iter().map(|s| s.shared_vertex_indices.len()).sum();
    let (run_table, first_runs) = data_run_table(&mesh.sections);
    let single_master = [[0u8; 5]];
    let master_tree_nodes: &[[u8; 5]] = if mesh.master_tree_nodes.is_empty() {
        if mesh.sections.len() != 1 {
            return Err(PrevisError::invalid("multi-section mesh is missing its master tree"));
        }
        &single_master
    } else {
        &mesh.master_tree_nodes
    };
    if master_tree_nodes.len() != mesh.sections.len() * 2 - 1 {
        return Err(PrevisError::invalid("master-tree node count does not match the section count"));
    }
    let max_key = mesh
        .triangle_is_interior_num_bits
        .checked_sub(1)
        .ok_or_else(|| PrevisError::invalid("triangle-is-interior bit domain is empty"))?;
    let key_bits = shape_key_bits(max_key);
    if mesh.quad_is_flat_words.len() != (mesh.quad_is_flat_num_bits as usize).div_ceil(32)
        || mesh.triangle_is_interior_words.len() != (mesh.triangle_is_interior_num_bits as usize).div_ceil(32)
    {
        return Err(PrevisError::invalid("shape bitfield storage does not match its bit domain"));
    }
    let default_material = [(mesh.collision_filter_info, DEFAULT_MATERIAL_CRC)];
    let material_entries: &[(u32, u32)] = if mesh.material_entries.is_empty() { &default_material } else { &mesh.material_entries };

    let mut data: Vec<u8> = Vec::new();
    let mut local: Vec<[usize; 2]> = Vec::new();
    let mut global: Vec<[usize; 3]> = Vec::new();

    let physics_system = data.len();
    data.resize(layout.physics_system_size, 0);
    for (&offset, count) in layout.physics_arrays.iter().zip([1, 0, 0, 1, 0, 1]) {
        layout.put_array(&mut data, physics_system + offset, count);
    }

    let body_properties = data.len();
    data.extend_from_slice(if pointer_size == 8 { &BODY_PROPERTIES_64 } else { &BODY_PROPERTIES_32 });
    let body_cinfo = data.len();
    let mut cinfo = [0u8; 0x60];
    let ids = layout.pointer_size;
    put_u32(&mut cinfo, ids, 0x7FFF_FFFF);
    put_u32(&mut cinfo, ids + 4, 0x7FFF_FFFF);
    put_u32(&mut cinfo, ids + 8, 0xFF);
    put_u32(&mut cinfo, layout.body_filter, mesh.collision_filter_info);
    match &mesh.body_cinfo_tail {
        Some(tail) => cinfo[0x20..].copy_from_slice(tail),
        None => put_f32(&mut cinfo, 0x4C, 1.0),
    }
    data.extend_from_slice(&cinfo);
    let shape_entry = data.len();
    data.extend_from_slice(&[0u8; 0x10]);

    let shape = data.len();
    let mut shape_bytes = vec![0u8; layout.shape_size];
    shape_bytes[0x10..0x14].copy_from_slice(&[4, 2, key_bits as u8, 2]);
    put_f32(&mut shape_bytes, 0x14, mesh.convex_radius);
    put_u32(&mut shape_bytes, 0x18, mesh.shape_user_data);
    put_u32(&mut shape_bytes, 0x30, 0xFFFF_FFFF);
    for offset in layout.shape_empty_arrays {
        layout.put_array(&mut shape_bytes, offset, 0);
    }
    put_u32(&mut shape_bytes, layout.shape_second_sentinel, 0xFFFF_FFFF);
    layout.put_array(&mut shape_bytes, layout.shape_quad.0, mesh.quad_is_flat_words.len());
    put_u32(&mut shape_bytes, layout.shape_quad.1, mesh.quad_is_flat_num_bits);
    layout.put_array(&mut shape_bytes, layout.shape_interior.0, mesh.triangle_is_interior_words.len());
    put_u32(&mut shape_bytes, layout.shape_interior.1, mesh.triangle_is_interior_num_bits);
    data.extend_from_slice(&shape_bytes);
    let quad_words = data.len();
    data.extend(mesh.quad_is_flat_words.iter().flat_map(|w| w.to_le_bytes()));
    align16(&mut data);
    let interior_words = data.len();
    data.extend(mesh.triangle_is_interior_words.iter().flat_map(|w| w.to_le_bytes()));
    align16(&mut data);

    let ref_properties = data.len();
    let mut properties = [0u8; 0x20];
    layout.put_array(&mut properties, 0, 1);
    put_u32(&mut properties, layout.ref_key, 0xF601);
    data.extend_from_slice(&properties);

    let bs_material = data.len();
    let mut material = vec![0u8; 0x20 + material_entries.len() * layout.material_entry_size];
    layout.put_array(&mut material, layout.material_array, material_entries.len());
    for (index, &(filter, crc)) in material_entries.iter().enumerate() {
        let entry = 0x20 + index * layout.material_entry_size + layout.material_entry_filter;
        put_u32(&mut material, entry, filter);
        put_u32(&mut material, entry + 4, crc);
    }
    data.extend_from_slice(&material);
    align16(&mut data);

    let shape_data = data.len();
    let mut header = vec![0u8; layout.data_size];
    layout.put_array(&mut header, 0x10, master_tree_nodes.len());
    put_v3(&mut header, 0x20, mesh.object_aabb_min);
    put_f32(&mut header, 0x2C, mesh.object_aabb_min_w);
    put_v3(&mut header, 0x30, mesh.object_aabb_max);
    put_f32(&mut header, 0x3C, mesh.object_aabb_max_w);
    put_u32(&mut header, 0x40, mesh.key_count.unwrap_or_else(|| shape_key_count(&mesh.sections)));
    put_u32(&mut header, 0x44, key_bits);
    put_u32(&mut header, 0x48, max_key);
    let counts = [
        mesh.sections.len(),
        primitive_count,
        shared_index_count,
        packed_vertex_count,
        mesh.shared_vertices.len(),
        run_table.len(),
        2,
    ];
    for (&offset, count) in layout.data_arrays.iter().zip(counts) {
        layout.put_array(&mut header, offset, count);
    }
    data.extend_from_slice(&header);
    align16(&mut data);

    let master_tree = data.len();
    data.extend(master_tree_nodes.iter().flatten());
    align16(&mut data);
    let sections = data.len();
    let (mut first_packed, mut first_shared, mut first_primitive) = (0usize, 0usize, 0usize);
    for (section, first_run) in mesh.sections.iter().zip(first_runs) {
        let mut bytes = [0u8; 0x60];
        layout.put_array(&mut bytes, 0, section.tree_nodes.len());
        put_v3(&mut bytes, 0x10, section.tree_minimum);
        put_f32(&mut bytes, 0x1C, section.tree_minimum_w);
        put_v3(&mut bytes, 0x20, section.tree_maximum);
        put_f32(&mut bytes, 0x2C, section.tree_maximum_w);
        put_v3(&mut bytes, 0x30, section.base);
        put_v3(&mut bytes, 0x3C, section.scale);
        put_u32(&mut bytes, 0x48, first_packed as u32);
        put_u32(&mut bytes, 0x4C, (first_shared << 8 | section.packed_vertices.len()) as u32);
        put_u32(&mut bytes, 0x50, (first_primitive << 8 | section.primitives.len()) as u32);
        put_u32(&mut bytes, 0x54, (first_run << 8 | section.primitive_data_runs.len()) as u32);
        bytes[0x58] = section.packed_vertices.len() as u8;
        bytes[0x59] = section.shared_vertex_indices.len() as u8;
        bytes[0x5A..0x5C].copy_from_slice(&section.leaf_index.to_le_bytes());
        bytes[0x5C] = section.page;
        bytes[0x5D] = section.section_flags;
        data.extend_from_slice(&bytes);
        first_packed += section.packed_vertices.len();
        first_shared += section.shared_vertex_indices.len();
        first_primitive += section.primitives.len();
    }
    let mut section_trees = Vec::with_capacity(mesh.sections.len());
    for section in &mesh.sections {
        section_trees.push(data.len());
        data.extend(section.tree_nodes.iter().flatten());
        align16(&mut data);
    }
    let primitives = data.len();
    data.extend(mesh.sections.iter().flat_map(|s| s.primitives.iter().flatten()));
    align16(&mut data);
    let shared_indices = data.len();
    data.extend(mesh.sections.iter().flat_map(|s| &s.shared_vertex_indices).flat_map(|i| i.to_le_bytes()));
    align16(&mut data);
    let packed_vertices = data.len();
    data.extend(mesh.sections.iter().flat_map(|s| &s.packed_vertices).flat_map(|v| v.to_le_bytes()));
    align16(&mut data);
    let shared_vertices = data.len();
    data.extend(mesh.shared_vertices.iter().flat_map(|v| v.to_le_bytes()));
    align16(&mut data);
    let data_runs = data.len();
    for &(value, index, count) in &run_table {
        data.extend_from_slice(&value.to_le_bytes());
        data.extend_from_slice(&[index, count]);
    }
    align16(&mut data);
    let simd_nodes = data.len();
    data.extend(simd_tree_nodes());

    let [sections_field, primitives_field, shared_indices_field, packed_field, shared_vertices_field, runs_field, simd_field] =
        layout.data_arrays;
    local.extend([
        [physics_system + layout.physics_arrays[0], body_properties],
        [physics_system + layout.physics_arrays[3], body_cinfo],
        [physics_system + layout.physics_arrays[5], shape_entry],
        [shape + layout.shape_quad.0, quad_words],
        [shape + layout.shape_interior.0, interior_words],
        [ref_properties, ref_properties + 0x10],
        [bs_material + layout.material_array, bs_material + 0x20],
        [shape_data + 0x10, master_tree],
        [shape_data + sections_field, sections],
    ]);
    local.extend(section_trees.iter().enumerate().map(|(index, &tree)| [sections + index * 0x60, tree]));
    local.push([shape_data + primitives_field, primitives]);
    if shared_index_count > 0 {
        local.push([shape_data + shared_indices_field, shared_indices]);
    }
    if packed_vertex_count > 0 {
        local.push([shape_data + packed_field, packed_vertices]);
    }
    if !mesh.shared_vertices.is_empty() {
        local.push([shape_data + shared_vertices_field, shared_vertices]);
    }
    local.push([shape_data + runs_field, data_runs]);
    local.push([shape_data + simd_field, simd_nodes]);
    global.extend([
        [body_cinfo, 2, shape],
        [shape_entry, 2, shape],
        [shape + 0x20, 2, ref_properties],
        [shape + 0x60, 2, shape_data],
        [ref_properties + 0x10, 2, bs_material],
    ]);
    let (classnames, name_offsets) = classnames();
    let virtual_: Vec<[usize; 3]> = [
        (physics_system, "hknpPhysicsSystemData"),
        (shape, "hknpCompressedMeshShape"),
        (ref_properties, "hkRefCountedProperties"),
        (bs_material, "hknpBSMaterialProperties"),
        (shape_data, "hknpCompressedMeshShapeData"),
    ]
    .map(|(offset, name)| [offset, 0, name_offsets[name] as usize])
    .to_vec();

    let local_start = data.len();
    data.extend(fixup_table(&local));
    let global_start = data.len();
    data.extend(fixup_table(&global));
    let virtual_start = data.len();
    data.extend(fixup_table(&virtual_));
    let exports = data.len();

    let classnames_start = 0x100;
    let data_start = classnames_start + classnames.len();
    let mut output = Vec::with_capacity(data_start + data.len());
    output.extend_from_slice(&file_header(pointer_size, name_offsets["hknpPhysicsSystemData"]));
    output.extend_from_slice(&section_header("__classnames__", classnames_start, data_start, data_start, data_start, data_start));
    output.extend_from_slice(&section_header("__types__", data_start, data_start, data_start, data_start, data_start));
    output.extend_from_slice(&section_header(
        "__data__",
        data_start,
        data_start + local_start,
        data_start + global_start,
        data_start + virtual_start,
        data_start + exports,
    ));
    output.extend_from_slice(&classnames);
    output.extend_from_slice(&data);
    Ok(output)
}

/// Clears what CK leaves undefined so two packfiles compare on content:
/// AABB W lanes (SIMD residue the tree codecs never read) and the unused
/// high bits of the last bitfield words.
pub fn canonicalize_packfile(blob: &[u8]) -> Result<Vec<u8>> {
    let packfile = Packfile::parse(blob)?;
    let layout = Layout::for_pointer_size(packfile.pointer_size)?;
    let shape = packfile.object("hknpCompressedMeshShape").ok_or_else(|| PrevisError::invalid("packfile has no compressed mesh"))?;
    let shape_data =
        packfile.object("hknpCompressedMeshShapeData").ok_or_else(|| PrevisError::invalid("packfile has no compressed mesh data"))?;
    let base = packfile.data;
    let mut out = blob.to_vec();
    for offset in [0x2C, 0x3C] {
        put_f32(&mut out, base + shape_data + offset, 0.0);
    }
    let (sections, section_count) = packfile.array(shape_data + layout.data_arrays[0])?;
    for index in 0..section_count {
        for offset in [0x1C, 0x2C] {
            put_f32(&mut out, base + sections + index * 0x60 + offset, 0.0);
        }
    }
    for (field, bits_field) in [layout.shape_quad, layout.shape_interior] {
        let (words, count) = packfile.array(shape + field)?;
        let bits = packfile.u32(shape + bits_field)? as usize;
        for index in 0..count {
            let remaining = bits.saturating_sub(index * 32);
            if remaining >= 32 {
                continue;
            }
            let offset = base + words + index * 4;
            let word = u32::from_le_bytes(out[offset..offset + 4].try_into().unwrap());
            put_u32(&mut out, offset, word & ((1u32 << remaining) - 1));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Physics.NIF
// ---------------------------------------------------------------------------

/// The four-block NIF CK writes around the merged packfile: an unrotated
/// `NiNode` root at `translation` carrying BSX 2 and one collision object.
pub fn physics_nif(packfile: &[u8], root_name: &str, translation: V3) -> Vec<u8> {
    const BLOCK_TYPES: [&str; 4] = ["NiNode", "BSXFlags", "bhkNPCollisionObject", "bhkPhysicsSystem"];
    let strings = [root_name, "BSX"];
    let mut out = Vec::with_capacity(packfile.len() + 512);
    let u32_le = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
    out.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    out.extend_from_slice(&0x1402_0007u32.to_le_bytes());
    out.push(1);
    u32_le(&mut out, 12);
    u32_le(&mut out, BLOCK_TYPES.len() as u32);
    u32_le(&mut out, 130);
    out.extend_from_slice(&PHYSICS_EXPORT_INFO);
    out.extend_from_slice(&(BLOCK_TYPES.len() as u16).to_le_bytes());
    for name in BLOCK_TYPES {
        u32_le(&mut out, name.len() as u32);
        out.extend_from_slice(name.as_bytes());
    }
    for index in 0..BLOCK_TYPES.len() as u16 {
        out.extend_from_slice(&index.to_le_bytes());
    }
    for size in [80, 8, 14, 4 + packfile.len()] {
        u32_le(&mut out, size as u32);
    }
    u32_le(&mut out, strings.len() as u32);
    u32_le(&mut out, strings.iter().map(|s| s.len()).max().unwrap_or(0) as u32);
    for s in strings {
        u32_le(&mut out, s.len() as u32);
        out.extend_from_slice(s.as_bytes());
    }
    u32_le(&mut out, 0);

    // NiNode: name, one extra data (BSX), no controller, flags, transform, collision, no children.
    for v in [0u32, 1, 1, u32::MAX, 14] {
        u32_le(&mut out, v);
    }
    for v in translation.into_iter().chain([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0]) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    u32_le(&mut out, 2);
    u32_le(&mut out, 0);
    // BSXFlags "BSX" = 2.
    u32_le(&mut out, 1);
    u32_le(&mut out, 2);
    // bhkNPCollisionObject: target root, flags, data, body id.
    u32_le(&mut out, 0);
    out.extend_from_slice(&0x80u16.to_le_bytes());
    u32_le(&mut out, 3);
    u32_le(&mut out, 0);
    u32_le(&mut out, packfile.len() as u32);
    out.extend_from_slice(packfile);
    u32_le(&mut out, 1);
    u32_le(&mut out, 0);
    out
}


#[cfg(test)]
mod tests {
    //! Expected hashes come from the Python prototype
    //! (`tools/re/ck_havok_packfile32.py`) run on the same fixtures; the
    //! two-glass hash is CK's own output.

    use super::*;
    use crate::f32ops::euler_rotation;

    fn sha256_upper(data: &[u8]) -> String {
        crate::sha256_hex(data).to_uppercase()
    }

    fn section(vertices: Vec<V3>, primitives: Vec<[u8; 4]>, runs: Vec<(u16, u8, u8)>) -> Section {
        Section {
            vertices,
            primitives,
            primitive_data_runs: runs,
            tree_nodes: vec![[0, 0, 0, 0]],
            ..Section::default()
        }
    }

    fn mesh(sections: Vec<Section>, bounds: Bounds, quad: (Vec<u32>, u32), interior_bits: u32) -> CompressedMesh {
        CompressedMesh {
            object_aabb_min: bounds.0,
            object_aabb_max: bounds.1,
            quad_is_flat_words: quad.0,
            quad_is_flat_num_bits: quad.1,
            triangle_is_interior_words: vec![0; (interior_bits as usize).div_ceil(32)],
            triangle_is_interior_num_bits: interior_bits,
            sections,
            ..CompressedMesh::empty()
        }
    }

    fn placed(source: &CompressedMesh, euler: V3, scale: f32, translation: V3) -> PhysicsInstance {
        PhysicsInstance {
            source: Arc::new(PrecombineSource::from_mesh(source.clone())),
            transform: Transform {
                rotation: euler_rotation(euler),
                translation,
                scale,
            },
            reference_scale: scale,
            component_scale: 1.0,
        }
    }

    fn at(source: &CompressedMesh, translation: V3) -> PhysicsInstance {
        placed(source, [0.0; 3], 1.0, translation)
    }

    fn canonical_hash(built: &CombinedPhysics) -> String {
        sha256_upper(&canonicalize_packfile(&serialize_packfile(&built.mesh, 4).unwrap()).unwrap())
    }

    fn custom_box(primitive_index: usize, vertices: Vec<V3>) -> CustomPrimitive {
        CustomPrimitive {
            primitive_index,
            tree_node_index: 0,
            header: (vertices.len() as u16) << 8 | 0x52,
            custom_type: 2,
            compression_mode: 1,
            vertex_count: vertices.len(),
            first_shared_vertex: 0,
            tags: vec![0x3C24],
            convex_radius: 0.010009765625,
            vertices,
        }
    }

    fn fan(with_custom: bool) -> CompressedMesh {
        let mut vertices = vec![[0.0f32; 3]];
        vertices.extend((0..129).map(|i| {
            let angle = i as f64 * std::f64::consts::TAU / 129.0;
            [(angle.cos() * 10.0) as f32, (angle.sin() * 10.0) as f32, 0.0]
        }));
        let mut primitives: Vec<[u8; 4]> = (0..128u8).map(|i| [0, i + 1, i + 2, i + 2]).collect();
        let mut runs: Vec<(u16, u8, u8)> = (0..128u8).map(|i| (u16::from(i & 1), i, 1)).collect();
        if !with_custom {
            let bounds = ([-10.0, -10.0, 0.0], [10.0, 10.0, 0.0]);
            let mut fan = mesh(vec![section(vertices, primitives, runs)], bounds, (vec![0; 4], 128), 256);
            fan.material_entries = vec![(1, 10), (1, 20)];
            return fan;
        }
        primitives.push([0, 1, 1, 1]);
        runs.push((2, 128, 1));
        let mut with_box = section(vertices, primitives, runs);
        with_box.custom_primitives =
            vec![custom_box(128, vec![[11.0, 0.0, 0.0], [12.0, 0.0, 0.0], [11.0, 1.0, 0.0], [11.0, 0.0, 1.0]])];
        let mut fan = mesh(vec![with_box], ([-10.0, -10.0, -0.1], [12.1, 10.0, 1.1]), (vec![0; 5], 129), 258);
        fan.material_entries = vec![(1, 10), (1, 20), (1, 30)];
        fan
    }

    fn mixed() -> Arc<CompressedMesh> {
        let mut triangle_and_box = section(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2, 2], [0, 1, 1, 1]],
            vec![(0, 0, 1), (1, 1, 1)],
        );
        triangle_and_box.custom_primitives =
            vec![custom_box(1, vec![[2.0, 0.0, 0.0], [3.0, 0.0, 0.0], [2.0, 1.0, 0.0], [2.0, 0.0, 1.0]])];
        let r = 0.010009765625;
        let mut mixed = mesh(vec![triangle_and_box], ([0.0, -r, -r], [3.0 + r, 1.0 + r, 1.0 + r]), (vec![0], 2), 3);
        mixed.material_entries = vec![(10, 0xAAAA_AAAA), (20, 0xBBBB_BBBB)];
        Arc::new(mixed)
    }

    #[test]
    fn shape_key_bits_reserve_the_invalid_key_sentinel() {
        assert_eq!(shape_key_bits(0), 1);
        assert_eq!(shape_key_bits(3), 3);
        assert_eq!(shape_key_bits(4), 3);
    }

    #[test]
    fn two_glass_panes_match_the_ck_oracle() {
        let (x, y, z) = (0.014287499710917473f32, 1.8287999629974365f32, 3.657599925994873f32);
        let vertices = vec![[-x, y, z], [-x, y, 0.0], [-x, -y, 0.0], [-x, -y, z], [x, y, z], [x, -y, z], [x, -y, 0.0], [x, y, 0.0]];
        let mut glass = section(vertices, vec![[4, 5, 6, 7], [0, 1, 2, 3]], vec![(0, 0, 2)]);
        glass.tree_nodes = vec![[0, 0, 0, 3], [15, 0, 0, 2], [240, 0, 0, 0]];
        let mut source = mesh(vec![glass], ([-x, -y, 0.0], [x, y, z]), (vec![3], 2), 4);
        source.material_entries = vec![(1, 3739830338)];
        let source = Arc::new(source);

        let built = build_combined_physics(&[at(&source, [256.0, 0.0, 0.0]), at(&source, [0.0; 3])]).unwrap();
        assert_eq!((built.candidate_count, built.selected_pair_count), (4, 4));
        assert_eq!(built.root_translation, [127.99999237060547, 0.0, 127.99999237060547]);
        assert_eq!(
            built.mesh.sections[0].tree_nodes,
            vec![[0, 0, 0, 5], [14, 0, 0, 3], [62, 0, 0, 4], [15, 0, 0, 6], [224, 0, 0, 3], [240, 0, 0, 0], [227, 0, 0, 2]]
        );
        assert_eq!(canonical_hash(&built), "0389266639E49A6B22FB43E45CAE3AB1D3D03ADC2E01008CEE7FFE806F613EEB");
    }

    #[test]
    fn single_section_builds_match_the_prototype() {
        let square = Arc::new(mesh(
            vec![section(
                vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]],
                vec![[0, 1, 2, 3]],
                vec![(0, 0, 1)],
            )],
            ([-1.0, -1.0, 0.0], [1.0, 1.0, 0.0]),
            (vec![1], 1),
            2,
        ));
        let built = build_combined_physics(&[at(&square, [0.0; 3])]).unwrap();
        assert_eq!((built.candidate_count, built.selected_pair_count), (1, 1));
        assert_eq!(built.mesh.sections[0].primitives, vec![[0, 1, 2, 3]]);
        assert_eq!(canonical_hash(&built), "BB04C2ED9AC482DEF1405178D877CFA72BC45E4C46140D5FCFABDB4E1A6F370D");

        let cross = Arc::new(mesh(
            vec![
                section(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]], vec![[0, 1, 2, 2]], vec![(0, 0, 1)]),
                section(vec![[0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]], vec![[0, 1, 2, 2]], vec![(0, 0, 1)]),
            ],
            ([0.0; 3], [1.0, 1.0, 0.0]),
            (vec![0; 5], 129),
            258,
        ));
        let built = build_combined_physics(&[at(&cross, [0.0; 3])]).unwrap();
        assert_eq!((built.candidate_count, built.selected_pair_count), (1, 1));
        assert_eq!(canonical_hash(&built), "F95AB2F85BFCDDFF64C9C0AE1C70A8314904E02E972CC0397A0A0C68079AC81D");

        let unpaired = Arc::new(mesh(
            vec![section(
                vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0], [3.0, 0.0, 0.0], [4.0, 0.0, 0.0], [3.0, 1.0, 0.0]],
                vec![[0, 1, 2, 3], [4, 5, 6, 6]],
                vec![(0, 0, 2)],
            )],
            ([-1.0, -1.0, 0.0], [4.0, 1.0, 0.0]),
            (vec![1], 2),
            3,
        ));
        let built = build_combined_physics(&[at(&unpaired, [0.0; 3])]).unwrap();
        assert_eq!(built.mesh.sections[0].primitives.iter().filter(|p| p[2] == p[3]).count(), 1);
        assert_eq!(canonical_hash(&built), "AB16DD09746520E5EBAB7491C78815C1D693123DDA0A563E3D24C9DFFDD7B4A6");
    }

    #[test]
    fn material_remap_follows_first_use_and_blocks_cross_material_pairs() {
        let mut remap = mesh(
            vec![section(
                vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
                vec![[0, 1, 2, 2], [0, 2, 3, 3]],
                vec![(1, 0, 1), (0, 1, 1)],
            )],
            ([0.0; 3], [1.0, 1.0, 0.0]),
            (vec![0], 2),
            3,
        );
        remap.material_entries = vec![(10, 0xAAAA_AAAA), (20, 0xBBBB_BBBB)];
        let built = build_combined_physics(&[at(&Arc::new(remap), [0.0; 3])]).unwrap();
        assert_eq!((built.candidate_count, built.selected_pair_count), (0, 0));
        assert_eq!(built.mesh.material_entries, vec![(20, 0xBBBB_BBBB), (10, 0xAAAA_AAAA)]);
        assert_eq!(canonical_hash(&built), "94201CC656BAD5487A8368D612244C6F97082303E723EEF0BBE0F60FA62B1186");
    }

    #[test]
    fn partitioned_builds_match_the_prototype() {
        let fan = Arc::new(fan(false));
        let built = build_combined_physics(&[at(&fan, [0.0; 3])]).unwrap();
        assert_eq!(built.mesh.sections.len(), 2);
        assert!(!built.mesh.shared_vertices.is_empty());
        assert_eq!(canonical_hash(&built), "303D74C9163B00F01C2D1AB692CB5A7AA23626FE876D6DBC8E3A591023BCCC16");

        let shared = Arc::new(mesh(
            vec![section(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], vec![[0, 1, 2, 2]; 128], vec![(0, 0, 128)])],
            ([0.0; 3], [1.0, 1.0, 0.0]),
            (vec![0; 4], 128),
            256,
        ));
        let built = build_combined_physics(&[at(&shared, [0.0; 3])]).unwrap();
        assert!(built.mesh.sections.iter().all(|s| s.packed_vertices.is_empty() && s.scale == [f32::NEG_INFINITY; 3]));
        assert_eq!(canonical_hash(&built), "3FDB4D356E9A653C3E1CE05B8AB5A204595C80C404F0C3BDEB10EDEBA964DC6F");
    }

    #[test]
    fn rotated_and_scaled_instances_match_the_prototype() {
        let fan = Arc::new(fan(false));
        let built = build_combined_physics(&[
            placed(&fan, [0.3, -0.4, 0.7], 1.25, [100.0, -50.0, 20.0]),
            placed(&fan, [0.0, 0.0, 1.2], 0.75, [-300.0, 40.0, 0.0]),
        ])
        .unwrap();
        assert_eq!(built.root_translation.map(f32::to_bits), [1120403452, 3259498487, 1101004796]);
        assert_eq!(built.mesh.sections.len(), 4);
        assert_eq!(canonical_hash(&built), "450FC66654B94FB02316B73836CFE13C8D71886BAD782B560D1B474235D05EF2");

        let built = build_combined_physics(&[placed(&mixed(), [0.1, 0.2, -1.3], 2.0, [10.0, 20.0, 30.0])]).unwrap();
        assert_eq!(built.root_translation.map(f32::to_bits), [3245101162, 1131887997, 1116986017]);
        // Unlike the prototype, the custom box scales in CK's surface mode.
        assert_eq!(canonical_hash(&built), "3C9418864B0C7E8D7F444ADB8EB12AF8F57225A88565B91AD891A39F81CCE217");
    }

    #[test]
    fn custom_primitive_builds_match_the_prototype() {
        let built = build_combined_physics(&[at(&mixed(), [0.0; 3])]).unwrap();
        let custom = &built.mesh.sections[0].custom_primitives[0];
        assert_eq!((custom.custom_type, custom.compression_mode, custom.vertex_count), (2, 1, 4));
        assert_eq!(canonical_hash(&built), "1972092D95908A1451ED83A43D718FEF6E5A30F5495344ECE31B6D67AAF24B83");

        let built = build_combined_physics(&[at(&Arc::new(fan(true)), [0.0; 3])]).unwrap();
        assert_eq!(built.mesh.sections.len(), 2);
        assert_eq!(built.mesh.sections.iter().map(|s| s.custom_primitives.len()).sum::<usize>(), 1);
        // The prototype flags every section of a mixed build; CK 1.11.240 flags only
        // the sections holding a custom primitive, so this is the prototype hash
        // with that one byte corrected.
        assert_eq!(built.mesh.sections.iter().map(|s| s.section_flags).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(canonical_hash(&built), "F7B3E2CC9CF0460C3BAFBDD80E7243BA6345E03825648AD86BA103DECA548007");

        let sign = |bit: bool| if bit { 1.0 } else { -1.0 };
        let corners = (0..8).map(|i| [sign(i & 1 != 0), sign(i & 2 != 0), sign(i & 4 != 0)]).collect();
        let mut only_box = section(Vec::new(), vec![[0, 0, 0, 0]], vec![(0, 0, 1)]);
        only_box.custom_primitives = vec![custom_box(0, corners)];
        let r = 1.010009765625;
        let mut source = mesh(vec![only_box], ([-r; 3], [r; 3]), (vec![0], 1), 1);
        source.material_entries = vec![(1, 0x2A1A_6690)];
        let built = build_combined_physics(&[at(&Arc::new(source), [0.0; 3])]).unwrap();
        assert_eq!(built.mesh.sections[0].base.map(f32::to_bits), [0x7F7F_FFEE; 3]);
        assert_eq!(canonical_hash(&built), "D007B04BE17BC73094B85310B9021BB1DF75B0C870B4CAEF9017DBE1D75E2D0B");
    }

    #[test]
    fn custom_vertices_are_scaled_before_rotation() {
        let rotation = physics_rotation(&euler_rotation([0.3, -0.4, 0.7]));
        let vertex = [4.356688805273734e-07, -1.7780253887176514, -0.6351689696311951];
        let transformed = transform_custom_vertices(&[vertex], &rotation, [1.25; 3]);
        assert_eq!(transformed[0].map(f32::to_bits), [3218104512, 3218501702, 1051474764]);
    }

    #[test]
    fn serializer_matches_the_prototype_byte_for_byte() {
        let corners = vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]];
        let mut quad = section(corners, vec![[0, 1, 2, 3]], vec![(0x1234, 0, 1)]);
        quad.tree_minimum = [-1.0, -1.0, 0.0];
        quad.tree_maximum = [1.0, 1.0, 0.0];
        quad.tree_minimum_w = -0.25;
        quad.tree_maximum_w = 0.5;
        quad.base = [-1.0, -1.0, 0.0];
        quad.scale = [(2.0 / 2047.0f64) as f32, (2.0 / 2047.0f64) as f32, (1.0 / 1023.0f64) as f32];
        quad.packed_vertices = vec![0, 0x7FF, 0x7FF | 0x7FF << 11, 0x7FF << 11];
        quad.section_flags = 1;
        let mut tail = [0u8; 0x40];
        tail[0x2C..0x30].copy_from_slice(&[0xFF, 0xFF, 0x7F, 0x3F]);
        let mut source = mesh(vec![quad], ([-1.0, -1.0, 0.0], [1.0, 1.0, 0.0]), (vec![1], 1), 2);
        source.object_aabb_min_w = -0.25;
        source.object_aabb_max_w = 0.5;
        source.convex_radius = 0.02;
        source.shape_user_data = 0xE538_F7DB;
        source.collision_filter_info = 26;
        source.body_cinfo_tail = Some(tail);
        source.material_entries = vec![(26, 0x55DF_AB90), (26, 0x4CCA_CC3B)];

        let blob = serialize_packfile(&source, 4).unwrap();
        assert_eq!(blob.len(), 1856);
        assert_eq!(sha256_upper(&blob), "5894CDD5464DB4218176CC1DE2B270FB6F7D5F9E66484E1431F8D1D07BCAC1A2");
        let canonical = canonicalize_packfile(&blob).unwrap();
        assert_eq!(sha256_upper(&canonical), "CC2B6D0224229FBBCF50BA587C09B06B497B36A84B3E8E67B222ED345640E397");
        let parsed = crate::havok_mesh::parse_compressed_mesh(&blob).unwrap();
        assert_eq!(serialize_packfile(&parsed, 4).unwrap(), blob);
        let wide = serialize_packfile(&parsed, 8).unwrap();
        assert_eq!(serialize_packfile(&crate::havok_mesh::parse_compressed_mesh(&wide).unwrap(), 8).unwrap(), wide);
    }

    /// Rebuilds a CK 1.11.240 Physics.NIF from its own parsed mesh: the
    /// container must match byte for byte and the pointer-8 packfile after
    /// clearing the W lanes CK leaves undefined.
    #[test]
    #[ignore = "set CK_PHYSICS_NIF to a CK-generated Meshes\\PreCombined\\<plugin>\\%08X_Physics.NIF"]
    fn ck_physics_nif_round_trips() {
        let path = std::path::PathBuf::from(std::env::var_os("CK_PHYSICS_NIF").expect("CK_PHYSICS_NIF is not set"));
        let ck = std::fs::read(&path).unwrap();
        let root_name = path.file_stem().unwrap().to_str().unwrap();
        let start = ck.windows(8).position(|w| w == HKX_MAGIC).unwrap();
        let size = u32::from_le_bytes(ck[start - 4..start].try_into().unwrap()) as usize;
        let packfile = &ck[start..start + size];

        let translation_at = physics_nif(packfile, root_name, [0.0; 3]).len() - size - 4 - 14 - 8 - 80 - 8 + 20;
        let translation: V3 = std::array::from_fn(|a| f32::from_le_bytes(ck[translation_at + a * 4..][..4].try_into().unwrap()));
        assert_eq!(physics_nif(packfile, root_name, translation), ck);

        let mesh = crate::havok_mesh::parse_compressed_mesh(packfile).unwrap();
        let rebuilt = serialize_packfile(&mesh, 8).unwrap();
        assert_eq!(rebuilt.len(), packfile.len());
        let (rebuilt, expected) = (canonicalize_packfile(&rebuilt).unwrap(), canonicalize_packfile(packfile).unwrap());
        let first_difference = rebuilt.iter().zip(&expected).position(|(a, b)| a != b);
        assert_eq!(first_difference, None);
    }

    #[test]
    fn physics_nif_wraps_the_packfile_like_ck() {
        let nif = physics_nif(&[0xAB; 6], &physics_root_name(0x0012_94F4), [1.0, 2.0, 3.0]);
        assert!(nif.starts_with(b"Gamebryo File Format, Version 20.2.0.7\n"));
        let strings = nif.windows(16).position(|w| w == b"001294F4_Physics").unwrap();
        assert_eq!(&nif[strings - 12..strings - 4], &[2, 0, 0, 0, 16, 0, 0, 0]);
        assert_eq!(&nif[nif.len() - 18..], &[6, 0, 0, 0, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(physics_file_name(0x0112_94F4), "001294F4_Physics.NIF");
    }
}
