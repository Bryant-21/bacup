//! FO4 fixes for the converted FO76 Pip-Boy 2000 worn meshes.
//!
//! First person (`PipBoy2000_1.nif`): FO76 skins every body piece to the NIF's own
//! animated nodes (`PipboyRoot`, `BoneToggleRadio`, ...). FO4 binds the bones of a
//! skinned biped object by name against the actor skeleton
//! (`BipedAnim::AttachToSkeleton` -> `BSFlattenedBoneTree::GetBoneByName`) and binds
//! every bone it can't find to the skeleton root, so the whole Pip-Boy is drawn at
//! the 1st-person skeleton origin instead of on the arm. Vanilla `Pipboy.nif` is
//! rigid: every piece is a child of the node that animates it. Every vertex of the
//! FO76 pieces is weighted 1.0 to a single bone, so each piece splits exactly into
//! rigid shapes parented to their bones, with the skin-to-bone transform as the
//! shape's local transform.
//!
//! Third person (`PipBoy2000.nif`): `PlayerCharacter::ShowHidePipboyFX` walks the
//! worn Pip-Boy and un-hides the geometry whose name starts with the INI string
//! `strPipboyScreenGlowEffect:GamePlay` (`ScreenGlowEffect01`). FO76's 3rd-person
//! glow is named `ScreenGlowEffect`, sits under a hidden node and carries a
//! stand-alone `NiVisController`, so the screen never lights up.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use nif_core_native::model::{NifFile, NifValue};

const FIRST_PERSON: &[&str] = &[
    "meshes",
    "pipboy2000",
    "characterassets",
    "pipboy2000_1.nif",
];
const THIRD_PERSON: &[&str] = &["meshes", "pipboy2000", "characterassets", "pipboy2000.nif"];

const FO4_SCREEN_GLOW: &str = "ScreenGlowEffect01";
const FO76_SCREEN_GLOW: &str = "ScreenGlowEffect";

const VERTEX_ATTRIBUTE_SHIFT: u32 = 44;
const VERTEX_ATTRIBUTE_SKINNED: u64 = 0x40;
const SKIN_WORDS: u64 = 3;
const SLSF1_SKINNED: i64 = 1 << 1;
const APP_CULLED: i64 = 1;

#[derive(Debug, Default)]
pub struct Pipboy2000NifSummary {
    pub rigid_shapes: usize,
    pub screen_glow_fixed: bool,
    pub failures: Vec<String>,
}

pub fn fix_pipboy2000_nifs(data_root: &Path) -> Pipboy2000NifSummary {
    let mut summary = Pipboy2000NifSummary::default();
    if let Some(path) = find_ci(data_root, FIRST_PERSON) {
        match edit(&path, rigidify_single_bone_skins) {
            Ok(count) => summary.rigid_shapes = count,
            Err(error) => summary.failures.push(error),
        }
    }
    if let Some(path) = find_ci(data_root, THIRD_PERSON) {
        match edit(&path, |nif| usize::from(enable_fo4_screen_glow(nif))) {
            Ok(count) => summary.screen_glow_fixed = count > 0,
            Err(error) => summary.failures.push(error),
        }
    }
    summary
}

fn edit(path: &Path, apply: impl FnOnce(&mut NifFile) -> usize) -> Result<usize, String> {
    let mut nif = NifFile::load(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let changed = apply(&mut nif);
    if changed > 0 {
        nif.save(Some(path.to_path_buf()))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(changed)
}

fn find_ci(root: &Path, components: &[&str]) -> Option<PathBuf> {
    let mut current = root.to_path_buf();
    for component in components {
        let entry = std::fs::read_dir(&current).ok()?.flatten().find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(component)
        })?;
        current = entry.path();
    }
    current.is_file().then_some(current)
}

