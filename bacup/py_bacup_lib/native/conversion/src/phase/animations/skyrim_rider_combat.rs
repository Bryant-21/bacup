use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::Path;

use havok_native::animation::parsers::SkeletonRecord;
use havok_native::hkx::descriptors::DescriptorRegistry;

use super::{RiderClipReceipt, RiderClipRequest};

pub(super) struct MountedCombatGraph {
    source: String,
    root: String,
    movement: String,
    closure: BTreeSet<String>,
    masks: BTreeMap<String, Vec<f32>>,
    bone_indices: BTreeMap<usize, usize>,
    timed_events: BTreeMap<String, Vec<NativeTrigger>>,
    pub requests: Vec<RiderClipRequest>,
}

pub(super) fn prepare(
    source_meshes: &Path,
    target_meshes: &Path,
) -> Result<MountedCombatGraph, String> {
    let read = |path: &Path| -> Result<String, String> {
        let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        havok_native::api::havok_hkx_to_xml(&bytes).map_err(|error| error.to_string())
    };
    let source = read(&source_meshes.join("actors/character/behaviors/horsebehavior.hkx"))?;
    let character = read(&source_meshes.join("actors/character/characters/defaultmale.hkx"))?;
    let source_skeleton = super::read_skeleton(&source_meshes.join(super::SOURCE_SKELETON))?;
    let target_skeleton = super::read_skeleton(&target_meshes.join(super::TARGET_SKELETON))?;
    let zoom = read(&source_meshes.join("actors/character/animations/Bow_Zoom.hkx"))?;
    let source = use_native_camera_zoom(&source, &zoom, &source_skeleton)?;
    let mut prepared = prepare_xml(source, &character, &source_skeleton, &target_skeleton)?;
    for request in &prepared.requests {
        if !is_crossbow(request) {
            continue;
        }
        let xml = read(&source_meshes.join(&request.source_path))?;
        let decoded = havok_native::animation::clip::extract_clip(&xml, Some(&source_skeleton))
            .map_err(|error| error.to_string())?;
        let release = request
            .source_path
            .to_ascii_lowercase()
            .ends_with("mc_crossbow_release.hkx");
        let reload = request
            .source_path
            .to_ascii_lowercase()
            .ends_with("mc_crossbow_reload.hkx");
        let mut timed = Vec::new();
        for event in &decoded.events {
            if release && event.text == "SoundPlay.WPNCrossbowReload" {
                timed.push(NativeTrigger {
                    time: event.time,
                    event: "attackState".into(),
                    payload: Some("Exit"),
                });
                timed.push(NativeTrigger {
                    time: event.time,
                    event: "reloadState".into(),
                    payload: Some("Enter"),
                });
            }
            match event.text.as_str() {
                "arrowRelease" => timed.push(NativeTrigger {
                    time: event.time,
                    event: "weaponFire".into(),
                    payload: None,
                }),
                "arrowAttach" if release || reload => {
                    timed.push(NativeTrigger {
                        time: event.time,
                        event: "reloadComplete".into(),
                        payload: None,
                    });
                    if release {
                        timed.push(NativeTrigger {
                            time: event.time,
                            event: "reloadState".into(),
                            payload: Some("Exit"),
                        });
                    }
                }
                other => timed.push(NativeTrigger {
                    time: event.time,
                    event: other.into(),
                    payload: None,
                }),
            }
        }
        if release
            && timed
                .iter()
                .filter(|event| event.event == "weaponFire")
                .count()
                != 1
        {
            return Err(format!(
                "{} lacks exactly one source projectile release",
                request.source_path
            ));
        }
        if (release || reload)
            && timed
                .iter()
                .filter(|event| event.event == "reloadComplete")
                .count()
                != 1
        {
            return Err(format!(
                "{} lacks exactly one source reload completion",
                request.source_path
            ));
        }
        if release {
            let first = |name: &str, payload: Option<&str>| {
                timed
                    .iter()
                    .filter(|event| event.event == name && event.payload == payload)
                    .map(|event| event.time)
                    .collect::<Vec<_>>()
            };
            let fire = first("weaponFire", None);
            let start = first("reloadState", Some("Enter"));
            let finish = first("reloadState", Some("Exit"));
            if start.len() != 1
                || finish.len() != 1
                || !(fire[0] < start[0] && start[0] < finish[0])
            {
                return Err(format!(
                    "{} has no ordered source crossbow fire/reload lifecycle",
                    request.source_path
                ));
            }
        }
        timed.sort_by(|a, b| a.time.total_cmp(&b.time));
        prepared.timed_events.insert(request.name.clone(), timed);
    }
    Ok(prepared)
}

fn use_native_camera_zoom(
    source: &str,
    animation: &str,
    skeleton: &SkeletonRecord,
) -> Result<String, String> {
    let clip = havok_native::animation::clip::extract_clip(animation, Some(skeleton))
        .map_err(|error| error.to_string())?;
    let doc = roxmltree::Document::parse(animation).map_err(|error| error.to_string())?;
    let additive = array(&doc, "blendHint")?.text().unwrap_or("").trim();
    if !matches!(additive, "ADDITIVE" | "ADDITIVE_DEPRECATED" | "1")
        || clip.channels.len() != 1
        || clip.track_to_bone_indices.len() != 1
        || skeleton
            .bone_names
            .get(clip.track_to_bone_indices[0] as usize)
            .map(String::as_str)
            != Some("Camera Control")
        || !clip.events.is_empty()
        || !clip.extracted_motion_ref.is_empty()
    {
        return Err("mounted zoom is not an isolated source camera animation".into());
    }
    let doc = roxmltree::Document::parse(source).map_err(|error| error.to_string())?;
    let objects = objects(&doc);
    let zoom_clips = objects
        .iter()
        .filter(|(_, node)| {
            node.attribute("class") == Some("hkbClipGenerator")
                && text(**node, "animationName").is_some_and(|path| {
                    path.replace('\\', "/")
                        .eq_ignore_ascii_case("Animations/Bow_Zoom.hkx")
                })
        })
        .map(|(id, _)| *id)
        .collect::<BTreeSet<_>>();
    let mut camera_children = BTreeSet::new();
    for (id, node) in &objects {
        if node.attribute("class") == Some("hkbBlenderGeneratorChild")
            && text(*node, "generator").is_some_and(|id| zoom_clips.contains(id))
        {
            let weights = text(*node, "boneWeights")
                .and_then(|id| objects.get(id))
                .ok_or("mounted camera has no body exclusion mask")?;
            if number_array(*weights, "boneWeights")?
                .iter()
                .any(|weight| *weight != 0.0)
            {
                return Err("mounted camera animation also affects body bones".into());
            }
            camera_children.insert(*id);
        }
    }
    let mut edits = Vec::new();
    for node in objects
        .values()
        .filter(|node| node.attribute("class") == Some("hkbBlenderGenerator"))
    {
        let children = param(*node, "children").ok_or("mounted blender has no children")?;
        let source_children = children
            .text()
            .unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>();
        let retained = source_children
            .iter()
            .copied()
            .filter(|id| !camera_children.contains(id))
            .collect::<Vec<_>>();
        if retained.len() != source_children.len() {
            if retained.len() != 1 || source_children.len() != 2 {
                return Err("mounted native zoom replacement would alter body blending".into());
            }
            edits.push((
                children.range(),
                format!(
                    "<hkparam name=\"children\" numelements=\"1\">{}</hkparam>",
                    retained[0]
                ),
            ));
        }
    }
    if edits.len() != 2 || zoom_clips.len() != 2 || camera_children.len() != 2 {
        return Err("mounted source camera zoom topology changed".into());
    }
    // FO4 consumes the preserved BowZoomStart/BowZoomStop events through its player camera handlers.
    apply_edits(source, edits)
}

