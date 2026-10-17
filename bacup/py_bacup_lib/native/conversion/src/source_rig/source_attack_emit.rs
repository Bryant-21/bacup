use super::*;

pub(super) fn suppress_aim_annotations(
    mut xml: String,
    role: &CapabilityClipRole,
    state_index: usize,
) -> String {
    let children = match &role.generator {
        CapabilityRoleGenerator::SourceAttack { contract, .. } => &contract.aim_children,
        CapabilityRoleGenerator::SourceAim { aim_children, .. } => aim_children,
        _ => return xml,
    };
    for (index, child) in children.iter().enumerate() {
        if child.clip_name == role.clip_name {
            continue;
        }
        let id = clip_clock_id(&xml, &capability_child_id(3, state_index, index));
        let start = xml.find(&format!("<hkobject name=\"{id}\"")).unwrap();
        let end = start + xml[start..].find("</hkobject>").unwrap();
        let object = xml[start..end].replace(
            "<hkparam name=\"flags\">0</hkparam>",
            "<hkparam name=\"flags\">16</hkparam>",
        );
        xml.replace_range(start..end, &object);
    }
    xml
}

pub(super) fn wrap_scope(
    mut xml: String,
    graph: &CapabilityGraphManifest,
    role: &CapabilityClipRole,
    index: usize,
    declarations: &GraphDeclarations,
) -> String {
    let gates = graph
        .roles
        .iter()
        .filter_map(|role| role.generator.source_attack())
        .map(|contract| (contract.gate_variable.as_str(), contract))
        .collect::<BTreeMap<_, _>>();
    if gates.is_empty() {
        return xml;
    }
    let contract = role.generator.source_attack();
    let prefix = format!("#source_attack_{index}");
    let root = format!("#010{index}");
    let child = format!("{prefix}_child");
    let clock = clip_clock_id(&xml, &root);
    let clock = if clock == root { child.clone() } else { clock };
    xml = xml.replacen(&format!("name=\"{root}\""), &format!("name=\"{child}\""), 1);
    let mut assignments = gates
        .iter()
        .map(|(name, source)| {
            let value = if matches!(
                role.role,
                CreatureClipRole::MeleeAttack
                    | CreatureClipRole::ProjectileAttack
                    | CreatureClipRole::ContinuousAttackStart
                    | CreatureClipRole::ContinuousAttackLoop
                    | CreatureClipRole::ContinuousAttackStop
            ) {
                "-1".to_string()
            } else if !role.generator.source_movement().is_empty() {
                source
                    .opening_variable
                    .clone()
                    .unwrap_or_else(|| source.opening_group.to_string())
            } else {
                source.opening_group.to_string()
            };
            (*name, value)
        })
        .collect::<Vec<_>>();
    if declarations
        .variables
        .iter()
        .any(|variable| variable.name == "B21SourceAttackActive")
    {
        assignments.push((
            "B21SourceAttackActive",
            if matches!(
                role.role,
                CreatureClipRole::MeleeAttack
                    | CreatureClipRole::ProjectileAttack
                    | CreatureClipRole::ContinuousAttackStart
                    | CreatureClipRole::ContinuousAttackLoop
                    | CreatureClipRole::ContinuousAttackStop
            ) {
                (index + 1).to_string()
            } else {
                "0".to_string()
            },
        ));
    }
    if declarations
        .variables
        .iter()
        .any(|variable| variable.name == "B21SourceMovementActive")
    {
        assignments.push((
            "B21SourceMovementActive",
            usize::from(!role.generator.source_movement().is_empty()).to_string(),
        ));
    }
    xml.push_str(&assignment(
        &format!("{prefix}_reset"),
        &assignments,
        declarations,
    ));
    let mut modifiers = vec![format!("{prefix}_reset")];
    if let Some(contract) = contract
        && let Some(window) = &contract.window
    {
        let [open, close] =
            crate::source_rig::SourceAttackContract::window_events(&role.state_name);
        let event_index = |name: &str| {
            declarations
                .events
                .iter()
                .position(|event| event.name == name)
                .unwrap()
        };
        let open = event_index(&open);
        let close = event_index(&close);
        let trigger = format!("{prefix}_triggers");
        let anchor = contract
            .aim_children
            .iter()
            .position(|sample| sample.clip_name == role.clip_name)
            .map(|sample| clip_clock_id(&xml, &capability_child_id(3, index, sample)))
            .unwrap_or(clock);
        let start = xml.find(&format!("<hkobject name=\"{anchor}\"")).unwrap();
        let end = start + xml[start..].find("</hkobject>").unwrap();
        let object = xml[start..end].replace(
            "<hkparam name=\"triggers\">null</hkparam>",
            &format!("<hkparam name=\"triggers\">{trigger}</hkparam>"),
        );
        xml.replace_range(start..end, &object);
        let rows = [(f32::from_bits(window.start_bits), open), (f32::from_bits(window.end_bits), close)].iter().map(|(time, event)| format!(
            "<hkobject><hkparam name=\"localTime\">{time}</hkparam><hkparam name=\"event\"><hkobject><hkparam name=\"id\">{event}</hkparam><hkparam name=\"payload\">null</hkparam></hkobject></hkparam><hkparam name=\"relativeToEndOfClip\">false</hkparam><hkparam name=\"acyclic\">true</hkparam><hkparam name=\"isAnnotation\">false</hkparam></hkobject>"
        )).collect::<String>();
        xml.push_str(&format!("<hkobject name=\"{trigger}\" class=\"hkbClipTriggerArray\" signature=\"0xf757cd66\"><hkparam name=\"triggers\" numelements=\"2\">{rows}</hkparam></hkobject>\n"));
        xml.push_str(&assignment(
            &format!("{prefix}_next"),
            &[(&contract.gate_variable, window.next_group.to_string())],
            declarations,
        ));
        xml.push_str(&format!("<hkobject name=\"{prefix}_window\" class=\"hkbEventDrivenModifier\" signature=\"0x35ef356b\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">SourceContinuationWindow</hkparam><hkparam name=\"enable\">true</hkparam><hkparam name=\"modifier\">{prefix}_next</hkparam><hkparam name=\"activateEventId\">{open}</hkparam><hkparam name=\"deactivateEventId\">{close}</hkparam><hkparam name=\"activeByDefault\">false</hkparam></hkobject>\n"));
        modifiers.push(format!("{prefix}_window"));
    }
    xml.push_str(&format!("<hkobject name=\"{prefix}_list\" class=\"hkbModifierList\" signature=\"0x0ded564c\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">SourceAttackEligibility</hkparam><hkparam name=\"enable\">true</hkparam><hkparam name=\"modifiers\" numelements=\"{}\">{}</hkparam></hkobject>\n", modifiers.len(), modifiers.join(" ")));
    xml.push_str(&format!("<hkobject name=\"{root}\" class=\"hkbModifierGenerator\" signature=\"0xc499fc9e\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">SourceAttackScope</hkparam><hkparam name=\"modifier\">{prefix}_list</hkparam><hkparam name=\"generator\">{child}</hkparam></hkobject>\n"));
    xml
}

