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
//   "bone_name_map":  null | { "BipBone": "FO4Bone", ... },      // null = use embedded tables
//   "preserve_source_rig": false | true,
//   "output_skeleton_path": null | "Actors/MyCreature/CharacterAssets/Skeleton.hkx",
//   "float_slot_names": null | ["SlotA", "SlotB"]
// }
//
// Phase output: writes Skeleton.hkx to mod_path/data/Meshes/Actors/<creature>/CharacterAssets/
// PhaseReport:
//   assets_written = 1 on success
//   warnings       = 1 on failure (source missing, conversion error, etc.)

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;

use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

pub struct ConvertSkeletonPhase;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceRigSkeletonArtifactReceipt {
    pub source_path: PathBuf,
    pub runtime_path: String,
    pub data_relative_path: String,
    pub skeleton_name: String,
    pub ordered_bone_names: Vec<String>,
    pub exact_node_indices: BTreeMap<String, usize>,
    pub parent_indices: Vec<i32>,
    pub float_slot_names: Vec<String>,
    pub hkx_bytes: Vec<u8>,
    pub hkx_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceRigControllerTargetEvidenceReport {
    pub source_path: PathBuf,
    pub entries: Vec<SourceRigControllerTargetEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceRigControllerTargetEvidence {
    ExactOrderedNode {
        target_name: String,
        index: usize,
    },
    NormalizedNameMismatch {
        target_name: String,
        source_name: String,
        index: usize,
    },
    AbsentFromSkeletonNif {
        target_name: String,
    },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum SourceRigSkeletonArtifactError {
    #[error("invalid output_skeleton_path: {0}")]
    InvalidOutputPath(String),
    #[error("failed to load source skeleton {path}: {message}")]
    Load { path: PathBuf, message: String },
    #[error("failed to extract source skeleton: {0}")]
    Extraction(String),
    #[error("invalid source rig: {0}")]
    Validation(String),
    #[error("failed to pack source rig: {0}")]
    Pack(String),
    #[error("failed to preserve exact source-rig strings: {0}")]
    ExactStringRestore(String),
    #[error("packed source rig failed validation: {0}")]
    PackedValidation(String),
}

pub(crate) fn convert_source_owned_skeleton_artifact(
    source_skeleton: &Path,
    output_skeleton_path: &str,
    skeleton_name: &str,
    float_slot_names: &[String],
) -> Result<SourceRigSkeletonArtifactReceipt, SourceRigSkeletonArtifactError> {
    let runtime_path = validate_output_skeleton_path(output_skeleton_path)
        .map_err(SourceRigSkeletonArtifactError::InvalidOutputPath)?;
    let nif = nif_core_native::model::NifFile::load(source_skeleton).map_err(|error| {
        SourceRigSkeletonArtifactError::Load {
            path: source_skeleton.to_path_buf(),
            message: error.to_string(),
        }
    })?;
    let mut skeleton = extract_nif_skeleton(&nif, skeleton_name, true)
        .map_err(|error| SourceRigSkeletonArtifactError::Extraction(error.to_string()))?;
    skeleton.float_slot_names = float_slot_names.to_vec();
    prepare_source_rig_skeleton(&mut skeleton)
        .map_err(|error| SourceRigSkeletonArtifactError::Validation(error.to_string()))?;

    let xml = skeleton_to_hkx_xml(&skeleton);
    let packed = havok_native::api::havok_xml_to_hkx(&xml)
        .map_err(|error| SourceRigSkeletonArtifactError::Pack(error.to_string()))?;
    let hkx_bytes = restore_packed_source_rig_strings(&packed, &skeleton)
        .map_err(SourceRigSkeletonArtifactError::ExactStringRestore)?;
    validate_packed_source_rig_skeleton(&hkx_bytes, &skeleton)
        .map_err(SourceRigSkeletonArtifactError::PackedValidation)?;

    let ordered_bone_names = skeleton
        .bones
        .iter()
        .map(|bone| bone.name.clone())
        .collect::<Vec<_>>();
    let exact_node_indices = ordered_bone_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect();
    Ok(SourceRigSkeletonArtifactReceipt {
        source_path: source_skeleton.to_path_buf(),
        runtime_path: runtime_path.clone(),
        data_relative_path: format!("Meshes/{}", runtime_path.replace('\\', "/")),
        skeleton_name: skeleton.name.clone(),
        ordered_bone_names,
        exact_node_indices,
        parent_indices: skeleton
            .bones
            .iter()
            .map(|bone| bone.parent_index)
            .collect(),
        float_slot_names: skeleton.float_slot_names.clone(),
        hkx_blake3: blake3::hash(&hkx_bytes).to_hex().to_string(),
        hkx_bytes,
    })
}

pub(crate) fn convert_skyrim_source_owned_skeleton_artifact(
    source_skeleton: &Path,
    output_skeleton_path: &str,
) -> Result<SourceRigSkeletonArtifactReceipt, SourceRigSkeletonArtifactError> {
    let runtime_path = validate_output_skeleton_path(output_skeleton_path)
        .map_err(SourceRigSkeletonArtifactError::InvalidOutputPath)?;
    let source_bytes =
        std::fs::read(source_skeleton).map_err(|error| SourceRigSkeletonArtifactError::Load {
            path: source_skeleton.to_path_buf(),
            message: error.to_string(),
        })?;
    let hkx_bytes =
        havok_native::api::havok_reemit_skyrim_2010_animation_asset_to_fo4(&source_bytes)
            .map_err(|error| SourceRigSkeletonArtifactError::Pack(error.to_string()))?;
    let xml = havok_native::api::havok_hkx_to_xml(&hkx_bytes)
        .map_err(|error| SourceRigSkeletonArtifactError::PackedValidation(error.to_string()))?;
    let skeleton = havok_native::animation::parse_skeleton_xml(&xml)
        .map_err(|error| SourceRigSkeletonArtifactError::PackedValidation(error.to_string()))?;
    let exact_node_indices = skeleton
        .bone_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect();
    Ok(SourceRigSkeletonArtifactReceipt {
        source_path: source_skeleton.to_path_buf(),
        runtime_path: runtime_path.clone(),
        data_relative_path: format!("Meshes/{}", runtime_path.replace('\\', "/")),
        skeleton_name: skeleton.name,
        ordered_bone_names: skeleton.bone_names,
        exact_node_indices,
        parent_indices: skeleton.parent_indices,
        float_slot_names: skeleton.float_slots,
        hkx_blake3: blake3::hash(&hkx_bytes).to_hex().to_string(),
        hkx_bytes,
    })
}

pub(crate) fn source_rig_controller_target_evidence(
    receipt: &SourceRigSkeletonArtifactReceipt,
    target_names: &[String],
) -> SourceRigControllerTargetEvidenceReport {
    let normalized_indices = receipt
        .ordered_bone_names
        .iter()
        .enumerate()
        .map(|(index, name)| (normalize_source_rig_bone_name(name), (name, index)))
        .collect::<BTreeMap<_, _>>();
    let entries = target_names
        .iter()
        .map(|target_name| {
            if let Some(index) = receipt.exact_node_indices.get(target_name) {
                return SourceRigControllerTargetEvidence::ExactOrderedNode {
                    target_name: target_name.clone(),
                    index: *index,
                };
            }
            if let Some((source_name, index)) =
                normalized_indices.get(&normalize_source_rig_bone_name(target_name))
            {
                return SourceRigControllerTargetEvidence::NormalizedNameMismatch {
                    target_name: target_name.clone(),
                    source_name: (*source_name).clone(),
                    index: *index,
                };
            }
            SourceRigControllerTargetEvidence::AbsentFromSkeletonNif {
                target_name: target_name.clone(),
            }
        })
        .collect();
    SourceRigControllerTargetEvidenceReport {
        source_path: receipt.source_path.clone(),
        entries,
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
enum SkeletonExtractionError {
    #[error("NIF does not contain any named NiNode bones")]
    NoNamedNodes,
    #[error(
        "bone block {child_block} is referenced by multiple parents {first_parent} and {second_parent}"
    )]
    MultipleParents {
        child_block: usize,
        first_parent: usize,
        second_parent: usize,
    },
}

#[derive(Debug, thiserror::Error, PartialEq)]
enum SourceRigValidationError {
    #[error("skeleton_name must not be empty")]
    EmptySkeletonName,
    #[error("source rig has no bones")]
    NoBones,
    #[error("bone {index} has an empty name")]
    EmptyBoneName { index: usize },
    #[error("duplicate bone name {name:?} at indices {first} and {second}")]
    DuplicateBoneName {
        name: String,
        first: usize,
        second: usize,
    },
    #[error(
        "ambiguous bone names {first_name:?} at {first} and {second_name:?} at {second} normalize to {normalized:?}"
    )]
    AmbiguousBoneName {
        normalized: String,
        first_name: String,
        first: usize,
        second_name: String,
        second: usize,
    },
    #[error("bone {name} at {index} has invalid or non-topological parent {parent}")]
    NonTopologicalParent {
        name: String,
        index: usize,
        parent: i32,
    },
    #[error("bone {name} has a non-finite translation")]
    NonFiniteTranslation { name: String },
    #[error("bone {name} has an invalid scale")]
    InvalidScale { name: String },
    #[error("bone {name} has an invalid rotation")]
    InvalidRotation { name: String },
    #[error("source rig must contain exactly one root, found {roots}")]
    RootCount { roots: usize },
    #[error("float slot {index} has an empty name")]
    EmptyFloatSlotName { index: usize },
    #[error("float_slot_names must be an array")]
    FloatSlotNamesNotArray,
    #[error("float slot {index} must be a string")]
    FloatSlotNameNotString { index: usize },
    #[error("duplicate float slot name {name:?} at indices {first} and {second}")]
    DuplicateFloatSlotName {
        name: String,
        first: usize,
        second: usize,
    },
    #[error(
        "ambiguous float slot names {first_name:?} at {first} and {second_name:?} at {second} normalize to {normalized:?}"
    )]
    AmbiguousFloatSlotName {
        normalized: String,
        first_name: String,
        first: usize,
        second_name: String,
        second: usize,
    },
}

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

        let preserve_source_rig = match p.get("preserve_source_rig") {
            None => false,
            Some(value) => value.as_bool().ok_or_else(|| {
                PhaseError::BadParams("preserve_source_rig must be a boolean".into())
            })?,
        };

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

        let output_skeleton_path = p
            .get("output_skeleton_path")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(validate_output_skeleton_path)
            .transpose()
            .map_err(PhaseError::BadParams)?;

        let float_slot_names = parse_float_slot_names(p.get("float_slot_names"))
            .map_err(|error| PhaseError::BadParams(error.to_string()))?;

        if preserve_source_rig {
            if skeleton_name_override.is_none() {
                return Err(PhaseError::BadParams(
                    "preserve_source_rig requires an explicit skeleton_name".into(),
                ));
            }
            if output_skeleton_path.is_none() {
                return Err(PhaseError::BadParams(
                    "preserve_source_rig requires an explicit output_skeleton_path".into(),
                ));
            }
            if p.get("bone_name_map").is_some_and(|value| !value.is_null()) {
                return Err(PhaseError::BadParams(
                    "preserve_source_rig forbids bone_name_map".into(),
                ));
            }
        }

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
                items_failed: preserve_source_rig as u32,
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
                    items_failed: preserve_source_rig as u32,
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

        let mut skeleton = match extract_nif_skeleton(&nif, &resolved_name, preserve_source_rig) {
            Ok(s) => s,
            Err(e) => {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Error,
                    message: format!("extract_nif_skeleton '{skeleton_nif}': {e}"),
                });
                return Ok(PhaseReport {
                    warnings: 1,
                    items_failed: preserve_source_rig as u32,
                    ..Default::default()
                });
            }
        };
        skeleton.float_slot_names = float_slot_names;

        ctx.check_cancel()?;

        if preserve_source_rig {
            if let Err(e) = prepare_source_rig_skeleton(&mut skeleton) {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Error,
                    message: format!("invalid source rig '{skeleton_nif}': {e}"),
                });
                return Ok(PhaseReport {
                    warnings: 1,
                    items_failed: 1,
                    ..Default::default()
                });
            }
        }

        let forward: HashMap<String, String> = if preserve_source_rig {
            HashMap::new()
        } else if let Some(explicit) = explicit_bone_map {
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
            float_slot_names: skeleton.float_slot_names.clone(),
        };

        let xml = skeleton_to_hkx_xml(&final_skeleton);

        let mut hkx_bytes = match havok_native::api::havok_xml_to_hkx(&xml) {
            Ok(b) => b,
            Err(e) => {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Error,
                    message: format!("havok_xml_to_hkx failed for '{skeleton_nif}': {e}"),
                });
                return Ok(PhaseReport {
                    warnings: 1,
                    items_failed: preserve_source_rig as u32,
                    ..Default::default()
                });
            }
        };

        if preserve_source_rig {
            hkx_bytes = match restore_packed_source_rig_strings(&hkx_bytes, &final_skeleton) {
                Ok(bytes) => bytes,
                Err(e) => {
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: self.name(),
                        level: LogLevel::Error,
                        message: format!("failed to restore exact source-rig strings: {e}"),
                    });
                    return Ok(PhaseReport {
                        warnings: 1,
                        items_failed: 1,
                        ..Default::default()
                    });
                }
            };
            if let Err(e) = validate_packed_source_rig_skeleton(&hkx_bytes, &final_skeleton) {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Error,
                    message: format!("invalid packed source rig '{skeleton_nif}': {e}"),
                });
                return Ok(PhaseReport {
                    warnings: 1,
                    items_failed: 1,
                    ..Default::default()
                });
            }
        }

        ctx.check_cancel()?;

        let output_path = output_skeleton_path
            .as_deref()
            .map(|runtime_path| runtime_skeleton_output_path(ctx.mod_path, runtime_path))
            .unwrap_or_else(|| skeleton_hkx_output_path(ctx.mod_path, &skeleton_nif));
        let data_root = ctx.mod_path.join("data");
        let data_relative_path = output_path
            .strip_prefix(&data_root)
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .map_err(|error| {
                PhaseError::Internal(format!(
                    "skeleton output '{}' is outside '{}': {error}",
                    output_path.display(),
                    data_root.display()
                ))
            })?;
        let already_registered = ctx
            .run
            .output_sink
            .as_ref()
            .and_then(|sink| sink.ba2.as_ref())
            .is_some_and(|ba2| ba2.contains(&data_relative_path));
        if preserve_source_rig && (output_path.exists() || already_registered) {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: self.name(),
                level: LogLevel::Error,
                message: format!(
                    "source rig output path is not unique: '{}'",
                    output_path.display()
                ),
            });
            return Ok(PhaseReport {
                warnings: 1,
                items_failed: 1,
                ..Default::default()
            });
        }
        if let Err(e) = crate::sinks::sink_write_or_fs(
            ctx.run.output_sink.as_ref(),
            &output_path,
            &data_relative_path,
            &hkx_bytes,
        ) {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: self.name(),
                level: LogLevel::Error,
                message: format!("write/register failed for '{}': {e}", output_path.display()),
            });
            return Ok(PhaseReport {
                warnings: 1,
                items_failed: preserve_source_rig as u32,
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
    float_slot_names: Vec<String>,
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
    strict_parentage: bool,
) -> Result<NifSkeleton, SkeletonExtractionError> {
    use nif_core_native::model::NifValue;

    let ni_node_ids = nif
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| {
            block.type_name == "NiNode"
                && matches!(
                    block.get_field("Name"),
                    Some(NifValue::String(name)) if !name.trim().is_empty()
                )
        })
        .map(|(id, _)| id)
        .collect::<std::collections::HashSet<_>>();
    let named_nodes: Vec<(usize, &nif_core_native::model::NifBlock)> = nif
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| {
            let is_named = matches!(
                block.get_field("Name"),
                Some(NifValue::String(name)) if !name.trim().is_empty()
            );
            if !is_named {
                return false;
            }
            if block.type_name == "NiNode" {
                return true;
            }
            strict_parentage
                && block.type_name == "BSFadeNode"
                && block
                    .get_field("Children")
                    .and_then(|value| match value {
                        NifValue::Array(children) => Some(
                            children
                                .iter()
                                .filter_map(|child| match child {
                                    NifValue::Ref(id) if *id >= 0 => Some(*id as usize),
                                    NifValue::Int(id) if *id >= 0 => Some(*id as usize),
                                    _ => None,
                                })
                                .filter(|id| ni_node_ids.contains(id))
                                .count(),
                        ),
                        _ => None,
                    })
                    .is_some_and(|named_children| named_children > 1)
        })
        .collect();

    if named_nodes.is_empty() {
        return Err(SkeletonExtractionError::NoNamedNodes);
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
                    if strict_parentage {
                        if let Some(first_parent) = parent_by_child.insert(child_id, *block_id) {
                            return Err(SkeletonExtractionError::MultipleParents {
                                child_block: child_id,
                                first_parent,
                                second_parent: *block_id,
                            });
                        }
                    } else {
                        parent_by_child.insert(child_id, *block_id);
                    }
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

    if strict_parentage {
        coalesce_exact_duplicate_leaf_bones(&mut bones);
    }

    Ok(NifSkeleton {
        name: skeleton_name.to_string(),
        bones,
        float_slot_names: Vec::new(),
    })
}

