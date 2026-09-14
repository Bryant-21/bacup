use std::collections::BTreeMap;

use super::manifest::{
    CapabilityClipRole, CapabilityGeneratorBlendingTransitionEffect,
    CapabilityGeneratorEventProperty, CapabilityGeneratorEventRef, CapabilityGeneratorNode,
    CapabilityGeneratorTransitionArray, CapabilityGraphManifest, CapabilityModifierNode,
    CapabilityRoleGenerator, CapabilityVariableBindingType, ClipDecl, CreatureClipRole,
    CreatureGraphTemplate, CreatureManifest, EventDecl, EventUsage, GraphDeclarations,
    MvpGraphManifest, MvpMotionManifest, OverlayClipRole, PropertyDecl, RAGDOLL_ENTER_EVENTS,
    RAGDOLL_TRANSITION_EVENTS, RagdollDisposition, SourceOwnedPoweredRagdollConfig,
    ValidationErrors, VariableDecl, VariableType, VariableValue,
};
use super::xml::{validate_fo4_havok_xml_signatures, validate_havok_xml};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScaffoldArtifact {
    pub runtime_path: String,
    pub source_xml_path: String,
    pub xml: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRigScaffold {
    pub race_visual_skeleton_nif: String,
    pub animation_clips: Vec<String>,
    pub artifacts: Vec<ScaffoldArtifact>,
}

impl SourceRigScaffold {
    pub fn artifact(&self, runtime_path: &str) -> Option<&ScaffoldArtifact> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.runtime_path.eq_ignore_ascii_case(runtime_path))
    }

    pub fn runtime_paths(&self) -> impl Iterator<Item = &str> {
        self.artifacts
            .iter()
            .map(|artifact| artifact.runtime_path.as_str())
    }
}

pub fn emit_idle_scaffold(
    manifest: &CreatureManifest,
) -> Result<SourceRigScaffold, ValidationErrors> {
    manifest.validate()?;

    emit_scaffold(
        manifest,
        emit_root_behavior(manifest, &manifest.root),
        emit_core_behavior(manifest),
        [manifest.idle_clip.as_str()].as_slice(),
    )
}

pub fn emit_mvp_scaffold(
    manifest: &CreatureManifest,
    graph: &MvpGraphManifest,
) -> Result<SourceRigScaffold, ValidationErrors> {
    emit_mvp_scaffold_with_motion(manifest, graph, &MvpMotionManifest::default())
}

pub fn emit_mvp_scaffold_with_motion(
    manifest: &CreatureManifest,
    graph: &MvpGraphManifest,
    motion: &MvpMotionManifest,
) -> Result<SourceRigScaffold, ValidationErrors> {
    manifest.validate_mvp_motion(graph, motion)?;
    let capability = CapabilityGraphManifest::from_mvp(graph, motion);
    emit_capability_scaffold_validated(manifest, &capability)
}

pub fn emit_capability_scaffold(
    manifest: &CreatureManifest,
    graph: &CapabilityGraphManifest,
) -> Result<SourceRigScaffold, ValidationErrors> {
    manifest.validate_capability_graph(graph)?;
    emit_capability_scaffold_validated(manifest, graph)
}

fn emit_capability_scaffold_validated(
    manifest: &CreatureManifest,
    graph: &CapabilityGraphManifest,
) -> Result<SourceRigScaffold, ValidationErrors> {
    let roles = ordered_capability_roles(graph);
    let mut clip_names: Vec<&str> = roles
        .iter()
        .flat_map(|role| role.generator.clip_names(&role.clip_name))
        .collect();
    clip_names.extend(
        graph
            .overlays
            .iter()
            .map(|overlay| overlay.clip_name.as_str()),
    );
    let root_declarations = declarations_with_capability(&manifest.root, graph);
    let core_declarations = declarations_with_capability(&manifest.core, graph);
    emit_scaffold(
        manifest,
        emit_root_behavior(manifest, &root_declarations),
        emit_capability_core_behavior(manifest, graph, &roles, &core_declarations),
        &clip_names,
    )
}

fn emit_scaffold(
    manifest: &CreatureManifest,
    root_xml: String,
    core_xml: String,
    core_clip_names: &[&str],
) -> Result<SourceRigScaffold, ValidationErrors> {
    let project = ScaffoldArtifact {
        runtime_path: manifest.paths.project.clone(),
        source_xml_path: xml_source_path(&manifest.paths.project),
        xml: emit_project(manifest, core_clip_names),
    };
    let character = ScaffoldArtifact {
        runtime_path: manifest.paths.character.clone(),
        source_xml_path: xml_source_path(&manifest.paths.character),
        xml: emit_character(manifest, core_clip_names),
    };
    let root = ScaffoldArtifact {
        runtime_path: manifest.paths.root_behavior.clone(),
        source_xml_path: xml_source_path(&manifest.paths.root_behavior),
        xml: root_xml,
    };
    let core = ScaffoldArtifact {
        runtime_path: manifest.paths.core_behavior.clone(),
        source_xml_path: xml_source_path(&manifest.paths.core_behavior),
        xml: core_xml,
    };

    let scaffold = SourceRigScaffold {
        race_visual_skeleton_nif: manifest.visual_skeleton_nif.clone(),
        animation_clips: core_clip_names
            .iter()
            .filter_map(|name| manifest.clips.iter().find(|clip| clip.name == **name))
            .map(|clip| clip.path.clone())
            .collect(),
        artifacts: vec![project, character, root, core],
    };
    validate_scaffold(manifest, &scaffold, core_clip_names)?;
    Ok(scaffold)
}

fn validate_scaffold(
    manifest: &CreatureManifest,
    scaffold: &SourceRigScaffold,
    core_clip_names: &[&str],
) -> Result<(), ValidationErrors> {
    let mut errors = ValidationErrors::default();
    for artifact in &scaffold.artifacts {
        if let Err(document_errors) = validate_havok_xml(&artifact.xml) {
            for error in document_errors.0 {
                errors.push(
                    error.code,
                    format!("{}: {}", artifact.source_xml_path, error.message),
                );
            }
        }
        if let Err(signature_errors) = validate_fo4_havok_xml_signatures(&artifact.xml) {
            for error in signature_errors.0 {
                errors.push(
                    error.code,
                    format!("{}: {}", artifact.source_xml_path, error.message),
                );
            }
        }
    }

    let Some(project) = scaffold.artifact(&manifest.paths.project) else {
        errors.push("missing_artifact", "project XML artifact was not emitted");
        return errors.finish();
    };
    let Some(character) = scaffold.artifact(&manifest.paths.character) else {
        errors.push("missing_artifact", "character XML artifact was not emitted");
        return errors.finish();
    };
    let Some(root) = scaffold.artifact(&manifest.paths.root_behavior) else {
        errors.push(
            "missing_artifact",
            "root behavior XML artifact was not emitted",
        );
        return errors.finish();
    };
    let Some(core) = scaffold.artifact(&manifest.paths.core_behavior) else {
        errors.push(
            "missing_artifact",
            "core behavior XML artifact was not emitted",
        );
        return errors.finish();
    };

    for (label, path) in [
        ("character", &manifest.paths.character),
        ("root behavior", &manifest.paths.root_behavior),
        ("core behavior", &manifest.paths.core_behavior),
        ("animation skeleton", &manifest.animation_skeleton.path),
    ] {
        let internal_path = manifest
            .project_relative_havok_path(path)
            .expect("manifest validation keeps Havok references below the project root");
        let escaped = escape_xml(internal_path);
        let linked = if label == "animation skeleton" {
            character.xml.contains(&escaped)
        } else {
            project.xml.contains(&escaped)
                || (label == "root behavior" && character.xml.contains(&escaped))
        };
        if !linked {
            errors.push(
                "broken_scaffold_link",
                format!("emitted scaffold does not reference {label} path {path:?}"),
            );
        }
    }
    if let Some(ragdoll_path) = manifest.ragdoll.runtime_path() {
        let internal_path = manifest
            .project_relative_havok_path(ragdoll_path)
            .expect("manifest validation keeps ragdoll below the project root");
        if !character.xml.contains(&format!(
            "<hkparam name=\"ragdollName\">{}</hkparam>",
            escape_xml(internal_path)
        )) {
            errors.push(
                "broken_scaffold_link",
                format!("character does not reference source-owned ragdoll {ragdoll_path:?}"),
            );
        }
        for class_name in [
            "hkbStateMachine",
            "hkbPoweredRagdollControlsModifier",
            "hkbReferencePoseGenerator",
        ] {
            if !root.xml.contains(&format!("class=\"{class_name}\"")) {
                errors.push(
                    "missing_ragdoll_state",
                    format!("ragdoll-enabled root graph omits {class_name}"),
                );
            }
        }
    } else if !character.xml.contains("<hkparam name=\"ragdollName\"/>") {
        errors.push(
            "ragdoll_disposition",
            "explicit no-ragdoll character must emit an empty ragdollName element",
        );
    }

    if character
        .xml
        .contains(&escape_xml(&manifest.visual_skeleton_nif))
    {
        errors.push(
            "skeleton_contract",
            "character XML must not use the RACE visual Skeleton.nif as its Havok rig",
        );
    }
    if !root.xml.contains("class=\"BSBehaviorGraphSwapGenerator\"")
        || !root.xml.contains("<hkparam name=\"userData\">1</hkparam>")
    {
        errors.push(
            "missing_swap_generator",
            "root graph must contain a userData=1 BSBehaviorGraphSwapGenerator",
        );
    }
    for clip_name in core_clip_names {
        let clip = manifest
            .clips
            .iter()
            .find(|clip| clip.name == *clip_name)
            .expect("scaffold validation follows manifest validation");
        let internal_path = manifest
            .project_relative_havok_path(&clip.path)
            .expect("manifest validation keeps clip references below the project root");
        if !core.xml.contains(&format!(
            "<hkparam name=\"animationName\">{}</hkparam>",
            escape_xml(internal_path)
        )) {
            errors.push(
                "core_clip_link",
                format!("core graph does not reference clip {:?}", clip.path),
            );
        }
    }

    errors.finish()
}

fn emit_project(manifest: &CreatureManifest, core_clip_names: &[&str]) -> String {
    let animations = string_array(
        "animationFilenames",
        core_clip_names
            .iter()
            .filter_map(|name| manifest.clips.iter().find(|clip| clip.name == **name))
            .map(|clip| {
                manifest
                    .project_relative_havok_path(&clip.path)
                    .expect("manifest validation keeps clip references below the project root")
            }),
    );
    let behaviors = string_array(
        "behaviorFilenames",
        [
            internal_path(manifest, &manifest.paths.root_behavior),
            internal_path(manifest, &manifest.paths.core_behavior),
        ],
    );
    let characters = string_array(
        "characterFilenames",
        [internal_path(manifest, &manifest.paths.character)],
    );

    format!(
        "<?xml version=\"1.0\" encoding=\"ascii\"?>\n\
<hkpackfile classversion=\"11\" contentsversion=\"hk_2014.1.0-r1\" toplevelobject=\"#0090\">\n\
  <hksection name=\"__data__\">\n\
    <hkobject name=\"#0090\" class=\"hkRootLevelContainer\" signature=\"0x2772c11e\">\n\
      <hkparam name=\"namedVariants\" numelements=\"1\"><hkobject><hkparam name=\"name\">hkbProjectData</hkparam><hkparam name=\"className\">hkbProjectData</hkparam><hkparam name=\"variant\">#0091</hkparam></hkobject></hkparam>\n\
    </hkobject>\n\
    <hkobject name=\"#0091\" class=\"hkbProjectData\" signature=\"0x363c1159\">\n\
      <hkparam name=\"worldUpWS\">(0.0 0.0 1.0 0.0)</hkparam><hkparam name=\"stringData\">#0092</hkparam><hkparam name=\"defaultEventMode\">2</hkparam>\n\
    </hkobject>\n\
    <hkobject name=\"#0092\" class=\"hkbProjectStringData\" signature=\"0xca08c2ba\">\n\
{animations}{behaviors}{characters}      <hkparam name=\"eventNames\" numelements=\"0\"/>\n\
      <hkparam name=\"animationPath\"/><hkparam name=\"behaviorPath\"/><hkparam name=\"characterPath\"/><hkparam name=\"scriptsPath\">\\</hkparam><hkparam name=\"fullPathToSource\"/>\n\
    </hkobject>\n\
  </hksection>\n\
</hkpackfile>\n"
    )
}