/// Rename FO76's 3rd-person screen glow to the name FO4 toggles, make its node
/// visible and drop the node's `NiVisController`. The shape itself stays hidden;
/// the engine un-hides it when the Pip-Boy light turns on.
pub(crate) fn enable_fo4_screen_glow(nif: &mut NifFile) -> bool {
    if nif.blocks.iter().any(|block| {
        is_geometry(&block.type_name)
            && name_of(block.get_field("Name"))
                .is_some_and(|name| name.starts_with(FO4_SCREEN_GLOW))
    }) {
        return false;
    }
    let Some(node_id) = nif.blocks.iter().position(|block| {
        block.type_name == "NiNode"
            && name_of(block.get_field("Name")).is_some_and(|name| name == FO76_SCREEN_GLOW)
    }) else {
        return false;
    };
    let shapes: Vec<usize> = children(nif, node_id)
        .into_iter()
        .filter(|&child| is_geometry(&nif.blocks[child].type_name))
        .collect();
    if shapes.is_empty() {
        return false;
    }

    for &shape in &shapes {
        let renamed = name_of(nif.blocks[shape].get_field("Name"))
            .map(|name| name.replacen(FO76_SCREEN_GLOW, FO4_SCREEN_GLOW, 1))
            .filter(|name| name.starts_with(FO4_SCREEN_GLOW))
            .unwrap_or_else(|| format!("{FO4_SCREEN_GLOW}:0"));
        let flags = int_of(nif.blocks[shape].get_field("Flags"));
        let block = &mut nif.blocks[shape];
        block.set_field("Name", NifValue::String(renamed));
        block.set_field("Flags", NifValue::UInt((flags | APP_CULLED) as u64));
    }

    let mut dropped = Vec::new();
    if let Some(controller) = ref_of(nif.blocks[node_id].get_field("Controller"))
        && nif.blocks[controller].type_name == "NiVisController"
        && ref_of(nif.blocks[controller].get_field("Next Controller")).is_none()
    {
        dropped.push(controller);
        if let Some(interpolator) = ref_of(nif.blocks[controller].get_field("Interpolator")) {
            dropped.push(interpolator);
            if let Some(data) = ref_of(nif.blocks[interpolator].get_field("Data")) {
                dropped.push(data);
            }
        }
    }
    let flags = int_of(nif.blocks[node_id].get_field("Flags"));
    let node = &mut nif.blocks[node_id];
    node.set_field("Name", NifValue::String(FO4_SCREEN_GLOW.to_string()));
    node.set_field("Flags", NifValue::UInt((flags & !APP_CULLED) as u64));
    if !dropped.is_empty() {
        node.set_field("Controller", NifValue::Ref(-1));
        nif.remove_blocks(&dropped);
    }
    true
}

/// Replace every `BSTriShape` skinned rigidly (each vertex fully weighted to one
/// bone, no triangle spanning two bones) with one unskinned shape per bone,
/// parented to that bone. Returns the number of rigid shapes produced.
pub(crate) fn rigidify_single_bone_skins(nif: &mut NifFile) -> usize {
    let candidates: Vec<usize> = nif
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| block.type_name == "BSTriShape")
        .filter(|(_, block)| ref_of(block.get_field("Skin")).is_some())
        .map(|(id, _)| id)
        .collect();

    let mut produced = 0;
    let mut dead_blocks = Vec::new();
    for shape in candidates {
        let Some(plan) = plan_rigid_split(nif, shape) else {
            continue;
        };
        produced += plan.groups.len();
        dead_blocks.push(plan.skin);
        dead_blocks.push(plan.bone_data);
        apply_rigid_split(nif, shape, plan);
    }
    if !dead_blocks.is_empty() {
        nif.remove_blocks(&dead_blocks);
    }
    produced
}

struct RigidGroup {
    bone_node: usize,
    bone_slot: usize,
    vertices: Vec<usize>,
    triangles: Vec<[usize; 3]>,
}

struct RigidPlan {
    skin: usize,
    bone_data: usize,
    rigid_desc: u64,
    groups: Vec<RigidGroup>,
}