fn coalesce_exact_duplicate_leaf_bones(bones: &mut Vec<SkeletonBone>) {
    let parents = bones
        .iter()
        .filter_map(|bone| (bone.parent_index >= 0).then_some(bone.parent_index as usize))
        .collect::<std::collections::HashSet<_>>();
    let mut first_by_name = HashMap::<String, usize>::new();
    let mut keep = vec![true; bones.len()];
    for (index, bone) in bones.iter().enumerate() {
        let Some(&first) = first_by_name.get(&bone.name) else {
            first_by_name.insert(bone.name.clone(), index);
            continue;
        };
        let original = &bones[first];
        if !parents.contains(&first)
            && !parents.contains(&index)
            && bone.parent_index == original.parent_index
            && bone.translation == original.translation
            && bone.rotation == original.rotation
            && bone.scale == original.scale
        {
            keep[index] = false;
        }
    }
    if keep.iter().all(|keep| *keep) {
        return;
    }

    let mut old_to_new = vec![usize::MAX; bones.len()];
    let mut next = 0;
    for (index, keep) in keep.iter().enumerate() {
        if *keep {
            old_to_new[index] = next;
            next += 1;
        }
    }
    let old_bones = std::mem::take(bones);
    for (index, mut bone) in old_bones.into_iter().enumerate() {
        if !keep[index] {
            continue;
        }
        if bone.parent_index >= 0 {
            bone.parent_index = old_to_new[bone.parent_index as usize] as i32;
        }
        bones.push(bone);
    }
}