struct NativeTrigger {
    time: f32,
    event: String,
    payload: Option<&'static str>,
}

fn is_crossbow(request: &RiderClipRequest) -> bool {
    request
        .source_path
        .to_ascii_lowercase()
        .contains("/mc_crossbow_")
}

pub(super) fn adapt_clip_events(
    clip: &mut super::AnimationClip,
    request: &RiderClipRequest,
) -> Result<(), String> {
    if request.name.starts_with("MountedCombat") && is_crossbow(request) {
        clip.events.clear();
    }
    Ok(())
}

fn prepare_xml(
    source: String,
    character: &str,
    source_skeleton: &SkeletonRecord,
    target_skeleton: &SkeletonRecord,
) -> Result<MountedCombatGraph, String> {
    let doc = roxmltree::Document::parse(&source).map_err(|error| error.to_string())?;
    let objects = objects(&doc);
    let named = |name: &str| {
        objects
            .iter()
            .find(|(_, node)| text(**node, "name") == Some(name))
            .map(|(id, _)| (*id).to_string())
            .ok_or_else(|| format!("mounted source lacks {name}"))
    };
    let root = named("DefaultBehavior")?;
    let movement = named("NonCombat_Behavior")?;
    let closure = closure(&objects, &root, &movement)?;
    let source_pose = super::target_reference_pose(source_skeleton)?;
    let target_pose = super::target_reference_pose(target_skeleton)?;
    let mapping = super::rider_bone_map(&source_pose, &target_pose)?;
    let bone_indices = mapping
        .iter()
        .map(|(source, target)| {
            let source = source_skeleton
                .bone_names
                .iter()
                .position(|bone| bone.eq_ignore_ascii_case(source))
                .unwrap();
            let target = target_skeleton
                .bone_names
                .iter()
                .position(|bone| bone.eq_ignore_ascii_case(target))
                .unwrap();
            (source, target)
        })
        .collect::<BTreeMap<_, _>>();
    let character_doc = roxmltree::Document::parse(character).map_err(|error| error.to_string())?;
    let mut masks = BTreeMap::new();
    for name in ["UpperBody", "MountedCombatBowBlend"] {
        let source_weights = property_weights(&character_doc, name)?;
        masks.insert(
            name.into(),
            retarget_weights(
                &source_weights,
                source_skeleton,
                target_skeleton,
                &bone_indices,
            )?,
        );
    }
    masks.insert(
        "LowerBody".into(),
        masks["UpperBody"]
            .iter()
            .map(|weight| 1.0 - weight)
            .collect(),
    );
    let mut requests = Vec::new();
    for id in &closure {
        let object = objects[id.as_str()];
        if object.attribute("class") == Some("hkbClipGenerator") {
            let source_path = text(object, "animationName")
                .ok_or("mounted clip has no source path")?
                .replace('\\', "/");
            if !source_path.to_ascii_lowercase().starts_with("animations/")
                || source_path.split('/').any(|component| component == "..")
            {
                return Err(format!(
                    "mounted combat clip escapes human animation root: {source_path}"
                ));
            }
            requests.push(RiderClipRequest {
                name: clip_name(id),
                source_path: format!("actors/character/{source_path}"),
                looping: text(object, "mode") == Some("MODE_LOOPING"),
            });
        } else if object.attribute("class") == Some("hkbBoneWeightArray") {
            let weights = number_array(object, "boneWeights")?;
            masks.insert(
                id.clone(),
                retarget_weights(&weights, source_skeleton, target_skeleton, &bone_indices)?,
            );
        }
    }
    Ok(MountedCombatGraph {
        source,
        root,
        movement,
        closure,
        masks,
        bone_indices,
        timed_events: BTreeMap::new(),
        requests,
    })
}

pub(super) fn graft(
    bytes: &[u8],
    prepared: &MountedCombatGraph,
    clips: &[RiderClipReceipt],
) -> Result<Vec<u8>, String> {
    let xml = havok_native::api::havok_hkx_to_xml(bytes).map_err(|error| error.to_string())?;
    let edited = graft_xml(&xml, prepared, clips)?;
    crate::source_rig::validate_fo4_havok_xml_signatures(&edited)
        .map_err(|error| error.to_string())?;
    let packed = havok_native::api::havok_xml_to_hkx(&edited).map_err(|error| error.to_string())?;
    let reread = havok_native::api::havok_hkx_to_xml(&packed).map_err(|error| error.to_string())?;
    validate_graft(&reread, prepared)?;
    Ok(packed)
}

