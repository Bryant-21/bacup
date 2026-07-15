// Phase: convert_skeleton
//
// Params shape (JSON):
// {
//   "skeleton_nif":   "Meshes/Actors/MyCreature/Skeleton.nif",   // relative game path
//   "resolved_path":  "/abs/path/to/Skeleton.nif",               // absolute disk path
//   "source_game":    "fnv" | "fo3",
//   "target_game":    "fo4",
//   "creature_type":  null | "deathclaw" | "dog" | ...,          // null = auto-detect
//   "skeleton_name":  null | "OverrideName",                     // null = stem of nif path
//   "bone_name_map":  null | { "BipBone": "FO4Bone", ... }       // null = use embedded tables
// }
//
// Phase output: writes Skeleton.hkx to mod_path/data/Meshes/Actors/<creature>/CharacterAssets/
// PhaseReport:
//   assets_written = 1 on success
//   warnings       = 1 on failure (source missing, conversion error, etc.)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;

use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

pub struct ConvertSkeletonPhase;

impl Phase for ConvertSkeletonPhase {
    fn name(&self) -> &'static str {
        "convert_skeleton"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let p = ctx.params;

        let skeleton_nif = p["skeleton_nif"]
            .as_str()
            .ok_or_else(|| PhaseError::BadParams("missing skeleton_nif".into()))?
            .to_string();

        let resolved_path = p["resolved_path"]
            .as_str()
            .ok_or_else(|| PhaseError::BadParams("missing resolved_path".into()))?
            .to_string();

        let source_game = p["source_game"]
            .as_str()
            .ok_or_else(|| PhaseError::BadParams("missing source_game".into()))?
            .to_string();

        let target_game = p
            .get("target_game")
            .and_then(|v| v.as_str())
            .unwrap_or("fo4")
            .to_string();

        let creature_type: Option<String> = p
            .get("creature_type")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let skeleton_name_override: Option<String> = p
            .get("skeleton_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        // Optional explicit bone name map (overrides embedded tables when provided).
        let explicit_bone_map: Option<HashMap<String, String>> =
            p.get("bone_name_map").and_then(|v| {
                v.as_object().map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect()
                })
            });

        let nif_path = Path::new(&resolved_path);
        if !nif_path.exists() {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: self.name(),
                level: LogLevel::Error,
                message: format!("skeleton NIF not found: {resolved_path}"),
            });
            return Ok(PhaseReport {
                warnings: 1,
                ..Default::default()
            });
        }

        ctx.check_cancel()?;

        let nif = match nif_core_native::model::NifFile::load(nif_path) {
            Ok(nif) => nif,
            Err(e) => {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Error,
                    message: format!("failed to load skeleton NIF '{skeleton_nif}': {e}"),
                });
                return Ok(PhaseReport {
                    warnings: 1,
                    ..Default::default()
                });
            }
        };

        ctx.check_cancel()?;

        let resolved_name = skeleton_name_override.unwrap_or_else(|| {
            Path::new(&skeleton_nif)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Skeleton")
                .to_string()
        });

        let skeleton = match extract_nif_skeleton(&nif, &resolved_name) {
            Ok(s) => s,
            Err(e) => {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Error,
                    message: format!("extract_nif_skeleton '{skeleton_nif}': {e}"),
                });
                return Ok(PhaseReport {
                    warnings: 1,
                    ..Default::default()
                });
            }
        };

        ctx.check_cancel()?;

        let forward: HashMap<String, String> = if let Some(explicit) = explicit_bone_map {
            explicit
        } else {
            build_bone_map(
                &source_game,
                &target_game,
                creature_type.as_deref(),
                &skeleton,
            )
        };

        let remapped_bones: Vec<SkeletonBone> = skeleton
            .bones
            .iter()
            .map(|bone| {
                let name = forward
                    .get(&bone.name)
                    .cloned()
                    .unwrap_or_else(|| bone.name.clone());
                SkeletonBone {
                    name,
                    parent_index: bone.parent_index,
                    translation: bone.translation,
                    rotation: bone.rotation,
                    scale: bone.scale,
                }
            })
            .collect();

        let final_skeleton = NifSkeleton {
            name: skeleton.name.clone(),
            bones: remapped_bones,
        };

        let xml = skeleton_to_hkx_xml(&final_skeleton);

        let hkx_bytes = match havok_native::api::havok_xml_to_hkx(&xml) {
            Ok(b) => b,
            Err(e) => {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Error,
                    message: format!("havok_xml_to_hkx failed for '{skeleton_nif}': {e}"),
                });
                return Ok(PhaseReport {
                    warnings: 1,
                    ..Default::default()
                });
            }
        };

        ctx.check_cancel()?;

        let output_path = skeleton_hkx_output_path(ctx.mod_path, &skeleton_nif);
        if let Some(parent) = output_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Error,
                    message: format!("mkdir failed for '{}': {e}", parent.display()),
                });
                return Ok(PhaseReport {
                    warnings: 1,
                    ..Default::default()
                });
            }
        }

        if let Err(e) = std::fs::write(&output_path, &hkx_bytes) {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: self.name(),
                level: LogLevel::Error,
                message: format!("write failed for '{}': {e}", output_path.display()),
            });
            return Ok(PhaseReport {
                warnings: 1,
                ..Default::default()
            });
        }

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: self.name(),
            level: LogLevel::Info,
            message: format!(
                "skeleton: {} bones → {}",
                final_skeleton.bones.len(),
                output_path.display()
            ),
        });

        Ok(PhaseReport {
            assets_written: 1,
            ..Default::default()
        })
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