fn plan_rigid_split(nif: &NifFile, shape: usize) -> Option<RigidPlan> {
    let block = &nif.blocks[shape];
    let skin = ref_of(block.get_field("Skin"))?;
    if nif.blocks[skin].type_name != "BSSkin::Instance" {
        return None;
    }
    let bone_data = ref_of(nif.blocks[skin].get_field("Data"))?;
    let bones: Vec<usize> = array_of(nif.blocks[skin].get_field("Bones"))
        .iter()
        .map(|value| ref_of(Some(value)))
        .collect::<Option<_>>()?;
    let bone_list_len = array_of(nif.blocks[bone_data].get_field("Bone List")).len();
    if bones.is_empty() || bone_list_len != bones.len() {
        return None;
    }
    let rigid_desc = unskinned_vertex_desc(int_of(block.get_field("Vertex Desc")) as u64)?;

    let vertex_bone: Vec<usize> = array_of(block.get_field("Vertex Data"))
        .iter()
        .map(single_bone)
        .collect::<Option<_>>()?;
    if vertex_bone.iter().any(|&slot| slot >= bones.len()) {
        return None;
    }

    let mut by_slot: BTreeMap<usize, Vec<[usize; 3]>> = BTreeMap::new();
    for triangle in array_of(block.get_field("Triangles")) {
        let corners = triangle_of(triangle)?;
        if corners.iter().any(|&v| v >= vertex_bone.len()) {
            return None;
        }
        let slot = vertex_bone[corners[0]];
        if corners.iter().any(|&v| vertex_bone[v] != slot) {
            return None;
        }
        by_slot.entry(slot).or_default().push(corners);
    }
    if by_slot.is_empty() {
        return None;
    }

    let mut groups: Vec<RigidGroup> = by_slot
        .into_iter()
        .map(|(slot, triangles)| {
            let mut vertices: Vec<usize> = triangles.iter().flatten().copied().collect();
            vertices.sort_unstable();
            vertices.dedup();
            RigidGroup {
                bone_node: bones[slot],
                bone_slot: slot,
                vertices,
                triangles,
            }
        })
        .collect();
    groups.sort_by(|a, b| b.vertices.len().cmp(&a.vertices.len()));
    Some(RigidPlan {
        skin,
        bone_data,
        rigid_desc,
        groups,
    })
}

fn apply_rigid_split(nif: &mut NifFile, shape: usize, plan: RigidPlan) {
    let base_name = name_of(nif.blocks[shape].get_field("Name")).unwrap_or_default();
    let source_vertices = array_of(nif.blocks[shape].get_field("Vertex Data")).to_vec();
    let bone_list = array_of(nif.blocks[plan.bone_data].get_field("Bone List")).to_vec();
    let template = nif.blocks[shape].fields.clone();
    detach(nif, shape);

    for (index, group) in plan.groups.iter().enumerate() {
        let target = if index == 0 {
            shape
        } else {
            let mut fields = template.clone();
            if let Some(shader) = ref_of(fields.get("Shader Property")) {
                fields.insert(
                    "Shader Property".into(),
                    NifValue::Ref(clone_shader(nif, shader)),
                );
            }
            if let Some(alpha) = ref_of(fields.get("Alpha Property")) {
                fields.insert(
                    "Alpha Property".into(),
                    NifValue::Ref(clone_block(nif, alpha)),
                );
            }
            nif.add_block("BSTriShape", Some(fields))
        };
        let bone_name = name_of(nif.blocks[group.bone_node].get_field("Name")).unwrap_or_default();
        let name = if index == 0 {
            base_name.clone()
        } else {
            format!("{base_name}_{bone_name}")
        };

        let remap: HashMap<usize, usize> = group
            .vertices
            .iter()
            .enumerate()
            .map(|(new, &old)| (old, new))
            .collect();
        let vertices: Vec<NifValue> = group
            .vertices
            .iter()
            .map(|&old| strip_skin_weights(&source_vertices[old]))
            .collect();
        let triangles: Vec<NifValue> = group
            .triangles
            .iter()
            .map(|corners| {
                NifValue::Struct(IndexMap::from([
                    ("v1".to_string(), NifValue::UInt(remap[&corners[0]] as u64)),
                    ("v2".to_string(), NifValue::UInt(remap[&corners[1]] as u64)),
                    ("v3".to_string(), NifValue::UInt(remap[&corners[2]] as u64)),
                ]))
            })
            .collect();
        let bound = bounding_sphere(&vertices);
        let skin_to_bone = &bone_list[group.bone_slot];
        let stride = (plan.rigid_desc & 0xF) * 4;

        let block = &mut nif.blocks[target];
        block.original_bytes = None;
        block.set_field("Name", NifValue::String(name));
        block.set_field("Skin", NifValue::Ref(-1));
        for field in ["Rotation", "Translation", "Scale"] {
            if let Some(value) = struct_member(skin_to_bone, field) {
                block.set_field(field, value.clone());
            }
        }
        block.set_field("Bounding Sphere", bound);
        block.set_field("Vertex Desc", NifValue::UInt(plan.rigid_desc));
        block.set_field("Num Vertices", NifValue::UInt(vertices.len() as u64));
        block.set_field("Num Triangles", NifValue::UInt(triangles.len() as u64));
        block.set_field(
            "Data Size",
            NifValue::UInt(stride * vertices.len() as u64 + 6 * triangles.len() as u64),
        );
        block.set_field("Vertex Data", NifValue::Array(vertices));
        block.set_field("Triangles", NifValue::Array(triangles));
        if let Some(shader) = ref_of(block.get_field("Shader Property")) {
            clear_skinned_shader_flag(nif, shader);
        }
        attach(nif, group.bone_node, target);
    }
}

