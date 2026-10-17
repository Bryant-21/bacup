use super::*;

pub(super) const BASE_STATE: &str = "iSourceIdleBaseState";

pub(super) fn is_overlay(role: &CapabilityClipRole) -> bool {
    matches!(role.generator, CapabilityRoleGenerator::SourceIdle { overlay: Some(_), .. })
}

fn metadata(role: &CapabilityClipRole) -> &super::super::manifest::SourceIdleOverlay {
    let CapabilityRoleGenerator::SourceIdle { overlay: Some(overlay), .. } = &role.generator else { unreachable!() };
    overlay
}

fn groups<'a>(roles: &[&'a CapabilityClipRole]) -> BTreeMap<u8, Vec<&'a CapabilityClipRole>> {
    let mut groups: BTreeMap<u8, Vec<&CapabilityClipRole>> = BTreeMap::new();
    for role in roles.iter().copied().filter(|role| is_overlay(role)) {
        groups.entry(metadata(role).source_group).or_default().push(role);
    }
    groups
}

fn state_variable(group: u8) -> String { format!("iSourceIdleGroup{group}") }

fn selected_priority(variable: &str, priorities: impl Iterator<Item = (usize, Option<u8>)>) -> String {
    let terms = priorities.filter_map(|(state, priority)| priority.map(|priority|
        format!("(({variable} == {state}) * {})", u16::from(priority) + 1)))
        .collect::<Vec<_>>();
    if terms.is_empty() { "-1".to_string() } else { format!("(-1 + {})", terms.join(" + ")) }
}

fn weight_expression(priority: &str, previous: &[String]) -> String {
    let eligible = std::iter::once(format!("({priority} >= 0)"))
        .chain(previous.iter().map(|other| format!("({priority} >= {other})")))
        .collect::<Vec<_>>().join(" * ");
    let equals = previous.iter().map(|other| format!("({priority} == {other})"))
        .collect::<Vec<_>>().join(" + ");
    format!("({eligible}) / (1 + {equals})")
}

struct WeightPlan {
    expressions: Vec<String>,
    bone_variables: BTreeMap<u8, Vec<usize>>,
}

fn weight_plan(manifest: &CreatureManifest, roles: &[&CapabilityClipRole]) -> WeightPlan {
    let groups = groups(roles);
    let base = roles.iter().copied().filter(|role| !is_overlay(role)).collect::<Vec<_>>();
    let source = metadata(groups.values().next().unwrap()[0]);
    let mut expressions = Vec::new();
    let mut bone_variables: BTreeMap<u8, Vec<usize>> = groups.keys().map(|group| (*group, Vec::new())).collect();
    for bone in &manifest.animation_skeleton.bones {
        let mut previous = vec![selected_priority(BASE_STATE, base.iter().enumerate().map(|(index, role)|
            (index, source.base_priorities.get(&role.state_name).and_then(|priorities| priorities.get(&bone.name)).copied())))];
        for (group, members) in &groups {
            let priority = selected_priority(&state_variable(*group), members.iter().enumerate().map(|(index, role)|
                (index + 1, metadata(role).transform_priorities.get(&bone.name).copied())));
            let expression = weight_expression(&priority, &previous);
            let variable = expressions.iter().position(|existing| existing == &expression).unwrap_or_else(|| {
                expressions.push(expression);
                expressions.len() - 1
            });
            bone_variables.get_mut(group).unwrap().push(variable);
            previous.push(priority);
        }
    }
    WeightPlan { expressions, bone_variables }
}

pub(super) fn add_declarations(manifest: &CreatureManifest, roles: &[&CapabilityClipRole], declarations: &mut GraphDeclarations) {
    if !roles.iter().any(|role| is_overlay(role)) { return; }
    let plan = weight_plan(manifest, roles);
    let mut push = |name: String, variable_type, initial_value| {
        if !declarations.variables.iter().any(|variable| variable.name == name) {
            declarations.variables.push(VariableDecl { name, variable_type, initial_value });
        }
    };
    push(BASE_STATE.to_string(), VariableType::Int32, VariableValue::Int32(0));
    for group in groups(roles).keys() {
        push(state_variable(*group), VariableType::Int32, VariableValue::Int32(0));
    }
    for index in 0..plan.expressions.len() {
        push(format!("fSourceIdleBoneWeight{index}"), VariableType::Real, VariableValue::Real(0.0));
    }
}

