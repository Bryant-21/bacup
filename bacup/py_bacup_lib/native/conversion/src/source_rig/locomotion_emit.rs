use super::*;
use crate::source_rig::CapabilityLocomotionDirection;

pub(super) fn emit(
    manifest: &CreatureManifest,
    role: &CapabilityClipRole,
    state: usize,
    declarations: &GraphDeclarations,
    directions: &[CapabilityLocomotionDirection],
) -> String {
    let mut xml = String::new();
    let mut direction_children = Vec::new();
    for (direction_index, direction) in directions.iter().enumerate() {
        let mut speed_children = Vec::new();
        for (speed_index, speed) in direction.speeds.iter().enumerate() {
            let clip = manifest
                .clips
                .iter()
                .find(|clip| clip.name == speed.clip_name)
                .unwrap();
            let id = format!("#locomotion_clip_{state}_{direction_index}_{speed_index}");
            xml.push_str(&emit_clip_generator(manifest, &id, &speed.clip_name, clip));
            speed_children.push((id, f32::from_bits(speed.weight_bits)));
        }
        let speed_id = format!("#locomotion_speed_{state}_{direction_index}");
        xml.push_str(&blend(
            &speed_id,
            &format!("{}_speed_{direction_index}", role.state_name),
            "Speed",
            false,
            &speed_children,
            declarations,
        ));
        direction_children.push((speed_id, f32::from_bits(direction.direction_bits)));
    }
    xml.push_str(&blend(
        &format!("#010{state}"),
        &role.state_name,
        "Direction",
        true,
        &direction_children,
        declarations,
    ));
    xml
}

fn blend(
    id: &str,
    name: &str,
    variable: &str,
    cyclic: bool,
    children: &[(String, f32)],
    declarations: &GraphDeclarations,
) -> String {
    let binding = format!("#locomotion_binding_{}", id.trim_start_matches('#'));
    let variable_index = declarations
        .variables
        .iter()
        .position(|candidate| candidate.name == variable)
        .unwrap();
    let mut xml = format!(
        "<hkobject name=\"{binding}\" class=\"hkbVariableBindingSet\" signature=\"0xe942f339\"><hkparam name=\"bindings\" numelements=\"1\">{}</hkparam><hkparam name=\"indexOfBindingToEnable\">-1</hkparam></hkobject>\n",
        emit_variable_binding("blendParameter", variable_index, "BINDING_TYPE_VARIABLE")
    );
    let mut references = Vec::new();
    for (index, (generator, weight)) in children.iter().enumerate() {
        let child_id = format!("#locomotion_child_{}_{index}", id.trim_start_matches('#'));
        xml.push_str(&format!("<hkobject name=\"{child_id}\" class=\"hkbBlenderGeneratorChild\" signature=\"0xb35bbfd3\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"generator\">{generator}</hkparam><hkparam name=\"boneWeights\">null</hkparam><hkparam name=\"weight\">{weight}</hkparam><hkparam name=\"worldFromModelWeight\">1.0</hkparam></hkobject>\n"));
        references.push(child_id);
    }
    xml.push_str(&format!("<hkobject name=\"{id}\" class=\"hkbBlenderGenerator\" signature=\"0xce45c088\"><hkparam name=\"variableBindingSet\">{binding}</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"referencePoseWeightThreshold\">0.0</hkparam><hkparam name=\"blendParameter\">0.0</hkparam><hkparam name=\"minCyclicBlendParameter\">0.0</hkparam><hkparam name=\"maxCyclicBlendParameter\">1.0</hkparam><hkparam name=\"indexOfSyncMasterChild\">-1</hkparam><hkparam name=\"flags\">{flags}</hkparam><hkparam name=\"subtractLastChild\">false</hkparam><hkparam name=\"children\" numelements=\"{count}\">{children}</hkparam></hkobject>\n", name=escape_xml(name), flags=if cyclic {49} else {17}, count=children.len(), children=references.join(" ")));
    xml
}