fn prepare_source_rig_skeleton(skeleton: &mut NifSkeleton) -> Result<(), SourceRigValidationError> {
    if skeleton.name.trim().is_empty() {
        return Err(SourceRigValidationError::EmptySkeletonName);
    }
    if skeleton.bones.is_empty() {
        return Err(SourceRigValidationError::NoBones);
    }

    let mut exact_names = std::collections::HashMap::<&str, usize>::new();
    let mut normalized_names = std::collections::HashMap::<String, (usize, &str)>::new();
    let mut roots = 0;
    for (index, bone) in skeleton.bones.iter_mut().enumerate() {
        if bone.name.trim().is_empty() {
            return Err(SourceRigValidationError::EmptyBoneName { index });
        }
        if let Some(first) = exact_names.insert(&bone.name, index) {
            return Err(SourceRigValidationError::DuplicateBoneName {
                name: bone.name.clone(),
                first,
                second: index,
            });
        }
        let normalized = normalize_source_rig_bone_name(&bone.name);
        if let Some((first, first_name)) =
            normalized_names.insert(normalized.clone(), (index, &bone.name))
        {
            return Err(SourceRigValidationError::AmbiguousBoneName {
                normalized,
                first_name: first_name.to_string(),
                first,
                second_name: bone.name.clone(),
                second: index,
            });
        }

        if bone.parent_index == -1 {
            roots += 1;
        } else if bone.parent_index < 0 || bone.parent_index as usize >= index {
            return Err(SourceRigValidationError::NonTopologicalParent {
                name: bone.name.clone(),
                index,
                parent: bone.parent_index,
            });
        }
        if !bone.translation.iter().all(|value| value.is_finite()) {
            return Err(SourceRigValidationError::NonFiniteTranslation {
                name: bone.name.clone(),
            });
        }
        if !bone
            .scale
            .iter()
            .all(|value| value.is_finite() && *value != 0.0)
        {
            return Err(SourceRigValidationError::InvalidScale {
                name: bone.name.clone(),
            });
        }
        let magnitude_squared = bone.rotation.iter().map(|value| value * value).sum::<f32>();
        if !magnitude_squared.is_finite() || magnitude_squared <= f32::EPSILON {
            return Err(SourceRigValidationError::InvalidRotation {
                name: bone.name.clone(),
            });
        }
        let magnitude = magnitude_squared.sqrt();
        for value in &mut bone.rotation {
            *value /= magnitude;
        }
    }
    if roots != 1 {
        return Err(SourceRigValidationError::RootCount { roots });
    }
    validate_float_slot_names(&skeleton.float_slot_names)
}

fn normalize_source_rig_bone_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn parse_float_slot_names(
    value: Option<&JsonValue>,
) -> Result<Vec<String>, SourceRigValidationError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(Vec::new());
    };
    let Some(values) = value.as_array() else {
        return Err(SourceRigValidationError::FloatSlotNamesNotArray);
    };
    let mut names = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let Some(name) = value.as_str() else {
            return Err(SourceRigValidationError::FloatSlotNameNotString { index });
        };
        names.push(name.to_string());
    }
    validate_float_slot_names(&names)?;
    Ok(names)
}

fn validate_float_slot_names(names: &[String]) -> Result<(), SourceRigValidationError> {
    let mut exact_names = HashMap::<&str, usize>::new();
    let mut normalized_names = HashMap::<String, (usize, &str)>::new();
    for (index, name) in names.iter().enumerate() {
        if name.trim().is_empty() {
            return Err(SourceRigValidationError::EmptyFloatSlotName { index });
        }
        if let Some(first) = exact_names.insert(name, index) {
            return Err(SourceRigValidationError::DuplicateFloatSlotName {
                name: name.clone(),
                first,
                second: index,
            });
        }
        let normalized = normalize_source_rig_bone_name(name);
        if let Some((first, first_name)) =
            normalized_names.insert(normalized.clone(), (index, name))
        {
            return Err(SourceRigValidationError::AmbiguousFloatSlotName {
                normalized,
                first_name: first_name.to_string(),
                first,
                second_name: name.clone(),
                second: index,
            });
        }
    }
    Ok(())
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
            // Parsed on-disk fields are column-labelled; the math matrix is row-major.
            [
                [get_f("m11"), get_f("m21"), get_f("m31")],
                [get_f("m12"), get_f("m22"), get_f("m32")],
                [get_f("m13"), get_f("m23"), get_f("m33")],
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
        "        <hkobject name=\"#0001\" class=\"hkRootLevelContainer\" signature=\"0x2772c11e\">\n",
    );
    xml.push_str("            <hkparam name=\"namedVariants\" numelements=\"2\">\n");
    xml.push_str("                <hkobject>\n");
    xml.push_str(
        "                    <hkparam name=\"name\">Merged Animation Container</hkparam>\n",
    );
    xml.push_str(
        "                    <hkparam name=\"className\">hkaAnimationContainer</hkparam>\n",
    );
    xml.push_str("                    <hkparam name=\"variant\">#0002</hkparam>\n");
    xml.push_str("                </hkobject>\n");
    xml.push_str("                <hkobject>\n");
    xml.push_str("                    <hkparam name=\"name\">Resource Data</hkparam>\n");
    xml.push_str(
        "                    <hkparam name=\"className\">hkMemoryResourceContainer</hkparam>\n",
    );
    xml.push_str("                    <hkparam name=\"variant\">#0004</hkparam>\n");
    xml.push_str("                </hkobject>\n");
    xml.push_str("            </hkparam>\n");
    xml.push_str("        </hkobject>\n");

    // hkaAnimationContainer
    xml.push_str("        <hkobject name=\"#0002\" class=\"hkaAnimationContainer\" signature=\"0x8dc20333\">\n");
    xml.push_str("            <hkparam name=\"skeletons\" numelements=\"1\">#0003</hkparam>\n");
    xml.push_str("            <hkparam name=\"animations\" numelements=\"0\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"bindings\" numelements=\"0\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"attachments\" numelements=\"0\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"skins\" numelements=\"0\"></hkparam>\n");
    xml.push_str("        </hkobject>\n");

    // hkaSkeleton
    xml.push_str(
        "        <hkobject name=\"#0003\" class=\"hkaSkeleton\" signature=\"0x366e8220\">\n",
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

    if skeleton.float_slot_names.is_empty() {
        xml.push_str(
            "            <hkparam name=\"referenceFloats\" numelements=\"0\"></hkparam>\n",
        );
        xml.push_str("            <hkparam name=\"floatSlots\" numelements=\"0\"></hkparam>\n");
    } else {
        xml.push_str(&format!(
            "            <hkparam name=\"referenceFloats\" numelements=\"{}\">",
            skeleton.float_slot_names.len()
        ));
        for _ in &skeleton.float_slot_names {
            xml.push_str("0 ");
        }
        xml.push_str("</hkparam>\n");
        xml.push_str(&format!(
            "            <hkparam name=\"floatSlots\" numelements=\"{}\">\n",
            skeleton.float_slot_names.len()
        ));
        for name in &skeleton.float_slot_names {
            xml.push_str(&format!(
                "                <hkcstring>{}</hkcstring>\n",
                xml_escape(name)
            ));
        }
        xml.push_str("            </hkparam>\n");
    }
    xml.push_str("            <hkparam name=\"localFrames\" numelements=\"0\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"partitions\" numelements=\"0\"></hkparam>\n");
    xml.push_str("        </hkobject>\n");

    // hkMemoryResourceContainer
    xml.push_str("        <hkobject name=\"#0004\" class=\"hkMemoryResourceContainer\" signature=\"0x6a5abb3f\">\n");
    xml.push_str("            <hkparam name=\"name\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"resourceHandles\" numelements=\"0\"></hkparam>\n");
    xml.push_str("            <hkparam name=\"children\" numelements=\"0\"></hkparam>\n");
    xml.push_str("        </hkobject>\n");

    xml.push_str("    </hksection>\n");
    xml.push_str("</hkpackfile>\n");

    xml
}

fn validate_packed_source_rig_skeleton(bytes: &[u8], expected: &NifSkeleton) -> Result<(), String> {
    use havok_native::hkx::HkxFile;
    use havok_native::hkx::descriptors::DescriptorRegistry;
    use havok_native::hkx::types::HkxValue;

    if bytes.get(0x11) != Some(&1) {
        return Err("target packfile is not little-endian".to_string());
    }
    let hkx = HkxFile::read(bytes).map_err(|error| format!("failed to reread HKX: {error}"))?;
    if hkx.class_version() != 11 {
        return Err(format!(
            "expected classversion 11, found {}",
            hkx.class_version()
        ));
    }
    if hkx.contents_version() != "hk_2014.1.0-r1" {
        return Err(format!(
            "expected hk_2014.1.0-r1, found {}",
            hkx.contents_version()
        ));
    }
    if hkx.packfile().header.pointer_size != 8 {
        return Err(format!(
            "expected 8-byte pointers, found {}",
            hkx.packfile().header.pointer_size
        ));
    }

    let mut registry = DescriptorRegistry::for_contents_version(hkx.contents_version());
    for object in hkx.objects() {
        let descriptor = registry
            .get(&object.class_name)
            .map_err(|error| {
                format!(
                    "descriptor lookup failed for {}: {error}",
                    object.class_name
                )
            })?
            .ok_or_else(|| format!("missing target descriptor for {}", object.class_name))?;
        let descriptor_signature =
            u32::from_str_radix(descriptor.signature.trim_start_matches("0x"), 16).map_err(
                |error| {
                    format!(
                        "invalid target descriptor signature {} for {}: {error}",
                        descriptor.signature, object.class_name
                    )
                },
            )?;
        if object.signature != descriptor_signature {
            return Err(format!(
                "{} signature 0x{:08x} does not match target descriptor 0x{descriptor_signature:08x}",
                object.class_name, object.signature
            ));
        }
    }

    let containers = hkx
        .objects()
        .iter()
        .filter(|object| object.class_name == "hkaAnimationContainer")
        .collect::<Vec<_>>();
    let skeletons = hkx
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, object)| object.class_name == "hkaSkeleton")
        .collect::<Vec<_>>();
    if containers.len() != 1 || skeletons.len() != 1 {
        return Err(format!(
            "expected one hkaAnimationContainer and one hkaSkeleton, found {} and {}",
            containers.len(),
            skeletons.len()
        ));
    }
    let skeleton_refs = containers[0]
        .members
        .iter()
        .find(|member| member.name == "skeletons")
        .ok_or_else(|| "packed hkaAnimationContainer has no skeletons member".to_string())?;
    if !matches!(
        &skeleton_refs.value,
        HkxValue::Array(values)
            if matches!(values.as_slice(), [HkxValue::Pointer(Some(index))] if *index == skeletons[0].0)
    ) {
        return Err(
            "packed hkaAnimationContainer.skeletons does not reference the hkaSkeleton".to_string(),
        );
    }

    let xml = havok_native::api::havok_hkx_to_xml(bytes)
        .map_err(|error| format!("failed to export packed skeleton XML: {error}"))?;
    validate_packed_skeleton_xml(&xml, expected)
}

