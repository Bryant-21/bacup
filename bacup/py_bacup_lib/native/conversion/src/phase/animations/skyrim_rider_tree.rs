use std::ops::Range;

use super::RiderClipReceipt;
use crate::source_rig::{
    CapabilityGeneratorNode, CreatureManifest, EventDecl, EventUsage, GraphDeclarations,
    VariableDecl, VariableType, VariableValue,
};

pub(crate) struct RiderTreeContext<'a> {
    pub manifest: &'a CreatureManifest,
    pub declarations: &'a GraphDeclarations,
    pub states: &'a [CapabilityGeneratorNode],
}

pub(crate) fn stage_graph_with_trees(
    donor: &[u8],
    clips: &[RiderClipReceipt],
    context: &RiderTreeContext<'_>,
) -> Result<Vec<u8>, String> {
    if context.states.is_empty() {
        return Err("rider source graph has no synchronization states".into());
    }
    if uses_character_property(
        &serde_json::to_value(context.states).map_err(|error| error.to_string())?,
    ) {
        return Err("source rider tree requires character-property remapping".into());
    }
    let mut placeholders = clips.to_vec();
    let names = (0..context.states.len())
        .map(|index| format!("B21RiderTree{index}"))
        .collect::<Vec<_>>();
    for name in &names {
        placeholders.push(RiderClipReceipt {
            name: name.clone(),
            source_path: String::new(),
            runtime_path: format!("__rider_tree__/{name}.hkx"),
            duration: 0.0,
            looping: true,
            source_tracks: 0,
            target_tracks: 0,
            paired: false,
            paired_origin: None,
            source_events: Vec::new(),
            source_float_tracks: Vec::new(),
        });
    }
    let donor_xml = havok_native::api::havok_hkx_to_xml(donor).map_err(|e| e.to_string())?;
    let xml = super::graph::rider_graph_xml(&donor_xml, &placeholders, &names)?;
    let doc = roxmltree::Document::parse(&xml).map_err(|e| e.to_string())?;
    let mut declarations = GraphDeclarations::default();
    let mut edits = Vec::<(Range<usize>, String)>::new();
    let variables = array(&doc, "variableNames")?
        .children()
        .filter(roxmltree::Node::is_element)
        .filter_map(|node| node.text())
        .collect::<Vec<_>>();
    declarations.variables = variables
        .iter()
        .map(|name| VariableDecl {
            name: (*name).into(),
            variable_type: VariableType::Real,
            initial_value: VariableValue::Real(0.0),
        })
        .collect();
    let added_variables = context
        .declarations
        .variables
        .iter()
        .filter(|variable| !variables.contains(&variable.name.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    declarations
        .variables
        .extend(added_variables.iter().cloned());
    let variable_names = added_variables
        .iter()
        .map(|variable| format!("<hkcstring>{}</hkcstring>", escape(&variable.name)))
        .collect::<String>();
    let variable_infos=added_variables.iter().map(|variable|format!("<hkobject><hkparam name=\"role\"><hkobject><hkparam name=\"role\">ROLE_DEFAULT</hkparam><hkparam name=\"flags\">FLAG_NONE</hkparam></hkobject></hkparam><hkparam name=\"type\">{}</hkparam></hkobject>",variable.variable_type.hk_name())).collect::<String>();
    let variable_values = added_variables
        .iter()
        .map(|variable| {
            format!(
                "<hkobject><hkparam name=\"value\">{}</hkparam></hkobject>",
                variable.initial_value.word_bits()
            )
        })
        .collect::<String>();
    for (name, extra) in [
        ("variableNames", variable_names),
        ("variableInfos", variable_infos),
        ("wordVariableValues", variable_values),
    ] {
        append_array(
            &doc,
            &xml,
            name,
            variables.len(),
            added_variables.len(),
            &extra,
            &mut edits,
        )?;
    }
    let events = array(&doc, "eventNames")?
        .children()
        .filter(roxmltree::Node::is_element)
        .filter_map(|node| node.text())
        .collect::<Vec<_>>();
    declarations.events = events
        .iter()
        .map(|name| EventDecl {
            name: (*name).into(),
            usage: EventUsage::Generic,
            flags: 0,
        })
        .collect();
    let added_events = context
        .declarations
        .events
        .iter()
        .filter(|event| !events.contains(&event.name.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    declarations.events.extend(added_events.iter().cloned());
    let event_names = added_events
        .iter()
        .map(|event| format!("<hkcstring>{}</hkcstring>", escape(&event.name)))
        .collect::<String>();
    let event_infos = added_events
        .iter()
        .map(|event| {
            format!(
                "<hkobject><hkparam name=\"flags\">{}</hkparam></hkobject>",
                event.flags
            )
        })
        .collect::<String>();
    for (name, extra) in [("eventNames", event_names), ("eventInfos", event_infos)] {
        append_array(
            &doc,
            &xml,
            name,
            events.len(),
            added_events.len(),
            &extra,
            &mut edits,
        )?;
    }
    let mut replaced = 0;
    for node in doc
        .descendants()
        .filter(|node| node.attribute("class") == Some("hkbClipGenerator"))
    {
        let Some(name) = node
            .children()
            .find(|child| child.attribute("name") == Some("name"))
            .and_then(|node| node.text())
        else {
            continue;
        };
        let Some(index) = names.iter().position(|candidate| candidate == name) else {
            continue;
        };
        let root_id = node
            .attribute("name")
            .ok_or("rider generator has no object id")?
            .to_string();
        let mut tree = context.states[index].clone();
        remap_sync_indices(&mut tree, context.declarations, &declarations)?;
        let fragment = crate::source_rig::emit_generator_fragment(
            context.manifest,
            &declarations,
            &tree,
            root_id,
            100 + replaced,
        );
        edits.push((node.range(), fragment));
        replaced += 1;
    }
    if replaced != 3 * context.states.len() {
        return Err("rider synchronization tree count mismatch".into());
    }
    edits.sort_by_key(|(range, _)| range.start);
    let mut edited = xml.clone();
    for (range, replacement) in edits.into_iter().rev() {
        edited.replace_range(range, &replacement);
    }
    if edited.contains("__rider_tree__") {
        return Err("unresolved rider generator placeholder".into());
    }
    crate::source_rig::validate_fo4_havok_xml_signatures(&edited).map_err(|e| e.to_string())?;
    let packed = havok_native::api::havok_xml_to_hkx(&edited).map_err(|e| e.to_string())?;
    let reloaded = havok_native::api::havok_hkx_to_xml(&packed).map_err(|e| e.to_string())?;
    crate::source_rig::validate_fo4_havok_xml_signatures(&reloaded).map_err(|e| e.to_string())?;
    Ok(packed)
}

fn remap_sync_indices(
    node: &mut CapabilityGeneratorNode,
    source: &GraphDeclarations,
    target: &GraphDeclarations,
) -> Result<(), String> {
    match node {
        CapabilityGeneratorNode::Clip { .. } | CapabilityGeneratorNode::SourceClip { .. } => {}
        CapabilityGeneratorNode::ManualSelector { children, .. } => {
            for child in children {
                remap_sync_indices(child, source, target)?;
            }
        }
        CapabilityGeneratorNode::Blender { children, .. } => {
            for child in children {
                remap_sync_indices(&mut child.generator, source, target)?;
            }
        }
        CapabilityGeneratorNode::StateMachine {
            sync_variable_index,
            states,
            ..
        } => {
            if *sync_variable_index >= 0 {
                let variable = source
                    .variables
                    .get(*sync_variable_index as usize)
                    .ok_or("rider state-machine sync variable is outside source table")?;
                *sync_variable_index = target
                    .variables
                    .iter()
                    .position(|candidate| candidate.name == variable.name)
                    .ok_or("rider state-machine sync variable is absent from target table")?
                    as i32;
            }
            for state in states {
                remap_sync_indices(&mut state.generator, source, target)?;
            }
        }
        CapabilityGeneratorNode::ModifierGenerator { generator, .. }
        | CapabilityGeneratorNode::CyclicBlendTransition { generator, .. } => {
            remap_sync_indices(generator, source, target)?
        }
    }
    Ok(())
}

fn uses_character_property(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(fields) => {
            fields
                .get("binding_type")
                .and_then(serde_json::Value::as_str)
                == Some("character_property")
                || fields.values().any(uses_character_property)
        }
        serde_json::Value::Array(values) => values.iter().any(uses_character_property),
        _ => false,
    }
}

fn append_array(
    doc: &roxmltree::Document<'_>,
    xml: &str,
    name: &str,
    original_count: usize,
    added: usize,
    extra: &str,
    edits: &mut Vec<(Range<usize>, String)>,
) -> Result<(), String> {
    let node = array(doc, name)?;
    let children = node
        .children()
        .filter(roxmltree::Node::is_element)
        .collect::<Vec<_>>();
    if children.len() != original_count {
        return Err(format!("furniture {name} count mismatch"));
    }
    let contents = children
        .iter()
        .map(|node| &xml[node.range()])
        .collect::<String>();
    edits.push((
        node.range(),
        format!(
            "<hkparam name=\"{name}\" numelements=\"{}\">{contents}{extra}</hkparam>",
            original_count + added
        ),
    ));
    Ok(())
}
fn array<'a, 'input>(
    doc: &'a roxmltree::Document<'input>,
    name: &str,
) -> Result<roxmltree::Node<'a, 'input>, String> {
    doc.descendants()
        .find(|node| node.attribute("name") == Some(name))
        .ok_or_else(|| format!("rider graph has no {name}"))
}
fn escape(value: &str) -> String {
    super::super::xml_escape(value)
}
