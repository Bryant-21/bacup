use super::*;

pub(super) fn emit_interaction(
    manifest: &CreatureManifest,
    role: &CapabilityClipRole,
    role_index: usize,
    declarations: &GraphDeclarations,
) -> String {
    let clip = manifest
        .clips
        .iter()
        .find(|clip| clip.name == role.clip_name)
        .unwrap();
    let mut output = emit_clip_generator(
        manifest,
        &format!("#980{role_index}"),
        &role.state_name,
        clip,
    );
    let variable = declarations
        .variables
        .iter()
        .position(|variable| variable.name == "B21MountRequiresDismount")
        .unwrap();
    output.push_str(&format!(
        "<hkobject name=\"#986{role_index}\" class=\"hkbExpressionDataArray\" signature=\"0x1ebfc6d7\"><hkparam name=\"expressionsData\" numelements=\"1\"><hkobject><hkparam name=\"expression\">false</hkparam><hkparam name=\"assignmentVariableIndex\">{variable}</hkparam><hkparam name=\"assignmentEventIndex\">-1</hkparam><hkparam name=\"eventMode\">EVENT_MODE_SEND_ONCE</hkparam></hkobject></hkparam></hkobject>\n\
         <hkobject name=\"#987{role_index}\" class=\"hkbEvaluateExpressionModifier\" signature=\"0x4a3ac449\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">MountInteractionCompatible</hkparam><hkparam name=\"enable\">true</hkparam><hkparam name=\"expressions\">#986{role_index}</hkparam></hkobject>\n\
         <hkobject name=\"#010{role_index}\" class=\"hkbModifierGenerator\" signature=\"0xc499fc9e\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"modifier\">#987{role_index}</hkparam><hkparam name=\"generator\">#980{role_index}</hkparam></hkobject>\n",
        name = escape_xml(&role.state_name),
    ));
    output
}

pub(super) fn emit(
    manifest: &CreatureManifest,
    role: &CapabilityClipRole,
    role_index: usize,
    declarations: &GraphDeclarations,
) -> String {
    let CapabilityRoleGenerator::MountSynchronization {
        state_index,
        generator,
        speed_sampler_source,
        speed_sampler_bindings,
        turn_damping,
    } = &role.generator
    else {
        unreachable!()
    };
    let mut inner = role.clone();
    inner.generator = *generator.clone();
    let root = format!("#010{role_index}");
    let child = format!("#980{role_index}");
    let mut output = emit_capability_role_generator(manifest, &inner, role_index, declarations)
        .replace(&format!("name=\"{root}\""), &format!("name=\"{child}\""));
    let mut emitter =
        CapabilityTreeEmitter::in_namespace(manifest, declarations, role_index, "mount");
    let binding = emitter.emit_bindings(speed_sampler_bindings);
    let damping = emitter.emit_modifier(turn_damping);
    output.push_str(&emitter.output);
    let variable = declarations
        .variables
        .iter()
        .position(|v| v.name == "SyncLocomotionState")
        .unwrap();
    let expression = if role.role == CreatureClipRole::GroundForward {
        format!("{state_index} + iSyncSprintState")
    } else {
        state_index.to_string()
    };
    let dismount_variable = declarations
        .variables
        .iter()
        .position(|v| v.name == "B21MountRequiresDismount")
        .unwrap();
    let expression_data = |expression: &str, variable| {
        format!(
            "<hkobject><hkparam name=\"expression\">{expression}</hkparam><hkparam name=\"assignmentVariableIndex\">{variable}</hkparam><hkparam name=\"assignmentEventIndex\">-1</hkparam><hkparam name=\"eventMode\">EVENT_MODE_SEND_ONCE</hkparam></hkobject>"
        )
    };
    let mut expressions = expression_data(
        if *state_index < 0 { "true" } else { "false" },
        dismount_variable,
    );
    let expression_count = if *state_index < 0 {
        1
    } else {
        expressions.push_str(&expression_data(&expression, variable));
        2
    };
    output.push_str(&format!(
        "<hkobject name=\"#981{role_index}\" class=\"BSSpeedSamplerModifier\" signature=\"0x6b1abbe7\"><hkparam name=\"variableBindingSet\">{binding}</hkparam><hkparam name=\"userData\">2</hkparam><hkparam name=\"name\">{sampler_name}</hkparam><hkparam name=\"enable\">true</hkparam><hkparam name=\"state\">-1</hkparam><hkparam name=\"direction\">0.0</hkparam><hkparam name=\"goalSpeed\">0.0</hkparam><hkparam name=\"speedOut\">0.0</hkparam></hkobject>\n\
         <hkobject name=\"#982{role_index}\" class=\"hkbExpressionDataArray\" signature=\"0x1ebfc6d7\"><hkparam name=\"expressionsData\" numelements=\"{expression_count}\">{expressions}</hkparam></hkobject>\n\
         <hkobject name=\"#983{role_index}\" class=\"hkbEvaluateExpressionModifier\" signature=\"0x4a3ac449\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">MountSyncState</hkparam><hkparam name=\"enable\">true</hkparam><hkparam name=\"expressions\">#982{role_index}</hkparam></hkobject>\n\
         <hkobject name=\"#984{role_index}\" class=\"hkbModifierList\" signature=\"0x0ded564c\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">MountSyncProducers</hkparam><hkparam name=\"enable\">true</hkparam><hkparam name=\"modifiers\" numelements=\"3\">#981{role_index} {damping} #983{role_index}</hkparam></hkobject>\n\
         <hkobject name=\"{root}\" class=\"hkbModifierGenerator\" signature=\"0xc499fc9e\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">MountSynchronizedState</hkparam><hkparam name=\"modifier\">#984{role_index}</hkparam><hkparam name=\"generator\">{child}</hkparam></hkobject>\n",
        sampler_name = escape_xml(&speed_sampler_source.name),
    ));
    output
}