fn graft_xml(
    target: &str,
    prepared: &MountedCombatGraph,
    clips: &[RiderClipReceipt],
) -> Result<String, String> {
    let source = roxmltree::Document::parse(&prepared.source).map_err(|error| error.to_string())?;
    let target_doc = roxmltree::Document::parse(target).map_err(|error| error.to_string())?;
    let source_objects = objects(&source);
    let target_objects = objects(&target_doc);
    let (readied_state, readied) = source_objects
        .iter()
        .find(|(_, node)| text(**node, "name") == Some("MountedCombat_Readied_State"))
        .ok_or("mounted combat has no readied state")?;
    let readied_generator =
        text(*readied, "generator").ok_or("mounted readied state has no generator")?;
    let mut edits = Vec::new();
    let variable_map = merge_variables(&source, &target_doc, target, &mut edits)?;
    let event_map = merge_events(&source, &target_doc, target, prepared, &mut edits)?;
    let active_index = variable_map["B21MountCombatActive"];
    let mut next = target_objects
        .keys()
        .filter_map(|id| id.trim_start_matches('#').parse::<usize>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    let mut appended = String::new();
    let mut registry = DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
    let mut replaced = 0;
    for (target_id, selector) in &target_objects {
        if text(*selector, "name") != Some("B21SkyrimRiderSync") {
            continue;
        }
        let movement_id = take_id(&mut next);
        let copy = target[selector.range()].replacen(
            &format!("name=\"{target_id}\""),
            &format!("name=\"{movement_id}\""),
            1,
        );
        appended.push_str(&copy);
        let mut ids = prepared
            .closure
            .iter()
            .map(|id| (id.clone(), take_id(&mut next)))
            .collect::<BTreeMap<_, _>>();
        ids.insert(prepared.movement.clone(), movement_id.clone());
        let mask_ids = prepared
            .masks
            .keys()
            .map(|name| (name.clone(), take_id(&mut next)))
            .collect::<BTreeMap<_, _>>();
        for (name, weights) in &prepared.masks {
            appended.push_str(&weight_object(&mask_ids[name], weights));
        }
        let source_vars = strings(&source, "variableNames")?;
        let source_events = strings(&source, "eventNames")?;
        let properties = strings(&source, "characterPropertyNames")?;
        let payloads = ["Enter", "Exit"]
            .into_iter()
            .map(|value| {
                let id = take_id(&mut next);
                appended.push_str(&format!("<hkobject name=\"{id}\" class=\"hkbStringEventPayload\" signature=\"0x2b363cbe\"><hkparam name=\"data\">{value}</hkparam></hkobject>"));
                (value, id)
            })
            .collect::<BTreeMap<_, _>>();
        let clip_triggers = prepared
            .closure
            .iter()
            .filter(|id| prepared.timed_events.contains_key(&clip_name(id)))
            .map(|id| (id.clone(), take_id(&mut next)))
            .collect::<BTreeMap<_, _>>();
        let mut notifications = BTreeMap::new();
        for id in &prepared.closure {
            let node = source_objects[id.as_str()];
            if node.attribute("class") == Some("hkbStateMachineStateInfo")
                && matches!(
                    text(node, "name"),
                    Some("CrossBow_Release" | "CrossBow_ReloadState")
                )
            {
                for phase in ["enterNotifyEvents", "exitNotifyEvents"] {
                    notifications.insert((id.clone(), phase.to_string()), take_id(&mut next));
                }
            }
        }
        let readied_layer = take_id(&mut next);
        let context = ImportContext {
            ids: &ids,
            variables: &variable_map,
            source_vars: &source_vars,
            events: &event_map,
            source_events: &source_events,
            properties: &properties,
            masks: &mask_ids,
            bones: &prepared.bone_indices,
            clips,
            notifications: &notifications,
            clip_triggers: &clip_triggers,
            readied_state,
            readied_layer: &readied_layer,
        };
        for (id, output_id) in &clip_triggers {
            let clip = source_objects[id.as_str()];
            let mut triggers = String::new();
            let mut count = 0;
            if let Some(original) = text(clip, "triggers").filter(|id| *id != "null") {
                let array = source_objects
                    .get(original)
                    .ok_or("missing source clip triggers")?;
                for row in param(*array, "triggers")
                    .ok_or("source clip trigger array is empty")?
                    .children()
                    .filter(roxmltree::Node::is_element)
                {
                    triggers.push_str(&import_node(row, &context)?);
                    count += 1;
                }
            }
            for trigger in &prepared.timed_events[&clip_name(id)] {
                let event = event_map[event_name(&trigger.event)];
                let payload = trigger
                    .payload
                    .map(|value| payloads[value].as_str())
                    .unwrap_or("null");
                triggers.push_str(&format!("<hkobject><hkparam name=\"localTime\">{}</hkparam><hkparam name=\"event\"><hkobject><hkparam name=\"id\">{event}</hkparam><hkparam name=\"payload\">{payload}</hkparam></hkobject></hkparam><hkparam name=\"relativeToEndOfClip\">false</hkparam><hkparam name=\"acyclic\">true</hkparam><hkparam name=\"isAnnotation\">false</hkparam></hkobject>", trigger.time));
                count += 1;
            }
            appended.push_str(&format!("<hkobject name=\"{output_id}\" class=\"hkbClipTriggerArray\" signature=\"0xf757cd66\"><hkparam name=\"triggers\" numelements=\"{count}\">{triggers}</hkparam></hkobject>"));
        }
        for ((id, phase), output_id) in &notifications {
            let node = source_objects[id.as_str()];
            let original = text(node, phase).unwrap_or("null");
            let mut rows = String::new();
            let mut count = 1;
            if original != "null" {
                let array = source_objects
                    .get(original)
                    .ok_or("source mounted state has missing notifications")?;
                for row in param(*array, "events")
                    .ok_or("mounted notification array has no events")?
                    .children()
                    .filter(roxmltree::Node::is_element)
                {
                    rows.push_str(&import_node(row, &context)?);
                    count += 1;
                }
            }
            let payload = take_id(&mut next);
            let event = if text(node, "name") == Some("CrossBow_ReloadState") {
                "reloadState"
            } else {
                "attackState"
            };
            rows.push_str(&format!("<hkobject><hkparam name=\"id\">{}</hkparam><hkparam name=\"payload\">{payload}</hkparam></hkobject>", event_map[event]));
            if text(node, "name") == Some("CrossBow_Release") && phase == "exitNotifyEvents" {
                rows.push_str(&format!("<hkobject><hkparam name=\"id\">{}</hkparam><hkparam name=\"payload\">{payload}</hkparam></hkobject>", event_map["reloadState"]));
                count += 1;
            }
            appended.push_str(&format!("<hkobject name=\"{payload}\" class=\"hkbStringEventPayload\" signature=\"0x2b363cbe\"><hkparam name=\"data\">{}</hkparam></hkobject><hkobject name=\"{output_id}\" class=\"hkbStateMachineEventPropertyArray\" signature=\"0x71957c2d\"><hkparam name=\"events\" numelements=\"{count}\">{rows}</hkparam></hkobject>", if phase == "enterNotifyEvents" { "Enter" } else { "Exit" }));
        }
        for id in &prepared.closure {
            if id == &prepared.movement
                || source_objects[id.as_str()].attribute("class") == Some("hkbBoneWeightArray")
            {
                continue;
            }
            let node = source_objects[id.as_str()];
            appended.push_str(&import_object(node, id, &context, &mut registry)?);
        }
        let root_id = &ids[&prepared.root];
        let active = take_id(&mut next);
        let binding = take_id(&mut next);
        appended.push_str(&format!("<hkobject name=\"{binding}\" class=\"hkbVariableBindingSet\" signature=\"0xe942f339\"><hkparam name=\"bindings\" numelements=\"1\"><hkobject><hkparam name=\"memberPath\">bIsActive0</hkparam><hkparam name=\"variableIndex\">{active_index}</hkparam><hkparam name=\"bitIndex\">-1</hkparam><hkparam name=\"bindingType\">BINDING_TYPE_VARIABLE</hkparam></hkobject></hkparam><hkparam name=\"indexOfBindingToEnable\">-1</hkparam></hkobject><hkobject name=\"{active}\" class=\"BSIsActiveModifier\" signature=\"0x5bf1f5cf\"><hkparam name=\"variableBindingSet\">{binding}</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">B21MountedCombatActive</hkparam><hkparam name=\"enable\">true</hkparam><hkparam name=\"bIsActive0\">false</hkparam><hkparam name=\"bInvertActive0\">false</hkparam></hkobject>"));
        let lower = take_id(&mut next);
        let upper = take_id(&mut next);
        appended.push_str(&blend_child(&lower, &movement_id, &mask_ids["LowerBody"]));
        appended.push_str(&blend_child(
            &upper,
            &ids[readied_generator],
            &mask_ids["UpperBody"],
        ));
        appended.push_str(&format!("<hkobject name=\"{readied_layer}\" class=\"hkbBlenderGenerator\" signature=\"0xce45c088\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">B21MountedCombatLayers</hkparam><hkparam name=\"referencePoseWeightThreshold\">0</hkparam><hkparam name=\"blendParameter\">0</hkparam><hkparam name=\"minCyclicBlendParameter\">0</hkparam><hkparam name=\"maxCyclicBlendParameter\">1</hkparam><hkparam name=\"indexOfSyncMasterChild\">0</hkparam><hkparam name=\"flags\">0</hkparam><hkparam name=\"subtractLastChild\">false</hkparam><hkparam name=\"children\" numelements=\"2\">{lower} {upper}</hkparam></hkobject>"));
        edits.push((selector.range(), format!("<hkobject name=\"{target_id}\" class=\"hkbModifierGenerator\" signature=\"0xc499fc9e\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">B21MountedCombat</hkparam><hkparam name=\"modifier\">{active}</hkparam><hkparam name=\"generator\">{root_id}</hkparam></hkobject>")));
        replaced += 1;
    }
    if replaced != 3 {
        return Err(format!(
            "mounted combat expected three rider sync branches, found {replaced}"
        ));
    }
    let end = target
        .rfind("</hksection>")
        .ok_or("rider graph has no closing section")?;
    edits.push((end..end, appended));
    apply_edits(target, edits)
}

struct ImportContext<'a> {
    ids: &'a BTreeMap<String, String>,
    variables: &'a BTreeMap<String, usize>,
    source_vars: &'a [String],
    events: &'a BTreeMap<String, usize>,
    source_events: &'a [String],
    properties: &'a [String],
    masks: &'a BTreeMap<String, String>,
    bones: &'a BTreeMap<usize, usize>,
    clips: &'a [RiderClipReceipt],
    notifications: &'a BTreeMap<(String, String), String>,
    clip_triggers: &'a BTreeMap<String, String>,
    readied_state: &'a str,
    readied_layer: &'a str,
}