fn emit_character(manifest: &CreatureManifest, core_clip_names: &[&str]) -> String {
    let property_names = string_array(
        "characterPropertyNames",
        manifest
            .root
            .character_properties
            .iter()
            .map(|property| property.name.as_str()),
    );
    let property_values = value_array(
        "wordVariableValues",
        manifest
            .root
            .character_properties
            .iter()
            .map(|property| property.initial_value),
    );
    let property_infos = property_info_array(&manifest.root.character_properties);
    let asset_names = string_array(
        "assetNames",
        core_clip_names
            .iter()
            .filter_map(|name| manifest.clips.iter().find(|clip| clip.name == **name))
            .map(|clip| file_name(&clip.path)),
    );
    let ragdoll_name = manifest
        .ragdoll
        .runtime_path()
        .map(|path| {
            format!(
                "<hkparam name=\"ragdollName\">{}</hkparam>",
                escape_xml(internal_path(manifest, path))
            )
        })
        .unwrap_or_else(|| "<hkparam name=\"ragdollName\"/>".to_string());

    format!(
        "<?xml version=\"1.0\" encoding=\"ascii\"?>\n\
<hkpackfile classversion=\"11\" contentsversion=\"hk_2014.1.0-r1\" toplevelobject=\"#0090\">\n\
  <hksection name=\"__data__\">\n\
    <hkobject name=\"#0090\" class=\"hkRootLevelContainer\" signature=\"0x2772c11e\">\n\
      <hkparam name=\"namedVariants\" numelements=\"1\"><hkobject><hkparam name=\"name\">hkbCharacterData</hkparam><hkparam name=\"className\">hkbCharacterData</hkparam><hkparam name=\"variant\">#0093</hkparam></hkobject></hkparam>\n\
    </hkobject>\n\
    <hkobject name=\"#0091\" class=\"hkbCharacterStringData\" signature=\"0xb9d8a52\">\n\
      <hkparam name=\"skinNames\" numelements=\"1\"><hkobject><hkparam name=\"fileName\">{rig}</hkparam><hkparam name=\"meshName\">{creature}</hkparam></hkobject></hkparam>\n\
      <hkparam name=\"boneAttachmentNames\" numelements=\"0\"/>\n\
      <hkparam name=\"animationBundleNameData\" numelements=\"1\"><hkobject><hkparam name=\"bundleName\"/>{asset_names}</hkobject></hkparam>\n\
      <hkparam name=\"animationBundleFilenameData\" numelements=\"0\"/>\n\
{property_names}      <hkparam name=\"retargetingSkeletonMapperFilenames\" numelements=\"0\"/><hkparam name=\"lodNames\" numelements=\"0\"/><hkparam name=\"mirroredSyncPointSubstringsA\" numelements=\"0\"/><hkparam name=\"mirroredSyncPointSubstringsB\" numelements=\"0\"/>\n\
      <hkparam name=\"name\">{creature}Character</hkparam><hkparam name=\"rigName\">{rig}</hkparam>{ragdoll_name}<hkparam name=\"behaviorFilename\">{root_behavior}</hkparam>\n\
      <hkparam name=\"luaScriptOnCharacterActivated\"/><hkparam name=\"luaScriptOnCharacterDeactivated\"/><hkparam name=\"luaFiles\" numelements=\"0\"/>\n\
    </hkobject>\n\
    <hkobject name=\"#0092\" class=\"hkbVariableValueSet\" signature=\"0xeb5f7e25\">\n\
{property_values}      <hkparam name=\"quadVariableValues\" numelements=\"0\"/><hkparam name=\"variantVariableValues\" numelements=\"0\"/>\n\
    </hkobject>\n\
    <hkobject name=\"#0093\" class=\"hkbCharacterData\" signature=\"0xfec46c1f\">\n\
      <hkparam name=\"characterControllerSetup\"><hkobject class=\"hkbCharacterControllerSetup\" name=\"characterControllerSetup\" signature=\"0xaf5f7339\"><hkparam name=\"rigidBodySetup\"><hkobject class=\"hkbRigidBodySetup\" name=\"rigidBodySetup\" signature=\"0x3b082f95\"><hkparam name=\"collisionFilterInfo\">{collision_filter_info}</hkparam><hkparam name=\"type\">{rigid_body_type}</hkparam><hkparam name=\"shapeSetup\"><hkobject class=\"hkbShapeSetup\" name=\"shapeSetup\" signature=\"0xd7ff86be\"><hkparam name=\"capsuleHeight\">{height:.6}</hkparam><hkparam name=\"capsuleRadius\">{radius:.6}</hkparam><hkparam name=\"fileName\"/><hkparam name=\"type\">CAPSULE</hkparam></hkobject></hkparam></hkobject></hkparam><hkparam name=\"controllerCinfo\">null</hkparam></hkobject></hkparam>\n\
      <hkparam name=\"modelUpMS\">{model_up}</hkparam><hkparam name=\"modelForwardMS\">{model_forward}</hkparam><hkparam name=\"modelRightMS\">{model_right}</hkparam>\n\
{property_infos}      <hkparam name=\"numBonesPerLod\" numelements=\"0\"/><hkparam name=\"characterPropertyValues\">#0092</hkparam><hkparam name=\"footIkDriverInfo\">null</hkparam><hkparam name=\"handIkDriverInfo\">null</hkparam><hkparam name=\"aiControlDriverInfo\">null</hkparam><hkparam name=\"stringData\">#0091</hkparam><hkparam name=\"mirroredSkeletonInfo\">null</hkparam><hkparam name=\"boneAttachmentBoneIndices\" numelements=\"0\"/><hkparam name=\"boneAttachmentTransforms\" numelements=\"0\"/><hkparam name=\"scale\">{model_scale}</hkparam>\n\
    </hkobject>\n\
  </hksection>\n\
</hkpackfile>\n",
        rig = escape_xml(internal_path(manifest, &manifest.animation_skeleton.path)),
        creature = escape_xml(&manifest.creature_name),
        root_behavior = escape_xml(internal_path(manifest, &manifest.paths.root_behavior)),
        ragdoll_name = ragdoll_name,
        height = manifest.capsule.height,
        radius = manifest.capsule.radius,
        collision_filter_info = manifest.controller.collision_filter_info,
        rigid_body_type = manifest.controller.rigid_body_type,
        model_up = vector4(manifest.controller.model_up_ms),
        model_forward = vector4(manifest.controller.model_forward_ms),
        model_right = vector4(manifest.controller.model_right_ms),
        model_scale = manifest.controller.model_scale,
    )
}

fn vector4(value: [f32; 4]) -> String {
    format!("({} {} {} {})", value[0], value[1], value[2], value[3])
}

fn emit_root_behavior(manifest: &CreatureManifest, declarations: &GraphDeclarations) -> String {
    let generator = match &manifest.ragdoll {
        RagdollDisposition::SourceOwned { receipt, .. } => {
            emit_powered_ragdoll_root(declarations, receipt.powered_ragdoll.as_ref())
        }
        RagdollDisposition::NoRagdoll { .. } => "    <hkobject name=\"#0093\" class=\"BSBehaviorGraphSwapGenerator\" signature=\"0xe7633191\">\n      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">1</hkparam><hkparam name=\"name\">BSBehaviorGraphSwapGenerator</hkparam><hkparam name=\"pDefaultGenerator\">null</hkparam>\n    </hkobject>\n".to_string(),
    };
    emit_behavior_graph(
        &format!("{}RootBehavior", manifest.creature_name),
        declarations,
        &generator,
    )
}

