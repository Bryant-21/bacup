use std::ops::Range;

use super::RiderClipReceipt;

#[cfg(test)]
pub(super) fn stage_graph(
    donor: &[u8],
    clips: &[RiderClipReceipt],
    synchronized_clips: &[String],
) -> Result<Vec<u8>, String> {
    let xml = havok_native::api::havok_hkx_to_xml(donor).map_err(|e| e.to_string())?;
    let edited = rider_graph_xml(&xml, clips, synchronized_clips)?;
    crate::source_rig::validate_fo4_havok_xml_signatures(&edited).map_err(|e| e.to_string())?;
    let packed = havok_native::api::havok_xml_to_hkx(&edited).map_err(|e| e.to_string())?;
    let reloaded = havok_native::api::havok_hkx_to_xml(&packed).map_err(|e| e.to_string())?;
    let metadata = havok_native::animation::parsers::parse_behavior_xml(&reloaded)
        .map_err(|e| e.to_string())?;
    if !metadata
        .variables
        .iter()
        .any(|(name, _)| name == "SyncLocomotionState")
        || !metadata
            .variables
            .iter()
            .any(|(name, _)| name == "HorseSpeedSampled")
    {
        return Err("rider synchronization variables lost during packing".into());
    }
    Ok(packed)
}

pub(super) fn rider_graph_xml(
    xml: &str,
    clips: &[RiderClipReceipt],
    synchronized_clips: &[String],
) -> Result<String, String> {
    if synchronized_clips.is_empty() {
        return Err("rider graph has no horse synchronization states".into());
    }
    for name in synchronized_clips
        .iter()
        .map(String::as_str)
        .chain(["Mount", "Dismount"])
    {
        if !clips.iter().any(|clip| clip.name == name) {
            return Err(format!("rider graph references unstaged clip {name}"));
        }
    }
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let variables = array(&doc, "variableNames")?;
    let variable_names = variables
        .children()
        .filter(roxmltree::Node::is_element)
        .filter_map(|node| node.text())
        .collect::<Vec<_>>();
    if variable_names.contains(&"SyncLocomotionState")
        || variable_names.contains(&"HorseSpeedSampled")
    {
        return Err("furniture donor unexpectedly declares rider synchronization".into());
    }
    let variable_index = variable_names.len();
    let mut edits = Vec::<(Range<usize>, String)>::new();
    let event_array = array(&doc, "eventNames")?;
    let mut events = event_array
        .children()
        .filter(roxmltree::Node::is_element)
        .filter_map(|node| node.text())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut added_events = Vec::new();
    for event in [
        "idleChairSitting",
        "MountEnd",
        "sitStop",
        "DismountEnd",
        "idleChairGetUp",
        "ReevaluateGraphState",
    ] {
        if !events.iter().any(|name| name == event) {
            events.push(event.into());
            added_events.push(event);
        }
    }
    for (name, extra) in [
        (
            "eventNames",
            added_events
                .iter()
                .map(|event| format!("<hkcstring>{event}</hkcstring>"))
                .collect::<String>(),
        ),
        (
            "eventInfos",
            added_events
                .iter()
                .map(|_| "<hkobject><hkparam name=\"flags\">0</hkparam></hkobject>")
                .collect::<String>(),
        ),
    ] {
        let node = array(&doc, name)?;
        let rows = node
            .children()
            .filter(roxmltree::Node::is_element)
            .collect::<Vec<_>>();
        if rows.len() + added_events.len() != events.len() {
            return Err(format!("furniture donor {name} count mismatch"));
        }
        let contents = rows.iter().map(|row| &xml[row.range()]).collect::<String>();
        edits.push((
            node.range(),
            format!(
                "<hkparam name=\"{name}\" numelements=\"{}\">{contents}{extra}</hkparam>",
                events.len()
            ),
        ));
    }
    for (name, extra) in [
        ("variableNames", "<hkcstring>SyncLocomotionState</hkcstring><hkcstring>HorseSpeedSampled</hkcstring>".to_string()),
        ("variableInfos", format!("{}{}", variable_info("VARIABLE_TYPE_INT32"), variable_info("VARIABLE_TYPE_REAL"))),
        ("wordVariableValues", "<hkobject><hkparam name=\"value\">0</hkparam></hkobject><hkobject><hkparam name=\"value\">0</hkparam></hkobject>".into()),
    ] {
        let node = array(&doc, name)?;
        let rows = node.children().filter(roxmltree::Node::is_element).collect::<Vec<_>>();
        if rows.len() != variable_index { return Err(format!("furniture donor {name} count mismatch")); }
        let contents = rows.iter().map(|row| &xml[row.range()]).collect::<String>();
        edits.push((node.range(), format!("<hkparam name=\"{name}\" numelements=\"{}\">{contents}{extra}</hkparam>", variable_index + 2)));
    }
    let maximum_id = doc
        .descendants()
        .filter_map(|node| node.attribute("name"))
        .filter_map(|name| name.strip_prefix('#'))
        .filter_map(|name| name.parse::<usize>().ok())
        .max()
        .unwrap_or(0);
    let mut next_id = maximum_id + 1;
    let mut appended = String::new();
    let mut selectors = 0;
    let mut enters = 0;
    let mut exits = 0;
    let seated = doc
        .descendants()
        .find(|node| {
            node.attribute("class") == Some("hkbStateMachineStateInfo")
                && param(*node, "name").and_then(|field| field.text()) == Some("Idle_Sitting")
        })
        .ok_or("furniture donor lacks its seated state")?;
    let mut seated_generator = reference(&doc, seated, "generator")?;
    while seated_generator.attribute("class") == Some("hkbModifierGenerator") {
        seated_generator = reference(&doc, seated_generator, "generator")?;
    }
    if seated_generator.attribute("class") != Some("hkbBehaviorReferenceGenerator")
        || param(seated_generator, "behaviorName")
            .and_then(|node| node.text())
            .map(|name| name.replace('\\', "/").to_ascii_lowercase())
            .as_deref()
            != Some("behaviors/dialoguebehavior.hkx")
    {
        return Err("furniture seated generator does not match native donor".into());
    }
    edits.push((
        seated_generator.range(),
        selector(
            seated_generator.attribute("name").unwrap(),
            variable_index,
            &mut next_id,
            &mut appended,
            clips,
            synchronized_clips,
        ),
    ));
    selectors += 1;
    // Mount completion owns this notification so the native interaction finalizes once.
    let notifications = reference(&doc, seated, "enterNotifyEvents")?;
    let notify_events = param(notifications, "events").ok_or("seated state lacks entry events")?;
    let remaining = notify_events
        .children()
        .filter(roxmltree::Node::is_element)
        .filter(|node| {
            param(*node, "id")
                .and_then(|id| id.text())
                .and_then(|id| id.parse::<usize>().ok())
                .and_then(|id| events.get(id))
                .is_none_or(|name| !name.eq_ignore_ascii_case("idleChairSitting"))
        })
        .collect::<Vec<_>>();
    edits.push((
        notify_events.range(),
        format!(
            "<hkparam name=\"events\" numelements=\"{}\">{}</hkparam>",
            remaining.len(),
            remaining
                .iter()
                .map(|node| &xml[node.range()])
                .collect::<String>()
        ),
    ));
    for object in doc
        .descendants()
        .filter(|node| node.attribute("class") == Some("hkbClipGenerator"))
    {
        let name = param(object, "name")
            .and_then(|node| node.text())
            .unwrap_or("");
        let animation =
            param(object, "animationName").ok_or("furniture donor clip has no animation name")?;
        let animation_name = animation
            .text()
            .unwrap_or("")
            .replace('\\', "/")
            .to_ascii_lowercase();
        if matches!(name, "Idle" | "IdleLoop") {
            let id = object
                .attribute("name")
                .ok_or("furniture idle has no object id")?;
            edits.push((
                object.range(),
                selector(
                    id,
                    variable_index,
                    &mut next_id,
                    &mut appended,
                    clips,
                    synchronized_clips,
                ),
            ));
            selectors += 1;
        } else {
            let mapped = if animation_name.contains("enter") {
                enters += 1;
                "Mount"
            } else if animation_name.contains("exit") || name == "FurnitureDeath" {
                exits += 1;
                "Dismount"
            } else {
                return Err(format!(
                    "unmapped furniture donor clip {name}: {animation_name}"
                ));
            };
            edits.push((
                animation.range(),
                format!(
                    "<hkparam name=\"animationName\">Animations\\B21SkyrimRider\\{mapped}.hkx</hkparam>"
                ),
            ));
            let lifecycle = if mapped == "Mount" {
                ["idleChairSitting", "MountEnd", "sitStop"]
            } else {
                ["DismountEnd", "idleChairGetUp", "ReevaluateGraphState"]
            };
            let triggers = take_id(&mut next_id);
            let rows = lifecycle.iter().map(|event| {
                let id = events.iter().position(|name| name == event).unwrap();
                format!("<hkobject><hkparam name=\"localTime\">0</hkparam><hkparam name=\"event\"><hkobject><hkparam name=\"id\">{id}</hkparam><hkparam name=\"payload\">null</hkparam></hkobject></hkparam><hkparam name=\"relativeToEndOfClip\">true</hkparam><hkparam name=\"acyclic\">true</hkparam><hkparam name=\"isAnnotation\">false</hkparam></hkobject>")
            }).collect::<String>();
            appended.push_str(&format!("<hkobject name=\"{triggers}\" class=\"hkbClipTriggerArray\" signature=\"0xf757cd66\"><hkparam name=\"triggers\" numelements=\"3\">{rows}</hkparam></hkobject>"));
            let field = param(object, "triggers")
                .ok_or("furniture lifecycle clip has no triggers field")?;
            edits.push((
                field.range(),
                format!("<hkparam name=\"triggers\">{triggers}</hkparam>"),
            ));
        }
    }
    if selectors != 3 || enters == 0 || exits == 0 {
        return Err("furniture donor lifecycle does not match rider integration".into());
    }
    let section = doc
        .descendants()
        .find(|node| node.has_tag_name("hksection"))
        .ok_or("furniture donor has no data section")?;
    let end = xml[..section.range().end]
        .rfind("</hksection>")
        .ok_or("furniture donor section is not closed")?;
    edits.push((end..end, appended));
    edits.sort_by_key(|(range, _)| range.start);
    for pair in edits.windows(2) {
        if pair[0].0.end > pair[1].0.start {
            return Err("overlapping rider graph edits".into());
        }
    }
    let mut output = xml.to_string();
    for (range, replacement) in edits.into_iter().rev() {
        output.replace_range(range, &replacement);
    }
    Ok(output)
}