fn import_object(
    node: roxmltree::Node<'_, '_>,
    source_id: &str,
    context: &ImportContext<'_>,
    registry: &mut DescriptorRegistry,
) -> Result<String, String> {
    let class = node
        .attribute("class")
        .ok_or("mounted graph object has no class")?;
    let descriptor = registry
        .get(class)
        .map_err(|error| error.to_string())?
        .cloned()
        .ok_or_else(|| format!("FO4 has no mounted node {class}"))?;
    let members = registry
        .get_all_members(class)
        .map_err(|error| error.to_string())?;
    let mut output = format!(
        "<hkobject name=\"{}\" class=\"{class}\" signature=\"{}\">",
        context.ids[source_id], descriptor.signature
    );
    if class == "BSDirectAtModifier" {
        output.push_str(&direct_at(node, context)?);
    } else {
        for child in node.children().filter(roxmltree::Node::is_element) {
            let name = child.attribute("name").unwrap_or("");
            if !members.iter().any(|member| member.name == name) {
                return Err(format!("mounted node {class}.{name} has no FO4 field"));
            }
            if source_id == context.readied_state && name == "generator" {
                output.push_str(&field(name, context.readied_layer));
            } else if let Some(notification) =
                context.notifications.get(&(source_id.into(), name.into()))
            {
                output.push_str(&field(name, notification));
            } else if class == "hkbClipGenerator"
                && name == "triggers"
                && context.clip_triggers.contains_key(source_id)
            {
                output.push_str(&field(name, &context.clip_triggers[source_id]));
            } else if class == "hkbClipGenerator" && name == "animationName" {
                let name = clip_name(source_id);
                let clip = context
                    .clips
                    .iter()
                    .find(|clip| clip.name == name)
                    .ok_or_else(|| format!("mounted clip {name} was not staged"))?;
                output.push_str(&field(
                    "animationName",
                    &super::rider_animation_name(&clip.runtime_path)?,
                ));
            } else if class == "hkbClipGenerator" && name == "animationBindingIndex" {
                output.push_str(&field(name, "-1"));
            } else if class == "hkbBlenderGeneratorChild" && name == "boneWeights" {
                let property = property_binding(node, context)?;
                let source = child.text().unwrap_or("null").trim();
                output.push_str(&field(
                    name,
                    property
                        .as_deref()
                        .or_else(|| context.masks.get(source).map(String::as_str))
                        .unwrap_or("null"),
                ));
            } else {
                output.push_str(&import_node(child, context)?);
            }
        }
    }
    output.push_str("</hkobject>");
    Ok(output)
}

fn import_node(
    node: roxmltree::Node<'_, '_>,
    context: &ImportContext<'_>,
) -> Result<String, String> {
    let name = node.attribute("name").unwrap_or("");
    if node.has_tag_name("hkobject")
        && text(node, "bindingType") == Some("BINDING_TYPE_CHARACTER_PROPERTY")
    {
        if text(node, "memberPath") != Some("boneWeights") {
            return Err("mounted character property requires explicit target adaptation".into());
        }
        return Ok(String::new());
    }
    let mut output = format!("<{}", node.tag_name().name());
    for attribute in node.attributes() {
        if attribute.name() == "numelements" && name == "bindings" {
            let count = node
                .children()
                .filter(roxmltree::Node::is_element)
                .filter(|child| {
                    text(*child, "bindingType") != Some("BINDING_TYPE_CHARACTER_PROPERTY")
                })
                .map(|child| {
                    if matches!(
                        text(child, "memberPath"),
                        Some("limitHeadingDegrees" | "limitPitchDegrees")
                    ) {
                        2
                    } else {
                        1
                    }
                })
                .sum::<usize>();
            output.push_str(&format!(" numelements=\"{count}\""));
        } else {
            output.push_str(&format!(
                " {}=\"{}\"",
                attribute.name(),
                escape(attribute.value())
            ));
        }
    }
    output.push('>');
    if node.children().any(|child| child.is_element()) {
        for child in node.children().filter(roxmltree::Node::is_element) {
            output.push_str(&import_node(child, context)?);
        }
    } else {
        let original = node.text().unwrap_or("").trim();
        let mut value = original.to_string();
        if original.starts_with('#')
            && original
                .split_whitespace()
                .all(|token| token.starts_with('#'))
        {
            value = original
                .split_whitespace()
                .map(|id| {
                    context
                        .ids
                        .get(id)
                        .or_else(|| context.masks.get(id))
                        .cloned()
                        .ok_or_else(|| format!("mounted graph retains unresolved {id}"))
                })
                .collect::<Result<Vec<_>, _>>()?
                .join(" ");
        } else if matches!(
            name,
            "variableIndex" | "syncVariableIndex" | "assignmentVariableIndex"
        ) {
            value = remap_index(
                original,
                context.source_vars,
                context.variables,
                variable_name,
            )?;
        } else if name == "id"
            || name.ends_with("EventId")
            || matches!(name, "eventId" | "assignmentEventIndex")
        {
            value = remap_index(original, context.source_events, context.events, event_name)?;
        } else if name == "expression" {
            value = replace_identifier(&value, "Speed", "HorseSpeedSampled");
            value = replace_identifier(&value, "TurnDelta", "TurnDeltaDamped");
            value = replace_identifier(&value, "bowZoomStart", "BowZoomStart");
            value = replace_identifier(&value, "bowZoomStop", "BowZoomStop");
        } else if name == "memberPath" {
            value = match original {
                "offsetHeadingDegrees" => "initialHeadingOffset",
                "offsetPitchDegrees" => "initialPitchOffset",
                _ => original,
            }
            .into();
        }
        output.push_str(&escape(&value));
    }
    output.push_str(&format!("</{}>", node.tag_name().name()));
    if node.has_tag_name("hkobject") {
        if let Some(source @ ("limitHeadingDegrees" | "limitPitchDegrees")) =
            text(node, "memberPath")
        {
            let targets = if source == "limitHeadingDegrees" {
                ["limitHeadingDegreesCCW", "limitHeadingDegreesCW"]
            } else {
                ["limitPitchDegreesUp", "limitPitchDegreesDown"]
            };
            return Ok(targets
                .iter()
                .map(|target| {
                    output.replace(&field("memberPath", source), &field("memberPath", target))
                })
                .collect());
        }
    }
    Ok(output)
}