pub(super) fn clip_clock_id(xml: &str, id: &str) -> String {
    let clock = format!("#property_clock_{}", id.trim_start_matches('#'));
    if xml.contains(&format!("<hkobject name=\"{clock}\"")) {
        clock
    } else {
        id.to_string()
    }
}

pub(super) fn assignment(
    id: &str,
    values: &[(&str, String)],
    declarations: &GraphDeclarations,
) -> String {
    let rows = values.iter().map(|(name, value)| {
        let index = declarations.variables.iter().position(|variable| variable.name == *name).unwrap();
        format!("<hkobject><hkparam name=\"expression\">{value}</hkparam><hkparam name=\"assignmentVariableIndex\">{index}</hkparam><hkparam name=\"assignmentEventIndex\">-1</hkparam><hkparam name=\"eventMode\">EVENT_MODE_SEND_ONCE</hkparam></hkobject>")
    }).collect::<String>();
    format!(
        "<hkobject name=\"{id}_data\" class=\"hkbExpressionDataArray\" signature=\"0x1ebfc6d7\"><hkparam name=\"expressionsData\" numelements=\"{}\">{rows}</hkparam></hkobject>\n<hkobject name=\"{id}\" class=\"hkbEvaluateExpressionModifier\" signature=\"0x4a3ac449\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">SourceAttackGate</hkparam><hkparam name=\"enable\">true</hkparam><hkparam name=\"expressions\">{id}_data</hkparam></hkobject>\n",
        values.len()
    )
}

pub(super) fn transitions(
    id: &str,
    transitions: &[(usize, i32, bool)],
    roles: &[&CapabilityClipRole],
) -> String {
    let mut conditions = Vec::new();
    let mut objects = String::new();
    for (index, (_, target, completion)) in transitions.iter().enumerate() {
        let contract = (!completion)
            .then(|| roles[*target as usize].generator.source_attack())
            .flatten();
        let condition = contract.map(|contract| {
            let object = format!("#source_attack_condition_{}_{index}", id.trim_start_matches('#'));
            objects.push_str(&format!("<hkobject name=\"{object}\" class=\"hkbExpressionCondition\" signature=\"0x78a69526\"><hkparam name=\"expression\">{} == {}</hkparam></hkobject>\n", escape_xml(&contract.gate_variable), contract.group));
            object
        });
        conditions.push(condition);
    }
    let mut output = emit_transition_array_conditions(id, transitions, &conditions);
    output.push_str(&objects);
    output
}