struct NifSkeleton {
    name: String,
    bones: Vec<SkeletonBone>,
}

struct SkeletonBone {
    name: String,
    parent_index: i32,
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
}

// ---------------------------------------------------------------------------
// NIF skeleton extraction
// ---------------------------------------------------------------------------

fn extract_nif_skeleton(
    nif: &nif_core_native::model::NifFile,
    skeleton_name: &str,
) -> Result<NifSkeleton, String> {
    use nif_core_native::model::NifValue;

    let named_nodes: Vec<(usize, &nif_core_native::model::NifBlock)> = nif
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| {
            block.type_name == "NiNode"
                && block
                    .get_field("Name")
                    .map(|v| !matches!(v, NifValue::String(s) if s.is_empty()))
                    .unwrap_or(false)
        })
        .collect();

    if named_nodes.is_empty() {
        return Err("NIF does not contain any named NiNode bones".to_string());
    }

    let node_index_set: std::collections::HashSet<usize> =
        named_nodes.iter().map(|(id, _)| *id).collect();

    let mut parent_by_child: HashMap<usize, usize> = HashMap::new();
    for (block_id, block) in &named_nodes {
        if let Some(NifValue::Array(children)) = block.get_field("Children") {
            for child_val in children {
                let child_id = match child_val {
                    NifValue::Ref(r) if *r >= 0 => *r as usize,
                    NifValue::Int(i) if *i >= 0 => *i as usize,
                    _ => continue,
                };
                if node_index_set.contains(&child_id) {
                    parent_by_child.insert(child_id, *block_id);
                }
            }
        }
    }

    // Topological sort: parents before children.
    let mut ordered_ids: Vec<usize> = Vec::with_capacity(named_nodes.len());
    let mut visited: std::collections::HashSet<usize> = std::collections::HashSet::new();

    fn visit(
        id: usize,
        named_nodes: &[(usize, &nif_core_native::model::NifBlock)],
        node_index_set: &std::collections::HashSet<usize>,
        ordered_ids: &mut Vec<usize>,
        visited: &mut std::collections::HashSet<usize>,
    ) {
        use nif_core_native::model::NifValue;
        if visited.contains(&id) || !node_index_set.contains(&id) {
            return;
        }
        visited.insert(id);
        ordered_ids.push(id);
        if let Some((_, block)) = named_nodes.iter().find(|(bid, _)| *bid == id) {
            if let Some(NifValue::Array(children)) = block.get_field("Children") {
                for cv in children {
                    let cid = match cv {
                        NifValue::Ref(r) if *r >= 0 => *r as usize,
                        NifValue::Int(i) if *i >= 0 => *i as usize,
                        _ => continue,
                    };
                    visit(cid, named_nodes, node_index_set, ordered_ids, visited);
                }
            }
        }
    }

    // Roots first, then any stragglers.
    let root_ids: Vec<usize> = named_nodes
        .iter()
        .filter(|(id, _)| !parent_by_child.contains_key(id))
        .map(|(id, _)| *id)
        .collect();

    for root_id in &root_ids {
        visit(
            *root_id,
            &named_nodes,
            &node_index_set,
            &mut ordered_ids,
            &mut visited,
        );
    }
    for (id, _) in &named_nodes {
        visit(
            *id,
            &named_nodes,
            &node_index_set,
            &mut ordered_ids,
            &mut visited,
        );
    }

    let index_by_block_id: HashMap<usize, usize> = ordered_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();

    let mut bones: Vec<SkeletonBone> = Vec::with_capacity(ordered_ids.len());
    for block_id in &ordered_ids {
        let (_, block) = named_nodes.iter().find(|(bid, _)| bid == block_id).unwrap();

        let name = block
            .get_field("Name")
            .and_then(|v| {
                if let NifValue::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        let parent_index = parent_by_child
            .get(block_id)
            .and_then(|pid| index_by_block_id.get(pid))
            .map(|i| *i as i32)
            .unwrap_or(-1);

        let translation = extract_translation(block);
        let rotation = extract_rotation(block);
        let scale_v = block
            .get_field("Scale")
            .and_then(|v| match v {
                NifValue::Float(f) => Some(*f as f32),
                NifValue::Int(i) => Some(*i as f32),
                _ => None,
            })
            .unwrap_or(1.0);

        bones.push(SkeletonBone {
            name,
            parent_index,
            translation,
            rotation,
            scale: [scale_v, scale_v, scale_v],
        });
    }

    Ok(NifSkeleton {
        name: skeleton_name.to_string(),
        bones,
    })
}

fn extract_translation(block: &nif_core_native::model::NifBlock) -> [f32; 3] {
    use nif_core_native::model::NifValue;
    match block.get_field("Translation") {
        Some(NifValue::Vec3(v)) => *v,
        Some(NifValue::Struct(m)) => {
            let x = m
                .get("x")
                .and_then(|v| {
                    if let NifValue::Float(f) = v {
                        Some(*f as f32)
                    } else {
                        None
                    }
                })
                .unwrap_or(0.0);
            let y = m
                .get("y")
                .and_then(|v| {
                    if let NifValue::Float(f) = v {
                        Some(*f as f32)
                    } else {
                        None
                    }
                })
                .unwrap_or(0.0);
            let z = m
                .get("z")
                .and_then(|v| {
                    if let NifValue::Float(f) = v {
                        Some(*f as f32)
                    } else {
                        None
                    }
                })
                .unwrap_or(0.0);
            [x, y, z]
        }
        _ => [0.0, 0.0, 0.0],
    }
}

fn extract_rotation(block: &nif_core_native::model::NifBlock) -> [f32; 4] {
    use nif_core_native::model::NifValue;

    let rot = match block.get_field("Rotation") {
        Some(NifValue::Matrix33(m)) => *m,
        Some(NifValue::Struct(fields)) => {
            let get_f = |key: &str| {
                fields
                    .get(key)
                    .and_then(|v| {
                        if let NifValue::Float(f) = v {
                            Some(*f as f32)
                        } else if let NifValue::Int(i) = v {
                            Some(*i as f32)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0.0)
            };
            [
                [get_f("m11"), get_f("m12"), get_f("m13")],
                [get_f("m21"), get_f("m22"), get_f("m23")],
                [get_f("m31"), get_f("m32"), get_f("m33")],
            ]
        }
        _ => return [0.0, 0.0, 0.0, 1.0],
    };

    matrix33_to_quat(rot)
}

/// Convert a 3×3 rotation matrix to a quaternion (x, y, z, w).
fn matrix33_to_quat(m: [[f32; 3]; 3]) -> [f32; 4] {
    let m11 = m[0][0];
    let m12 = m[0][1];
    let m13 = m[0][2];
    let m21 = m[1][0];
    let m22 = m[1][1];
    let m23 = m[1][2];
    let m31 = m[2][0];
    let m32 = m[2][1];
    let m33 = m[2][2];

    let trace = m11 + m22 + m33;
    if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        return [(m32 - m23) / s, (m13 - m31) / s, (m21 - m12) / s, 0.25 * s];
    }
    if m11 > m22 && m11 > m33 {
        let s = (1.0 + m11 - m22 - m33).sqrt() * 2.0;
        return [0.25 * s, (m12 + m21) / s, (m13 + m31) / s, (m32 - m23) / s];
    }
    if m22 > m33 {
        let s = (1.0 + m22 - m11 - m33).sqrt() * 2.0;
        return [(m12 + m21) / s, 0.25 * s, (m23 + m32) / s, (m13 - m31) / s];
    }
    let s = (1.0 + m33 - m11 - m22).sqrt() * 2.0;
    [(m13 + m31) / s, (m23 + m32) / s, 0.25 * s, (m21 - m12) / s]
}

// ---------------------------------------------------------------------------
// Bone name mapping
// ---------------------------------------------------------------------------

/// Build a forward bone-name map from embedded skeleton YAML tables.
fn build_bone_map(
    source_game: &str,
    target_game: &str,
    creature_type: Option<&str>,
    skeleton: &NifSkeleton,
) -> HashMap<String, String> {
    let src = source_game.to_lowercase();
    let tgt = target_game.to_lowercase();

    // For FNV, fall back to fo3 tables if no fnv-specific table.
    let src_candidates: &[&str] = if src == "fnv" {
        &["fnv", "fo3"]
    } else {
        &[src.as_str()]
    };

    // Try creature/robot table first.
    if let Some(ctype) = creature_type {
        for src_c in src_candidates {
            if let Some(map) = load_creature_bone_map(src_c, &tgt, ctype) {
                return map;
            }
            if let Some(map) = load_robot_bone_map(src_c, &tgt, ctype) {
                return map;
            }
        }
    }

    // Auto-detect creature type from bone names.
    let bone_names: std::collections::HashSet<String> =
        skeleton.bones.iter().map(|b| b.name.clone()).collect();
    for src_c in src_candidates {
        if let Some(detected) = auto_detect_creature_type(src_c, &tgt, &bone_names) {
            if let Some(map) = load_creature_bone_map(src_c, &tgt, &detected) {
                return map;
            }
        }
    }

    // Fall back to humanoid direct mapping.
    for src_c in src_candidates {
        if let Some(map) = load_humanoid_bone_map(src_c, &tgt) {
            return map;
        }
    }

    // Try inverse (tgt → src) table and invert it.
    for src_c in src_candidates {
        if let Some(map) = load_humanoid_bone_map_inverse(&tgt, src_c) {
            return map;
        }
    }

    HashMap::new()
}

fn skeleton_yaml_for(src: &str, tgt: &str) -> Option<&'static str> {
    match (src, tgt) {
        ("fo3", "fo4") => Some(crate::embedded::SKELETON_FO3_TO_FO4),
        _ => None,
    }
}

fn creatures_yaml_for(src: &str, tgt: &str) -> Option<&'static str> {
    match (src, tgt) {
        ("fnv", "fo4") => Some(crate::embedded::SKELETON_FNV_TO_FO4_CREATURES),
        ("fo3", "fo4") => Some(crate::embedded::SKELETON_FO3_TO_FO4_CREATURES),
        _ => None,
    }
}