fn property_binding(
    node: roxmltree::Node<'_, '_>,
    context: &ImportContext<'_>,
) -> Result<Option<String>, String> {
    let binding = text(node, "variableBindingSet").unwrap_or("null");
    if binding == "null" {
        return Ok(None);
    }
    let Some(binding) = node
        .document()
        .descendants()
        .find(|node| node.attribute("name") == Some(binding))
    else {
        return Err("missing mounted variable binding".into());
    };
    for row in binding
        .descendants()
        .filter(|node| node.has_tag_name("hkobject"))
    {
        if text(row, "bindingType") != Some("BINDING_TYPE_CHARACTER_PROPERTY") {
            continue;
        }
        if text(row, "memberPath") != Some("boneWeights") {
            return Err("unsupported mounted character property member".into());
        }
        let index = text(row, "variableIndex")
            .ok_or("property has no index")?
            .parse::<usize>()
            .map_err(|error| error.to_string())?;
        let name = context
            .properties
            .get(index)
            .ok_or("mounted property index out of range")?;
        return context
            .masks
            .get(name)
            .cloned()
            .map(Some)
            .ok_or_else(|| format!("unmapped mounted property {name}"));
    }
    Ok(None)
}

fn direct_at(node: roxmltree::Node<'_, '_>, context: &ImportContext<'_>) -> Result<String, String> {
    let mut result = String::new();
    for name in [
        "variableBindingSet",
        "userData",
        "name",
        "enable",
        "onGain",
        "offGain",
        "directAtCameraX",
        "directAtCameraY",
        "directAtCameraZ",
        "active",
        "currentHeadingOffset",
        "currentPitchOffset",
    ] {
        result.push_str(&import_node(
            param(node, name).ok_or_else(|| format!("source mounted aim lacks {name}"))?,
            context,
        )?);
    }
    for name in ["sourceBoneIndex", "startBoneIndex", "endBoneIndex"] {
        let source = text(node, name)
            .ok_or("source aim has no bone")?
            .parse::<i32>()
            .map_err(|error| error.to_string())?;
        let mapped = if source < 0 {
            source
        } else {
            *context
                .bones
                .get(&(source as usize))
                .ok_or_else(|| format!("unmapped mounted aim bone {source}"))? as i32
        };
        result.push_str(&field(name, &mapped.to_string()));
    }
    for (source, targets) in [
        ("directAtTarget", &["directAtOn"][..]),
        (
            "limitHeadingDegrees",
            &["limitHeadingDegreesCCW", "limitHeadingDegreesCW"][..],
        ),
        (
            "limitPitchDegrees",
            &["limitPitchDegreesUp", "limitPitchDegreesDown"][..],
        ),
        ("offsetHeadingDegrees", &["initialHeadingOffset"][..]),
        ("offsetPitchDegrees", &["initialPitchOffset"][..]),
    ] {
        let value =
            text(node, source).ok_or_else(|| format!("source mounted aim lacks {source}"))?;
        for target in targets {
            result.push_str(&field(target, value));
        }
    }
    // FO4's native BowAttack target follows player look direction; Skyrim's camera XYZ channels do not exist here.
    result.push_str(&field("target", "AppSupplied"));
    result.push_str(&field("context", "BowAttack"));
    result.push_str(&field("offsetsCharacterRelative", "true"));
    result.push_str(&field("useCurrentSourceBonePoseEventId", "-1"));
    result.push_str(&field("snapToTargetEventId", "-1"));
    Ok(result)
}

fn merge_variables(
    source: &roxmltree::Document<'_>,
    target: &roxmltree::Document<'_>,
    xml: &str,
    edits: &mut Vec<(Range<usize>, String)>,
) -> Result<BTreeMap<String, usize>, String> {
    let source_names = strings(source, "variableNames")?;
    let mut names = strings(target, "variableNames")?;
    let infos = rows(source, "variableInfos")?;
    let values = rows(source, "wordVariableValues")?;
    if infos.len() != source_names.len() || values.len() != source_names.len() {
        return Err("mounted source variable arrays disagree".into());
    }
    let original = names.len();
    let source_quad = array(source, "quadVariableValues")?;
    let target_quad = array(target, "quadVariableValues")?;
    let source_quad_count = source_quad
        .attribute("numelements")
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|error| error.to_string())?;
    let target_quad_count = target_quad
        .attribute("numelements")
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|error| error.to_string())?;
    let mut added_names = String::new();
    let mut added_infos = String::new();
    let mut added_values = String::new();
    for (index, source_name) in source_names.iter().enumerate() {
        let name = variable_name(source_name);
        if names.iter().any(|existing| existing == name) {
            continue;
        }
        let kind = text(infos[index], "type").ok_or("mounted source variable lacks type")?;
        if !matches!(
            kind,
            "VARIABLE_TYPE_BOOL"
                | "VARIABLE_TYPE_INT8"
                | "VARIABLE_TYPE_INT16"
                | "VARIABLE_TYPE_INT32"
                | "VARIABLE_TYPE_REAL"
                | "VARIABLE_TYPE_VECTOR4"
        ) {
            return Err(format!("unsupported mounted variable {name}: {kind}"));
        }
        names.push(name.into());
        added_names.push_str(&format!("<hkcstring>{}</hkcstring>", escape(name)));
        added_infos.push_str(&source.input_text()[infos[index].range()]);
        if kind == "VARIABLE_TYPE_VECTOR4" {
            let source_index = text(values[index], "value")
                .ok_or("mounted vector variable has no value")?
                .parse::<usize>()
                .map_err(|error| error.to_string())?;
            if source_index >= source_quad_count {
                return Err("mounted vector variable index is out of range".into());
            }
            added_values.push_str(&format!(
                "<hkobject><hkparam name=\"value\">{}</hkparam></hkobject>",
                target_quad_count + source_index
            ));
        } else {
            added_values.push_str(&source.input_text()[values[index].range()]);
        }
    }
    names.push("B21MountCombatActive".into());
    added_names.push_str("<hkcstring>B21MountCombatActive</hkcstring>");
    added_infos.push_str("<hkobject><hkparam name=\"role\"><hkobject><hkparam name=\"role\">ROLE_DEFAULT</hkparam><hkparam name=\"flags\">FLAG_NONE</hkparam></hkobject></hkparam><hkparam name=\"type\">VARIABLE_TYPE_BOOL</hkparam></hkobject>");
    added_values.push_str("<hkobject><hkparam name=\"value\">0</hkparam></hkobject>");
    for (name, extra) in [
        ("variableNames", added_names),
        ("variableInfos", added_infos),
        ("wordVariableValues", added_values),
    ] {
        append_array(target, xml, name, original, names.len(), &extra, edits)?;
    }
    if source_quad_count != 0 {
        edits.push((
            target_quad.range(),
            format!(
                "<hkparam name=\"quadVariableValues\" numelements=\"{}\">{} {}</hkparam>",
                target_quad_count + source_quad_count,
                target_quad.text().unwrap_or(""),
                source_quad.text().unwrap_or("")
            ),
        ));
    }
    Ok(names
        .into_iter()
        .enumerate()
        .map(|(index, name)| (name, index))
        .collect())
}