fn emit_powered_ragdoll_root(
    declarations: &GraphDeclarations,
    config: Option<&SourceOwnedPoweredRagdollConfig>,
) -> String {
    let event_index = |name: &str| {
        declarations
            .events
            .iter()
            .position(|event| event.name == name)
            .expect("ragdoll manifest validation guarantees root event declarations")
    };
    let ragdoll = event_index(RAGDOLL_TRANSITION_EVENTS[0]);
    let ragdoll_instant = event_index(RAGDOLL_TRANSITION_EVENTS[1]);
    let initialize = event_index(RAGDOLL_TRANSITION_EVENTS[2]);
    let initialize_instant = event_index(RAGDOLL_TRANSITION_EVENTS[3]);
    let add_to_world = event_index(RAGDOLL_ENTER_EVENTS[0]);
    let remove_controller = event_index(RAGDOLL_ENTER_EVENTS[1]);
    let root_transitions = emit_root_transition_array(
        "#0202",
        &[(ragdoll, 1, "#0209"), (ragdoll_instant, 1, "null")],
    );
    let death_transitions = emit_root_transition_array(
        "#0203",
        &[(initialize, 0, "#0209"), (initialize_instant, 0, "null")],
    );
    let root_state = emit_state_info("#0200", "RootState", 0, "#0202", "#0204", None);
    let ragdoll_state =
        emit_state_info("#0201", "RagdollState", 1, "#0203", "#0207", Some("#0208"));
    let (bones_ref, bones_object, pose_matching_bones) = config.map_or_else(
        || ("null", String::new(), [-1_i32; 3]),
        |config| {
            let feedback_bones = config
                .source
                .feedback
                .as_ref()
                .filter(|_| config.source.enabled_feedback)
                .map(|feedback| feedback.bones.as_slice())
                .unwrap_or_default();
            let bones_object = if feedback_bones.is_empty() {
                String::new()
            } else {
                format!(
                    "    <hkobject name=\"#0210\" class=\"hkbBoneIndexArray\" signature=\"0x3d26f425\"><hkparam name=\"boneIndices\" numelements=\"{}\">{}</hkparam></hkobject>\n",
                    feedback_bones.len(),
                    feedback_bones
                        .iter()
                        .map(u16::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            };
            let pose_matching_bones = config
                .source
                .pose_matching
                .as_ref()
                .filter(|_| config.source.enabled_pose_matching)
                .map(|pose| {
                    pose.matching_bones
                        .map(|bone| bone.map(i32::from).unwrap_or(-1))
                })
                .unwrap_or([-1; 3]);
            (
                if bones_object.is_empty() { "null" } else { "#0210" },
                bones_object,
                pose_matching_bones,
            )
        },
    );
    format!(
        "    <hkobject name=\"#0204\" class=\"BSBehaviorGraphSwapGenerator\" signature=\"0xe7633191\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">1</hkparam><hkparam name=\"name\">BSBehaviorGraphSwapGenerator</hkparam><hkparam name=\"pDefaultGenerator\">null</hkparam>\n\
    </hkobject>\n\
    <hkobject name=\"#0205\" class=\"hkbPoweredRagdollControlsModifier\" signature=\"0x522211cb\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">PoweredRagdollDeath</hkparam><hkparam name=\"enable\">true</hkparam>\n\
      <hkparam name=\"controlData\"><hkobject><hkparam name=\"maxForce\">50.0</hkparam><hkparam name=\"tau\">0.8</hkparam><hkparam name=\"damping\">1.0</hkparam><hkparam name=\"proportionalRecoveryVelocity\">2.0</hkparam><hkparam name=\"constantRecoveryVelocity\">1.0</hkparam></hkobject></hkparam>\n\
      <hkparam name=\"bones\">{bones_ref}</hkparam><hkparam name=\"worldFromModelModeData\"><hkobject><hkparam name=\"poseMatchingBone0\">{pose_matching_bone0}</hkparam><hkparam name=\"poseMatchingBone1\">{pose_matching_bone1}</hkparam><hkparam name=\"poseMatchingBone2\">{pose_matching_bone2}</hkparam><hkparam name=\"mode\">WORLD_FROM_MODEL_MODE_RAGDOLL</hkparam></hkobject></hkparam><hkparam name=\"boneWeights\">null</hkparam><hkparam name=\"animationBlendFraction\">0.0</hkparam>\n\
    </hkobject>\n\
    <hkobject name=\"#0206\" class=\"hkbReferencePoseGenerator\" signature=\"0xbc1536ee\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">RagdollReferencePose</hkparam></hkobject>\n\
    <hkobject name=\"#0207\" class=\"hkbModifierGenerator\" signature=\"0xc499fc9e\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">PoweredRagdollDeathGenerator</hkparam><hkparam name=\"modifier\">#0205</hkparam><hkparam name=\"generator\">#0206</hkparam></hkobject>\n\
    <hkobject name=\"#0208\" class=\"hkbStateMachineEventPropertyArray\" signature=\"0x71957c2d\"><hkparam name=\"events\" numelements=\"2\"><hkobject><hkparam name=\"id\">{add_to_world}</hkparam><hkparam name=\"payload\">null</hkparam></hkobject><hkobject><hkparam name=\"id\">{remove_controller}</hkparam><hkparam name=\"payload\">null</hkparam></hkobject></hkparam></hkobject>\n\
    <hkobject name=\"#0209\" class=\"hkbBlendingTransitionEffect\" signature=\"0x14e54c5c\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">RagdollBlend</hkparam><hkparam name=\"selfTransitionMode\">SELF_TRANSITION_MODE_CONTINUE_IF_CYCLIC_BLEND_IF_ACYCLIC</hkparam><hkparam name=\"eventMode\">EVENT_MODE_DEFAULT</hkparam><hkparam name=\"duration\">0.5</hkparam><hkparam name=\"toGeneratorStartTimeFraction\">0.0</hkparam><hkparam name=\"flags\">FLAG_NONE</hkparam><hkparam name=\"endMode\">END_MODE_NONE</hkparam><hkparam name=\"blendCurve\">0</hkparam><hkparam name=\"alignmentBone\">-1</hkparam></hkobject>\n\
{bones_object}{root_transitions}{death_transitions}{root_state}{ragdoll_state}    <hkobject name=\"#0093\" class=\"hkbStateMachine\" signature=\"0xa5896bcf\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">RootDeathSM</hkparam><hkparam name=\"eventToSendWhenStateOrTransitionChanges\"><hkobject><hkparam name=\"id\">-1</hkparam><hkparam name=\"payload\">null</hkparam></hkobject></hkparam><hkparam name=\"startStateIdSelector\">null</hkparam><hkparam name=\"startStateId\">0</hkparam><hkparam name=\"returnToPreviousStateEventId\">-1</hkparam><hkparam name=\"randomTransitionEventId\">-1</hkparam><hkparam name=\"transitionToNextHigherStateEventId\">-1</hkparam><hkparam name=\"transitionToNextLowerStateEventId\">-1</hkparam><hkparam name=\"syncVariableIndex\">-1</hkparam><hkparam name=\"wrapAroundStateId\">false</hkparam><hkparam name=\"maxSimultaneousTransitions\">1</hkparam><hkparam name=\"startStateMode\">START_STATE_MODE_DEFAULT</hkparam><hkparam name=\"selfTransitionMode\">SELF_TRANSITION_MODE_NO_TRANSITION</hkparam><hkparam name=\"states\" numelements=\"2\">#0200 #0201</hkparam><hkparam name=\"wildcardTransitions\">null</hkparam>\n\
    </hkobject>\n",
        bones_ref = bones_ref,
        bones_object = bones_object,
        pose_matching_bone0 = pose_matching_bones[0],
        pose_matching_bone1 = pose_matching_bones[1],
        pose_matching_bone2 = pose_matching_bones[2],
    )
}

fn emit_root_transition_array(id: &str, transitions: &[(usize, i32, &str)]) -> String {
    let mut output = format!(
        "    <hkobject name=\"{id}\" class=\"hkbStateMachineTransitionInfoArray\" signature=\"0x704a19af\"><hkparam name=\"transitions\" numelements=\"{}\">\n",
        transitions.len()
    );
    for (event_id, to_state_id, effect) in transitions {
        output.push_str(&format!(
            "      <hkobject><hkparam name=\"triggerInterval\"><hkobject><hkparam name=\"enterEventId\">-1</hkparam><hkparam name=\"exitEventId\">-1</hkparam><hkparam name=\"enterTime\">0.0</hkparam><hkparam name=\"exitTime\">0.0</hkparam></hkobject></hkparam><hkparam name=\"initiateInterval\"><hkobject><hkparam name=\"enterEventId\">-1</hkparam><hkparam name=\"exitEventId\">-1</hkparam><hkparam name=\"enterTime\">0.0</hkparam><hkparam name=\"exitTime\">0.0</hkparam></hkobject></hkparam><hkparam name=\"transition\">{effect}</hkparam><hkparam name=\"condition\">null</hkparam><hkparam name=\"eventId\">{event_id}</hkparam><hkparam name=\"toStateId\">{to_state_id}</hkparam><hkparam name=\"fromNestedStateId\">0</hkparam><hkparam name=\"toNestedStateId\">0</hkparam><hkparam name=\"priority\">0</hkparam><hkparam name=\"flags\">0</hkparam></hkobject>\n"
        ));
    }
    output.push_str("    </hkparam></hkobject>\n");
    output
}

fn emit_core_behavior(manifest: &CreatureManifest) -> String {
    let idle = manifest
        .clips
        .iter()
        .find(|clip| clip.name == manifest.idle_clip)
        .expect("manifest validation guarantees one idle clip");
    let mode = if idle.looping {
        "MODE_LOOPING"
    } else {
        "MODE_SINGLE_PLAY"
    };
    let generator = format!(
        "    <hkobject name=\"#0093\" class=\"hkbClipGenerator\" signature=\"0xd4cc9f6\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"animationBundleName\"/><hkparam name=\"animationName\">{animation}</hkparam><hkparam name=\"triggers\">null</hkparam><hkparam name=\"userPartitionMask\">0</hkparam><hkparam name=\"cropStartAmountLocalTime\">0.0</hkparam><hkparam name=\"cropEndAmountLocalTime\">0.0</hkparam><hkparam name=\"startTime\">0.0</hkparam><hkparam name=\"playbackSpeed\">1.0</hkparam><hkparam name=\"enforcedDuration\">0.0</hkparam><hkparam name=\"userControlledTimeFraction\">0.0</hkparam><hkparam name=\"animationBindingIndex\">-1</hkparam><hkparam name=\"mode\">{mode}</hkparam><hkparam name=\"flags\">0</hkparam>\n\
    </hkobject>\n",
        name = escape_xml(&idle.name),
        animation = escape_xml(internal_path(manifest, &idle.path)),
    );
    emit_behavior_graph(
        &format!("{}CoreBehavior", manifest.creature_name),
        &manifest.core,
        &generator,
    )
}

fn ordered_capability_roles(graph: &CapabilityGraphManifest) -> Vec<&CapabilityClipRole> {
    let mut roles: Vec<&CapabilityClipRole> = graph.roles.iter().collect();
    roles.sort_by_key(|role| capability_role_order(graph.template, role.role));
    roles
}

fn capability_role_order(template: CreatureGraphTemplate, role: CreatureClipRole) -> u8 {
    match template {
        CreatureGraphTemplate::Swim => match role {
            CreatureClipRole::SwimIdle => 0,
            CreatureClipRole::SwimForward => 1,
            CreatureClipRole::MeleeAttack => 2,
            CreatureClipRole::ProjectileAttack => 3,
            _ => 10,
        },
        CreatureGraphTemplate::Fly => match role {
            CreatureClipRole::FlyIdle => 0,
            CreatureClipRole::FlyForward => 1,
            CreatureClipRole::MeleeAttack => 2,
            CreatureClipRole::ProjectileAttack => 3,
            _ => 10,
        },
        CreatureGraphTemplate::GroundSwim => match role {
            CreatureClipRole::Idle => 0,
            CreatureClipRole::GroundForward => 1,
            CreatureClipRole::SwimIdle => 2,
            CreatureClipRole::SwimForward => 3,
            CreatureClipRole::TurnLeft90 => 4,
            CreatureClipRole::TurnRight90 => 5,
            CreatureClipRole::MeleeAttack => 6,
            CreatureClipRole::ProjectileAttack => 7,
            _ => 10,
        },
        CreatureGraphTemplate::GroundFly => match role {
            CreatureClipRole::Idle => 0,
            CreatureClipRole::GroundForward => 1,
            CreatureClipRole::FlyIdle => 2,
            CreatureClipRole::FlyForward => 3,
            CreatureClipRole::TurnLeft90 => 4,
            CreatureClipRole::TurnRight90 => 5,
            CreatureClipRole::MeleeAttack => 6,
            CreatureClipRole::ProjectileAttack => 7,
            _ => 10,
        },
        _ => match role {
            CreatureClipRole::Idle | CreatureClipRole::StationaryIdle => 0,
            CreatureClipRole::GroundForward => 1,
            CreatureClipRole::TurnLeft90 => 2,
            CreatureClipRole::TurnRight90 => 3,
            CreatureClipRole::MeleeAttack => 4,
            CreatureClipRole::ProjectileAttack => 5,
            CreatureClipRole::ContinuousAttackStart => 6,
            CreatureClipRole::ContinuousAttackLoop => 7,
            CreatureClipRole::ContinuousAttackStop => 8,
            CreatureClipRole::SwimIdle
            | CreatureClipRole::SwimForward
            | CreatureClipRole::FlyIdle
            | CreatureClipRole::FlyForward => 10,
        },
    }
}

fn emit_capability_role_generator(
    manifest: &CreatureManifest,
    role: &CapabilityClipRole,
    state_index: usize,
    declarations: &GraphDeclarations,
) -> String {
    let root_id = format!("#010{state_index}");
    match &role.generator {
        CapabilityRoleGenerator::Single => {
            let clip = manifest
                .clips
                .iter()
                .find(|clip| clip.name == role.clip_name)
                .expect("capability validation guarantees every role clip");
            emit_clip_generator(manifest, &root_id, &role.state_name, clip)
        }
        CapabilityRoleGenerator::ManualSelector {
            children,
            selected_generator_index,
            selected_index_can_change_after_activate,
            selected_index_variable,
        } => {
            let mut output = String::new();
            let child_ids = children
                .iter()
                .enumerate()
                .map(|(child_index, clip_name)| {
                    let child_id = capability_child_id(3, state_index, child_index);
                    let clip = manifest
                        .clips
                        .iter()
                        .find(|clip| clip.name == *clip_name)
                        .expect("capability validation guarantees selector children");
                    output.push_str(&emit_clip_generator(
                        manifest,
                        &child_id,
                        &format!("{}_{child_index}", role.state_name),
                        clip,
                    ));
                    child_id
                })
                .collect::<Vec<_>>();
            let variable_binding = selected_index_variable.as_ref().map(|variable| {
                let binding_id = capability_binding_id(state_index);
                let variable_index = declarations
                    .variables
                    .iter()
                    .position(|candidate| candidate.name == *variable)
                    .expect("capability validation guarantees selector variable");
                output.push_str(&format!(
                    "    <hkobject name=\"{binding_id}\" class=\"hkbVariableBindingSet\" signature=\"0xe942f339\">\n\
      <hkparam name=\"bindings\" numelements=\"1\">\n{}      </hkparam><hkparam name=\"indexOfBindingToEnable\">-1</hkparam>\n\
    </hkobject>\n",
                    emit_variable_binding(
                        "selectedGeneratorIndex",
                        variable_index,
                        "BINDING_TYPE_VARIABLE"
                    )
                ));
                binding_id
            });
            let binding_ref = variable_binding.as_deref().unwrap_or("null");
            output.push_str(&format!(
                "    <hkobject name=\"{root_id}\" class=\"hkbManualSelectorGenerator\" signature=\"0xeed8d5cd\">\n\
      <hkparam name=\"variableBindingSet\">{binding_ref}</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{name}</hkparam>\n\
      <hkparam name=\"generators\" numelements=\"{child_count}\">{child_refs}</hkparam><hkparam name=\"selectedGeneratorIndex\">{selected_generator_index}</hkparam><hkparam name=\"indexSelector\">null</hkparam><hkparam name=\"selectedIndexCanChangeAfterActivate\">{selected_index_can_change_after_activate}</hkparam><hkparam name=\"generatorChangedTransitionEffect\">null</hkparam>\n\
    </hkobject>\n",
                name = escape_xml(&role.state_name),
                child_count = child_ids.len(),
                child_refs = child_ids.join(" "),
            ));
            output
        }
        CapabilityRoleGenerator::Blender {
            children,
            reference_pose_weight_threshold_bits,
            blend_parameter_bits,
            min_cyclic_blend_parameter_bits,
            max_cyclic_blend_parameter_bits,
            index_of_sync_master_child,
            flags,
            subtract_last_child,
            blend_parameter_variable,
        } => {
            let mut output = String::new();
            let child_ids = children
                .iter()
                .enumerate()
                .map(|(child_index, child)| {
                    let clip_id = capability_child_id(3, state_index, child_index);
                    let child_id = capability_child_id(4, state_index, child_index);
                    let clip = manifest
                        .clips
                        .iter()
                        .find(|clip| clip.name == child.clip_name)
                        .expect("capability validation guarantees blender children");
                    output.push_str(&emit_clip_generator(
                        manifest,
                        &clip_id,
                        &format!("{}_{child_index}", role.state_name),
                        clip,
                    ));
                    output.push_str(&format!(
                        "    <hkobject name=\"{child_id}\" class=\"hkbBlenderGeneratorChild\" signature=\"0xb35bbfd3\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"generator\">{clip_id}</hkparam><hkparam name=\"boneWeights\">null</hkparam><hkparam name=\"weight\">{weight}</hkparam><hkparam name=\"worldFromModelWeight\">{world_weight}</hkparam>\n\
    </hkobject>\n",
                        weight = f32::from_bits(child.weight_bits),
                        world_weight = f32::from_bits(child.world_from_model_weight_bits),
                    ));
                    child_id
                })
                .collect::<Vec<_>>();
            let variable_binding = blend_parameter_variable.as_ref().map(|variable| {
                let binding_id = capability_binding_id(state_index);
                let variable_index = declarations
                    .variables
                    .iter()
                    .position(|candidate| candidate.name == *variable)
                    .expect("capability validation guarantees blender variable");
                output.push_str(&format!(
                    "    <hkobject name=\"{binding_id}\" class=\"hkbVariableBindingSet\" signature=\"0xe942f339\">\n\
      <hkparam name=\"bindings\" numelements=\"1\">\n{}      </hkparam><hkparam name=\"indexOfBindingToEnable\">-1</hkparam>\n\
    </hkobject>\n",
                    emit_variable_binding(
                        "blendParameter",
                        variable_index,
                        "BINDING_TYPE_VARIABLE"
                    )
                ));
                binding_id
            });
            let binding_ref = variable_binding.as_deref().unwrap_or("null");
            output.push_str(&format!(
                "    <hkobject name=\"{root_id}\" class=\"hkbBlenderGenerator\" signature=\"0xce45c088\">\n\
      <hkparam name=\"variableBindingSet\">{binding_ref}</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"referencePoseWeightThreshold\">{reference_threshold}</hkparam><hkparam name=\"blendParameter\">{blend_parameter}</hkparam><hkparam name=\"minCyclicBlendParameter\">{min_cyclic}</hkparam><hkparam name=\"maxCyclicBlendParameter\">{max_cyclic}</hkparam><hkparam name=\"indexOfSyncMasterChild\">{index_of_sync_master_child}</hkparam><hkparam name=\"flags\">{flags}</hkparam><hkparam name=\"subtractLastChild\">{subtract_last_child}</hkparam><hkparam name=\"children\" numelements=\"{child_count}\">{child_refs}</hkparam>\n\
    </hkobject>\n",
                name = escape_xml(&role.state_name),
                reference_threshold = f32::from_bits(*reference_pose_weight_threshold_bits),
                blend_parameter = f32::from_bits(*blend_parameter_bits),
                min_cyclic = f32::from_bits(*min_cyclic_blend_parameter_bits),
                max_cyclic = f32::from_bits(*max_cyclic_blend_parameter_bits),
                child_count = child_ids.len(),
                child_refs = child_ids.join(" "),
            ));
            output
        }
        CapabilityRoleGenerator::Tree { root } => {
            let mut emitter = CapabilityTreeEmitter::new(manifest, declarations, state_index);
            emitter.emit_node(root, Some(root_id));
            emitter.output
        }
    }
}

struct CapabilityTreeEmitter<'a> {
    manifest: &'a CreatureManifest,
    declarations: &'a GraphDeclarations,
    state_index: usize,
    next_object: usize,
    transition_effect_ids: BTreeMap<(String, u32), String>,
    output: String,
}

impl<'a> CapabilityTreeEmitter<'a> {
    fn new(
        manifest: &'a CreatureManifest,
        declarations: &'a GraphDeclarations,
        state_index: usize,
    ) -> Self {
        Self {
            manifest,
            declarations,
            state_index,
            next_object: 0,
            transition_effect_ids: BTreeMap::new(),
            output: String::new(),
        }
    }

    fn allocate_id(&mut self) -> String {
        let id = format!("#8{:02}{:04}", self.state_index, self.next_object);
        self.next_object += 1;
        id
    }

    fn emit_node(
        &mut self,
        node: &CapabilityGeneratorNode,
        preferred_id: Option<String>,
    ) -> String {
        let id = preferred_id.unwrap_or_else(|| self.allocate_id());
        match node {
            CapabilityGeneratorNode::Clip { source, clip_name } => {
                let clip = self
                    .manifest
                    .clips
                    .iter()
                    .find(|clip| clip.name == *clip_name)
                    .expect("capability validation guarantees recursive clip declarations");
                self.output
                    .push_str(&emit_clip_generator(self.manifest, &id, &source.name, clip));
            }
            CapabilityGeneratorNode::ManualSelector {
                source,
                bindings,
                children,
                selected_generator_index,
                selected_index_can_change_after_activate,
                ..
            } => {
                let child_ids = children
                    .iter()
                    .map(|child| self.emit_node(child, None))
                    .collect::<Vec<_>>();
                let binding_ref = self.emit_bindings(bindings);
                self.output.push_str(&format!(
                    "    <hkobject name=\"{id}\" class=\"hkbManualSelectorGenerator\" signature=\"0xeed8d5cd\">\n\
      <hkparam name=\"variableBindingSet\">{binding_ref}</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{name}</hkparam>\n\
      <hkparam name=\"generators\" numelements=\"{child_count}\">{child_refs}</hkparam><hkparam name=\"selectedGeneratorIndex\">{selected_generator_index}</hkparam><hkparam name=\"indexSelector\">null</hkparam><hkparam name=\"selectedIndexCanChangeAfterActivate\">{selected_index_can_change_after_activate}</hkparam><hkparam name=\"generatorChangedTransitionEffect\">null</hkparam>\n\
    </hkobject>\n",
                    name = escape_xml(&source.name),
                    child_count = child_ids.len(),
                    child_refs = child_ids.join(" "),
                ));
            }
            CapabilityGeneratorNode::Blender {
                source,
                bindings,
                children,
                reference_pose_weight_threshold_bits,
                blend_parameter_bits,
                min_cyclic_blend_parameter_bits,
                max_cyclic_blend_parameter_bits,
                index_of_sync_master_child,
                flags,
                subtract_last_child,
            } => {
                let child_ids = children
                    .iter()
                    .map(|child| {
                        let generator_id = self.emit_node(&child.generator, None);
                        let child_id = self.allocate_id();
                        self.output.push_str(&format!(
                            "    <hkobject name=\"{child_id}\" class=\"hkbBlenderGeneratorChild\" signature=\"0xb35bbfd3\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"generator\">{generator_id}</hkparam><hkparam name=\"boneWeights\">null</hkparam><hkparam name=\"weight\">{weight}</hkparam><hkparam name=\"worldFromModelWeight\">{world_weight}</hkparam>\n\
    </hkobject>\n",
                            weight = f32::from_bits(child.weight_bits),
                            world_weight = f32::from_bits(child.world_from_model_weight_bits),
                        ));
                        child_id
                    })
                    .collect::<Vec<_>>();
                let binding_ref = self.emit_bindings(bindings);
                self.output.push_str(&format!(
                    "    <hkobject name=\"{id}\" class=\"hkbBlenderGenerator\" signature=\"0xce45c088\">\n\
      <hkparam name=\"variableBindingSet\">{binding_ref}</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"referencePoseWeightThreshold\">{reference_threshold}</hkparam><hkparam name=\"blendParameter\">{blend_parameter}</hkparam><hkparam name=\"minCyclicBlendParameter\">{min_cyclic}</hkparam><hkparam name=\"maxCyclicBlendParameter\">{max_cyclic}</hkparam><hkparam name=\"indexOfSyncMasterChild\">{index_of_sync_master_child}</hkparam><hkparam name=\"flags\">{flags}</hkparam><hkparam name=\"subtractLastChild\">{subtract_last_child}</hkparam><hkparam name=\"children\" numelements=\"{child_count}\">{child_refs}</hkparam>\n\
    </hkobject>\n",
                    name = escape_xml(&source.name),
                    reference_threshold = f32::from_bits(*reference_pose_weight_threshold_bits),
                    blend_parameter = f32::from_bits(*blend_parameter_bits),
                    min_cyclic = f32::from_bits(*min_cyclic_blend_parameter_bits),
                    max_cyclic = f32::from_bits(*max_cyclic_blend_parameter_bits),
                    child_count = child_ids.len(),
                    child_refs = child_ids.join(" "),
                ));
            }
            CapabilityGeneratorNode::StateMachine {
                source,
                bindings,
                event_to_send_when_state_or_transition_changes,
                start_state_id,
                return_to_previous_state_event,
                random_transition_event,
                transition_to_next_higher_state_event,
                transition_to_next_lower_state_event,
                sync_variable_index,
                wrap_around_state_id,
                max_simultaneous_transitions,
                states,
                wildcard_transitions,
                ..
            } => {
                let state_ids = states
                    .iter()
                    .map(|state| {
                        let generator_id = self.emit_node(&state.generator, None);
                        let transition_ref = state
                            .transitions
                            .as_ref()
                            .map(|transitions| self.emit_transition_array(transitions))
                            .unwrap_or_else(|| "null".to_string());
                        let enter_ref = self.emit_event_properties(&state.enter_notify_events);
                        let exit_ref = self.emit_event_properties(&state.exit_notify_events);
                        let state_id = self.allocate_id();
                        self.output.push_str(&format!(
                            "    <hkobject name=\"{state_id}\" class=\"hkbStateMachineStateInfo\" signature=\"0x39d76713\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"listeners\" numelements=\"0\"/><hkparam name=\"enterNotifyEvents\">{enter_ref}</hkparam><hkparam name=\"exitNotifyEvents\">{exit_ref}</hkparam><hkparam name=\"transitions\">{transition_ref}</hkparam><hkparam name=\"generator\">{generator_id}</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"stateId\">{source_state_id}</hkparam><hkparam name=\"probability\">{probability}</hkparam><hkparam name=\"enable\">{enable}</hkparam>\n\
    </hkobject>\n",
                            name = escape_xml(&state.name),
                            source_state_id = state.state_id,
                            probability = f32::from_bits(state.probability_bits),
                            enable = state.enable,
                        ));
                        state_id
                    })
                    .collect::<Vec<_>>();
                let wildcard_ref = wildcard_transitions
                    .as_ref()
                    .map(|transitions| self.emit_transition_array(transitions))
                    .unwrap_or_else(|| "null".to_string());
                let binding_ref = self.emit_bindings(bindings);
                self.output.push_str(&format!(
                    "    <hkobject name=\"{id}\" class=\"hkbStateMachine\" signature=\"0xa5896bcf\">\n\
      <hkparam name=\"variableBindingSet\">{binding_ref}</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"eventToSendWhenStateOrTransitionChanges\">{state_event}</hkparam><hkparam name=\"startStateIdSelector\">null</hkparam><hkparam name=\"startStateId\">{start_state_id}</hkparam><hkparam name=\"returnToPreviousStateEventId\">{return_event}</hkparam><hkparam name=\"randomTransitionEventId\">{random_event}</hkparam><hkparam name=\"transitionToNextHigherStateEventId\">{higher_event}</hkparam><hkparam name=\"transitionToNextLowerStateEventId\">{lower_event}</hkparam><hkparam name=\"syncVariableIndex\">{sync_variable_index}</hkparam><hkparam name=\"wrapAroundStateId\">{wrap_around_state_id}</hkparam><hkparam name=\"maxSimultaneousTransitions\">{max_simultaneous_transitions}</hkparam><hkparam name=\"startStateMode\">START_STATE_MODE_DEFAULT</hkparam><hkparam name=\"selfTransitionMode\">SELF_TRANSITION_MODE_NO_TRANSITION</hkparam><hkparam name=\"states\" numelements=\"{state_count}\">{state_refs}</hkparam><hkparam name=\"wildcardTransitions\">{wildcard_ref}</hkparam>\n\
    </hkobject>\n",
                    name = escape_xml(&source.name),
                    state_event = self.emit_inline_event_property(
                        event_to_send_when_state_or_transition_changes
                    ),
                    return_event = self.event_id(return_to_previous_state_event),
                    random_event = self.event_id(random_transition_event),
                    higher_event = self.event_id(transition_to_next_higher_state_event),
                    lower_event = self.event_id(transition_to_next_lower_state_event),
                    state_count = state_ids.len(),
                    state_refs = state_ids.join(" "),
                ));
            }
            CapabilityGeneratorNode::ModifierGenerator {
                source,
                bindings,
                user_data,
                modifier,
                generator,
            } => {
                let generator_ref = self.emit_node(generator, None);
                let modifier_ref = self.emit_modifier(modifier);
                let binding_ref = self.emit_bindings(bindings);
                self.output.push_str(&format!(
                    "    <hkobject name=\"{id}\" class=\"hkbModifierGenerator\" signature=\"0xc499fc9e\"><hkparam name=\"variableBindingSet\">{binding_ref}</hkparam><hkparam name=\"userData\">{user_data}</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"modifier\">{modifier_ref}</hkparam><hkparam name=\"generator\">{generator_ref}</hkparam></hkobject>\n",
                    name = escape_xml(&source.name),
                ));
            }
        }
        id
    }

    fn emit_modifier(&mut self, modifier: &CapabilityModifierNode) -> String {
        let id = self.allocate_id();
        match modifier {
            CapabilityModifierNode::List {
                source,
                bindings,
                user_data,
                enable,
                modifiers,
            } => {
                let modifier_refs = modifiers
                    .iter()
                    .map(|modifier| self.emit_modifier(modifier))
                    .collect::<Vec<_>>();
                let binding_ref = self.emit_bindings(bindings);
                self.output.push_str(&format!(
                    "    <hkobject name=\"{id}\" class=\"hkbModifierList\" signature=\"0x0ded564c\"><hkparam name=\"variableBindingSet\">{binding_ref}</hkparam><hkparam name=\"userData\">{user_data}</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"enable\">{enable}</hkparam><hkparam name=\"modifiers\" numelements=\"{count}\">{refs}</hkparam></hkobject>\n",
                    name = escape_xml(&source.name),
                    count = modifier_refs.len(),
                    refs = modifier_refs.join(" "),
                ));
            }
            CapabilityModifierNode::Damping {
                source,
                bindings,
                user_data,
                enable,
                k_p_bits,
                k_i_bits,
                k_d_bits,
                enable_scalar_damping,
                enable_vector_damping,
                raw_value_bits,
                damped_value_bits,
                raw_vector_bits,
                damped_vector_bits,
                vector_error_sum_bits,
                vector_previous_error_bits,
                error_sum_bits,
                previous_error_bits,
            } => {
                let binding_ref = self.emit_bindings(bindings);
                self.output.push_str(&format!(
                    "    <hkobject name=\"{id}\" class=\"hkbDampingModifier\" signature=\"0x68a51d05\"><hkparam name=\"variableBindingSet\">{binding_ref}</hkparam><hkparam name=\"userData\">{user_data}</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"enable\">{enable}</hkparam><hkparam name=\"kP\">{k_p}</hkparam><hkparam name=\"kI\">{k_i}</hkparam><hkparam name=\"kD\">{k_d}</hkparam><hkparam name=\"enableScalarDamping\">{enable_scalar_damping}</hkparam><hkparam name=\"enableVectorDamping\">{enable_vector_damping}</hkparam><hkparam name=\"rawValue\">{raw_value}</hkparam><hkparam name=\"dampedValue\">{damped_value}</hkparam><hkparam name=\"rawVector\">{raw_vector}</hkparam><hkparam name=\"dampedVector\">{damped_vector}</hkparam><hkparam name=\"vecErrorSum\">{error_vector}</hkparam><hkparam name=\"vecPreviousError\">{previous_error_vector}</hkparam><hkparam name=\"errorSum\">{error_sum}</hkparam><hkparam name=\"previousError\">{previous_error}</hkparam></hkobject>\n",
                    name = escape_xml(&source.name),
                    k_p = f32::from_bits(*k_p_bits),
                    k_i = f32::from_bits(*k_i_bits),
                    k_d = f32::from_bits(*k_d_bits),
                    raw_value = f32::from_bits(*raw_value_bits),
                    damped_value = f32::from_bits(*damped_value_bits),
                    raw_vector = format_vector4(*raw_vector_bits),
                    damped_vector = format_vector4(*damped_vector_bits),
                    error_vector = format_vector4(*vector_error_sum_bits),
                    previous_error_vector = format_vector4(*vector_previous_error_bits),
                    error_sum = f32::from_bits(*error_sum_bits),
                    previous_error = f32::from_bits(*previous_error_bits),
                ));
            }
            CapabilityModifierNode::EvaluateExpression {
                source,
                bindings,
                user_data,
                enable,
                expressions,
            } => {
                let expression_ref = self.allocate_id();
                let rows = expressions
                    .expressions
                    .iter()
                    .map(|expression| {
                        let event_mode = match expression.event_mode {
                            0 => "EVENT_MODE_SEND_ONCE",
                            1 => "EVENT_MODE_SEND_ON_TRUE",
                            2 => "EVENT_MODE_SEND_ON_FALSE_TO_TRUE",
                            3 => "EVENT_MODE_SEND_EVERY_FRAME_ONCE_TRUE",
                            _ => unreachable!("capability validation guarantees expression event mode"),
                        };
                        format!(
                            "<hkobject><hkparam name=\"expression\">{}</hkparam><hkparam name=\"assignmentVariableIndex\">{}</hkparam><hkparam name=\"assignmentEventIndex\">{}</hkparam><hkparam name=\"eventMode\">{event_mode}</hkparam></hkobject>",
                            escape_xml(&expression.expression),
                            expression.assignment_variable_index,
                            expression.assignment_event_index,
                        )
                    })
                    .collect::<String>();
                self.output.push_str(&format!(
                    "    <hkobject name=\"{expression_ref}\" class=\"hkbExpressionDataArray\" signature=\"0x1ebfc6d7\"><hkparam name=\"expressionsData\" numelements=\"{}\">{rows}</hkparam></hkobject>\n",
                    expressions.expressions.len()
                ));
                let binding_ref = self.emit_bindings(bindings);
                self.output.push_str(&format!(
                    "    <hkobject name=\"{id}\" class=\"hkbEvaluateExpressionModifier\" signature=\"0x4a3ac449\"><hkparam name=\"variableBindingSet\">{binding_ref}</hkparam><hkparam name=\"userData\">{user_data}</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"enable\">{enable}</hkparam><hkparam name=\"expressions\">{expression_ref}</hkparam></hkobject>\n",
                    name = escape_xml(&source.name),
                ));
            }
        }
        id
    }

    fn emit_bindings(
        &mut self,
        bindings: &[super::manifest::CapabilityGeneratorVariableBinding],
    ) -> String {
        if bindings.is_empty() {
            return "null".to_string();
        }
        let id = self.allocate_id();
        let mut rows = String::new();
        for binding in bindings {
            let index = match binding.binding_type {
                CapabilityVariableBindingType::Variable => self
                    .declarations
                    .variables
                    .iter()
                    .position(|variable| variable.name == binding.variable_name),
                CapabilityVariableBindingType::CharacterProperty => self
                    .declarations
                    .character_properties
                    .iter()
                    .position(|property| property.name == binding.variable_name),
            }
            .expect("capability validation guarantees recursive binding declarations");
            let binding_type = match binding.binding_type {
                CapabilityVariableBindingType::Variable => "BINDING_TYPE_VARIABLE",
                CapabilityVariableBindingType::CharacterProperty => {
                    "BINDING_TYPE_CHARACTER_PROPERTY"
                }
            };
            rows.push_str(&emit_variable_binding(
                &binding.member_path,
                index,
                binding_type,
            ));
        }
        self.output.push_str(&format!(
            "    <hkobject name=\"{id}\" class=\"hkbVariableBindingSet\" signature=\"0xe942f339\">\n      <hkparam name=\"bindings\" numelements=\"{}\">\n{rows}      </hkparam><hkparam name=\"indexOfBindingToEnable\">-1</hkparam>\n    </hkobject>\n",
            bindings.len()
        ));
        id
    }

    fn event_id(&self, event: &CapabilityGeneratorEventRef) -> i32 {
        event.event_name.as_ref().map_or(-1, |name| {
            i32::try_from(
                self.declarations
                    .events
                    .iter()
                    .position(|event| event.name == *name)
                    .expect("capability validation guarantees recursive event declarations"),
            )
            .expect("event index fits Havok i32")
        })
    }

    fn emit_inline_event_property(&self, event: &CapabilityGeneratorEventProperty) -> String {
        format!(
            "<hkobject><hkparam name=\"id\">{}</hkparam><hkparam name=\"payload\">null</hkparam></hkobject>",
            self.event_id(&event.event)
        )
    }

    fn emit_event_properties(&mut self, events: &[CapabilityGeneratorEventProperty]) -> String {
        if events.is_empty() {
            return "null".to_string();
        }
        let id = self.allocate_id();
        let rows = events
            .iter()
            .map(|event| self.emit_inline_event_property(event))
            .collect::<String>();
        self.output.push_str(&format!(
            "    <hkobject name=\"{id}\" class=\"hkbStateMachineEventPropertyArray\" signature=\"0x71957c2d\"><hkparam name=\"events\" numelements=\"{}\">{rows}</hkparam></hkobject>\n",
            events.len()
        ));
        id
    }

    fn emit_transition_array(&mut self, array: &CapabilityGeneratorTransitionArray) -> String {
        let id = self.allocate_id();
        let mut rows = String::new();
        for transition in &array.transitions {
            let transition_ref = transition
                .transition_effect
                .as_ref()
                .map(|effect| self.emit_blending_transition_effect(effect))
                .unwrap_or_else(|| "null".to_string());
            let interval = |interval: &super::manifest::CapabilityGeneratorInterval| {
                format!(
                    "<hkobject><hkparam name=\"enterEventId\">{}</hkparam><hkparam name=\"exitEventId\">{}</hkparam><hkparam name=\"enterTime\">{}</hkparam><hkparam name=\"exitTime\">{}</hkparam></hkobject>",
                    self.event_id(&interval.enter_event),
                    self.event_id(&interval.exit_event),
                    f32::from_bits(interval.enter_time_bits),
                    f32::from_bits(interval.exit_time_bits),
                )
            };
            rows.push_str(&format!(
                "        <hkobject><hkparam name=\"triggerInterval\">{}</hkparam><hkparam name=\"initiateInterval\">{}</hkparam><hkparam name=\"transition\">{transition_ref}</hkparam><hkparam name=\"condition\">null</hkparam><hkparam name=\"eventId\">{}</hkparam><hkparam name=\"toStateId\">{}</hkparam><hkparam name=\"fromNestedStateId\">{}</hkparam><hkparam name=\"toNestedStateId\">{}</hkparam><hkparam name=\"priority\">{}</hkparam><hkparam name=\"flags\">{}</hkparam></hkobject>\n",
                interval(&transition.trigger_interval),
                interval(&transition.initiate_interval),
                self.event_id(&transition.event),
                transition.to_state_id,
                transition.from_nested_state_id,
                transition.to_nested_state_id,
                transition.priority,
                transition.flags,
            ));
        }
        self.output.push_str(&format!(
            "    <hkobject name=\"{id}\" class=\"hkbStateMachineTransitionInfoArray\" signature=\"0x704a19af\"><hkparam name=\"transitions\" numelements=\"{}\">\n{rows}      </hkparam></hkobject>\n",
            array.transitions.len()
        ));
        id
    }

    fn emit_blending_transition_effect(
        &mut self,
        effect: &CapabilityGeneratorBlendingTransitionEffect,
    ) -> String {
        let key = (
            effect.source.behavior_path.to_ascii_lowercase(),
            effect.source.object_index,
        );
        if let Some(id) = self.transition_effect_ids.get(&key) {
            return id.clone();
        }
        let id = self.allocate_id();
        self.transition_effect_ids.insert(key, id.clone());
        let binding_ref = self.emit_bindings(&effect.bindings);
        self.output.push_str(&format!(
            "    <hkobject name=\"{id}\" class=\"hkbBlendingTransitionEffect\" signature=\"0x14e54c5c\"><hkparam name=\"variableBindingSet\">{binding_ref}</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"selfTransitionMode\">{self_transition_mode}</hkparam><hkparam name=\"eventMode\">{event_mode}</hkparam><hkparam name=\"duration\">{duration}</hkparam><hkparam name=\"toGeneratorStartTimeFraction\">{start_fraction}</hkparam><hkparam name=\"flags\">{flags}</hkparam><hkparam name=\"endMode\">{end_mode}</hkparam><hkparam name=\"blendCurve\">{blend_curve}</hkparam><hkparam name=\"alignmentBone\">{alignment_bone}</hkparam></hkobject>\n",
            name = escape_xml(&effect.source.name),
            self_transition_mode = effect.self_transition_mode,
            event_mode = effect.event_mode,
            duration = f32::from_bits(effect.duration_bits),
            start_fraction = f32::from_bits(effect.to_generator_start_fraction_bits),
            flags = effect.flags,
            end_mode = effect.end_mode,
            blend_curve = effect.blend_curve,
            alignment_bone = effect.alignment_bone,
        ));
        id
    }
}

fn capability_child_id(prefix: u8, state_index: usize, child_index: usize) -> String {
    format!("#{prefix}{state_index:02}{child_index:02}")
}

fn capability_binding_id(state_index: usize) -> String {
    format!("#5{state_index:02}00")
}

fn emit_capability_core_behavior(
    manifest: &CreatureManifest,
    graph: &CapabilityGraphManifest,
    roles: &[&CapabilityClipRole],
    declarations: &GraphDeclarations,
) -> String {
    let mut generator = String::new();
    for (index, role) in roles.iter().enumerate() {
        generator.push_str(&emit_capability_role_generator(
            manifest,
            role,
            index,
            declarations,
        ));
    }
    for (index, overlay) in graph.overlays.iter().enumerate() {
        let clip = manifest
            .clips
            .iter()
            .find(|clip| clip.name == overlay.clip_name)
            .expect("capability validation guarantees every overlay clip");
        generator.push_str(&emit_clip_generator(
            manifest,
            &format!("#{:04}", 200 + index),
            &overlay.name,
            clip,
        ));
    }

    let animation_driven_event_index = declarations
        .events
        .iter()
        .position(|event| event.name == "startAnimationDriven");
    let animation_driven_variable_index = declarations
        .variables
        .iter()
        .position(|variable| variable.name == "bAnimationDriven");
    for (index, role) in roles.iter().enumerate() {
        if !role.motion.animation_driven {
            continue;
        }
        generator.push_str(&emit_animation_driven_state_support(
            index,
            &role.state_name,
            &format!("#010{index}"),
            animation_driven_event_index.expect("motion declarations include startAnimationDriven"),
            animation_driven_variable_index.expect("motion declarations include bAnimationDriven"),
        ));
    }

    let transitions = capability_state_transitions(graph, roles, declarations);
    for (index, transitions) in transitions.iter().enumerate() {
        generator.push_str(&emit_transition_array(&format!("#011{index}"), transitions));
    }

    for (index, role) in roles.iter().enumerate() {
        let enter_notify_events = role.motion.animation_driven.then(|| format!("#013{index}"));
        let state_generator = role
            .motion
            .animation_driven
            .then(|| format!("#016{index}"))
            .unwrap_or_else(|| format!("#010{index}"));
        generator.push_str(&emit_state_info(
            &format!("#012{index}"),
            &role.state_name,
            index as i32,
            &format!("#011{index}"),
            &state_generator,
            enter_notify_events.as_deref(),
        ));
    }
    let state_references = (0..roles.len())
        .map(|index| format!("#012{index}"))
        .collect::<Vec<_>>()
        .join(" ");
    let wrapped_root =
        graph.template == CreatureGraphTemplate::StationaryTurret || !graph.overlays.is_empty();
    let state_machine_id = if wrapped_root { "#0190" } else { "#0093" };
    let machine_name = match graph.template {
        CreatureGraphTemplate::GroundMelee
        | CreatureGraphTemplate::GroundRangedProjectile
        | CreatureGraphTemplate::GroundMeleeRanged => "CapabilityLocomotionCombatSM",
        CreatureGraphTemplate::PassiveGround => "PassiveGroundLocomotionSM",
        CreatureGraphTemplate::GroundSwim => "GroundSwimLocomotionCombatSM",
        CreatureGraphTemplate::GroundFly => "GroundFlightLocomotionCombatSM",
        CreatureGraphTemplate::Swim => "SwimLocomotionCombatSM",
        CreatureGraphTemplate::Fly => "FlightLocomotionCombatSM",
        CreatureGraphTemplate::StationaryTurret => "StationaryTurretCombatSM",
        CreatureGraphTemplate::RobotContinuousAttack => "RobotContinuousAttackSM",
    };
    generator.push_str(&format!(
        "    <hkobject name=\"{state_machine_id}\" class=\"hkbStateMachine\" signature=\"0xa5896bcf\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{machine_name}</hkparam>\n\
      <hkparam name=\"eventToSendWhenStateOrTransitionChanges\"><hkobject><hkparam name=\"id\">-1</hkparam><hkparam name=\"payload\">null</hkparam></hkobject></hkparam>\n\
      <hkparam name=\"startStateIdSelector\">null</hkparam><hkparam name=\"startStateId\">0</hkparam><hkparam name=\"returnToPreviousStateEventId\">-1</hkparam><hkparam name=\"randomTransitionEventId\">-1</hkparam><hkparam name=\"transitionToNextHigherStateEventId\">-1</hkparam><hkparam name=\"transitionToNextLowerStateEventId\">-1</hkparam><hkparam name=\"syncVariableIndex\">-1</hkparam>\n\
      <hkparam name=\"wrapAroundStateId\">false</hkparam><hkparam name=\"maxSimultaneousTransitions\">1</hkparam><hkparam name=\"startStateMode\">START_STATE_MODE_DEFAULT</hkparam><hkparam name=\"selfTransitionMode\">SELF_TRANSITION_MODE_NO_TRANSITION</hkparam>\n\
      <hkparam name=\"states\" numelements=\"{state_count}\">{state_references}</hkparam><hkparam name=\"wildcardTransitions\">null</hkparam>\n\
    </hkobject>\n",
        state_count = roles.len(),
    ));

    let mut base_generator = state_machine_id;
    if graph.template == CreatureGraphTemplate::StationaryTurret {
        let wrapper_id = if graph.overlays.is_empty() {
            "#0093"
        } else {
            "#0193"
        };
        generator.push_str(&emit_stationary_turret_direct_at(
            declarations,
            state_machine_id,
            wrapper_id,
        ));
        base_generator = wrapper_id;
    }
    if !graph.overlays.is_empty() {
        generator.push_str(&emit_overlay_layers(graph, declarations, base_generator));
    }

    emit_behavior_graph(
        &format!("{}CoreBehavior", manifest.creature_name),
        declarations,
        &generator,
    )
}

fn capability_state_transitions(
    graph: &CapabilityGraphManifest,
    roles: &[&CapabilityClipRole],
    declarations: &GraphDeclarations,
) -> Vec<Vec<(usize, i32, bool)>> {
    let event_index = |name: &str| {
        declarations
            .events
            .iter()
            .position(|event| event.name == name)
            .expect("capability validation guarantees every transition event")
    };
    let role_transitions = |index: usize| {
        roles[index]
            .trigger_event
            .iter()
            .chain(&roles[index].trigger_aliases)
            .map(|event| (event_index(event), index as i32, false))
            .collect::<Vec<_>>()
    };
    let completion = || vec![(0, 0, true)];
    let non_idle_targets: Vec<(usize, i32, bool)> =
        (1..roles.len()).flat_map(&role_transitions).collect();
    let idle_transitions = std::iter::once(graph.idle_event.as_str())
        .chain(roles[0].trigger_aliases.iter().map(String::as_str))
        .map(|event| (event_index(event), 0, false))
        .collect::<Vec<_>>();

    match graph.template {
        CreatureGraphTemplate::GroundMelee
        | CreatureGraphTemplate::PassiveGround
        | CreatureGraphTemplate::GroundRangedProjectile
        | CreatureGraphTemplate::GroundMeleeRanged => roles
            .iter()
            .enumerate()
            .map(|(index, role)| {
                if index == 0 {
                    non_idle_targets.clone()
                } else if role.role == CreatureClipRole::GroundForward {
                    let mut transitions = idle_transitions.clone();
                    transitions.extend(
                        non_idle_targets
                            .iter()
                            .copied()
                            .filter(|(_, state_id, _)| *state_id != index as i32),
                    );
                    transitions
                } else {
                    completion()
                }
            })
            .collect(),
        CreatureGraphTemplate::GroundSwim | CreatureGraphTemplate::GroundFly => roles
            .iter()
            .enumerate()
            .map(|(index, role)| {
                if index == 0 {
                    return non_idle_targets.clone();
                }
                if matches!(
                    role.role,
                    CreatureClipRole::Idle
                        | CreatureClipRole::SwimIdle
                        | CreatureClipRole::FlyIdle
                        | CreatureClipRole::StationaryIdle
                ) {
                    let mut transitions = idle_transitions.clone();
                    transitions.extend(
                        non_idle_targets
                            .iter()
                            .copied()
                            .filter(|(_, state_id, _)| *state_id != index as i32),
                    );
                    return transitions;
                }
                if matches!(
                    role.role,
                    CreatureClipRole::GroundForward
                        | CreatureClipRole::SwimForward
                        | CreatureClipRole::FlyForward
                ) {
                    let mut transitions = idle_transitions.clone();
                    let mode_idle = match role.role {
                        CreatureClipRole::SwimForward => CreatureClipRole::SwimIdle,
                        CreatureClipRole::FlyForward => CreatureClipRole::FlyIdle,
                        _ => CreatureClipRole::Idle,
                    };
                    if let Some((target, _)) = roles
                        .iter()
                        .enumerate()
                        .find(|(_, candidate)| candidate.role == mode_idle)
                    {
                        for transition in role_transitions(target) {
                            push_unique_transition(&mut transitions, transition);
                        }
                    }
                    transitions.extend(
                        non_idle_targets
                            .iter()
                            .copied()
                            .filter(|(_, state_id, _)| *state_id != index as i32),
                    );
                    transitions
                } else {
                    completion()
                }
            })
            .collect(),
        CreatureGraphTemplate::Swim | CreatureGraphTemplate::Fly => roles
            .iter()
            .enumerate()
            .map(|(index, role)| {
                if index == 0 {
                    non_idle_targets.clone()
                } else if matches!(
                    role.role,
                    CreatureClipRole::SwimForward | CreatureClipRole::FlyForward
                ) {
                    let mut transitions = idle_transitions.clone();
                    transitions.extend(
                        roles
                            .iter()
                            .enumerate()
                            .filter(|(_, candidate)| {
                                matches!(
                                    candidate.role,
                                    CreatureClipRole::MeleeAttack
                                        | CreatureClipRole::ProjectileAttack
                                )
                            })
                            .flat_map(|(target, _)| role_transitions(target)),
                    );
                    transitions
                } else {
                    completion()
                }
            })
            .collect(),
        CreatureGraphTemplate::StationaryTurret => roles
            .iter()
            .enumerate()
            .map(|(index, _)| {
                if index == 0 {
                    non_idle_targets.clone()
                } else {
                    completion()
                }
            })
            .collect(),
        CreatureGraphTemplate::RobotContinuousAttack => {
            let index_of = |role| {
                roles
                    .iter()
                    .position(|candidate| candidate.role == role)
                    .expect("robot capability validation guarantees every phase")
            };
            let start = index_of(CreatureClipRole::ContinuousAttackStart);
            let attack_loop = index_of(CreatureClipRole::ContinuousAttackLoop);
            let stop = index_of(CreatureClipRole::ContinuousAttackStop);
            roles
                .iter()
                .enumerate()
                .map(|(index, _)| {
                    if index == 0 {
                        role_transitions(start)
                    } else if index == start {
                        let mut transitions = role_transitions(attack_loop);
                        for transition in &idle_transitions {
                            push_unique_transition(&mut transitions, *transition);
                        }
                        transitions
                    } else if index == attack_loop {
                        let mut transitions = role_transitions(stop);
                        for transition in &idle_transitions {
                            push_unique_transition(&mut transitions, *transition);
                        }
                        transitions
                    } else {
                        completion()
                    }
                })
                .collect()
        }
    }
}

fn push_unique_transition(
    transitions: &mut Vec<(usize, i32, bool)>,
    transition: (usize, i32, bool),
) {
    if !transitions
        .iter()
        .any(|existing| existing.0 == transition.0)
    {
        transitions.push(transition);
    }
}

fn emit_stationary_turret_direct_at(
    declarations: &GraphDeclarations,
    child_generator: &str,
    wrapper_id: &str,
) -> String {
    let variable_index = |name: &str| {
        declarations
            .variables
            .iter()
            .position(|variable| variable.name == name)
            .expect("stationary turret validation guarantees aim variables")
    };
    let property_index = |name: &str| {
        declarations
            .character_properties
            .iter()
            .position(|property| property.name == name)
            .expect("stationary turret validation guarantees aim properties")
    };
    let variable_bindings = [
        ("limitHeadingDegreesCCW", "AimHeadingMaxCCW"),
        ("limitHeadingDegreesCW", "AimHeadingMaxCW"),
        ("onGain", "fAimOnGain"),
        ("initialGain", "fDirectAtHeadingSavedGain"),
        ("directAtCameraX", "camerafromx"),
        ("directAtCameraY", "camerafromy"),
        ("directAtCameraZ", "camerafromz"),
        ("active", "bAimActive"),
        ("currentHeadingOffset", "AimHeadingCurrent"),
        ("currentPitchOffset", "AimPitchCurrent"),
        ("currentGain", "fDirectAtHeadingSavedGain"),
    ];
    let property_bindings = [
        ("sourceBoneIndex", "DirectAtHeadingSourceBoneIndex"),
        ("startBoneIndex", "DirectAtHeadingBoneIndex"),
        ("endBoneIndex", "DirectAtHeadingBoneIndex"),
    ];
    let mut bindings = String::new();
    for (member_path, variable) in variable_bindings {
        bindings.push_str(&emit_variable_binding(
            member_path,
            variable_index(variable),
            "BINDING_TYPE_VARIABLE",
        ));
    }
    for (member_path, property) in property_bindings {
        bindings.push_str(&emit_variable_binding(
            member_path,
            property_index(property),
            "BINDING_TYPE_CHARACTER_PROPERTY",
        ));
    }

    format!(
        "    <hkobject name=\"#0191\" class=\"hkbVariableBindingSet\" signature=\"0xe942f339\">\n\
      <hkparam name=\"bindings\" numelements=\"14\">\n{bindings}      </hkparam><hkparam name=\"indexOfBindingToEnable\">-1</hkparam>\n\
    </hkobject>\n\
    <hkobject name=\"#0192\" class=\"BSDirectAtModifier\" signature=\"0xcda56038\">\n\
      <hkparam name=\"variableBindingSet\">#0191</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">StationaryTurret_DirectAt</hkparam><hkparam name=\"enable\">true</hkparam>\n\
      <hkparam name=\"directAtOn\">true</hkparam><hkparam name=\"target\">0</hkparam><hkparam name=\"context\">3</hkparam><hkparam name=\"sourceBoneIndex\">0</hkparam><hkparam name=\"offsetsCharacterRelative\">true</hkparam><hkparam name=\"startBoneIndex\">0</hkparam><hkparam name=\"endBoneIndex\">0</hkparam>\n\
      <hkparam name=\"limitHeadingDegreesCCW\">90.0</hkparam><hkparam name=\"limitHeadingDegreesCW\">90.0</hkparam><hkparam name=\"limitPitchDegreesUp\">180.0</hkparam><hkparam name=\"limitPitchDegreesDown\">180.0</hkparam>\n\
      <hkparam name=\"onGain\">0.05</hkparam><hkparam name=\"offGain\">0.05</hkparam><hkparam name=\"onLeadIn\">1.0</hkparam><hkparam name=\"offLeadIn\">1.0</hkparam><hkparam name=\"initialHeadingOffset\">0.0</hkparam><hkparam name=\"initialPitchOffset\">0.0</hkparam><hkparam name=\"initialGain\">0.0</hkparam>\n\
      <hkparam name=\"allowSourceBonePoseCapture\">true</hkparam><hkparam name=\"sourceBonePoseCaptureBlendGain\">0.05</hkparam><hkparam name=\"useCurrentSourceBonePoseEventId\">-1</hkparam><hkparam name=\"counteractSourceBoneWorldSpaceDelta\">false</hkparam><hkparam name=\"snapToTargetEventId\">-1</hkparam>\n\
      <hkparam name=\"directAtCameraX\">0.0</hkparam><hkparam name=\"directAtCameraY\">0.0</hkparam><hkparam name=\"directAtCameraZ\">0.0</hkparam><hkparam name=\"drawDebugInfo\">false</hkparam>\n\
      <hkparam name=\"active\">false</hkparam><hkparam name=\"currentHeadingOffset\">0.0</hkparam><hkparam name=\"currentPitchOffset\">0.0</hkparam><hkparam name=\"currentGain\">0.0</hkparam>\n\
    </hkobject>\n\
    <hkobject name=\"{wrapper_id}\" class=\"hkbModifierGenerator\" signature=\"0xc499fc9e\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">1</hkparam><hkparam name=\"name\">StationaryTurretDirectAtGenerator</hkparam><hkparam name=\"modifier\">#0192</hkparam><hkparam name=\"generator\">{child_generator}</hkparam>\n\
    </hkobject>\n"
    )
}

fn emit_variable_binding(member_path: &str, index: usize, binding_type: &str) -> String {
    format!(
        "        <hkobject><hkparam name=\"memberPath\">{member_path}</hkparam><hkparam name=\"variableIndex\">{index}</hkparam><hkparam name=\"bitIndex\">-1</hkparam><hkparam name=\"bindingType\">{binding_type}</hkparam></hkobject>\n"
    )
}

fn emit_overlay_layers(
    graph: &CapabilityGraphManifest,
    declarations: &GraphDeclarations,
    base_generator: &str,
) -> String {
    let event_index = |name: &str| {
        declarations
            .events
            .iter()
            .position(|event| event.name == name)
            .expect("overlay validation guarantees start and stop events")
    };
    let mut output = format!(
        "    <hkobject name=\"#0299\" class=\"hkbLayer\" signature=\"0x2916a243\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"generator\">{base_generator}</hkparam><hkparam name=\"weight\">1.0</hkparam><hkparam name=\"boneWeights\">null</hkparam><hkparam name=\"fadeInDuration\">0.0</hkparam><hkparam name=\"fadeOutDuration\">0.0</hkparam><hkparam name=\"onEventId\">-1</hkparam><hkparam name=\"offEventId\">-1</hkparam><hkparam name=\"onByDefault\">true</hkparam><hkparam name=\"useMotion\">true</hkparam><hkparam name=\"forceFullFadeDurations\">false</hkparam>\n\
    </hkobject>\n"
    );
    for (index, overlay) in graph.overlays.iter().enumerate() {
        output.push_str(&emit_overlay_layer(
            index,
            overlay,
            event_index(&overlay.start_event),
            event_index(&overlay.stop_event),
        ));
    }
    let layer_references = std::iter::once("#0299".to_string())
        .chain(
            graph
                .overlays
                .iter()
                .enumerate()
                .map(|(index, _)| format!("#{:04}", 300 + index)),
        )
        .collect::<Vec<_>>()
        .join(" ");
    output.push_str(&format!(
        "    <hkobject name=\"#0093\" class=\"hkbLayerGenerator\" signature=\"0xb4e0c52f\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">CapabilityOverlayLayerGenerator</hkparam><hkparam name=\"layers\" numelements=\"{layer_count}\">{layer_references}</hkparam><hkparam name=\"indexOfSyncMasterChild\">0</hkparam><hkparam name=\"flags\">1</hkparam>\n\
    </hkobject>\n",
        layer_count = graph.overlays.len() + 1,
    ));
    output
}

fn emit_overlay_layer(
    index: usize,
    overlay: &OverlayClipRole,
    start_event_index: usize,
    stop_event_index: usize,
) -> String {
    format!(
        "    <hkobject name=\"#{layer_id:04}\" class=\"hkbLayer\" signature=\"0x2916a243\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"generator\">#{clip_id:04}</hkparam><hkparam name=\"weight\">1.0</hkparam><hkparam name=\"boneWeights\">null</hkparam><hkparam name=\"fadeInDuration\">0.2</hkparam><hkparam name=\"fadeOutDuration\">0.2</hkparam><hkparam name=\"onEventId\">{start_event_index}</hkparam><hkparam name=\"offEventId\">{stop_event_index}</hkparam><hkparam name=\"onByDefault\">false</hkparam><hkparam name=\"useMotion\">false</hkparam><hkparam name=\"forceFullFadeDurations\">false</hkparam>\n\
    </hkobject>\n",
        layer_id = 300 + index,
        clip_id = 200 + index,
    )
}

fn declarations_with_capability(
    declarations: &GraphDeclarations,
    graph: &CapabilityGraphManifest,
) -> GraphDeclarations {
    let mut declarations = declarations.clone();
    for event in &graph.explicit_events {
        if !declarations
            .events
            .iter()
            .any(|existing| existing.name.eq_ignore_ascii_case(&event.name))
        {
            declarations.events.push(event.clone());
        }
    }
    if !graph.any_animation_driven() {
        return declarations;
    }
    if !declarations
        .events
        .iter()
        .any(|event| event.name == "startAnimationDriven")
    {
        declarations.events.push(EventDecl {
            name: "startAnimationDriven".to_string(),
            usage: EventUsage::Generic,
            flags: 0,
        });
    }
    if !declarations
        .variables
        .iter()
        .any(|variable| variable.name == "bAnimationDriven")
    {
        declarations.variables.push(VariableDecl {
            name: "bAnimationDriven".to_string(),
            variable_type: VariableType::Bool,
            initial_value: VariableValue::Bool(false),
        });
    }
    declarations
}

fn emit_clip_generator(
    manifest: &CreatureManifest,
    id: &str,
    state_name: &str,
    clip: &ClipDecl,
) -> String {
    let mode = if clip.looping {
        "MODE_LOOPING"
    } else {
        "MODE_SINGLE_PLAY"
    };
    format!(
        "    <hkobject name=\"{id}\" class=\"hkbClipGenerator\" signature=\"0xd4cc9f6\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{state_name}</hkparam><hkparam name=\"animationBundleName\"/><hkparam name=\"animationName\">{animation}</hkparam><hkparam name=\"triggers\">null</hkparam><hkparam name=\"userPartitionMask\">0</hkparam><hkparam name=\"cropStartAmountLocalTime\">0.0</hkparam><hkparam name=\"cropEndAmountLocalTime\">0.0</hkparam><hkparam name=\"startTime\">0.0</hkparam><hkparam name=\"playbackSpeed\">1.0</hkparam><hkparam name=\"enforcedDuration\">0.0</hkparam><hkparam name=\"userControlledTimeFraction\">0.0</hkparam><hkparam name=\"animationBindingIndex\">-1</hkparam><hkparam name=\"mode\">{mode}</hkparam><hkparam name=\"flags\">0</hkparam>\n\
    </hkobject>\n",
        animation = escape_xml(internal_path(manifest, &clip.path)),
    )
}

fn emit_transition_array(id: &str, transitions: &[(usize, i32, bool)]) -> String {
    let mut output = format!(
        "    <hkobject name=\"{id}\" class=\"hkbStateMachineTransitionInfoArray\" signature=\"0x704a19af\">\n      <hkparam name=\"transitions\" numelements=\"{}\">\n",
        transitions.len()
    );
    for (event_index, to_state_id, abut_at_end) in transitions {
        let event_id = if *abut_at_end {
            -1
        } else {
            i32::try_from(*event_index).expect("event index fits Havok i32")
        };
        let flags = if *abut_at_end { 16_384 } else { 0 };
        output.push_str(&format!(
            "        <hkobject><hkparam name=\"triggerInterval\"><hkobject><hkparam name=\"enterEventId\">-1</hkparam><hkparam name=\"exitEventId\">-1</hkparam><hkparam name=\"enterTime\">0.0</hkparam><hkparam name=\"exitTime\">0.0</hkparam></hkobject></hkparam><hkparam name=\"initiateInterval\"><hkobject><hkparam name=\"enterEventId\">-1</hkparam><hkparam name=\"exitEventId\">-1</hkparam><hkparam name=\"enterTime\">0.0</hkparam><hkparam name=\"exitTime\">0.0</hkparam></hkobject></hkparam><hkparam name=\"transition\">null</hkparam><hkparam name=\"condition\">null</hkparam><hkparam name=\"eventId\">{event_id}</hkparam><hkparam name=\"toStateId\">{to_state_id}</hkparam><hkparam name=\"fromNestedStateId\">0</hkparam><hkparam name=\"toNestedStateId\">0</hkparam><hkparam name=\"priority\">0</hkparam><hkparam name=\"flags\">{flags}</hkparam></hkobject>\n"
        ));
    }
    output.push_str("      </hkparam>\n    </hkobject>\n");
    output
}

fn emit_animation_driven_state_support(
    state_index: usize,
    state_name: &str,
    clip_generator: &str,
    event_index: usize,
    variable_index: usize,
) -> String {
    format!(
        "    <hkobject name=\"#013{state_index}\" class=\"hkbStateMachineEventPropertyArray\" signature=\"0x71957c2d\">\n\
      <hkparam name=\"events\" numelements=\"1\"><hkobject><hkparam name=\"id\">{event_index}</hkparam><hkparam name=\"payload\">null</hkparam></hkobject></hkparam>\n\
    </hkobject>\n\
    <hkobject name=\"#014{state_index}\" class=\"hkbVariableBindingSet\" signature=\"0xe942f339\">\n\
      <hkparam name=\"bindings\" numelements=\"1\"><hkobject><hkparam name=\"memberPath\">bIsActive0</hkparam><hkparam name=\"variableIndex\">{variable_index}</hkparam><hkparam name=\"bitIndex\">-1</hkparam><hkparam name=\"bindingType\">BINDING_TYPE_VARIABLE</hkparam></hkobject></hkparam><hkparam name=\"indexOfBindingToEnable\">-1</hkparam>\n\
    </hkobject>\n\
    <hkobject name=\"#015{state_index}\" class=\"BSIsActiveModifier\" signature=\"0x5bf1f5cf\">\n\
      <hkparam name=\"variableBindingSet\">#014{state_index}</hkparam><hkparam name=\"userData\">2</hkparam><hkparam name=\"name\">{state_name}AnimationDriven</hkparam><hkparam name=\"enable\">true</hkparam><hkparam name=\"bIsActive0\">false</hkparam><hkparam name=\"bInvertActive0\">false</hkparam><hkparam name=\"bIsActive1\">false</hkparam><hkparam name=\"bInvertActive1\">false</hkparam><hkparam name=\"bIsActive2\">false</hkparam><hkparam name=\"bInvertActive2\">false</hkparam><hkparam name=\"bIsActive3\">false</hkparam><hkparam name=\"bInvertActive3\">false</hkparam><hkparam name=\"bIsActive4\">false</hkparam><hkparam name=\"bInvertActive4\">false</hkparam>\n\
    </hkobject>\n\
    <hkobject name=\"#016{state_index}\" class=\"hkbModifierGenerator\" signature=\"0xc499fc9e\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">1</hkparam><hkparam name=\"name\">{state_name}AnimationDrivenGenerator</hkparam><hkparam name=\"modifier\">#015{state_index}</hkparam><hkparam name=\"generator\">{clip_generator}</hkparam>\n\
    </hkobject>\n"
    )
}

fn emit_state_info(
    id: &str,
    state_name: &str,
    state_id: i32,
    transitions: &str,
    generator: &str,
    enter_notify_events: Option<&str>,
) -> String {
    let enter_notify_events = enter_notify_events.unwrap_or("null");
    format!(
        "    <hkobject name=\"{id}\" class=\"hkbStateMachineStateInfo\" signature=\"0x39d76713\">\n\
      <hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"listeners\" numelements=\"0\"/><hkparam name=\"enterNotifyEvents\">{enter_notify_events}</hkparam><hkparam name=\"exitNotifyEvents\">null</hkparam><hkparam name=\"transitions\">{transitions}</hkparam><hkparam name=\"generator\">{generator}</hkparam><hkparam name=\"name\">{state_name}</hkparam><hkparam name=\"stateId\">{state_id}</hkparam><hkparam name=\"probability\">1.0</hkparam><hkparam name=\"enable\">true</hkparam>\n\
    </hkobject>\n"
    )
}

fn emit_behavior_graph(name: &str, declarations: &GraphDeclarations, generator: &str) -> String {
    let event_names = string_array(
        "eventNames",
        declarations.events.iter().map(|event| event.name.as_str()),
    );
    let variable_names = string_array(
        "variableNames",
        declarations
            .variables
            .iter()
            .map(|variable| variable.name.as_str()),
    );
    let property_names = string_array(
        "characterPropertyNames",
        declarations
            .character_properties
            .iter()
            .map(|property| property.name.as_str()),
    );
    let variable_values = value_array(
        "wordVariableValues",
        declarations
            .variables
            .iter()
            .map(|variable| variable.initial_value),
    );
    let variable_infos = variable_info_array(&declarations.variables);
    let property_infos = property_info_array(&declarations.character_properties);
    let event_infos = event_info_array(&declarations.events);
    let bounds = variable_bounds_array(&declarations.variables);

    format!(
        "<?xml version=\"1.0\" encoding=\"ascii\"?>\n\
<hkpackfile classversion=\"11\" contentsversion=\"hk_2014.1.0-r1\" toplevelobject=\"#0095\">\n\
  <hksection name=\"__data__\">\n\
    <hkobject name=\"#0095\" class=\"hkRootLevelContainer\" signature=\"0x2772c11e\"><hkparam name=\"namedVariants\" numelements=\"1\"><hkobject><hkparam name=\"name\">hkbBehaviorGraph</hkparam><hkparam name=\"className\">hkbBehaviorGraph</hkparam><hkparam name=\"variant\">#0094</hkparam></hkobject></hkparam></hkobject>\n\
    <hkobject name=\"#0090\" class=\"hkbBehaviorGraphStringData\" signature=\"0x1bd27f38\">\n\
{event_names}      <hkparam name=\"attributeNames\" numelements=\"0\"/>\n\
{variable_names}{property_names}    </hkobject>\n\
    <hkobject name=\"#0091\" class=\"hkbVariableValueSet\" signature=\"0xeb5f7e25\">\n\
{variable_values}      <hkparam name=\"quadVariableValues\" numelements=\"0\"/><hkparam name=\"variantVariableValues\" numelements=\"0\"/>\n\
    </hkobject>\n\
    <hkobject name=\"#0092\" class=\"hkbBehaviorGraphData\" signature=\"0x907a8222\">\n\
      <hkparam name=\"attributeDefaults\" numelements=\"0\"/>\n\
{variable_infos}{property_infos}{event_infos}{bounds}      <hkparam name=\"variableInitialValues\">#0091</hkparam><hkparam name=\"stringData\">#0090</hkparam>\n\
    </hkobject>\n\
{generator}    <hkobject name=\"#0094\" class=\"hkbBehaviorGraph\" signature=\"0xfdedb83b\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{graph_name}.hkb</hkparam><hkparam name=\"variableMode\">VARIABLE_MODE_DISCARD_WHEN_INACTIVE</hkparam><hkparam name=\"rootGenerator\">#0093</hkparam><hkparam name=\"data\">#0092</hkparam></hkobject>\n\
  </hksection>\n\
</hkpackfile>\n",
        graph_name = escape_xml(name),
    )
}

fn string_array<'a>(name: &str, values: impl IntoIterator<Item = &'a str>) -> String {
    let values: Vec<&str> = values.into_iter().collect();
    if values.is_empty() {
        return format!("      <hkparam name=\"{name}\" numelements=\"0\"/>\n");
    }
    let mut output = format!(
        "      <hkparam name=\"{}\" numelements=\"{}\">\n",
        name,
        values.len()
    );
    for value in values {
        output.push_str("        <hkcstring>");
        output.push_str(&escape_xml(value));
        output.push_str("</hkcstring>\n");
    }
    output.push_str("      </hkparam>\n");
    output
}