fn robots_yaml_for(src: &str, tgt: &str) -> Option<&'static str> {
    match (src, tgt) {
        ("fnv", "fo4") => Some(crate::embedded::SKELETON_FNV_TO_FO4_ROBOTS),
        _ => None,
    }
}

fn parse_yaml_object(yaml: &str) -> Option<serde_json::Value> {
    serde_saphyr::from_str(yaml).ok()
}

fn load_humanoid_bone_map(src: &str, tgt: &str) -> Option<HashMap<String, String>> {
    let yaml_text = skeleton_yaml_for(src, tgt)?;
    let v: serde_json::Value = parse_yaml_object(yaml_text)?;
    let bones = v.get("bones")?.as_object()?;
    Some(
        bones
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect(),
    )
}

fn load_humanoid_bone_map_inverse(tgt: &str, src: &str) -> Option<HashMap<String, String>> {
    // Load the forward map (tgt→src direction) and invert it.
    let yaml_text = skeleton_yaml_for(tgt, src)?;
    let v: serde_json::Value = parse_yaml_object(yaml_text)?;
    let bones = v.get("bones")?.as_object()?;
    let mut forward: HashMap<String, String> = HashMap::new();
    let mut seen_targets: std::collections::HashSet<String> = std::collections::HashSet::new();
    // bones here: tgt_bone → src_bone — invert to src_bone → tgt_bone
    for (tgt_bone, src_val) in bones {
        if let Some(src_bone) = src_val.as_str() {
            if !seen_targets.contains(src_bone) {
                forward.insert(src_bone.to_string(), tgt_bone.clone());
                seen_targets.insert(src_bone.to_string());
            }
        }
    }
    if forward.is_empty() {
        None
    } else {
        Some(forward)
    }
}