fn merge_events(
    source: &roxmltree::Document<'_>,
    target: &roxmltree::Document<'_>,
    xml: &str,
    prepared: &MountedCombatGraph,
    edits: &mut Vec<(Range<usize>, String)>,
) -> Result<BTreeMap<String, usize>, String> {
    let mut names = strings(target, "eventNames")?;
    let original = names.len();
    let mut extra_names = String::new();
    let mut flags = rows(target, "eventInfos")?
        .into_iter()
        .map(event_flags)
        .collect::<Result<Vec<_>, _>>()?;
    let source_names = strings(source, "eventNames")?;
    let source_flags = rows(source, "eventInfos")?
        .into_iter()
        .map(event_flags)
        .collect::<Result<Vec<_>, _>>()?;
    if flags.len() != names.len() || source_flags.len() != source_names.len() {
        return Err("mounted event flag arrays disagree".into());
    }
    for (source_name, source_flag) in source_names
        .into_iter()
        .zip(source_flags)
        .chain(
            [
                "reloadState".into(),
                "weaponFire".into(),
                "attackState".into(),
            ]
            .into_iter()
            .map(|name| (name, 0)),
        )
        .chain(
            prepared
                .timed_events
                .values()
                .flatten()
                .map(|event| (event.event.clone(), 0)),
        )
    {
        let name = event_name(&source_name);
        if let Some(index) = names.iter().position(|existing| existing == name) {
            flags[index] |= source_flag;
            continue;
        }
        names.push(name.into());
        extra_names.push_str(&format!("<hkcstring>{}</hkcstring>", escape(name)));
        flags.push(source_flag);
    }
    append_array(
        target,
        xml,
        "eventNames",
        original,
        names.len(),
        &extra_names,
        edits,
    )?;
    let infos = flags
        .iter()
        .map(|flag| format!("<hkobject><hkparam name=\"flags\">{flag}</hkparam></hkobject>"))
        .collect::<String>();
    edits.push((
        array(target, "eventInfos")?.range(),
        format!(
            "<hkparam name=\"eventInfos\" numelements=\"{}\">{infos}</hkparam>",
            names.len()
        ),
    ));
    Ok(names
        .into_iter()
        .enumerate()
        .map(|(index, name)| (name, index))
        .collect())
}

fn event_flags(node: roxmltree::Node<'_, '_>) -> Result<u32, String> {
    let value = text(node, "flags").ok_or("mounted event flags missing")?;
    if let Ok(number) = value.parse::<u32>() {
        return Ok(number);
    }
    value
        .split('|')
        .map(str::trim)
        .try_fold(0, |bits, flag| match flag {
            "FLAG_SILENT" => Ok(bits | 1),
            "FLAG_SYNC_POINT" => Ok(bits | 2),
            _ => Err(format!("invalid mounted event flag {flag}")),
        })
}

fn variable_name(name: &str) -> &str {
    match name {
        "Speed" => "HorseSpeedSampled",
        "TurnDelta" => "TurnDeltaDamped",
        _ => name,
    }
}
fn event_name(name: &str) -> &str {
    match name {
        "bowDrawn" => "BowDrawn",
        "bowZoomStart" => "BowZoomStart",
        "bowZoomStop" => "BowZoomStop",
        "weaponLeftSwing" => "weaponSwing",
        _ => name,
    }
}

fn property_weights(doc: &roxmltree::Document<'_>, name: &str) -> Result<Vec<f32>, String> {
    let names = strings(doc, "characterPropertyNames")?;
    let index = names
        .iter()
        .position(|candidate| candidate == name)
        .ok_or_else(|| format!("source character lacks {name}"))?;
    let values = rows(doc, "wordVariableValues")?;
    let variant = text(
        *values
            .get(index)
            .ok_or("source property has no initial value")?,
        "value",
    )
    .ok_or("source property initial value missing")?
    .parse::<usize>()
    .map_err(|error| error.to_string())?;
    let refs = array(doc, "variantVariableValues")?
        .text()
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>();
    let id = refs
        .get(variant)
        .ok_or("source property variant index out of range")?;
    let objects = objects(doc);
    number_array(
        *objects
            .get(id)
            .ok_or("source property has no bone weight object")?,
        "boneWeights",
    )
}

fn retarget_weights(
    weights: &[f32],
    source: &SkeletonRecord,
    target: &SkeletonRecord,
    mapping: &BTreeMap<usize, usize>,
) -> Result<Vec<f32>, String> {
    if weights.len() > source.bone_names.len()
        || weights
            .iter()
            .any(|weight| !weight.is_finite() || !(0.0..=1.0).contains(weight))
    {
        return Err("invalid mounted source bone weights".into());
    }
    let inverse = mapping
        .iter()
        .map(|(source, target)| (*target, *source))
        .collect::<BTreeMap<_, _>>();
    let mut output = vec![0.0; target.bone_names.len()];
    for index in 0..output.len() {
        let mut ancestor = index as i32;
        while ancestor >= 0 {
            if let Some(source_index) = inverse.get(&(ancestor as usize)) {
                output[index] = weights.get(*source_index).copied().unwrap_or(0.0);
                break;
            }
            ancestor = i32::from(target.parent_indices[ancestor as usize]);
        }
    }
    Ok(output)
}

fn closure(
    objects: &BTreeMap<&str, roxmltree::Node<'_, '_>>,
    root: &str,
    cutoff: &str,
) -> Result<BTreeSet<String>, String> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root.to_string()];
    while let Some(id) = pending.pop() {
        if !seen.insert(id.clone()) || id == cutoff {
            continue;
        }
        let object = objects
            .get(id.as_str())
            .ok_or_else(|| format!("source mounted graph references missing {id}"))?;
        for node in object
            .descendants()
            .filter(|node| node.is_element() && !node.children().any(|child| child.is_element()))
        {
            for token in node
                .text()
                .unwrap_or("")
                .split_whitespace()
                .filter(|token| token.starts_with('#'))
            {
                pending.push(token.into());
            }
        }
    }
    Ok(seen)
}