fn restore_packed_source_rig_strings(
    bytes: &[u8],
    expected: &NifSkeleton,
) -> Result<Vec<u8>, String> {
    use havok_native::hkx::HkxFile;
    use havok_native::hkx::types::HkxValue;

    let mut hkx = HkxFile::read(bytes).map_err(|error| format!("failed to reread HKX: {error}"))?;
    let skeleton = hkx
        .objects_mut()
        .iter_mut()
        .find(|object| object.class_name == "hkaSkeleton")
        .ok_or_else(|| "packed HKX has no hkaSkeleton".to_string())?;

    let name = skeleton
        .members
        .iter_mut()
        .find(|member| member.name == "name")
        .ok_or_else(|| "packed hkaSkeleton has no name".to_string())?;
    set_packed_string(&mut name.value, &expected.name, "skeleton name")?;

    let bones = skeleton
        .members
        .iter_mut()
        .find(|member| member.name == "bones")
        .ok_or_else(|| "packed hkaSkeleton has no bones".to_string())?;
    let HkxValue::Array(packed_bones) = &mut bones.value else {
        return Err("packed hkaSkeleton bones is not an array".to_string());
    };
    if packed_bones.len() != expected.bones.len() {
        return Err(format!(
            "packed hkaSkeleton has {} bones, expected {}",
            packed_bones.len(),
            expected.bones.len()
        ));
    }
    for (index, (packed, expected)) in packed_bones.iter_mut().zip(&expected.bones).enumerate() {
        let members = packed
            .as_object_members_mut()
            .ok_or_else(|| format!("packed bone {index} is not an object"))?;
        let name = members
            .iter_mut()
            .find(|member| member.name == "name")
            .ok_or_else(|| format!("packed bone {index} has no name"))?;
        set_packed_string(
            &mut name.value,
            &expected.name,
            &format!("bone {index} name"),
        )?;
    }

    let float_slots = skeleton
        .members
        .iter_mut()
        .find(|member| member.name == "floatSlots")
        .ok_or_else(|| "packed hkaSkeleton has no floatSlots".to_string())?;
    let HkxValue::Array(packed_slots) = &mut float_slots.value else {
        return Err("packed hkaSkeleton floatSlots is not an array".to_string());
    };
    if packed_slots.len() != expected.float_slot_names.len() {
        return Err(format!(
            "packed hkaSkeleton has {} float slots, expected {}",
            packed_slots.len(),
            expected.float_slot_names.len()
        ));
    }
    for (index, (packed, expected)) in packed_slots
        .iter_mut()
        .zip(&expected.float_slot_names)
        .enumerate()
    {
        set_packed_string(packed, expected, &format!("float slot {index}"))?;
    }

    Ok(hkx.save())
}

fn set_packed_string(
    value: &mut havok_native::hkx::types::HkxValue,
    expected: &str,
    field: &str,
) -> Result<(), String> {
    use havok_native::hkx::types::HkxValue;

    let HkxValue::String { value, is_null } = value else {
        return Err(format!("packed {field} is not a string"));
    };
    *value = expected.to_string();
    *is_null = false;
    Ok(())
}

fn validate_packed_skeleton_xml(xml: &str, expected: &NifSkeleton) -> Result<(), String> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| format!("failed to parse packed skeleton XML: {error}"))?;
    let skeleton = document
        .descendants()
        .find(|node| {
            node.has_tag_name("hkobject") && node.attribute("class") == Some("hkaSkeleton")
        })
        .ok_or_else(|| "packed HKX has no hkaSkeleton".to_string())?;
    let param = |name: &str| {
        skeleton
            .children()
            .find(|node| node.has_tag_name("hkparam") && node.attribute("name") == Some(name))
            .ok_or_else(|| format!("packed hkaSkeleton is missing {name}"))
    };

    let actual_name = param("name")?.text().unwrap_or_default();
    if actual_name != expected.name {
        return Err(format!(
            "packed skeleton name {actual_name:?} does not match {:?}",
            expected.name
        ));
    }

    let parents = parse_i32_values(param("parentIndices")?.text().unwrap_or_default())?;
    let expected_parents = expected
        .bones
        .iter()
        .map(|bone| bone.parent_index)
        .collect::<Vec<_>>();
    if parents != expected_parents {
        return Err("packed parentIndices differ from the source rig".to_string());
    }

    let bone_names = param("bones")?
        .children()
        .filter(|node| node.has_tag_name("hkobject"))
        .map(|bone| {
            bone.children()
                .find(|node| node.has_tag_name("hkparam") && node.attribute("name") == Some("name"))
                .and_then(|node| node.text())
                .unwrap_or_default()
                .to_string()
        })
        .collect::<Vec<_>>();
    let expected_names = expected
        .bones
        .iter()
        .map(|bone| bone.name.clone())
        .collect::<Vec<_>>();
    if bone_names != expected_names {
        let mismatch = bone_names
            .iter()
            .zip(&expected_names)
            .position(|(actual, expected)| actual != expected)
            .unwrap_or_else(|| bone_names.len().min(expected_names.len()));
        return Err(format!(
            "packed bone names or order differ at {mismatch}: actual={:?}, expected={:?} ({} vs {} bones)",
            bone_names.get(mismatch),
            expected_names.get(mismatch),
            bone_names.len(),
            expected_names.len()
        ));
    }

    let pose_values = parse_f32_values(param("referencePose")?.text().unwrap_or_default())?;
    if pose_values.len() != expected.bones.len() * 12 {
        return Err(format!(
            "packed referencePose has {} values for {} bones",
            pose_values.len(),
            expected.bones.len()
        ));
    }
    for (index, bone) in expected.bones.iter().enumerate() {
        let actual = &pose_values[index * 12..index * 12 + 12];
        let expected_values = [
            bone.translation[0],
            bone.translation[1],
            bone.translation[2],
            0.0,
            bone.rotation[0],
            bone.rotation[1],
            bone.rotation[2],
            bone.rotation[3],
            bone.scale[0],
            bone.scale[1],
            bone.scale[2],
            0.0,
        ];
        if actual
            .iter()
            .zip(expected_values)
            .any(|(actual, expected)| (actual - expected).abs() > 1.0e-5)
        {
            return Err(format!(
                "packed referencePose differs for bone {} at index {index}",
                bone.name
            ));
        }
    }

    let float_slots = param("floatSlots")?
        .children()
        .filter(|node| node.has_tag_name("hkcstring"))
        .map(|node| node.text().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    if float_slots != expected.float_slot_names {
        return Err("packed floatSlots differ from the explicit source-rig contract".to_string());
    }
    let reference_floats = parse_f32_values(param("referenceFloats")?.text().unwrap_or_default())?;
    if reference_floats.len() != expected.float_slot_names.len()
        || reference_floats.iter().any(|value| *value != 0.0)
    {
        return Err("packed referenceFloats differ from the zero reference contract".to_string());
    }
    Ok(())
}

fn parse_i32_values(text: &str) -> Result<Vec<i32>, String> {
    text.split_whitespace()
        .map(|value| {
            value
                .parse::<i32>()
                .map_err(|error| format!("invalid integer {value:?}: {error}"))
        })
        .collect()
}