fn load_creature_bone_map(src: &str, tgt: &str, ctype: &str) -> Option<HashMap<String, String>> {
    let yaml_text = creatures_yaml_for(src, tgt)?;
    let v: serde_json::Value = parse_yaml_object(yaml_text)?;
    let cdata = v.get("creatures")?.get(ctype)?;
    let bones = cdata.get("bones")?.as_object()?;
    Some(
        bones
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect(),
    )
}

fn load_robot_bone_map(src: &str, tgt: &str, ctype: &str) -> Option<HashMap<String, String>> {
    let yaml_text = robots_yaml_for(src, tgt)?;
    let v: serde_json::Value = parse_yaml_object(yaml_text)?;
    let cdata = v.get("robots")?.get(ctype)?;
    let bones = cdata.get("bones")?.as_object()?;
    Some(
        bones
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect(),
    )
}

fn auto_detect_creature_type(
    src: &str,
    tgt: &str,
    bone_names: &std::collections::HashSet<String>,
) -> Option<String> {
    let yaml_text = creatures_yaml_for(src, tgt)?;
    let v: serde_json::Value = parse_yaml_object(yaml_text)?;
    let creatures = v.get("creatures")?.as_object()?;

    let mut best_match: Option<String> = None;
    let mut best_count: usize = 0;

    for (ctype, cdata) in creatures {
        let sigs = cdata.get("signature_bones")?.as_array()?;
        let count = sigs
            .iter()
            .filter(|b| b.as_str().map(|s| bone_names.contains(s)).unwrap_or(false))
            .count();
        if count >= 2 && count > best_count {
            best_count = count;
            best_match = Some(ctype.clone());
        }
    }

    best_match
}