fn value_array(name: &str, values: impl IntoIterator<Item = VariableValue>) -> String {
    let values: Vec<VariableValue> = values.into_iter().collect();
    if values.is_empty() {
        return format!("      <hkparam name=\"{name}\" numelements=\"0\"/>\n");
    }
    let mut output = format!(
        "      <hkparam name=\"{}\" numelements=\"{}\">\n",
        name,
        values.len()
    );
    for value in values {
        output.push_str(&format!(
            "        <hkobject><hkparam name=\"value\">{}</hkparam></hkobject>\n",
            value.word_bits()
        ));
    }
    output.push_str("      </hkparam>\n");
    output
}

fn variable_info_array(variables: &[VariableDecl]) -> String {
    typed_info_array(
        "variableInfos",
        variables.iter().map(|variable| variable.variable_type),
    )
}

fn property_info_array(properties: &[PropertyDecl]) -> String {
    typed_info_array(
        "characterPropertyInfos",
        properties.iter().map(|property| property.variable_type),
    )
}

fn typed_info_array(name: &str, types: impl IntoIterator<Item = VariableType>) -> String {
    let types: Vec<VariableType> = types.into_iter().collect();
    if types.is_empty() {
        return format!("      <hkparam name=\"{name}\" numelements=\"0\"/>\n");
    }
    let mut output = format!(
        "      <hkparam name=\"{}\" numelements=\"{}\">\n",
        name,
        types.len()
    );
    for variable_type in types {
        output.push_str(&format!(
            "        <hkobject><hkparam name=\"role\"><hkobject><hkparam name=\"role\">ROLE_DEFAULT</hkparam><hkparam name=\"flags\">0</hkparam></hkobject></hkparam><hkparam name=\"type\">{}</hkparam></hkobject>\n",
            variable_type.hk_name()
        ));
    }
    output.push_str("      </hkparam>\n");
    output
}