/// The skin block must be the last vertex attribute so dropping it only shortens
/// the stride; every FO76 skinned BSTriShape seen so far is laid out that way.
fn unskinned_vertex_desc(desc: u64) -> Option<u64> {
    let attributes = desc >> VERTEX_ATTRIBUTE_SHIFT;
    let words = desc & 0xF;
    let skin_offset = (desc >> 28) & 0xF;
    if attributes & VERTEX_ATTRIBUTE_SKINNED == 0 || skin_offset + SKIN_WORDS != words {
        return None;
    }
    let cleared =
        desc & !(0xF << 28) & !0xF & !(VERTEX_ATTRIBUTE_SKINNED << VERTEX_ATTRIBUTE_SHIFT);
    Some(cleared | (words - SKIN_WORDS))
}

fn single_bone(vertex: &NifValue) -> Option<usize> {
    let weights = array_of(struct_member(vertex, "Bone Weights"));
    let indices = array_of(struct_member(vertex, "Bone Indices"));
    let mut bone = None;
    for (weight, index) in weights.iter().zip(indices) {
        let weight = float_of(weight);
        if weight <= 0.001 {
            continue;
        }
        if weight < 0.999 || bone.is_some() {
            return None;
        }
        bone = Some(int_of(Some(index)) as usize);
    }
    bone
}

