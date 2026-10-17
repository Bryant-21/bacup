use super::*;
use crate::source_rig::recovery::{SourceRecoveryGraph, SourceRecoveryValue};

pub(super) fn emit(
    manifest: &CreatureManifest,
    declarations: &GraphDeclarations,
    graph: &SourceRecoveryGraph,
) -> String {
    let mut graph = graph.clone();
    add_target_lifecycle_routes(&mut graph, declarations);
    let mut declarations = declarations.clone();
    for name in graph.event_names() {
        if !declarations.events.iter().any(|event| event.name == name) {
            declarations.events.push(EventDecl {
                name: name.into(),
                usage: EventUsage::Generic,
                flags: 0,
            });
        }
    }
    for variable in &graph.variables {
        if !declarations
            .variables
            .iter()
            .any(|decl| decl.name == variable.name)
        {
            declarations.variables.push(VariableDecl {
                name: variable.name.clone(),
                variable_type: variable.variable_type,
                initial_value: match variable.variable_type {
                    VariableType::Bool => VariableValue::Bool(variable.initial_word != 0),
                    VariableType::Int32 => VariableValue::Int32(variable.initial_word as i32),
                    VariableType::Real => {
                        VariableValue::Real(f32::from_bits(variable.initial_word))
                    }
                },
            });
        }
    }
    let object_id = |index| {
        if index == graph.root {
            "#0093".into()
        } else {
            format!("#RecoverySource{index}")
        }
    };
    let mut registry =
        havok_native::hkx::descriptors::DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
    let mut output = format!(
        "<hkobject name=\"{}\" class=\"BSBehaviorGraphSwapGenerator\" signature=\"0xe7633191\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">1</hkparam><hkparam name=\"name\">SourceRecoveryLivingGraph</hkparam><hkparam name=\"pDefaultGenerator\">null</hkparam></hkobject>\n",
        object_id(graph.living_generator)
    );
    for (index, node) in &graph.nodes {
        let signature = registry
            .get(&node.class_name)
            .expect("validated target descriptor")
            .expect("validated recovery class")
            .signature
            .clone();
        output.push_str(&format!(
            "<hkobject name=\"{}\" class=\"{}\" signature=\"{signature}\">",
            object_id(*index),
            node.class_name
        ));
        for (name, value) in &node.members {
            output.push_str(&parameter(name, value, manifest, &declarations, &object_id));
        }
        output.push_str("</hkobject>\n");
    }
    emit_behavior_graph(
        &format!("{}RootBehavior", manifest.creature_name),
        &declarations,
        &output,
    )
}

fn add_target_lifecycle_routes(graph: &mut SourceRecoveryGraph, declarations: &GraphDeclarations) {
    use SourceRecoveryValue as V;
    let root = &graph.nodes[&graph.root];
    let start = root.members["startStateId"].clone();
    let Some(V::Reference(Some(array))) = root.members.get("wildcardTransitions") else {
        unreachable!("validated source recovery wildcard array");
    };
    let array = *array;
    let Some(V::Array(transitions)) = graph
        .nodes
        .get_mut(&array)
        .and_then(|node| node.members.get_mut("transitions"))
    else {
        unreachable!("validated source recovery transitions");
    };
    let Some(V::Object(ragdoll)) = transitions.iter().find(|value| matches!(value, V::Object(fields) if fields.get("eventId") == Some(&V::Event("Ragdoll".into())))).cloned() else { unreachable!("validated source Ragdoll destination"); };
    // Converted core deaths complete through the target contract; the source
    // root still owns the ragdoll destination and its physical controllers.
    for event in [
        "B21_DeathAnimationComplete",
        "g_InitializeGraph",
        "g_InitializeGraphInstant",
    ] {
        if !declarations.events.iter().any(|decl| decl.name == event) {
            continue;
        }
        let mut transition = ragdoll.clone();
        transition.insert("eventId".into(), V::Event(event.into()));
        if event != "B21_DeathAnimationComplete" {
            transition.insert("toStateId".into(), start.clone());
            transition.insert("transition".into(), V::Reference(None));
        }
        transitions.push(V::Object(transition));
    }
}

fn parameter(
    name: &str,
    value: &SourceRecoveryValue,
    manifest: &CreatureManifest,
    declarations: &GraphDeclarations,
    object_id: &impl Fn(u32) -> String,
) -> String {
    let count = match value {
        SourceRecoveryValue::Array(items) => format!(" numelements=\"{}\"", items.len()),
        _ => String::new(),
    };
    format!(
        "<hkparam name=\"{}\"{count}>{}</hkparam>",
        escape_xml(name),
        contents(value, manifest, declarations, object_id)
    )
}

fn contents(
    value: &SourceRecoveryValue,
    manifest: &CreatureManifest,
    declarations: &GraphDeclarations,
    object_id: &impl Fn(u32) -> String,
) -> String {
    match value {
        SourceRecoveryValue::Bool(value) => value.to_string(),
        SourceRecoveryValue::Int(value) => value.to_string(),
        SourceRecoveryValue::Real(bits) => f32::from_bits(*bits).to_string(),
        SourceRecoveryValue::Vector(bits) => format!(
            "({})",
            bits.iter()
                .map(|bits| f32::from_bits(*bits).to_string())
                .collect::<Vec<_>>()
                .join(" ")
        ),
        SourceRecoveryValue::String(value) => escape_xml(value),
        SourceRecoveryValue::Reference(index) => {
            index.map(object_id).unwrap_or_else(|| "null".into())
        }
        SourceRecoveryValue::Event(name) => declarations
            .events
            .iter()
            .position(|event| event.name == *name)
            .expect("recovery event declaration")
            .to_string(),
        SourceRecoveryValue::Variable(name) => declarations
            .variables
            .iter()
            .position(|variable| variable.name == *name)
            .expect("recovery variable declaration")
            .to_string(),
        SourceRecoveryValue::Clip(name) => escape_xml(internal_path(
            manifest,
            &manifest
                .clips
                .iter()
                .find(|clip| clip.name == *name)
                .expect("validated recovery clip")
                .path,
        )),
        SourceRecoveryValue::Array(values) => values
            .iter()
            .map(|value| contents(value, manifest, declarations, object_id))
            .collect::<Vec<_>>()
            .join(" "),
        SourceRecoveryValue::Object(members) => format!(
            "<hkobject>{}</hkobject>",
            members
                .iter()
                .map(|(name, value)| parameter(name, value, manifest, declarations, object_id))
                .collect::<String>()
        ),
    }
}