fn reference<'a, 'input>(
    doc: &'a roxmltree::Document<'input>,
    object: roxmltree::Node<'a, 'input>,
    field: &str,
) -> Result<roxmltree::Node<'a, 'input>, String> {
    let id = param(object, field)
        .and_then(|node| node.text())
        .ok_or_else(|| format!("furniture object lacks {field} reference"))?;
    doc.descendants()
        .find(|node| node.attribute("name") == Some(id))
        .ok_or_else(|| format!("furniture {field} reference {id} is absent"))
}

fn selector(
    id: &str,
    variable_index: usize,
    next_id: &mut usize,
    appended: &mut String,
    clips: &[RiderClipReceipt],
    names: &[String],
) -> String {
    let binding = take_id(next_id);
    appended.push_str(&format!("<hkobject name=\"{binding}\" class=\"hkbVariableBindingSet\" signature=\"0xe942f339\"><hkparam name=\"bindings\" numelements=\"1\"><hkobject><hkparam name=\"memberPath\">selectedGeneratorIndex</hkparam><hkparam name=\"variableIndex\">{variable_index}</hkparam><hkparam name=\"bitIndex\">-1</hkparam><hkparam name=\"bindingType\">BINDING_TYPE_VARIABLE</hkparam></hkobject></hkparam><hkparam name=\"indexOfBindingToEnable\">-1</hkparam></hkobject>"));
    let children = names.iter().map(|name| {
        let clip = clips.iter().find(|clip| &clip.name == name).unwrap();
        let child = take_id(next_id);
        let path = if clip.runtime_path.starts_with("__rider_tree__/") { clip.runtime_path.clone() } else { super::rider_animation_name(&clip.runtime_path).expect("rider clips belong to the character project") };
        appended.push_str(&format!("<hkobject name=\"{child}\" class=\"hkbClipGenerator\" signature=\"0x0d4cc9f6\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"animationBundleName\">null</hkparam><hkparam name=\"animationName\">{path}</hkparam><hkparam name=\"triggers\">null</hkparam><hkparam name=\"userPartitionMask\">0</hkparam><hkparam name=\"cropStartAmountLocalTime\">0</hkparam><hkparam name=\"cropEndAmountLocalTime\">0</hkparam><hkparam name=\"startTime\">0</hkparam><hkparam name=\"playbackSpeed\">1</hkparam><hkparam name=\"enforcedDuration\">0</hkparam><hkparam name=\"userControlledTimeFraction\">0</hkparam><hkparam name=\"animationBindingIndex\">-1</hkparam><hkparam name=\"mode\">{mode}</hkparam><hkparam name=\"flags\">0</hkparam></hkobject>",mode=if clip.looping {"MODE_LOOPING"} else {"MODE_SINGLE_PLAY"}));
        child
    }).collect::<Vec<_>>();
    format!(
        "<hkobject name=\"{id}\" class=\"hkbManualSelectorGenerator\" signature=\"0xeed8d5cd\"><hkparam name=\"variableBindingSet\">{binding}</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">B21SkyrimRiderSync</hkparam><hkparam name=\"generators\" numelements=\"{}\">{}</hkparam><hkparam name=\"selectedGeneratorIndex\">0</hkparam><hkparam name=\"indexSelector\">null</hkparam><hkparam name=\"selectedIndexCanChangeAfterActivate\">true</hkparam><hkparam name=\"generatorChangedTransitionEffect\">null</hkparam></hkobject>",
        children.len(),
        children.join(" ")
    )
}