// ---------------------------------------------------------------------------
// HKX XML generation
// ---------------------------------------------------------------------------

const IDENTITY_ROTATION: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const IDENTITY_SCALE: [f32; 3] = [1.0, 1.0, 1.0];

fn skeleton_to_hkx_xml(skeleton: &NifSkeleton) -> String {
    let bones = &skeleton.bones;
    let mut xml = String::new();

    xml.push_str("<?xml version=\"1.0\" encoding=\"ASCII\" standalone=\"no\"?>\n");
    xml.push_str("<hkpackfile classversion=\"11\" contentsversion=\"hk_2014.1.0-r1\">\n");
    xml.push_str("    <hksection name=\"__data__\">\n");

    // hkRootLevelContainer
    xml.push_str(
        "        <hkobject name=\"#90\" class=\"hkRootLevelContainer\" signature=\"0x2772c11e\">\n",
    );
    xml.push_str("            <hkparam name=\"namedVariants\" numelements=\"2\">\n");
    xml.push_str("                <hkobject>\n");
    xml.push_str(
        "                    <hkparam name=\"name\">Merged Animation Container</hkparam>\n",
    );
    xml.push_str(
        "                    <hkparam name=\"className\">hkaAnimationContainer</hkparam>\n",
    );
    xml.push_str("                    <hkparam name=\"variant\">#91</hkparam>\n");
    xml.push_str("                </hkobject>\n");
    xml.push_str("                <hkobject>\n");
    xml.push_str("                    <hkparam name=\"name\">Resource Data</hkparam>\n");
    xml.push_str(
        "                    <hkparam name=\"className\">hkMemoryResourceContainer</hkparam>\n",
    );
    xml.push_str("                    <hkparam name=\"variant\">#93</hkparam>\n");
    xml.push_str("                </hkobject>\n");
    xml.push_str("            </hkparam>\n");
    xml.push_str("        </hkobject>\n");

    // hkaAnimationContainer
    xml.push_str("        <hkobject name=\"#91\" class=\"hkaAnimationContainer\" signature=\"0x8dc20333\">\n");
    xml.push_str("            <hkparam name=\"skeletons\" numelements=\"1\">#92</hkparam>\n");
    xml.push_str("            <hkparam name=\"animations\" numelements=\"0\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"bindings\" numelements=\"0\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"attachments\" numelements=\"0\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"skins\" numelements=\"0\"></hkparam>\n");
    xml.push_str("        </hkobject>\n");

    // hkaSkeleton
    xml.push_str(
        "        <hkobject name=\"#92\" class=\"hkaSkeleton\" signature=\"0x366e8220\">\n",
    );
    xml.push_str(&format!(
        "            <hkparam name=\"name\">{}</hkparam>\n",
        xml_escape(&skeleton.name)
    ));

    // parentIndices
    xml.push_str(&format!(
        "            <hkparam name=\"parentIndices\" numelements=\"{}\">\n",
        bones.len()
    ));
    xml.push_str("                ");
    for bone in bones {
        xml.push_str(&format!("{} ", bone.parent_index));
    }
    xml.push_str("\n            </hkparam>\n");

    // bones array
    xml.push_str(&format!(
        "            <hkparam name=\"bones\" numelements=\"{}\">\n",
        bones.len()
    ));
    for bone in bones {
        xml.push_str("                <hkobject>\n");
        xml.push_str(&format!(
            "                    <hkparam name=\"name\">{}</hkparam>\n",
            xml_escape(&bone.name)
        ));
        xml.push_str("                    <hkparam name=\"lockTranslation\">false</hkparam>\n");
        xml.push_str("                </hkobject>\n");
    }
    xml.push_str("            </hkparam>\n");

    // referencePose
    xml.push_str(&format!(
        "            <hkparam name=\"referencePose\" numelements=\"{}\">\n",
        bones.len()
    ));
    for bone in bones {
        let t = bone.translation;
        let r = if bone.rotation == [0.0, 0.0, 0.0, 0.0] {
            IDENTITY_ROTATION
        } else {
            bone.rotation
        };
        let s = if bone.scale == [0.0, 0.0, 0.0] {
            IDENTITY_SCALE
        } else {
            bone.scale
        };
        xml.push_str(&format!(
            "                ({} {} {} 0)({} {} {} {})({} {} {} 0)\n",
            t[0], t[1], t[2], r[0], r[1], r[2], r[3], s[0], s[1], s[2]
        ));
    }
    xml.push_str("            </hkparam>\n");

    xml.push_str("            <hkparam name=\"referenceFloats\" numelements=\"0\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"floatSlots\" numelements=\"0\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"localFrames\" numelements=\"0\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"partitions\" numelements=\"0\"></hkparam>\n");
    xml.push_str("        </hkobject>\n");

    // hkMemoryResourceContainer
    xml.push_str("        <hkobject name=\"#93\" class=\"hkMemoryResourceContainer\" signature=\"0x6a5abb3f\">\n");
    xml.push_str("            <hkparam name=\"name\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"resourceHandles\" numelements=\"0\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"children\" numelements=\"0\"></hkparam>\n");
    xml.push_str("        </hkobject>\n");

    xml.push_str("    </hksection>\n");
    xml.push_str("</hkpackfile>\n");

    xml
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// Output path helper
// ---------------------------------------------------------------------------

/// Determine the output path for a skeleton HKX.
///
/// Input  `skeleton_nif`:  `"Meshes/Actors/MyCreature/CharacterAssets/Skeleton.nif"`
/// Output path:            `<mod_path>/data/Meshes/Actors/MyCreature/CharacterAssets/Skeleton.hkx`
fn skeleton_hkx_output_path(mod_path: &Path, skeleton_nif: &str) -> PathBuf {
    let rel = mesh_relative_skeleton_path(skeleton_nif);

    // Replace .nif extension with .hkx
    let hkx_rel = if rel.to_lowercase().ends_with(".nif") {
        format!("{}.hkx", &rel[..rel.len() - 4])
    } else {
        format!("{}.hkx", rel)
    };

    let mut out = mod_path.to_path_buf();
    out.push("data");
    out.push("Meshes");
    for component in hkx_rel.split('/') {
        if !component.is_empty() {
            out.push(component);
        }
    }
    out
}

fn mesh_relative_skeleton_path(source_path: &str) -> String {
    let mut rel = source_path.replace('\\', "/");
    rel = rel.trim_start_matches('/').to_string();
    if rel.len() >= 5 && rel[..5].eq_ignore_ascii_case("data/") {
        rel = rel[5..].to_string();
    }
    if rel.len() >= 7 && rel[..7].eq_ignore_ascii_case("meshes/") {
        rel = rel[7..].to_string();
    }
    strip_known_asset_prefix(&rel).to_string()
}

fn strip_known_asset_prefix(path: &str) -> &str {
    let Some((first, rest)) = path.split_once('/') else {
        return path;
    };
    if is_known_asset_prefix(first) {
        rest
    } else {
        path
    }
}

fn is_known_asset_prefix(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "fo4" | "fo76" | "fnv" | "fo3" | "skyrim" | "skyrimse" | "starfield" | "oblivion"
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;

    fn make_run() -> u64 {
        create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap()
    }

    #[test]
    fn skeleton_hkx_output_path_characterassets() {
        let base = Path::new("/mod");
        let result = skeleton_hkx_output_path(
            base,
            "Meshes/Actors/MyCreature/CharacterAssets/Skeleton.nif",
        );
        assert_eq!(
            result,
            Path::new("/mod/data/Meshes/Actors/MyCreature/CharacterAssets/Skeleton.hkx")
        );
    }

    #[test]
    fn skeleton_hkx_output_path_backslash_input() {
        let base = Path::new("/mod");
        let result = skeleton_hkx_output_path(base, "Meshes\\Actors\\MyCreature\\Skeleton.nif");
        assert_eq!(
            result,
            Path::new("/mod/data/Meshes/Actors/MyCreature/Skeleton.hkx")
        );
    }

    #[test]
    fn skeleton_hkx_output_path_adds_meshes_root_for_actor_relative_path() {
        let base = Path::new("/mod");
        let result =
            skeleton_hkx_output_path(base, "Actors/GraftonMonster/CharacterAssets/skeleton.nif");
        assert_eq!(
            result,
            Path::new("/mod/data/Meshes/Actors/GraftonMonster/CharacterAssets/skeleton.hkx")
        );
    }

    #[test]
    fn matrix33_identity_gives_identity_quat() {
        let identity = [[1.0_f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let q = matrix33_to_quat(identity);
        // Identity quaternion: (0,0,0,1)
        assert!((q[0]).abs() < 1e-4, "x={}", q[0]);
        assert!((q[1]).abs() < 1e-4, "y={}", q[1]);
        assert!((q[2]).abs() < 1e-4, "z={}", q[2]);
        assert!((q[3] - 1.0).abs() < 1e-4, "w={}", q[3]);
    }

    #[test]
    fn skeleton_xml_contains_bone_names() {
        let skeleton = NifSkeleton {
            name: "TestSkeleton".to_string(),
            bones: vec![
                SkeletonBone {
                    name: "Root".to_string(),
                    parent_index: -1,
                    translation: [0.0, 0.0, 0.0],
                    rotation: IDENTITY_ROTATION,
                    scale: IDENTITY_SCALE,
                },
                SkeletonBone {
                    name: "Spine1".to_string(),
                    parent_index: 0,
                    translation: [0.0, 5.0, 0.0],
                    rotation: IDENTITY_ROTATION,
                    scale: IDENTITY_SCALE,
                },
            ],
        };
        let xml = skeleton_to_hkx_xml(&skeleton);
        assert!(xml.contains("TestSkeleton"), "name not in xml");
        assert!(xml.contains("Root"), "Root bone not in xml");
        assert!(xml.contains("Spine1"), "Spine1 bone not in xml");
        assert!(xml.contains("hkaSkeleton"), "hkaSkeleton missing");
        assert!(xml.contains("-1"), "parent index -1 missing");
    }

    #[test]
    fn fo3_to_fo4_humanoid_bone_map_loads() {
        let map = load_humanoid_bone_map("fo3", "fo4");
        assert!(map.is_some(), "fo3→fo4 humanoid map should be present");
        let m = map.unwrap();
        assert_eq!(m.get("Bip01"), Some(&"Root".to_string()));
        assert_eq!(m.get("Bip01 R Hand"), Some(&"RArm_Hand".to_string()));
    }

    #[test]
    fn fnv_falls_back_to_fo3_humanoid_map() {
        let skeleton = NifSkeleton {
            name: "Skel".to_string(),
            bones: vec![SkeletonBone {
                name: "Bip01 R Hand".to_string(),
                parent_index: -1,
                translation: [0.0, 0.0, 0.0],
                rotation: IDENTITY_ROTATION,
                scale: IDENTITY_SCALE,
            }],
        };
        let map = build_bone_map("fnv", "fo4", None, &skeleton);
        assert_eq!(map.get("Bip01 R Hand"), Some(&"RArm_Hand".to_string()));
    }

    #[test]
    fn missing_nif_returns_warning() {
        let id = make_run();
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "skeleton_nif": "Meshes/Actors/Foo/Skeleton.nif",
                "resolved_path": "/nonexistent/Skeleton.nif",
                "source_game": "fnv",
                "target_game": "fo4"
            });
            let source_dir = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = crate::phase::PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertSkeletonPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 1);
        drop_run(id).unwrap();
    }

    #[test]
    fn missing_params_returns_bad_params_error() {
        let id = make_run();
        let result = with_run(
            id,
            |run| -> Result<Result<PhaseReport, crate::phase::PhaseError>, RunError> {
                let cancel = std::sync::Arc::new(AtomicBool::new(false));
                let params = serde_json::json!({});
                let source_dir = std::path::PathBuf::from("/nonexistent");
                let mod_dir = std::path::PathBuf::from("/nonexistent");
                let mut ctx = crate::phase::PhaseCtx {
                    run,
                    mod_path: &mod_dir,
                    source_extracted_dir: &source_dir,
                    target_extracted_dir: None,
                    target_data_dir: None,
                    params: &params,
                    cancel: &cancel,
                };
                Ok(ConvertSkeletonPhase.run(&mut ctx))
            },
        )
        .unwrap();

        assert!(
            matches!(result, Err(crate::phase::PhaseError::BadParams(_))),
            "expected BadParams, got {:?}",
            result
        );
        drop_run(id).unwrap();
    }
}