fn remap_index(
    value: &str,
    names: &[String],
    mapped: &BTreeMap<String, usize>,
    rename: fn(&str) -> &str,
) -> Result<String, String> {
    let index = value.parse::<i32>().map_err(|error| error.to_string())?;
    if index < 0 {
        return Ok(value.into());
    }
    let name = names
        .get(index as usize)
        .ok_or("mounted graph index out of range")?;
    mapped
        .get(rename(name))
        .map(usize::to_string)
        .ok_or_else(|| format!("mounted graph lacks remapped {name}"))
}
fn clip_name(id: &str) -> String {
    format!("MountedCombat{}", id.trim_start_matches('#'))
}
fn field(name: &str, value: &str) -> String {
    format!("<hkparam name=\"{name}\">{}</hkparam>", escape(value))
}
fn take_id(next: &mut usize) -> String {
    let id = format!("#{:04}", *next);
    *next += 1;
    id
}
fn weight_object(id: &str, weights: &[f32]) -> String {
    format!(
        "<hkobject name=\"{id}\" class=\"hkbBoneWeightArray\" signature=\"0xcd902b77\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"boneWeights\" numelements=\"{}\">{}</hkparam></hkobject>",
        weights.len(),
        weights
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    )
}
fn blend_child(id: &str, generator: &str, weights: &str) -> String {
    format!(
        "<hkobject name=\"{id}\" class=\"hkbBlenderGeneratorChild\" signature=\"0xb35bbfd3\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"generator\">{generator}</hkparam><hkparam name=\"boneWeights\">{weights}</hkparam><hkparam name=\"weight\">1</hkparam><hkparam name=\"worldFromModelWeight\">0</hkparam></hkobject>"
    )
}
fn objects<'a, 'input>(
    doc: &'a roxmltree::Document<'input>,
) -> BTreeMap<&'a str, roxmltree::Node<'a, 'input>> {
    doc.descendants()
        .filter(|node| node.has_tag_name("hkobject") && node.attribute("class").is_some())
        .filter_map(|node| node.attribute("name").map(|name| (name, node)))
        .collect()
}
fn param<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children()
        .find(|child| child.attribute("name") == Some(name))
}
fn text<'a>(node: roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    param(node, name)
        .and_then(|node| node.text())
        .map(str::trim)
}
fn array<'a, 'input>(
    doc: &'a roxmltree::Document<'input>,
    name: &str,
) -> Result<roxmltree::Node<'a, 'input>, String> {
    doc.descendants()
        .find(|node| node.attribute("name") == Some(name))
        .ok_or_else(|| format!("graph lacks {name}"))
}
fn rows<'a, 'input>(
    doc: &'a roxmltree::Document<'input>,
    name: &str,
) -> Result<Vec<roxmltree::Node<'a, 'input>>, String> {
    Ok(array(doc, name)?
        .children()
        .filter(roxmltree::Node::is_element)
        .collect())
}
fn strings(doc: &roxmltree::Document<'_>, name: &str) -> Result<Vec<String>, String> {
    Ok(rows(doc, name)?
        .iter()
        .filter_map(|node| node.text().map(str::to_string))
        .collect())
}
fn number_array(node: roxmltree::Node<'_, '_>, name: &str) -> Result<Vec<f32>, String> {
    text(node, name)
        .unwrap_or("")
        .split_whitespace()
        .map(|value| value.parse::<f32>().map_err(|error| error.to_string()))
        .collect()
}
fn escape(value: &str) -> String {
    super::super::xml_escape(value)
}
fn append_array(
    doc: &roxmltree::Document<'_>,
    xml: &str,
    name: &str,
    old_count: usize,
    count: usize,
    added: &str,
    edits: &mut Vec<(Range<usize>, String)>,
) -> Result<(), String> {
    let node = array(doc, name)?;
    let entries = rows(doc, name)?;
    if entries.len() != old_count {
        return Err(format!("mounted target {name} count mismatch"));
    }
    let body = entries
        .iter()
        .map(|row| &xml[row.range()])
        .collect::<String>();
    edits.push((
        node.range(),
        format!("<hkparam name=\"{name}\" numelements=\"{count}\">{body}{added}</hkparam>"),
    ));
    Ok(())
}
fn apply_edits(xml: &str, mut edits: Vec<(Range<usize>, String)>) -> Result<String, String> {
    edits.sort_by_key(|(range, _)| range.start);
    if edits.windows(2).any(|pair| pair[0].0.end > pair[1].0.start) {
        return Err("overlapping mounted graph edits".into());
    }
    let mut result = xml.to_string();
    for (range, value) in edits.into_iter().rev() {
        result.replace_range(range, &value);
    }
    Ok(result)
}
fn replace_identifier(source: &str, from: &str, to: &str) -> String {
    let mut output = String::new();
    let mut token = String::new();
    for character in source.chars().chain(['\0']) {
        if character.is_alphanumeric() || character == '_' {
            token.push(character);
        } else {
            output.push_str(if token == from { to } else { &token });
            token.clear();
            if character != '\0' {
                output.push(character);
            }
        }
    }
    output
}
fn validate_graft(xml: &str, prepared: &MountedCombatGraph) -> Result<(), String> {
    let doc = roxmltree::Document::parse(xml).map_err(|error| error.to_string())?;
    let objects = objects(&doc);
    let names = strings(&doc, "eventNames")?;
    let mut roots = 0;
    for wrapper in objects
        .values()
        .filter(|node| text(**node, "name") == Some("B21MountedCombat"))
    {
        let root = text(*wrapper, "generator")
            .and_then(|id| objects.get(id))
            .ok_or("mounted graph has no default state machine")?;
        if text(*root, "name") != Some("DefaultBehavior") {
            return Err("mounted lifecycle is outside its source state machine".into());
        }
        for id in text(*root, "states")
            .ok_or("mounted default state machine has no states")?
            .split_whitespace()
        {
            let state = *objects.get(id).ok_or("mounted default state is missing")?;
            let generator = text(state, "generator")
                .and_then(|id| objects.get(id))
                .ok_or("mounted default state has no generator")?;
            let expected = match text(state, "name") {
                Some("NonCombat_State") => "B21SkyrimRiderSync",
                Some("MountedCombat_Readied_State") => "B21MountedCombatLayers",
                Some("MC 1HM Equip") => "MC_1HM_EquipBlend",
                Some("MC 1HM Unequip") => "MC_1HM_UnequipBlend",
                _ => return Err("unrecognized mounted default state".into()),
            };
            if text(*generator, "name") != Some(expected) {
                return Err(format!("mounted state lost its {expected} composition"));
            }
        }
        roots += 1;
    }
    if roots != 3 {
        return Err(format!(
            "mounted graph has {roots} lifecycle roots, expected three"
        ));
    }
    for required in [
        "attackStart_MC_1HMRight",
        "attackStart_MC_1HMLeft",
        "bowAttackStart",
        "crossbowAttackStart",
        "weaponFire",
        "reloadComplete",
    ] {
        if !names.iter().any(|name| name == required) {
            return Err(format!("mounted graph lost {required}"));
        }
    }
    for request in &prepared.requests {
        let clips = objects
            .values()
            .copied()
            .filter(|node| {
                node.attribute("class") == Some("hkbClipGenerator")
                    && text(*node, "animationName").is_some_and(|path| {
                        path.replace('\\', "/")
                            .ends_with(&format!("/{}.hkx", request.name))
                    })
            })
            .collect::<Vec<_>>();
        if clips.len() != 3 {
            return Err(format!(
                "mounted graph needs three branches for {}, found {}",
                request.name,
                clips.len()
            ));
        }
        if let Some(expected) = prepared.timed_events.get(&request.name) {
            for clip in clips {
                let trigger_id =
                    text(clip, "triggers").ok_or("mounted native clip has no triggers")?;
                let trigger_array = objects
                    .get(trigger_id)
                    .ok_or("mounted native clip has missing trigger array")?;
                let actual = param(*trigger_array, "triggers")
                    .ok_or("mounted native trigger array has no rows")?
                    .children()
                    .filter(roxmltree::Node::is_element)
                    .collect::<Vec<_>>();
                for trigger in expected {
                    let mut count = 0;
                    for row in &actual {
                        let event = param(*row, "event")
                            .and_then(|node| node.children().find(roxmltree::Node::is_element))
                            .ok_or("mounted native trigger has no event")?;
                        let (name, payload) = resolved_event(event, &objects, &names)?;
                        let time = text(*row, "localTime")
                            .ok_or("mounted trigger has no time")?
                            .parse::<f32>()
                            .map_err(|error| error.to_string())?;
                        if name == event_name(&trigger.event)
                            && payload == trigger.payload.unwrap_or("null")
                            && (time - trigger.time).abs() < 0.00001
                        {
                            count += 1;
                        }
                    }
                    if count != 1 {
                        return Err(format!(
                            "{} requires one {} {:?} at {}, found {count}",
                            request.name, trigger.event, trigger.payload, trigger.time
                        ));
                    }
                }
            }
        }
    }
    for node in objects
        .values()
        .filter(|node| node.attribute("class") == Some("hkbVariableBindingSet"))
    {
        if node.descendants().any(|child| {
            child.attribute("name") == Some("bindingType")
                && child.text() == Some("BINDING_TYPE_CHARACTER_PROPERTY")
        }) {
            return Err("mounted graph retained source character property".into());
        }
    }
    for node in objects.values().filter(|node| {
        node.attribute("class") == Some("hkbTimerModifier")
            && matches!(
                text(**node, "name"),
                Some("SendEvent_BowRelease" | "SendEvent_BowReleaseFast")
            )
    }) {
        let event = param(*node, "alarmEvent")
            .and_then(|node| node.children().find(roxmltree::Node::is_element))
            .ok_or("mounted crossbow timer lacks alarm")?;
        let expected = text(*node, "name")
            .unwrap()
            .trim_start_matches("SendEvent_");
        if resolved_event(event, &objects, &names)? != (expected, "null") {
            return Err("mounted crossbow timer lost weapon graph release".into());
        }
    }
    for node in objects.values().filter(|node| {
        node.attribute("class") == Some("hkbStateMachineStateInfo")
            && matches!(
                text(**node, "name"),
                Some("CrossBow_Release" | "CrossBow_ReloadState")
            )
    }) {
        for phase in ["enterNotifyEvents", "exitNotifyEvents"] {
            let array = text(*node, phase)
                .and_then(|id| objects.get(id))
                .ok_or("mounted crossbow state lacks native lifecycle")?;
            let payload = if phase == "enterNotifyEvents" {
                "Enter"
            } else {
                "Exit"
            };
            let event = if text(*node, "name") == Some("CrossBow_ReloadState") {
                "reloadState"
            } else {
                "attackState"
            };
            let emitted = param(*array, "events")
                .ok_or("mounted crossbow lifecycle has no rows")?
                .children()
                .filter(roxmltree::Node::is_element)
                .map(|row| resolved_event(row, &objects, &names))
                .collect::<Result<Vec<_>, _>>()?;
            if !emitted.contains(&(event, payload))
                || (phase == "exitNotifyEvents" && !emitted.contains(&("reloadState", "Exit")))
            {
                return Err("mounted crossbow state lost firing/reload cleanup".into());
            }
        }
    }
    Ok(())
}