fn variable_info(kind: &str) -> String {
    format!(
        "<hkobject><hkparam name=\"role\"><hkobject><hkparam name=\"role\">ROLE_DEFAULT</hkparam><hkparam name=\"flags\">FLAG_NONE</hkparam></hkobject></hkparam><hkparam name=\"type\">{kind}</hkparam></hkobject>"
    )
}
fn take_id(next: &mut usize) -> String {
    let id = format!("#{:04}", *next);
    *next += 1;
    id
}
fn param<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children()
        .find(|child| child.attribute("name") == Some(name))
}
fn array<'a, 'input>(
    doc: &'a roxmltree::Document<'input>,
    name: &str,
) -> Result<roxmltree::Node<'a, 'input>, String> {
    doc.descendants()
        .find(|node| node.attribute("name") == Some(name))
        .ok_or_else(|| format!("furniture donor has no {name}"))
}

#[cfg(test)]
mod tests {
    use super::super::RIDER_ANIMATIONS;
    use super::*;
    #[test]
    #[ignore = "requires Fallout 4 extracted furniture behavior"]
    fn rider_furniture_graph_packs_with_native_interaction_and_sync() {
        let root = std::env::var("MODKIT_REPO_ROOT").expect("MODKIT_REPO_ROOT");
        let donor = std::fs::read(
            std::path::Path::new(&root)
                .join("extracted/fo4/meshes/actors/character/behaviors/furniturebehavior.hkx"),
        )
        .unwrap();
        let clips = ["Mount", "Dismount", "Idle", "Run"]
            .into_iter()
            .map(|name| RiderClipReceipt {
                name: name.into(),
                source_path: String::new(),
                runtime_path: format!("{RIDER_ANIMATIONS}/{name}.hkx"),
                duration: 1.0,
                looping: matches!(name, "Idle" | "Run"),
                source_tracks: 99,
                target_tracks: 70,
                paired: matches!(name, "Mount" | "Dismount"),
                paired_origin: None,
                source_events: Vec::new(),
                source_float_tracks: Vec::new(),
            })
            .collect::<Vec<_>>();
        let packed = stage_graph(&donor, &clips, &["Idle".into(), "Run".into()]).unwrap();
        let xml = havok_native::api::havok_hkx_to_xml(&packed).unwrap();
        assert!(xml.contains("sitStartFromStand"));
        assert!(xml.contains("standStart"));
        assert!(xml.contains("SyncLocomotionState"));
        assert!(!xml.contains("Animations\\1HM\\Idle.hkt"));
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let seated = doc
            .descendants()
            .find(|node| {
                node.attribute("class") == Some("hkbStateMachineStateInfo")
                    && param(*node, "name").and_then(|field| field.text()) == Some("Idle_Sitting")
            })
            .unwrap();
        let modifier = reference(&doc, seated, "generator").unwrap();
        let synchronized = reference(&doc, modifier, "generator").unwrap();
        assert_eq!(
            synchronized.attribute("class"),
            Some("hkbManualSelectorGenerator")
        );
        assert_eq!(
            param(synchronized, "name").unwrap().text(),
            Some("B21SkyrimRiderSync")
        );
        assert!(!xml.contains("Behaviors\\DialogueBehavior.hkx"));
        let events = array(&doc, "eventNames")
            .unwrap()
            .children()
            .filter(roxmltree::Node::is_element)
            .filter_map(|node| node.text())
            .collect::<Vec<_>>();
        for clip in doc
            .descendants()
            .filter(|node| node.attribute("class") == Some("hkbClipGenerator"))
        {
            let path = param(clip, "animationName").unwrap().text().unwrap();
            assert!(path.starts_with("Animations\\B21SkyrimRider\\"));
            let expected = if path.ends_with("\\Mount.hkx") {
                ["idleChairSitting", "MountEnd", "sitStop"]
            } else if path.ends_with("\\Dismount.hkx") {
                ["DismountEnd", "idleChairGetUp", "ReevaluateGraphState"]
            } else {
                continue;
            };
            let reference = param(clip, "triggers").unwrap().text().unwrap();
            let triggers = doc
                .descendants()
                .find(|node| node.attribute("name") == Some(reference))
                .unwrap();
            let names = triggers
                .descendants()
                .filter(|node| node.attribute("name") == Some("id"))
                .map(|node| events[node.text().unwrap().parse::<usize>().unwrap()])
                .collect::<Vec<_>>();
            assert_eq!(names, expected);
        }
    }
}