fn strip_skin_weights(vertex: &NifValue) -> NifValue {
    match vertex {
        NifValue::Struct(fields) => NifValue::Struct(
            fields
                .iter()
                .filter(|(key, _)| key.as_str() != "Bone Weights" && key.as_str() != "Bone Indices")
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn bounding_sphere(vertices: &[NifValue]) -> NifValue {
    let points: Vec<[f64; 3]> = vertices
        .iter()
        .filter_map(|vertex| vec3_of(struct_member(vertex, "Vertex")?))
        .collect();
    let (mut min, mut max) = ([f64::MAX; 3], [f64::MIN; 3]);
    for point in &points {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    let center = if points.is_empty() {
        [0.0; 3]
    } else {
        [0, 1, 2].map(|axis| (min[axis] + max[axis]) / 2.0)
    };
    let radius = points
        .iter()
        .map(|point| {
            (0..3)
                .map(|axis| (point[axis] - center[axis]).powi(2))
                .sum::<f64>()
                .sqrt()
        })
        .fold(0.0, f64::max);
    NifValue::Struct(IndexMap::from([
        (
            "Center".to_string(),
            NifValue::Struct(IndexMap::from([
                ("x".to_string(), NifValue::Float(center[0])),
                ("y".to_string(), NifValue::Float(center[1])),
                ("z".to_string(), NifValue::Float(center[2])),
            ])),
        ),
        ("Radius".to_string(), NifValue::Float(radius)),
    ]))
}

fn clear_skinned_shader_flag(nif: &mut NifFile, shader: usize) {
    let block = &mut nif.blocks[shader];
    if let Some(value) = block.get_field("Shader Flags 1") {
        let flags = value.as_i64();
        if flags & SLSF1_SKINNED != 0 {
            block.set_field(
                "Shader Flags 1",
                NifValue::UInt((flags & !SLSF1_SKINNED) as u64),
            );
        }
    }
}

fn clone_shader(nif: &mut NifFile, shader: usize) -> i32 {
    let clone = clone_block(nif, shader) as usize;
    if let Some(textures) = ref_of(nif.blocks[clone].get_field("Texture Set")) {
        let textures = clone_block(nif, textures);
        nif.blocks[clone].set_field("Texture Set", NifValue::Ref(textures));
    }
    clone as i32
}

fn clone_block(nif: &mut NifFile, id: usize) -> i32 {
    let type_name = nif.blocks[id].type_name.clone();
    let fields = nif.blocks[id].fields.clone();
    nif.add_block(type_name, Some(fields)) as i32
}

fn detach(nif: &mut NifFile, child: usize) {
    for parent in 0..nif.blocks.len() {
        let kids = children(nif, parent);
        if kids.contains(&child) {
            set_children(
                nif,
                parent,
                kids.into_iter().filter(|&kid| kid != child).collect(),
            );
        }
    }
}

fn attach(nif: &mut NifFile, parent: usize, child: usize) {
    let mut kids = children(nif, parent);
    if !kids.contains(&child) {
        kids.push(child);
    }
    set_children(nif, parent, kids);
}

fn children(nif: &NifFile, parent: usize) -> Vec<usize> {
    array_of(nif.blocks[parent].get_field("Children"))
        .iter()
        .filter_map(|value| ref_of(Some(value)))
        .collect()
}

fn set_children(nif: &mut NifFile, parent: usize, kids: Vec<usize>) {
    let block = &mut nif.blocks[parent];
    block.set_field("Num Children", NifValue::UInt(kids.len() as u64));
    block.set_field(
        "Children",
        NifValue::Array(
            kids.into_iter()
                .map(|kid| NifValue::Ref(kid as i32))
                .collect(),
        ),
    );
}

fn is_geometry(type_name: &str) -> bool {
    matches!(
        type_name,
        "BSTriShape" | "BSSubIndexTriShape" | "BSDynamicTriShape"
    )
}

fn struct_member<'a>(value: &'a NifValue, member: &str) -> Option<&'a NifValue> {
    match value {
        NifValue::Struct(fields) => fields.get(member),
        _ => None,
    }
}

fn array_of(value: Option<&NifValue>) -> &[NifValue] {
    match value {
        Some(NifValue::Array(values)) => values,
        _ => &[],
    }
}

fn ref_of(value: Option<&NifValue>) -> Option<usize> {
    match value {
        Some(NifValue::Ref(id)) if *id >= 0 => Some(*id as usize),
        _ => None,
    }
}

fn name_of(value: Option<&NifValue>) -> Option<String> {
    match value {
        Some(NifValue::String(name)) => Some(name.trim_end_matches('\0').to_string()),
        _ => None,
    }
}

fn int_of(value: Option<&NifValue>) -> i64 {
    value.map(NifValue::as_i64).unwrap_or(0)
}

fn float_of(value: &NifValue) -> f64 {
    match value {
        NifValue::Float(f) => *f,
        other => other.as_i64() as f64,
    }
}

fn vec3_of(value: &NifValue) -> Option<[f64; 3]> {
    match value {
        NifValue::Vec3(v) => Some(v.map(f64::from)),
        NifValue::Struct(_) => Some([
            float_of(struct_member(value, "x")?),
            float_of(struct_member(value, "y")?),
            float_of(struct_member(value, "z")?),
        ]),
        _ => None,
    }
}

fn triangle_of(value: &NifValue) -> Option<[usize; 3]> {
    Some([
        int_of(Some(struct_member(value, "v1")?)) as usize,
        int_of(Some(struct_member(value, "v2")?)) as usize,
        int_of(Some(struct_member(value, "v3")?)) as usize,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec3(x: f64, y: f64, z: f64) -> NifValue {
        NifValue::Struct(IndexMap::from([
            ("x".to_string(), NifValue::Float(x)),
            ("y".to_string(), NifValue::Float(y)),
            ("z".to_string(), NifValue::Float(z)),
        ]))
    }

    fn node(nif: &mut NifFile, name: &str, flags: u64, kids: &[usize]) -> usize {
        nif.add_block(
            "NiNode",
            Some(IndexMap::from([
                ("Name".to_string(), NifValue::String(name.to_string())),
                ("Flags".to_string(), NifValue::UInt(flags)),
                (
                    "Num Children".to_string(),
                    NifValue::UInt(kids.len() as u64),
                ),
                (
                    "Children".to_string(),
                    NifValue::Array(kids.iter().map(|&k| NifValue::Ref(k as i32)).collect()),
                ),
            ])),
        )
    }

    fn skinned_vertex(position: [f64; 3], bone: u64) -> NifValue {
        NifValue::Struct(IndexMap::from([
            (
                "Vertex".to_string(),
                vec3(position[0], position[1], position[2]),
            ),
            (
                "Bone Weights".to_string(),
                NifValue::Array(vec![
                    NifValue::Float(1.0),
                    NifValue::Float(0.0),
                    NifValue::Float(0.0),
                    NifValue::Float(0.0),
                ]),
            ),
            (
                "Bone Indices".to_string(),
                NifValue::Array(vec![
                    NifValue::UInt(bone),
                    NifValue::UInt(0),
                    NifValue::UInt(0),
                    NifValue::UInt(0),
                ]),
            ),
        ]))
    }

    fn triangle(a: u64, b: u64, c: u64) -> NifValue {
        NifValue::Struct(IndexMap::from([
            ("v1".to_string(), NifValue::UInt(a)),
            ("v2".to_string(), NifValue::UInt(b)),
            ("v3".to_string(), NifValue::UInt(c)),
        ]))
    }

    fn bone_entry(translation: [f64; 3]) -> NifValue {
        let identity = [
            ("m11", 1.0),
            ("m21", 0.0),
            ("m31", 0.0),
            ("m12", 0.0),
            ("m22", 1.0),
            ("m32", 0.0),
            ("m13", 0.0),
            ("m23", 0.0),
            ("m33", 1.0),
        ]
        .map(|(member, value)| (member.to_string(), NifValue::Float(value)));
        NifValue::Struct(IndexMap::from([
            (
                "Bounding Sphere".to_string(),
                NifValue::Struct(IndexMap::from([
                    ("Center".to_string(), vec3(0.0, 0.0, 0.0)),
                    ("Radius".to_string(), NifValue::Float(1.0)),
                ])),
            ),
            (
                "Rotation".to_string(),
                NifValue::Struct(IndexMap::from(identity)),
            ),
            (
                "Translation".to_string(),
                vec3(translation[0], translation[1], translation[2]),
            ),
            ("Scale".to_string(), NifValue::Float(1.0)),
        ]))
    }

    const FO76_SKINNED_DESC: u64 = 0x5b00050430208;
    const FO4_RIGID_DESC: u64 = 0x1b00000430205;
    // Position + skin only, so the round-trip fixtures need no UV/normal data.
    const POSITION_SKINNED_DESC: u64 = (0x41 << 44) | (2 << 28) | 5;
    const POSITION_RIGID_DESC: u64 = (0x01 << 44) | 2;

    /// Root -> [PipboyRoot -> [Knob], Body(skinned to PipboyRoot + Knob)].
    fn fo76_style_first_person() -> (NifFile, usize, usize) {
        let mut nif = NifFile::new("fo4");
        let knob = node(&mut nif, "BoneKnobRadio", 14, &[]);
        let pipboy_root = node(&mut nif, "PipboyRoot", 14, &[knob]);
        let texture_set = nif.add_block("BSShaderTextureSet", None);
        let shader = nif.add_block(
            "BSLightingShaderProperty",
            Some(IndexMap::from([
                (
                    "Name".to_string(),
                    NifValue::String("Materials\\Body.bgsm".into()),
                ),
                ("Shader Flags 1".to_string(), NifValue::UInt(0x3)),
                ("Texture Set".to_string(), NifValue::Ref(texture_set as i32)),
            ])),
        );
        let bone_data = nif.add_block(
            "BSSkin::BoneData",
            Some(IndexMap::from([
                ("Num Bones".to_string(), NifValue::UInt(2)),
                (
                    "Bone List".to_string(),
                    NifValue::Array(vec![bone_entry([0.0; 3]), bone_entry([-5.0, 0.0, 0.0])]),
                ),
            ])),
        );
        let skin = nif.add_block(
            "BSSkin::Instance",
            Some(IndexMap::from([
                ("Data".to_string(), NifValue::Ref(bone_data as i32)),
                ("Num Bones".to_string(), NifValue::UInt(2)),
                (
                    "Bones".to_string(),
                    NifValue::Array(vec![
                        NifValue::Ref(pipboy_root as i32),
                        NifValue::Ref(knob as i32),
                    ]),
                ),
            ])),
        );
        let body = nif.add_block(
            "BSTriShape",
            Some(IndexMap::from([
                ("Name".to_string(), NifValue::String("Pipboy2000:0".into())),
                ("Flags".to_string(), NifValue::UInt(14)),
                ("Skin".to_string(), NifValue::Ref(skin as i32)),
                ("Shader Property".to_string(), NifValue::Ref(shader as i32)),
                (
                    "Vertex Desc".to_string(),
                    NifValue::UInt(POSITION_SKINNED_DESC),
                ),
                ("Num Vertices".to_string(), NifValue::UInt(7)),
                ("Num Triangles".to_string(), NifValue::UInt(3)),
                (
                    "Vertex Data".to_string(),
                    NifValue::Array(vec![
                        skinned_vertex([0.0, 0.0, 0.0], 0),
                        skinned_vertex([2.0, 0.0, 0.0], 0),
                        skinned_vertex([0.0, 2.0, 0.0], 0),
                        skinned_vertex([0.0, 0.0, 2.0], 0),
                        skinned_vertex([5.0, 0.0, 0.0], 1),
                        skinned_vertex([6.0, 0.0, 0.0], 1),
                        skinned_vertex([5.0, 1.0, 0.0], 1),
                    ]),
                ),
                (
                    "Triangles".to_string(),
                    NifValue::Array(vec![
                        triangle(0, 1, 2),
                        triangle(0, 2, 3),
                        triangle(4, 5, 6),
                    ]),
                ),
            ])),
        );
        let root = node(&mut nif, "PipboyRoot_NIF_ONLY", 14, &[pipboy_root, body]);
        nif.header.footer_roots = vec![root as i32];
        (nif, root, body)
    }

    fn find(nif: &NifFile, name: &str) -> usize {
        nif.blocks
            .iter()
            .position(|block| name_of(block.get_field("Name")).as_deref() == Some(name))
            .unwrap_or_else(|| panic!("no block named {name}"))
    }

    #[test]
    fn vertex_desc_drops_trailing_skin_block() {
        assert_eq!(
            unskinned_vertex_desc(FO76_SKINNED_DESC),
            Some(FO4_RIGID_DESC)
        );
        assert_eq!(unskinned_vertex_desc(FO4_RIGID_DESC), None, "not skinned");
        assert_eq!(
            unskinned_vertex_desc(POSITION_SKINNED_DESC),
            Some(POSITION_RIGID_DESC)
        );
    }

    #[test]
    fn single_bone_skin_becomes_rigid_shapes_under_their_bones() {
        let (mut nif, _, _) = fo76_style_first_person();
        assert_eq!(rigidify_single_bone_skins(&mut nif), 2);
        let bytes = nif.to_bytes().expect("rigid nif serializes");
        let nif = NifFile::from_bytes(&bytes, None).expect("rigid nif reloads");

        assert!(
            !nif.blocks.iter().any(|b| b.type_name.starts_with("BSSkin")),
            "skin blocks removed"
        );
        let root = nif.header.footer_roots[0] as usize;
        let pipboy_root = find(&nif, "PipboyRoot");
        let knob = find(&nif, "BoneKnobRadio");
        let body = find(&nif, "Pipboy2000:0");
        let knob_piece = find(&nif, "Pipboy2000:0_BoneKnobRadio");
        assert_eq!(
            children(&nif, root),
            vec![pipboy_root],
            "no geometry left on the root"
        );
        assert!(children(&nif, pipboy_root).contains(&body));
        assert_eq!(children(&nif, knob), vec![knob_piece]);

        for (shape, vertex_count, triangle_count) in [(body, 4, 2), (knob_piece, 3, 1)] {
            let block = &nif.blocks[shape];
            assert_eq!(int_of(block.get_field("Skin")), -1);
            assert_eq!(
                int_of(block.get_field("Vertex Desc")) as u64,
                POSITION_RIGID_DESC
            );
            assert_eq!(array_of(block.get_field("Vertex Data")).len(), vertex_count);
            assert_eq!(array_of(block.get_field("Triangles")).len(), triangle_count);
            let shader = ref_of(block.get_field("Shader Property")).unwrap();
            assert_eq!(
                int_of(nif.blocks[shader].get_field("Shader Flags 1")) & SLSF1_SKINNED,
                0
            );
            let radius = struct_member(block.get_field("Bounding Sphere").unwrap(), "Radius")
                .map(float_of)
                .unwrap();
            assert!(radius > 0.0, "rigid geometry needs a real bound");
        }
        let knob_translation = vec3_of(nif.blocks[knob_piece].get_field("Translation").unwrap());
        assert_eq!(
            knob_translation,
            Some([-5.0, 0.0, 0.0]),
            "skin-to-bone becomes the local transform"
        );
        let body_shader = ref_of(nif.blocks[body].get_field("Shader Property"));
        let knob_shader = ref_of(nif.blocks[knob_piece].get_field("Shader Property"));
        assert_ne!(body_shader, knob_shader, "each piece owns its shader");
    }

    #[test]
    fn blended_or_spanning_skins_are_left_alone() {
        let (mut nif, _, body) = fo76_style_first_person();
        let mut triangles = array_of(nif.blocks[body].get_field("Triangles")).to_vec();
        triangles.push(triangle(0, 4, 5));
        nif.blocks[body].set_field("Triangles", NifValue::Array(triangles));
        assert_eq!(rigidify_single_bone_skins(&mut nif), 0);
        assert!(ref_of(nif.blocks[body].get_field("Skin")).is_some());
    }

    fn fo76_style_third_person() -> NifFile {
        let mut nif = NifFile::new("fo4");
        let data = nif.add_block("NiBoolData", None);
        let interpolator = nif.add_block(
            "NiBoolInterpolator",
            Some(IndexMap::from([(
                "Data".to_string(),
                NifValue::Ref(data as i32),
            )])),
        );
        let controller = nif.add_block(
            "NiVisController",
            Some(IndexMap::from([
                (
                    "Interpolator".to_string(),
                    NifValue::Ref(interpolator as i32),
                ),
                ("Next Controller".to_string(), NifValue::Ref(-1)),
            ])),
        );
        let shape = nif.add_block(
            "BSTriShape",
            Some(IndexMap::from([
                (
                    "Name".to_string(),
                    NifValue::String("ScreenGlowEffect:0".into()),
                ),
                ("Flags".to_string(), NifValue::UInt(15)),
            ])),
        );
        let glow = node(&mut nif, "ScreenGlowEffect", 15, &[shape]);
        nif.blocks[glow].set_field("Controller", NifValue::Ref(controller as i32));
        nif.blocks[controller].set_field("Target", NifValue::Ref(glow as i32));
        let screen = nif.add_block(
            "BSTriShape",
            Some(IndexMap::from([(
                "Name".to_string(),
                NifValue::String("Screen:0".into()),
            )])),
        );
        let root = node(
            &mut nif,
            "BASE meshes\\Pipboy2000\\CharacterAssets\\PipBoy2000.nif",
            14,
            &[glow, screen],
        );
        nif.header.footer_roots = vec![root as i32];
        nif
    }

    #[test]
    fn third_person_glow_gets_the_name_fo4_toggles() {
        let mut nif = fo76_style_third_person();
        assert!(enable_fo4_screen_glow(&mut nif));
        let nif = NifFile::from_bytes(&nif.to_bytes().unwrap(), None).unwrap();

        let glow = find(&nif, "ScreenGlowEffect01");
        let shape = find(&nif, "ScreenGlowEffect01:0");
        assert_eq!(
            int_of(nif.blocks[glow].get_field("Flags")) & APP_CULLED,
            0,
            "node visible"
        );
        assert_eq!(
            int_of(nif.blocks[shape].get_field("Flags")) & APP_CULLED,
            1,
            "glow off until the light"
        );
        assert_eq!(int_of(nif.blocks[glow].get_field("Controller")), -1);
        assert!(!nif.blocks.iter().any(|b| b.type_name == "NiVisController"
            || b.type_name == "NiBoolInterpolator"
            || b.type_name == "NiBoolData"));
    }

    #[test]
    fn third_person_glow_fix_is_idempotent() {
        let mut nif = fo76_style_third_person();
        assert!(enable_fo4_screen_glow(&mut nif));
        assert!(!enable_fo4_screen_glow(&mut nif));
    }

    #[test]
    fn fix_pipboy2000_nifs_edits_only_the_worn_meshes() {
        let tmp = tempfile::tempdir().unwrap();
        let assets = tmp.path().join("data/Meshes/Pipboy2000/CharacterAssets");
        std::fs::create_dir_all(&assets).unwrap();
        let (mut first, _, _) = fo76_style_first_person();
        first.save(Some(assets.join("PipBoy2000_1.nif"))).unwrap();
        fo76_style_third_person()
            .save(Some(assets.join("PipBoy2000.nif")))
            .unwrap();
        let (mut other, _, _) = fo76_style_first_person();
        other
            .save(Some(assets.join("Pipboy2000_InventoryObject.nif")))
            .unwrap();

        let summary = fix_pipboy2000_nifs(&tmp.path().join("data"));
        assert_eq!(summary.rigid_shapes, 2);
        assert!(summary.screen_glow_fixed);
        assert!(summary.failures.is_empty(), "{:?}", summary.failures);
        let untouched = NifFile::load(assets.join("Pipboy2000_InventoryObject.nif")).unwrap();
        assert!(
            untouched
                .blocks
                .iter()
                .any(|b| b.type_name == "BSSkin::Instance")
        );

        let again = fix_pipboy2000_nifs(&tmp.path().join("data"));
        assert_eq!(again.rigid_shapes, 0);
        assert!(!again.screen_glow_fixed);
    }
}