fn parse_f32_values(text: &str) -> Result<Vec<f32>, String> {
    text.replace(['(', ')'], " ")
        .split_whitespace()
        .map(|value| {
            value
                .parse::<f32>()
                .map_err(|error| format!("invalid float {value:?}: {error}"))
        })
        .collect()
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

fn validate_output_skeleton_path(value: &str) -> Result<String, String> {
    let normalized = value.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Err("expected a non-empty path relative to the Meshes directory".to_string());
    }
    if normalized.starts_with('/') || normalized.contains(':') {
        return Err("expected a relative path without a drive or root prefix".to_string());
    }
    if normalized
        .split('/')
        .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(
            "empty, current-directory, and parent-directory components are forbidden".to_string(),
        );
    }
    if !normalized.to_ascii_lowercase().ends_with(".hkx") {
        return Err("expected an .hkx target".to_string());
    }
    let lowercase = normalized.to_ascii_lowercase();
    if lowercase.starts_with("meshes/") || lowercase.starts_with("data/") {
        return Err("path must be relative to the Meshes directory".to_string());
    }
    Ok(normalized.replace('/', "\\"))
}

fn runtime_skeleton_output_path(mod_path: &Path, runtime_path: &str) -> PathBuf {
    let mut output = mod_path.join("data").join("Meshes");
    for component in runtime_path.split(['/', '\\']) {
        output.push(component);
    }
    output
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
    use sha2::{Digest, Sha256};
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

    fn gecko_skeleton_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../extracted/fnv/meshes/creatures/nvgecko/skeleton.nif")
    }

    fn gecko_idle_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../extracted/fnv/meshes/creatures/nvgecko/mtidle.kf")
    }

    fn mrhouse_skeleton_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../extracted/fnv/meshes/creatures/nvmrhouse/skeleton.nif")
    }

    fn mrhouse_shellbuttons_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../extracted/fnv/meshes/creatures/nvmrhouse/nvmrhouse_shellbuttons.nif")
    }

    fn combatdroid_skeleton_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../extracted/fo3/meshes/dlc05/creatures/combatdroid/skeleton.nif")
    }

    fn skyrim_wolf_animation_skeleton_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "../../../../extracted/skyrimse/meshes/actors/canine/character assets wolf/skeleton.hkx",
        )
    }

    fn source_rig_fingerprint(skeleton: &NifSkeleton) -> String {
        let mut hasher = Sha256::new();
        hasher.update(skeleton.name.as_bytes());
        hasher.update([0]);
        for bone in &skeleton.bones {
            hasher.update(bone.name.as_bytes());
            hasher.update([0]);
            hasher.update(bone.parent_index.to_le_bytes());
            for value in bone
                .translation
                .iter()
                .chain(bone.rotation.iter())
                .chain(bone.scale.iter())
            {
                hasher.update(value.to_bits().to_le_bytes());
            }
        }
        for name in &skeleton.float_slot_names {
            hasher.update(name.as_bytes());
            hasher.update([0]);
        }
        hex::encode(hasher.finalize())
    }

    fn pack_source_rig_xml(xml: &str, skeleton: &NifSkeleton) -> Vec<u8> {
        let bytes = havok_native::api::havok_xml_to_hkx(xml).unwrap();
        restore_packed_source_rig_strings(&bytes, skeleton).unwrap()
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "expected {expected}, found {actual}"
        );
    }

    const FNV_CREATURE_SKELETONS: &[&str] = &[
        "Creatures/Blowfly/Skeleton.nif",
        "Creatures/Brahmin/Skeleton.nif",
        "Creatures/Centaur/Skeleton.nif",
        "creatures/centaur/skeletonEvolved.nif",
        "Creatures/DeathClaw/Skeleton.nif",
        "creatures/dog/skeleton_SonicBark.nif",
        "Creatures/Dog/Skeleton.nif",
        "Creatures/Eyebot/Skeleton.nif",
        "Creatures/Ghoul/Skeleton.nif",
        "Creatures/GiantAnt/skeleton.nif",
        "Creatures/MiniSentryTurret/Skeleton.nif",
        "Creatures/Mirelurk/Skeleton.nif",
        "Creatures/MirelurkKing/Skeleton.nif",
        "Creatures/MisterGutsy/Skeleton.nif",
        "Creatures/MoleRat/skeleton.nif",
        "creatures/NightStalker/Skeleton.nif",
        "creatures/NVBigHorner/skeleton.nif",
        "creatures/NVCazadores/Skeleton.nif",
        "creatures/NVFestus/skeleton.nif",
        "creatures/NVGecko/skeleton.nif",
        "creatures/NVgiantrat/skeleton.nif",
        "creatures/NVMantis/Skeleton.NIF",
        "creatures/NVMrHouse/Skeleton.nif",
        "creatures/NVPenthoueMainComputer/Skeleton.nif",
        "creatures/NVRaven/Skeleton.nif",
        "creatures/NVSecuritron/Skeleton.nif",
        "creatures/NVSporeCarrier/skeleton.nif",
        "creatures/NVSporePlant/skeleton.nif",
        "creatures/NVTumbleweed/skeleton.nif",
        "creatures/NVVoid/skeleton.nif",
        "Creatures/Protectron/skeleton.nif",
        "Creatures/QueenAnt/skeleton.nif",
        "Creatures/Radroach/skeleton.nif",
        "creatures/radscorpion/skeleton_RoboScopion.nif",
        "Creatures/Radscorpion/Skeleton.nif",
        "Creatures/Robobrain/skeleton.nif",
        "Creatures/SentryBot/Skeleton.nif",
        "Creatures/SentryTurret/skeleton.nif",
        "Creatures/SMBehemoth/Skeleton.nif",
        "Creatures/SMSpineBreaker/Skeleton.nif",
        "DLC03/Creatures/SMBoneCrusher/Skeleton.nif",
        "DLC05/Creatures/Alien/Skeleton.nif",
        "DLCPitt/Creatures/StreetTrog/Skeleton.nif",
        "NVDLC01/Creatures/Ghosts/skeleton.nif",
        "NVDLC01/Creatures/Hologram/skeleton.nif",
        "NVDLC02/Creatures/YaoGuai/Skeleton.nif",
        "NVDLC03/Creatures/BrainBot/Skeleton.nif",
        "NVDLC03/Creatures/BrainTank/Skeleton.nif",
        "NVDLC03/Creatures/Hologram/skeleton.nif",
        "NVDLC03/Creatures/Roboscorpion/skeleton_RoboScopion.nif",
        "NVDLC04/Creatures/NVDLC04EdeClone/skeleton_Low.nif",
    ];

    const FO3_CREATURE_SKELETONS: &[&str] = &[
        "Creatures/Blowfly/Skeleton.nif",
        "Creatures/Brahmin/Skeleton.nif",
        "Creatures/Centaur/Skeleton.nif",
        "Creatures/DeathClaw/Skeleton.nif",
        "Creatures/Dog/Skeleton.nif",
        "Creatures/Eyebot/Skeleton.nif",
        "Creatures/Ghoul/Skeleton.nif",
        "Creatures/GiantAnt/skeleton.nif",
        "Creatures/LibertyPrime/skeleton.nif",
        "Creatures/MiniSentryTurret/Skeleton.nif",
        "Creatures/Mirelurk/Skeleton.nif",
        "Creatures/Mirelurk/Skeleton2.nif",
        "Creatures/MirelurkKing/Skeleton.nif",
        "Creatures/MisterGutsy/Skeleton.nif",
        "Creatures/MoleRat/skeleton.nif",
        "Creatures/Protectron/skeleton.nif",
        "Creatures/QueenAnt/skeleton.nif",
        "Creatures/Radroach/skeleton.nif",
        "Creatures/Radscorpion/Skeleton.nif",
        "Creatures/Robobrain/skeleton.nif",
        "Creatures/SentryBot/Skeleton.nif",
        "Creatures/SentryTurret/skeleton.nif",
        "Creatures/SMBehemoth/Skeleton.nif",
        "Creatures/SMSpineBreaker/Skeleton.nif",
        "Creatures/YaoGuai/Skeleton.nif",
        "Creatures/ZaxEye/Skeleton.nif",
        "DLC03/Creatures/SMBoneCrusher/Skeleton.nif",
        "DLC04/Creatures/Hillfolk1/Skeleton.nif",
        "DLC04/Creatures/Hillfolk2and3/Anims/Skeleton.nif",
        "DLC05/Creatures/Abomination/Skeleton.nif",
        "DLC05/Creatures/Alien/Skeleton.nif",
        "DLC05/Creatures/CombatDroid/Skeleton.nif",
        "DLC05/Creatures/MaintenanceRobot/Skeleton.nif",
        "DLCAnch/Creatures/Chimera/Skeleton.nif",
        "DLCAnch/Creatures/SpiderMine/Skeleton.nif",
        "DLCPitt/Creatures/StreetTrog/Skeleton.nif",
    ];

    fn corpus_skeleton_name(relative_path: &str) -> String {
        let components = relative_path.split('/').collect::<Vec<_>>();
        components[..components.len() - 1]
            .iter()
            .rev()
            .find(|component| {
                !matches!(
                    component.to_ascii_lowercase().as_str(),
                    "anims" | "animations" | "characterassets"
                )
            })
            .unwrap()
            .to_string()
    }

    fn collect_kf_paths(root: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_kf_paths(&path, out);
            } else if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("kf"))
            {
                out.push(path);
            }
        }
    }

    fn kf_transform_targets(path: &Path) -> Result<Vec<String>, String> {
        use nif_core_native::model::NifValue;

        let nif = nif_core_native::model::NifFile::load(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let mut targets = Vec::new();
        for sequence in nif
            .blocks
            .iter()
            .filter(|block| block.type_name == "NiControllerSequence")
        {
            let Some(NifValue::Array(controlled_blocks)) = sequence.get_field("Controlled Blocks")
            else {
                continue;
            };
            for controlled in controlled_blocks {
                let NifValue::Struct(fields) = controlled else {
                    continue;
                };
                let reference = match fields.get("Interpolator") {
                    Some(NifValue::Ref(reference)) if *reference >= 0 => Some(*reference as usize),
                    Some(NifValue::Int(reference)) if *reference >= 0 => Some(*reference as usize),
                    _ => None,
                };
                let is_transform = reference
                    .and_then(|reference| nif.blocks.get(reference))
                    .is_some_and(|interpolator| {
                        matches!(
                            interpolator.type_name.as_str(),
                            "NiTransformInterpolator" | "NiBSplineCompTransformInterpolator"
                        )
                    });
                if !is_transform {
                    continue;
                }
                let name = match fields.get("Node Name") {
                    Some(NifValue::String(name)) | Some(NifValue::Char(name)) => name,
                    _ => continue,
                };
                if !name.trim().is_empty() && !name.starts_with("##") {
                    targets.push(normalize_source_rig_bone_name(name));
                }
            }
        }
        targets.sort_unstable();
        targets.dedup();
        Ok(targets)
    }

    fn assert_corpus_source_rigs(
        game: &str,
        paths: &[&str],
        require_every_path: bool,
        allow_typed_rejections: bool,
    ) -> (String, usize, usize, Vec<String>) {
        let mesh_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../../../extracted/{game}/meshes"));
        let mut corpus_hasher = Sha256::new();
        let mut runtime_paths = std::collections::HashSet::new();
        let mut checked = 0;
        let mut feasible_kfs = 0;
        let mut inspected_kfs = 0;
        let mut typed_rejections = Vec::new();

        for relative_path in paths {
            let source = mesh_root.join(relative_path);
            if !source.is_file() {
                assert!(!require_every_path, "missing {}", source.display());
                continue;
            }
            checked += 1;
            let name = corpus_skeleton_name(relative_path);
            let nif = nif_core_native::model::NifFile::load(&source).unwrap();
            let mut skeleton = match extract_nif_skeleton(&nif, &name, true) {
                Ok(skeleton) => skeleton,
                Err(error) if allow_typed_rejections => {
                    let rejection = format!("{relative_path}: {error}");
                    corpus_hasher.update(game.as_bytes());
                    corpus_hasher.update([0]);
                    corpus_hasher.update(rejection.as_bytes());
                    corpus_hasher.update([0]);
                    typed_rejections.push(rejection);
                    continue;
                }
                Err(error) => panic!("{game}:{relative_path}: {error}"),
            };
            if let Err(error) = prepare_source_rig_skeleton(&mut skeleton) {
                if allow_typed_rejections {
                    let rejection = format!("{relative_path}: {error}");
                    corpus_hasher.update(game.as_bytes());
                    corpus_hasher.update([0]);
                    corpus_hasher.update(rejection.as_bytes());
                    corpus_hasher.update([0]);
                    typed_rejections.push(rejection);
                    continue;
                }
                panic!("{game}:{relative_path}: {error}");
            }

            let fingerprint = source_rig_fingerprint(&skeleton);
            corpus_hasher.update(game.as_bytes());
            corpus_hasher.update([0]);
            corpus_hasher.update(relative_path.replace('\\', "/").to_ascii_lowercase());
            corpus_hasher.update([0]);
            corpus_hasher.update(fingerprint.as_bytes());
            corpus_hasher.update([0]);

            let mut output_hasher = Sha256::new();
            output_hasher.update(game.as_bytes());
            output_hasher.update([0]);
            output_hasher.update(relative_path.replace('\\', "/").to_ascii_lowercase());
            let output_id = hex::encode(output_hasher.finalize());
            let output_path = validate_output_skeleton_path(&format!(
                "Actors/B21_{game}_{}/CharacterAssets/Skeleton.hkx",
                &output_id[..16]
            ))
            .unwrap();
            assert!(
                runtime_paths.insert(output_path.clone()),
                "duplicate runtime path {output_path}"
            );

            let bytes = pack_source_rig_xml(&skeleton_to_hkx_xml(&skeleton), &skeleton);
            validate_packed_source_rig_skeleton(&bytes, &skeleton)
                .unwrap_or_else(|error| panic!("{game}:{relative_path}: {error}"));
            let roundtrip_xml = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
            let roundtrip = pack_source_rig_xml(&roundtrip_xml, &skeleton);
            validate_packed_source_rig_skeleton(&roundtrip, &skeleton)
                .unwrap_or_else(|error| panic!("{game}:{relative_path} roundtrip: {error}"));

            let normalized_bones = skeleton
                .bones
                .iter()
                .map(|bone| normalize_source_rig_bone_name(&bone.name))
                .collect::<std::collections::HashSet<_>>();
            let mut kfs = Vec::new();
            collect_kf_paths(source.parent().unwrap(), &mut kfs);
            kfs.sort_unstable();
            for kf in kfs {
                let targets = kf_transform_targets(&kf).unwrap();
                inspected_kfs += 1;
                if !targets.is_empty()
                    && targets
                        .iter()
                        .all(|target| normalized_bones.contains(target))
                {
                    feasible_kfs += 1;
                    assert!(
                        targets
                            .iter()
                            .all(|target| normalized_bones.contains(target)),
                        "{} has an unresolved binding for {game}:{relative_path}",
                        kf.display()
                    );
                }
            }
        }

        if require_every_path {
            assert_eq!(checked, paths.len(), "incomplete {game} skeleton corpus");
        } else {
            assert!(checked > 0, "no available {game} skeleton fixtures");
        }
        assert_eq!(runtime_paths.len() + typed_rejections.len(), checked);
        (
            hex::encode(corpus_hasher.finalize()),
            inspected_kfs,
            feasible_kfs,
            typed_rejections,
        )
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
    fn explicit_runtime_skeleton_path_is_canonical_and_drives_deploy_path() {
        let runtime =
            validate_output_skeleton_path("Actors/B21_FNVGecko/CharacterAssets/Skeleton.hkx")
                .unwrap();
        assert_eq!(
            runtime,
            "Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"
        );
        assert_eq!(
            runtime_skeleton_output_path(Path::new("/mod"), &runtime),
            Path::new("/mod/data/Meshes/Actors/B21_FNVGecko/CharacterAssets/Skeleton.hkx")
        );
        for invalid in [
            "",
            "C:/Actors/Gecko/Skeleton.hkx",
            "Meshes/Actors/Gecko/Skeleton.hkx",
            "Actors/../Gecko/Skeleton.hkx",
            "Actors/Gecko/Skeleton.hkt",
        ] {
            assert!(
                validate_output_skeleton_path(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
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
            float_slot_names: Vec::new(),
        };
        let xml = skeleton_to_hkx_xml(&skeleton);
        assert!(xml.contains("TestSkeleton"), "name not in xml");
        assert!(xml.contains("Root"), "Root bone not in xml");
        assert!(xml.contains("Spine1"), "Spine1 bone not in xml");
        assert!(xml.contains("hkaSkeleton"), "hkaSkeleton missing");
        assert!(xml.contains("-1"), "parent index -1 missing");
    }

    #[test]
    fn explicit_float_slots_preserve_order_and_zero_references_through_pack_reread() {
        let mut skeleton = NifSkeleton {
            name: "FloatCreature".to_string(),
            bones: vec![SkeletonBone {
                name: "Root".to_string(),
                parent_index: -1,
                translation: [0.0; 3],
                rotation: IDENTITY_ROTATION,
                scale: IDENTITY_SCALE,
            }],
            float_slot_names: vec![
                "Jaw Weight".to_string(),
                "EyeBlinkLeft".to_string(),
                "IsAttacking".to_string(),
            ],
        };
        prepare_source_rig_skeleton(&mut skeleton).unwrap();
        let bytes = pack_source_rig_xml(&skeleton_to_hkx_xml(&skeleton), &skeleton);
        validate_packed_source_rig_skeleton(&bytes, &skeleton).unwrap();
        let reread_xml = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
        let reread_bytes = pack_source_rig_xml(&reread_xml, &skeleton);
        validate_packed_source_rig_skeleton(&reread_bytes, &skeleton).unwrap();

        assert!(matches!(
            validate_float_slot_names(&["Blink".to_string(), " blink ".to_string()]),
            Err(SourceRigValidationError::AmbiguousFloatSlotName {
                first: 0,
                second: 1,
                ..
            })
        ));
        assert!(matches!(
            validate_float_slot_names(&["Blink".to_string(), "Blink".to_string()]),
            Err(SourceRigValidationError::DuplicateFloatSlotName {
                first: 0,
                second: 1,
                ..
            })
        ));
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
            float_slot_names: Vec::new(),
        };
        let map = build_bone_map("fnv", "fo4", None, &skeleton);
        assert_eq!(map.get("Bip01 R Hand"), Some(&"RArm_Hand".to_string()));
    }

    #[test]
    fn preserve_source_rig_rejects_duplicate_and_ambiguous_bones() {
        let mut duplicate = NifSkeleton {
            name: "NVGecko".to_string(),
            bones: vec![
                SkeletonBone {
                    name: "Bip01".to_string(),
                    parent_index: -1,
                    translation: [0.0; 3],
                    rotation: IDENTITY_ROTATION,
                    scale: IDENTITY_SCALE,
                },
                SkeletonBone {
                    name: "Bip01".to_string(),
                    parent_index: 0,
                    translation: [0.0; 3],
                    rotation: IDENTITY_ROTATION,
                    scale: IDENTITY_SCALE,
                },
            ],
            float_slot_names: Vec::new(),
        };
        assert!(matches!(
            prepare_source_rig_skeleton(&mut duplicate),
            Err(SourceRigValidationError::DuplicateBoneName {
                first: 0,
                second: 1,
                ..
            })
        ));

        duplicate.bones[1].name = " bip01 ".to_string();
        assert!(matches!(
            prepare_source_rig_skeleton(&mut duplicate),
            Err(SourceRigValidationError::AmbiguousBoneName {
                first: 0,
                second: 1,
                ..
            })
        ));
    }

    #[test]
    fn preserve_source_rig_typed_rejects_multi_root_and_non_topological_bones() {
        let root = |name: &str| SkeletonBone {
            name: name.to_string(),
            parent_index: -1,
            translation: [0.0; 3],
            rotation: IDENTITY_ROTATION,
            scale: IDENTITY_SCALE,
        };
        let mut multiple_roots = NifSkeleton {
            name: "Creature".to_string(),
            bones: vec![root("RootA"), root("RootB")],
            float_slot_names: Vec::new(),
        };
        assert_eq!(
            prepare_source_rig_skeleton(&mut multiple_roots),
            Err(SourceRigValidationError::RootCount { roots: 2 })
        );

        let mut non_topological = NifSkeleton {
            name: "Creature".to_string(),
            bones: vec![
                SkeletonBone {
                    parent_index: 1,
                    ..root("Child")
                },
                root("Root"),
            ],
            float_slot_names: Vec::new(),
        };
        assert!(matches!(
            prepare_source_rig_skeleton(&mut non_topological),
            Err(SourceRigValidationError::NonTopologicalParent {
                index: 0,
                parent: 1,
                ..
            })
        ));
    }

    #[test]
    fn preserve_source_rig_requires_explicit_name_path_and_no_bone_map() {
        for params in [
            serde_json::json!({
                "skeleton_nif": "creatures/nvgecko/skeleton.nif",
                "resolved_path": "/unused/skeleton.nif",
                "source_game": "fnv",
                "preserve_source_rig": true,
                "output_skeleton_path": "Actors/B21_FNVGecko/CharacterAssets/Skeleton.hkx"
            }),
            serde_json::json!({
                "skeleton_nif": "creatures/nvgecko/skeleton.nif",
                "resolved_path": "/unused/skeleton.nif",
                "source_game": "fnv",
                "preserve_source_rig": true,
                "skeleton_name": "NVGecko"
            }),
            serde_json::json!({
                "skeleton_nif": "creatures/nvgecko/skeleton.nif",
                "resolved_path": "/unused/skeleton.nif",
                "source_game": "fnv",
                "preserve_source_rig": true,
                "skeleton_name": "NVGecko",
                "output_skeleton_path": "Actors/B21_FNVGecko/CharacterAssets/Skeleton.hkx",
                "bone_name_map": {}
            }),
        ] {
            let id = make_run();
            let result = with_run(
                id,
                |run| -> Result<Result<PhaseReport, PhaseError>, RunError> {
                    let cancel = std::sync::Arc::new(AtomicBool::new(false));
                    let source_dir = PathBuf::from("/unused");
                    let mod_dir = PathBuf::from("/unused");
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
            assert!(matches!(result, Err(PhaseError::BadParams(_))));
            drop_run(id).unwrap();
        }
    }

    #[test]
    fn real_fnv_gecko_preserves_own_rig_and_packs_fo4_hkx() {
        let source = gecko_skeleton_path();
        assert!(source.is_file(), "missing {}", source.display());
        let nif = nif_core_native::model::NifFile::load(&source).unwrap();
        let mut skeleton = extract_nif_skeleton(&nif, "NVGecko", true).unwrap();
        prepare_source_rig_skeleton(&mut skeleton).unwrap();

        assert_eq!(skeleton.name, "NVGecko");
        assert_eq!(skeleton.bones.len(), 87);
        assert_eq!(skeleton.bones[0].name, "Bip01");
        assert_eq!(skeleton.bones[0].parent_index, -1);
        assert!(
            skeleton
                .bones
                .iter()
                .any(|bone| bone.name == "Bip01 L Hand")
        );
        assert!(
            skeleton
                .bones
                .iter()
                .any(|bone| bone.name == "Bip01 R Hand")
        );
        let pelvis = skeleton
            .bones
            .iter()
            .find(|bone| bone.name == "Bip01 Pelvis")
            .unwrap();
        let left_upper_arm = skeleton
            .bones
            .iter()
            .find(|bone| bone.name == "Bip01 L UpperArm")
            .unwrap();
        assert_close(skeleton.bones[0].translation[0], -0.00039974958);
        assert_close(skeleton.bones[0].translation[1], 0.00025978082);
        assert_close(pelvis.translation[0], 0.00043334666);
        assert_close(pelvis.translation[1], -5.0022345);
        assert_close(pelvis.translation[2], 36.097256);
        for (actual, expected) in
            left_upper_arm
                .rotation
                .iter()
                .zip([-0.16251615, -0.42609107, -0.10560675, 0.88367534])
        {
            assert_close(*actual, expected);
        }
        assert!(skeleton.bones.iter().all(|bone| {
            bone.translation.iter().all(|value| value.is_finite())
                && bone.rotation.iter().all(|value| value.is_finite())
                && bone.scale.iter().all(|value| value.is_finite())
        }));

        let fingerprint = source_rig_fingerprint(&skeleton);
        assert_eq!(
            fingerprint,
            "e27ae3d11ef0deb6f58a07c245515a30c64f38848a119fafd1574c5950989544"
        );

        let bytes = pack_source_rig_xml(&skeleton_to_hkx_xml(&skeleton), &skeleton);
        validate_packed_source_rig_skeleton(&bytes, &skeleton).unwrap();
        let roundtrip_xml = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
        let roundtrip = pack_source_rig_xml(&roundtrip_xml, &skeleton);
        validate_packed_source_rig_skeleton(&roundtrip, &skeleton).unwrap();
    }

    #[test]
    fn real_skyrim_wolf_reemits_authoritative_animation_skeleton() {
        let source = skyrim_wolf_animation_skeleton_path();
        assert!(source.is_file(), "missing {}", source.display());
        let receipt = convert_skyrim_source_owned_skeleton_artifact(
            &source,
            "Actors/B21_SkyrimWolf/CharacterAssets/Skeleton.hkx",
        )
        .unwrap();

        assert_eq!(receipt.skeleton_name, "NPC Root [Root]");
        assert_eq!(receipt.ordered_bone_names.len(), 50);
        assert_eq!(receipt.ordered_bone_names[0], "NPC Root [Root]");
        assert_eq!(receipt.parent_indices[0], -1);
        assert_eq!(
            receipt
                .parent_indices
                .iter()
                .filter(|parent| **parent < 0)
                .count(),
            1
        );
        assert_eq!(
            havok_native::hkx::HkxFile::read(&receipt.hkx_bytes)
                .unwrap()
                .contents_version(),
            "hk_2014.1.0-r1"
        );
    }

    #[test]
    fn source_owned_gecko_artifact_returns_validated_staging_receipt_without_writing() {
        let corpus_source = gecko_skeleton_path();
        assert!(
            corpus_source.is_file(),
            "missing {}",
            corpus_source.display()
        );
        let temp = tempfile::tempdir().unwrap();
        let staged_source = temp.path().join("GeckoSkeleton.nif");
        std::fs::copy(&corpus_source, &staged_source).unwrap();
        let float_slots = vec!["Jaw Weight".to_string(), "IsAttacking".to_string()];

        let receipt = convert_source_owned_skeleton_artifact(
            &staged_source,
            "Actors/B21_FNVGecko/CharacterAssets/Skeleton.hkx",
            "NVGecko",
            &float_slots,
        )
        .unwrap();

        assert_eq!(receipt.source_path, staged_source);
        assert_eq!(
            receipt.runtime_path,
            "Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"
        );
        assert_eq!(
            receipt.data_relative_path,
            "Meshes/Actors/B21_FNVGecko/CharacterAssets/Skeleton.hkx"
        );
        assert_eq!(receipt.skeleton_name, "NVGecko");
        assert_eq!(receipt.ordered_bone_names.len(), 87);
        assert_eq!(receipt.ordered_bone_names[0], "Bip01");
        assert_eq!(receipt.parent_indices.len(), 87);
        assert_eq!(receipt.parent_indices[0], -1);
        assert_eq!(receipt.float_slot_names, float_slots);
        assert_eq!(
            receipt.hkx_blake3,
            blake3::hash(&receipt.hkx_bytes).to_hex().to_string()
        );
        let hkx = havok_native::hkx::HkxFile::read(&receipt.hkx_bytes).unwrap();
        assert_eq!(hkx.class_version(), 11);
        assert_eq!(hkx.contents_version(), "hk_2014.1.0-r1");
        assert_eq!(hkx.packfile().header.pointer_size, 8);

        let files = std::fs::read_dir(temp.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(files, [std::ffi::OsString::from("GeckoSkeleton.nif")]);
    }

    #[test]
    fn real_mrhouse_shellbuttons_target_is_typed_absent_not_a_float_slot() {
        use nif_core_native::model::NifValue;

        let skeleton_path = mrhouse_skeleton_path();
        let shellbuttons_path = mrhouse_shellbuttons_path();
        assert!(
            skeleton_path.is_file(),
            "missing {}",
            skeleton_path.display()
        );
        assert!(
            shellbuttons_path.is_file(),
            "missing {}",
            shellbuttons_path.display()
        );
        let receipt = convert_source_owned_skeleton_artifact(
            &skeleton_path,
            "Actors/B21_FNVMrHouse/CharacterAssets/Skeleton.hkx",
            "NVMrHouse",
            &[],
        )
        .unwrap();

        assert_eq!(receipt.exact_node_indices.get("Bip01 Shell"), Some(&6));
        assert!(!receipt.exact_node_indices.contains_key("##ShellButtons"));
        assert!(receipt.float_slot_names.is_empty());
        let target_names = vec!["Bip01 Shell".to_string(), "##ShellButtons".to_string()];
        let evidence = source_rig_controller_target_evidence(&receipt, &target_names);
        assert_eq!(
            evidence.entries,
            [
                SourceRigControllerTargetEvidence::ExactOrderedNode {
                    target_name: "Bip01 Shell".to_string(),
                    index: 6,
                },
                SourceRigControllerTargetEvidence::AbsentFromSkeletonNif {
                    target_name: "##ShellButtons".to_string(),
                },
            ]
        );

        let attachment = nif_core_native::model::NifFile::load(&shellbuttons_path).unwrap();
        let node_names = attachment
            .blocks
            .iter()
            .filter(|block| block.type_name == "NiNode")
            .filter_map(|block| match block.get_field("Name") {
                Some(NifValue::String(name)) => Some(name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(node_names.contains(&"##wShellButtons"));
        assert!(!node_names.contains(&"##ShellButtons"));
        assert!(attachment.blocks.iter().any(|block| {
            block.type_name == "NiStringExtraData"
                && matches!(block.get_field("Name"), Some(NifValue::String(name)) if name == "Prn")
                && matches!(block.get_field("String Data"), Some(NifValue::String(name)) if name == "Bip01 Shell")
        }));
    }

    #[test]
    fn fnv_and_available_fo3_creature_source_rig_corpus_packs_deterministically() {
        assert_eq!(FNV_CREATURE_SKELETONS.len(), 51);
        let (fnv_fingerprint, fnv_kfs, fnv_feasible_kfs, fnv_rejections) =
            assert_corpus_source_rigs("fnv", FNV_CREATURE_SKELETONS, true, false);
        let (fo3_fingerprint, fo3_kfs, fo3_feasible_kfs, fo3_rejections) =
            assert_corpus_source_rigs("fo3", FO3_CREATURE_SKELETONS, false, true);
        eprintln!(
            "fnv={fnv_fingerprint} kfs={fnv_kfs} feasible={fnv_feasible_kfs} rejected={fnv_rejections:?}; fo3={fo3_fingerprint} kfs={fo3_kfs} feasible={fo3_feasible_kfs} rejected={fo3_rejections:?}"
        );
        assert_eq!(
            fnv_fingerprint,
            "e3a29ae37f8c01b37b5039a640dd3e9c7abe677905b905144edd6781b564d573"
        );
        assert_eq!(
            fo3_fingerprint,
            "f7a8fa54c81a90d47af8a01dcf69245962c7231f18c6cb628a6bfad5d5c89277"
        );
        assert_eq!((fnv_kfs, fnv_feasible_kfs), (2068, 1947));
        assert_eq!((fo3_kfs, fo3_feasible_kfs), (1581, 1537));
        assert!(fnv_rejections.is_empty());
        assert!(fo3_rejections.is_empty());
    }

    #[test]
    fn real_combatdroid_identical_duplicate_leaf_bone_is_coalesced() {
        let source = combatdroid_skeleton_path();
        if !source.is_file() {
            return;
        }
        let receipt = convert_source_owned_skeleton_artifact(
            &source,
            "Actors/B21_FO3CombatDroid/CharacterAssets/Skeleton.hkx",
            "CombatDroid",
            &[],
        )
        .unwrap();
        assert_eq!(
            receipt
                .ordered_bone_names
                .iter()
                .filter(|name| name.as_str() == "Bip01 Head")
                .count(),
            1
        );
    }

    #[test]
    fn real_fnv_gecko_phase_uses_explicit_runtime_path_without_renaming() {
        use crate::sinks::{Ba2ShardWriter, LooseSink, SinkSet, TerrainSidecarSink};

        let source = gecko_skeleton_path();
        assert!(source.is_file(), "missing {}", source.display());
        let temp = tempfile::tempdir().unwrap();
        let sink = std::sync::Arc::new(SinkSet {
            ba2: Some(Ba2ShardWriter::new(temp.path().join("spill")).unwrap()),
            loose: LooseSink {
                enabled: true,
                mod_root: temp.path().to_path_buf(),
            },
            terrain: TerrainSidecarSink::default(),
        });
        let params = serde_json::json!({
            "skeleton_nif": "creatures/nvgecko/skeleton.nif",
            "resolved_path": source,
            "source_game": "fnv",
            "target_game": "fo4",
            "preserve_source_rig": true,
            "skeleton_name": "NVGecko",
            "output_skeleton_path": "Actors/B21_FNVGecko/CharacterAssets/Skeleton.hkx"
        });
        let id = make_run();
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            run.output_sink = Some(sink.clone());
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let source_dir = source.parent().unwrap().to_path_buf();
            let mut ctx = crate::phase::PhaseCtx {
                run,
                mod_path: temp.path(),
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertSkeletonPhase
                .run(&mut ctx)
                .map_err(|error| RunError::InvalidConfig(error.to_string()))
        })
        .unwrap();
        assert_eq!(report.assets_written, 1);
        assert_eq!(report.warnings, 0);
        assert!(
            sink.ba2
                .as_ref()
                .unwrap()
                .contains("Meshes/Actors/B21_FNVGecko/CharacterAssets/Skeleton.hkx"),
            "packed skeleton was not registered with the output sink"
        );

        let output = temp
            .path()
            .join("data/Meshes/Actors/B21_FNVGecko/CharacterAssets/Skeleton.hkx");
        assert!(output.is_file(), "missing {}", output.display());
        assert!(
            !temp
                .path()
                .join("data/Meshes/creatures/nvgecko/skeleton.hkx")
                .exists()
        );
        let output_xml =
            havok_native::api::havok_hkx_to_xml(&std::fs::read(output).unwrap()).unwrap();
        assert!(output_xml.contains("<hkparam name=\"name\">NVGecko</hkparam>"));
        assert!(output_xml.contains("<hkparam name=\"name\">Bip01</hkparam>"));
        assert!(!output_xml.contains(">Root</hkparam>"));

        let duplicate = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let source_dir = source.parent().unwrap().to_path_buf();
            let mut ctx = crate::phase::PhaseCtx {
                run,
                mod_path: temp.path(),
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertSkeletonPhase
                .run(&mut ctx)
                .map_err(|error| RunError::InvalidConfig(error.to_string()))
        })
        .unwrap();
        assert_eq!(duplicate.assets_written, 0);
        assert_eq!(duplicate.warnings, 1);
        assert_eq!(duplicate.items_failed, 1);
        drop_run(id).unwrap();
    }

    #[test]
    fn real_fnv_gecko_idle_kf_bindings_resolve_against_preserved_rig() {
        let skeleton = gecko_skeleton_path();
        let idle = gecko_idle_path();
        assert!(skeleton.is_file(), "missing {}", skeleton.display());
        assert!(idle.is_file(), "missing {}", idle.display());
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("Idle.hkx");
        crate::phase::animations::convert_kf_to_hkx(
            &idle,
            &output,
            &HashMap::new(),
            "creatures/nvgecko/mtidle.kf",
            "Actors/B21_FNVGecko/Animations/Idle.hkx",
            None,
            Some(&skeleton),
            Some("Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"),
            Some("NVGecko"),
            Some(30.0),
            crate::phase::animations::FnvExtractedMotionPolicy::RejectNonzero,
        )
        .unwrap();

        let xml = havok_native::api::havok_hkx_to_xml(&std::fs::read(output).unwrap()).unwrap();
        let document = roxmltree::Document::parse(&xml).unwrap();
        let binding = document
            .descendants()
            .find(|node| {
                node.has_tag_name("hkobject")
                    && node.attribute("class") == Some("hkaAnimationBinding")
            })
            .unwrap();
        let binding_param = |name: &str| {
            binding
                .children()
                .find(|node| node.has_tag_name("hkparam") && node.attribute("name") == Some(name))
                .unwrap()
        };
        assert_eq!(
            binding_param("originalSkeletonName")
                .text()
                .unwrap_or_default(),
            "NVGecko"
        );
        let indices = parse_i32_values(
            binding_param("transformTrackToBoneIndices")
                .text()
                .unwrap_or_default(),
        )
        .unwrap();
        assert_eq!(indices.len(), 84);
        assert!(indices.iter().all(|index| (0..87).contains(index)));
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