fn event_info_array(events: &[EventDecl]) -> String {
    if events.is_empty() {
        return "      <hkparam name=\"eventInfos\" numelements=\"0\"/>\n".to_string();
    }
    let mut output = format!(
        "      <hkparam name=\"eventInfos\" numelements=\"{}\">\n",
        events.len()
    );
    for event in events {
        output.push_str(&format!(
            "        <hkobject><hkparam name=\"flags\">{}</hkparam></hkobject>\n",
            event.flags
        ));
    }
    output.push_str("      </hkparam>\n");
    output
}

fn variable_bounds_array(variables: &[VariableDecl]) -> String {
    if variables.is_empty() {
        return "      <hkparam name=\"variableBounds\" numelements=\"0\"/>\n".to_string();
    }
    let mut output = format!(
        "      <hkparam name=\"variableBounds\" numelements=\"{}\">\n",
        variables.len()
    );
    for variable in variables {
        let (minimum, maximum) = match variable.variable_type {
            VariableType::Bool => (0_i64, 1_i64),
            VariableType::Int32 => (i64::from(i32::MIN), i64::from(i32::MAX)),
            VariableType::Real => (
                i64::from(f32::MIN.to_bits() as i32),
                i64::from(f32::MAX.to_bits() as i32),
            ),
        };
        output.push_str(&format!(
            "        <hkobject><hkparam name=\"min\"><hkobject><hkparam name=\"value\">{minimum}</hkparam></hkobject></hkparam><hkparam name=\"max\"><hkobject><hkparam name=\"value\">{maximum}</hkparam></hkobject></hkparam></hkobject>\n"
        ));
    }
    output.push_str("      </hkparam>\n");
    output
}

fn xml_source_path(runtime_path: &str) -> String {
    let stem = &runtime_path[..runtime_path.len() - ".hkx".len()];
    format!("{stem}.xml")
}

fn file_name(path: &str) -> &str {
    path.rsplit('\\').next().unwrap_or(path)
}

fn internal_path<'a>(manifest: &CreatureManifest, runtime_path: &'a str) -> &'a str {
    manifest
        .project_relative_havok_path(runtime_path)
        .expect("manifest validation keeps Havok references below the project root")
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn format_vector4(bits: [u32; 4]) -> String {
    format!(
        "({} {} {} {})",
        f32::from_bits(bits[0]),
        f32::from_bits(bits[1]),
        f32::from_bits(bits[2]),
        f32::from_bits(bits[3]),
    )
}