fn resolved_event<'a>(
    node: roxmltree::Node<'a, '_>,
    objects: &BTreeMap<&str, roxmltree::Node<'a, '_>>,
    names: &'a [String],
) -> Result<(&'a str, &'a str), String> {
    let index = text(node, "id")
        .ok_or("mounted event has no index")?
        .parse::<usize>()
        .map_err(|error| error.to_string())?;
    let name = names.get(index).ok_or("mounted event index out of range")?;
    let id = text(node, "payload").unwrap_or("null");
    let payload = if id == "null" {
        "null"
    } else {
        text(
            *objects.get(id).ok_or("mounted event payload missing")?,
            "data",
        )
        .ok_or("mounted native payload is not a string")?
    };
    Ok((name, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mounted_expression_remaps_identifiers_without_changing_other_variables() {
        assert_eq!(
            replace_identifier(
                "Speed + walkBackSpeedMult + (Speed > 0)",
                "Speed",
                "HorseSpeedSampled"
            ),
            "HorseSpeedSampled + walkBackSpeedMult + (HorseSpeedSampled > 0)"
        );
    }
    #[test]
    #[ignore = "requires Skyrim and FO4 extracted assets"]
    fn actual_mounted_combat_graph_preserves_source_attack_and_equip_lifecycles() {
        let root = std::env::var("MODKIT_REPO_ROOT").expect("MODKIT_REPO_ROOT");
        let root = Path::new(&root);
        let source = root.join("extracted/skyrimse/meshes");
        let target = root.join("extracted/fo4/meshes");
        let prepared = prepare(&source, &target).unwrap();
        assert!(prepared.requests.len() > 40);
        let mut requests = prepared.requests.clone();
        requests.extend(
            [
                (
                    "Mount",
                    "actors/sharedkillmoves/human&horse/paired_mount.hkx",
                    false,
                ),
                (
                    "Dismount",
                    "actors/sharedkillmoves/human&horse/paired_dismount.hkx",
                    false,
                ),
                (
                    "Idle",
                    "actors/character/animations/horse_rider/idle.hkx",
                    true,
                ),
            ]
            .map(|(name, path, looping)| RiderClipRequest {
                name: name.into(),
                source_path: path.into(),
                looping,
            }),
        );
        let staged = super::super::stage_rider_clips(&source, &target, &requests).unwrap();
        let donor =
            std::fs::read(target.join("actors/character/behaviors/furniturebehavior.hkx")).unwrap();
        let movement =
            super::super::graph::stage_graph(&donor, &staged.receipts, &["Idle".into()]).unwrap();
        let graph = graft(&movement, &prepared, &staged.receipts).unwrap();
        let xml = havok_native::api::havok_hkx_to_xml(&graph).unwrap();
        validate_graft(&xml, &prepared).unwrap();
        assert!(xml.contains("CrossBow_ReloadState"));
        assert!(xml.contains("MC_AttackState_Behavior"));
        assert!(xml.contains("B21MountedCombatLayers"));
        assert!(xml.contains("MountedCombat_Readied_Behavior"));
    }
}