pub(super) fn emit(manifest: &CreatureManifest, roles: &[&CapabilityClipRole], declarations: &GraphDeclarations, base_generator: &str) -> String {
    let base_count = roles.iter().filter(|role| !is_overlay(role)).count();
    let groups = groups(roles);
    let plan = weight_plan(manifest, roles);
    let variable_index = |name: &str| declarations.variables.iter().position(|variable| variable.name == name).unwrap();
    let event_index = |name: &str| declarations.events.iter().position(|event| event.name == name).unwrap();
    let mut output = String::new();
    let mut layers = vec!["#source_idle_base_layer".to_string()];
    output.push_str(&layer("#source_idle_base_layer", base_generator, "null", "null", true));
    let mut next_generator = base_count;
    for (group, members) in &groups {
        let prefix = format!("#source_idle_group{group}");
        let mut states = Vec::new();
        output.push_str(&format!("<hkobject name=\"{prefix}_off_pose\" class=\"hkbReferencePoseGenerator\" signature=\"0xbc1536ee\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">SourceIdleInactive</hkparam></hkobject>\n"));
        let starts = members.iter().enumerate().map(|(index, role)| (event_index(role.trigger_event.as_deref().unwrap()), (index + 1) as i32, false)).collect::<Vec<_>>();
        output.push_str(&emit_transition_array(&format!("{prefix}_off_transitions"), &starts));
        states.push(format!("{prefix}_state0"));
        output.push_str(&emit_state_info(&states[0], "Inactive", 0, &format!("{prefix}_off_transitions"), &format!("{prefix}_off_pose"), None, None));
        for (index, role) in members.iter().enumerate() {
            let CapabilityRoleGenerator::SourceIdle { complete_event, duration_bits, loop_end_bits, .. } = &role.generator else { unreachable!() };
            output.push_str(&source_idle_emit::emit(manifest, role, next_generator, declarations));
            let mut transitions = starts.iter().copied().filter(|(_, target, _)| *target != (index + 1) as i32).collect::<Vec<_>>();
            transitions.push((event_index(complete_event), 0, false));
            if f32::from_bits(*loop_end_bits) >= f32::from_bits(*duration_bits) {
                transitions.push((event_index("IdleStop"), 0, false));
            }
            for interrupt in roles.iter().filter(|role| matches!(role.role, CreatureClipRole::Hurt | CreatureClipRole::Death)) {
                for event in interrupt.trigger_event.iter().chain(&interrupt.trigger_aliases) {
                    transitions.push((event_index(event), 0, false));
                }
            }
            transitions.sort();
            transitions.dedup();
            let transition_id = format!("{prefix}_transitions{}", index + 1);
            output.push_str(&emit_transition_array(&transition_id, &transitions));
            let state_id = format!("{prefix}_state{}", index + 1);
            output.push_str(&emit_state_info(&state_id, &role.state_name, (index + 1) as i32, &transition_id, &format!("#010{next_generator}"), None, None));
            states.push(state_id);
            next_generator += 1;
        }
        output.push_str(&machine(&prefix, &states, variable_index(&state_variable(*group))));
        let weights_id = format!("{prefix}_weights");
        let weights_binding = format!("{prefix}_weight_bindings");
        let bindings = plan.bone_variables[group].iter().enumerate().map(|(bone, variable)|
            emit_variable_binding(&format!("boneWeights:{bone}"), variable_index(&format!("fSourceIdleBoneWeight{variable}")), "BINDING_TYPE_VARIABLE")).collect::<String>();
        output.push_str(&binding_set(&weights_binding, plan.bone_variables[group].len(), &bindings));
        output.push_str(&format!("<hkobject name=\"{weights_id}\" class=\"hkbBoneWeightArray\" signature=\"0xcd902b77\"><hkparam name=\"variableBindingSet\">{weights_binding}</hkparam><hkparam name=\"boneWeights\" numelements=\"{}\">{}</hkparam></hkobject>\n", manifest.animation_skeleton.bones.len(), vec!["0.0"; manifest.animation_skeleton.bones.len()].join(" ")));
        let layer_id = format!("{prefix}_layer");
        // A zero layer weight deactivates its state machine and loses future PlayIdle events.
        output.push_str(&layer(&layer_id, &prefix, &weights_id, "null", false));
        layers.push(layer_id);
    }
    output.push_str(&format!("<hkobject name=\"#source_idle_layers\" class=\"hkbLayerGenerator\" signature=\"0xb4e0c52f\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">SourceIdleLayers</hkparam><hkparam name=\"layers\" numelements=\"{}\">{}</hkparam><hkparam name=\"indexOfSyncMasterChild\">0</hkparam><hkparam name=\"flags\">0</hkparam></hkobject>\n", layers.len(), layers.join(" ")));
    let expression_rows = plan.expressions.iter().enumerate().map(|(index, expression)| (format!("fSourceIdleBoneWeight{index}"), expression.clone()))
        .map(|(name, expression)| format!("<hkobject><hkparam name=\"expression\">{}</hkparam><hkparam name=\"assignmentVariableIndex\">{}</hkparam><hkparam name=\"assignmentEventIndex\">-1</hkparam><hkparam name=\"eventMode\">EVENT_MODE_SEND_ONCE</hkparam></hkobject>", escape_xml(&expression), variable_index(&name))).collect::<String>();
    output.push_str(&format!("<hkobject name=\"#source_idle_weight_expressions\" class=\"hkbExpressionDataArray\" signature=\"0x1ebfc6d7\"><hkparam name=\"expressionsData\" numelements=\"{}\">{expression_rows}</hkparam></hkobject>\n<hkobject name=\"#source_idle_weight_modifier\" class=\"hkbEvaluateExpressionModifier\" signature=\"0x4a3ac449\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">SourceIdlePriorityWeights</hkparam><hkparam name=\"enable\">true</hkparam><hkparam name=\"expressions\">#source_idle_weight_expressions</hkparam></hkobject>\n<hkobject name=\"#0093\" class=\"hkbModifierGenerator\" signature=\"0xc499fc9e\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">SourceIdlePriorityGenerator</hkparam><hkparam name=\"modifier\">#source_idle_weight_modifier</hkparam><hkparam name=\"generator\">#source_idle_layers</hkparam></hkobject>\n", plan.expressions.len()));
    output
}

fn binding_set(id: &str, count: usize, bindings: &str) -> String {
    format!("<hkobject name=\"{id}\" class=\"hkbVariableBindingSet\" signature=\"0xe942f339\"><hkparam name=\"bindings\" numelements=\"{count}\">{bindings}</hkparam><hkparam name=\"indexOfBindingToEnable\">-1</hkparam></hkobject>\n")
}

fn layer(id: &str, generator: &str, weights: &str, binding: &str, motion: bool) -> String {
    format!("<hkobject name=\"{id}\" class=\"hkbLayer\" signature=\"0x2916a243\"><hkparam name=\"variableBindingSet\">{binding}</hkparam><hkparam name=\"generator\">{generator}</hkparam><hkparam name=\"weight\">1.0</hkparam><hkparam name=\"boneWeights\">{weights}</hkparam><hkparam name=\"fadeInDuration\">0.0</hkparam><hkparam name=\"fadeOutDuration\">0.0</hkparam><hkparam name=\"onEventId\">-1</hkparam><hkparam name=\"offEventId\">-1</hkparam><hkparam name=\"onByDefault\">true</hkparam><hkparam name=\"useMotion\">{motion}</hkparam><hkparam name=\"forceFullFadeDurations\">false</hkparam></hkobject>\n")
}

fn machine(id: &str, states: &[String], sync_variable: usize) -> String {
    format!("<hkobject name=\"{id}\" class=\"hkbStateMachine\" signature=\"0xa5896bcf\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">SourceIdleGroup</hkparam><hkparam name=\"eventToSendWhenStateOrTransitionChanges\"><hkobject><hkparam name=\"id\">-1</hkparam><hkparam name=\"payload\">null</hkparam></hkobject></hkparam><hkparam name=\"startStateIdSelector\">null</hkparam><hkparam name=\"startStateId\">0</hkparam><hkparam name=\"returnToPreviousStateEventId\">-1</hkparam><hkparam name=\"randomTransitionEventId\">-1</hkparam><hkparam name=\"transitionToNextHigherStateEventId\">-1</hkparam><hkparam name=\"transitionToNextLowerStateEventId\">-1</hkparam><hkparam name=\"syncVariableIndex\">{sync_variable}</hkparam><hkparam name=\"wrapAroundStateId\">false</hkparam><hkparam name=\"maxSimultaneousTransitions\">1</hkparam><hkparam name=\"startStateMode\">START_STATE_MODE_DEFAULT</hkparam><hkparam name=\"selfTransitionMode\">SELF_TRANSITION_MODE_NO_TRANSITION</hkparam><hkparam name=\"states\" numelements=\"{}\">{}</hkparam><hkparam name=\"wildcardTransitions\">null</hkparam></hkobject>\n", states.len(), states.join(" "))
}
