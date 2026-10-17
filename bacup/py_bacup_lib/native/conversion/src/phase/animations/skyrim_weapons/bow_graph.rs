use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::Path;

use havok_native::animation::parsers::{SkeletonRecord, parse_skeleton_xml};
use havok_native::hkx::descriptors::DescriptorRegistry;

use super::{SkyrimActorConsumer, StagedSkyrimActorClip, actor_clip_contract, stage_actor_clip};

pub(crate) struct StagedBowActorProfile {
    pub graph_path: String,
    pub graph_bytes: Vec<u8>,
    pub clips: Vec<StagedSkyrimActorClip>,
}

pub(crate) fn stage_profile(
    source_meshes: &Path,
    target_meshes: &Path,
    donor_graph: &[u8],
    first_person: bool,
) -> Result<StagedBowActorProfile, String> {
    let actor = if first_person {
        "Actors/Character/_1stPerson"
    } else {
        "Actors/Character"
    };
    let source = read_xml(&source_meshes.join(actor).join("Behaviors/1HM_Behavior.hkx"))?;
    let source_doc = roxmltree::Document::parse(&source).map_err(|e| e.to_string())?;
    let source_objects = objects(&source_doc);
    let root = named(&source_objects, "Bow_AttackStandingBehavior")?;
    let closure = closure(&source_objects, root)?;
    let mut paths = BTreeSet::from([
        "Animations/Bow_IdleHeld.hkx".to_string(),
        "Animations/Bow_Equip.hkx".to_string(),
        "Animations/Bow_Unequip.hkx".to_string(),
    ]);
    for id in &closure {
        let node = source_objects[id.as_str()];
        if node.attribute("class") == Some("hkbClipGenerator") {
            paths.insert(
                text(node, "animationName")
                    .ok_or("source bow clip has no path")?
                    .replace('\\', "/"),
            );
        }
    }
    let mut clips = Vec::new();
    for path in paths {
        let source_path = format!("{actor}/{path}");
        let contract = actor_clip_contract(&source_path)
            .ok_or_else(|| format!("unrecognized source bow clip {source_path}"))?;
        let bytes = read(&source_meshes.join(&contract.source_path))?;
        let source_skeleton = read(&source_meshes.join(&contract.source_skeleton_path))?;
        let target_skeleton = read(&target_meshes.join(&contract.target_skeleton_path))?;
        clips.push(stage_actor_clip(
            contract,
            SkyrimActorConsumer::SourceAnnotations,
            &bytes,
            &source_skeleton,
            &target_skeleton,
        )?);
    }
    let target_skeleton = parse_skeleton_xml(&read_xml(
        &target_meshes.join(&clips[0].contract.target_skeleton_path),
    )?)
    .map_err(|e| e.to_string())?;
    let source_skeleton = parse_skeleton_xml(&read_xml(
        &source_meshes.join(&clips[0].contract.source_skeleton_path),
    )?)
    .map_err(|e| e.to_string())?;
    let character = read_xml(&source_meshes.join(if first_person {
        "Actors/Character/_1stPerson/Characters/FirstPerson.hkx"
    } else {
        "Actors/Character/Characters/DefaultMale.hkx"
    }))?;
    let weights = upper_weights(&character, &source_skeleton, &target_skeleton)?;
    let donor = havok_native::api::havok_hkx_to_xml(donor_graph).map_err(|e| e.to_string())?;
    let bones = bone_map(&source_skeleton, &target_skeleton)?;
    let xml = graft(&donor, &source, &clips, &weights, &bones, first_person)?;
    let xml = super::equipment_graph::retime_equipment(
        &xml,
        &[clip_path(&clips, "Animations/Bow_Equip.hkx", first_person)?],
        &[clip_path(
            &clips,
            "Animations/Bow_Unequip.hkx",
            first_person,
        )?],
        true,
    )?;
    crate::source_rig::validate_fo4_havok_xml_signatures(&xml).map_err(|e| e.to_string())?;
    let graph_bytes = havok_native::api::havok_xml_to_hkx(&xml).map_err(|e| e.to_string())?;
    let reread = havok_native::api::havok_hkx_to_xml(&graph_bytes).map_err(|e| e.to_string())?;
    validate(&reread)?;
    Ok(StagedBowActorProfile {
        graph_path: format!("{actor}/Behaviors/B21_SkyrimBow.hkx"),
        graph_bytes,
        clips,
    })
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_xml(path: &Path) -> Result<String, String> {
    havok_native::api::havok_hkx_to_xml(&read(path)?)
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn graft(
    donor: &str,
    source: &str,
    clips: &[StagedSkyrimActorClip],
    upper_weights: &[f32],
    bones: &BTreeMap<usize, usize>,
    first_person: bool,
) -> Result<String, String> {
    let target = roxmltree::Document::parse(donor).map_err(|e| e.to_string())?;
    let source = roxmltree::Document::parse(source).map_err(|e| e.to_string())?;
    let target_objects = objects(&target);
    let source_objects = objects(&source);
    let attack_root = named(&source_objects, "Bow_AttackStandingBehavior")?;
    let modifier_root = named(&source_objects, "BowAttackModifierList")?;
    let mut closure = closure(&source_objects, attack_root)?;
    closure.extend(self::closure(&source_objects, modifier_root)?);
    let mut edits = Vec::new();
    let source_vars = strings(&source, "variableNames")?;
    let source_events = strings(&source, "eventNames")?;
    let variables = merge_variables(&source, &target, &closure, &source_objects, &mut edits)?;
    let events = merge_events(&source, &target, &mut edits)?;
    let mut next = target_objects
        .keys()
        .filter_map(|id| id.strip_prefix('#')?.parse::<usize>().ok())
        .max()
        .ok_or("bow donor has no object IDs")?
        + 1;
    let ids: BTreeMap<_, _> = closure
        .iter()
        .map(|id| (id.clone(), take_id(&mut next)))
        .collect();
    let mut appended = String::new();
    let mut registry = DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
    let context = Context {
        ids: &ids,
        variables: &variables,
        events: &events,
        source_vars: &source_vars,
        source_events: &source_events,
        bones,
    };
    for id in &closure {
        let node = source_objects[id.as_str()];
        let class = node
            .attribute("class")
            .ok_or("bow source object has no class")?;
        let descriptor = registry
            .get(class)
            .map_err(|e| e.to_string())?
            .cloned()
            .ok_or_else(|| format!("unsupported FO4 bow node {class}"))?;
        let members = registry.get_all_members(class).map_err(|e| e.to_string())?;
        let mut body = String::new();
        if class == "BSDirectAtModifier" {
            body.push_str(&direct_at(node, &context)?);
        } else {
            for child in node.children().filter(roxmltree::Node::is_element) {
                let name = child.attribute("name").unwrap_or("");
                if !members.iter().any(|member| member.name == name) {
                    return Err(format!(
                        "source bow member {class}.{name} has no target counterpart"
                    ));
                }
                if class == "hkbTwistModifier" && name == "enable" {
                    body.push_str(&field(name, if first_person { "false" } else { "true" }));
                } else if class == "hkbClipGenerator" && name == "animationName" {
                    let path = text(node, name)
                        .ok_or("bow clip path is empty")?
                        .replace('\\', "/");
                    body.push_str(&field(name, &clip_path(clips, &path, first_person)?));
                } else if class == "hkbClipGenerator" && name == "animationBindingIndex" {
                    body.push_str(&field(name, "-1"));
                } else {
                    body.push_str(&import_node(child, &context)?);
                }
            }
        }
        appended.push_str(&format!(
            "<hkobject name=\"{}\" class=\"{class}\" signature=\"{}\">{body}</hkobject>",
            ids[id], descriptor.signature
        ));
    }
    replace_equipment_clips(&target_objects, clips, first_person, &mut edits)?;
    let root = target_objects
        .values()
        .find(|node| node.attribute("class") == Some("hkbBehaviorGraph"))
        .and_then(|node| param(*node, "rootGenerator"))
        .ok_or("bow donor has no root generator")?;
    let donor_root = root.text().ok_or("bow donor root is empty")?;
    let idle = take_id(&mut next);
    let attack = take_id(&mut next);
    let idle_transitions = take_id(&mut next);
    let attack_transitions = take_id(&mut next);
    let machine = take_id(&mut next);
    let lower_mask = take_id(&mut next);
    let upper_mask = take_id(&mut next);
    let lower = take_id(&mut next);
    let upper = take_id(&mut next);
    let blend = take_id(&mut next);
    let active_binding = take_id(&mut next);
    let active = take_id(&mut next);
    let wrapper = take_id(&mut next);
    let attack_wrapper = take_id(&mut next);
    appended.push_str(&state(
        &idle,
        "B21BowReady",
        0,
        donor_root,
        &idle_transitions,
    ));
    appended.push_str(&state(
        &attack,
        "B21BowAttack",
        1,
        &attack_wrapper,
        &attack_transitions,
    ));
    appended.push_str(&transition_array(
        &idle_transitions,
        &[transition(events["bowAttackStart"], 1)],
    ));
    let stops = [
        "attackStop",
        "weaponSheathe",
        "unEquip",
        "staggerStart",
        "recoilStart",
        "forceIdle",
    ];
    appended.push_str(&transition_array(
        &attack_transitions,
        &stops
            .iter()
            .map(|event| transition(events[*event], 0))
            .collect::<Vec<_>>(),
    ));
    appended.push_str(&modifier_generator(
        &attack_wrapper,
        "B21SourceBowAttack",
        &ids[modifier_root],
        &ids[attack_root],
    ));
    appended.push_str(&state_machine(&machine, &idle, &attack));
    appended.push_str(&weights(&upper_mask, upper_weights));
    appended.push_str(&weights(
        &lower_mask,
        &upper_weights
            .iter()
            .map(|weight| 1.0 - weight)
            .collect::<Vec<_>>(),
    ));
    appended.push_str(&blend_child(&lower, donor_root, &lower_mask, 1));
    appended.push_str(&blend_child(&upper, &machine, &upper_mask, 0));
    appended.push_str(&format!("<hkobject name=\"{blend}\" class=\"hkbBlenderGenerator\" signature=\"0xce45c088\">{}{}{}{}{}{}{}{}{}{}<hkparam name=\"children\" numelements=\"2\">{lower} {upper}</hkparam></hkobject>", field("variableBindingSet", "null"), field("userData", "0"), field("name", "B21SourceBowLayers"), field("referencePoseWeightThreshold", "0"), field("blendParameter", "0"), field("minCyclicBlendParameter", "0"), field("maxCyclicBlendParameter", "1"), field("indexOfSyncMasterChild", "0"), field("flags", "0"), field("subtractLastChild", "false")));
    appended.push_str(&format!("<hkobject name=\"{active_binding}\" class=\"hkbVariableBindingSet\" signature=\"0xe942f339\"><hkparam name=\"bindings\" numelements=\"1\"><hkobject>{}{}{}{}</hkobject></hkparam>{}</hkobject>",field("memberPath","bIsActive0"),field("variableIndex",&variables["B21SourceBowActive"].to_string()),field("bitIndex","-1"),field("bindingType","BINDING_TYPE_VARIABLE"),field("indexOfBindingToEnable","-1")));
    appended.push_str(&format!("<hkobject name=\"{active}\" class=\"BSIsActiveModifier\" signature=\"0x5bf1f5cf\">{}{}{}{}{}{}</hkobject>",field("variableBindingSet",&active_binding),field("userData","0"),field("name","B21SourceBowActive"),field("enable","true"),field("bIsActive0","false"),field("bInvertActive0","false")));
    appended.push_str(&modifier_generator(
        &wrapper,
        "B21SourceBowProfile",
        &active,
        &blend,
    ));
    edits.push((root.range(), field("rootGenerator", &wrapper)));
    let end = donor
        .rfind("</hksection>")
        .ok_or("bow donor has no data section end")?;
    edits.push((end..end, appended));
    apply_edits(donor, edits)
}

struct Context<'a> {
    ids: &'a BTreeMap<String, String>,
    variables: &'a BTreeMap<String, usize>,
    events: &'a BTreeMap<String, usize>,
    source_vars: &'a [String],
    source_events: &'a [String],
    bones: &'a BTreeMap<usize, usize>,
}

fn import_node(node: roxmltree::Node<'_, '_>, context: &Context<'_>) -> Result<String, String> {
    if node.has_tag_name("hkobject")
        && text(node, "bindingType") == Some("BINDING_TYPE_CHARACTER_PROPERTY")
    {
        if text(node, "memberPath") != Some("enable") || text(node, "variableIndex") != Some("0") {
            return Err("unsupported source bow character property".into());
        }
        return Ok(String::new());
    }
    let mut output = format!("<{}", node.tag_name().name());
    for attr in node.attributes() {
        if attr.name() == "numelements" && node.attribute("name") == Some("bindings") {
            let count: usize = node
                .children()
                .filter(roxmltree::Node::is_element)
                .map(|row| {
                    if text(row, "bindingType") == Some("BINDING_TYPE_CHARACTER_PROPERTY") {
                        0
                    } else if matches!(
                        text(row, "memberPath"),
                        Some("limitHeadingDegrees" | "limitPitchDegrees")
                    ) {
                        2
                    } else {
                        1
                    }
                })
                .sum();
            output.push_str(&format!(" numelements=\"{count}\""));
        } else {
            output.push_str(&format!(" {}=\"{}\"", attr.name(), escape(attr.value())));
        }
    }
    output.push('>');
    if node.children().any(|child| child.is_element()) {
        for child in node.children().filter(roxmltree::Node::is_element) {
            output.push_str(&import_node(child, context)?);
        }
    } else {
        let name = node.attribute("name").unwrap_or("");
        let original = node.text().unwrap_or("").trim();
        let value = if original.starts_with('#') {
            original
                .split_whitespace()
                .map(|id| {
                    context
                        .ids
                        .get(id)
                        .cloned()
                        .ok_or_else(|| format!("unresolved bow graph object {id}"))
                })
                .collect::<Result<Vec<_>, _>>()?
                .join(" ")
        } else if name == "indexOfBindingToEnable"
            && node.parent().is_some_and(|parent| {
                parent
                    .descendants()
                    .any(|row| text(row, "bindingType") == Some("BINDING_TYPE_CHARACTER_PROPERTY"))
            })
        {
            "-1".into()
        } else if matches!(name, "sourceBoneIndex" | "startBoneIndex" | "endBoneIndex") {
            remap_bone(original, context.bones)?
        } else if matches!(
            name,
            "variableIndex" | "syncVariableIndex" | "assignmentVariableIndex"
        ) {
            remap(original, context.source_vars, context.variables)?
        } else if name == "id"
            || name.ends_with("EventId")
            || matches!(name, "eventId" | "assignmentEventIndex")
        {
            remap(original, context.source_events, context.events)?
        } else if name == "expression" {
            original
                .replace("bowZoomStart", "BowZoomStart")
                .replace("bowZoomStop", "BowZoomStop")
        } else if name == "memberPath" {
            match original {
                "offsetHeadingDegrees" => "initialHeadingOffset",
                "offsetPitchDegrees" => "initialPitchOffset",
                _ => original,
            }
            .into()
        } else {
            original.into()
        };
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

fn direct_at(node: roxmltree::Node<'_, '_>, context: &Context<'_>) -> Result<String, String> {
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
        "sourceBoneIndex",
        "startBoneIndex",
        "endBoneIndex",
    ] {
        result.push_str(&import_node(
            param(node, name).ok_or_else(|| format!("source bow aim lacks {name}"))?,
            context,
        )?);
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
        let value = text(node, source).ok_or_else(|| format!("source bow aim lacks {source}"))?;
        for target in targets {
            result.push_str(&field(target, value));
        }
    }
    result.push_str(&field("target", "AppSupplied"));
    result.push_str(&field("context", "BowAttack"));
    result.push_str(&field("offsetsCharacterRelative", "true"));
    result.push_str(&field("useCurrentSourceBonePoseEventId", "-1"));
    result.push_str(&field("snapToTargetEventId", "-1"));
    Ok(result)
}

fn remap_bone(value: &str, bones: &BTreeMap<usize, usize>) -> Result<String, String> {
    let source = value.parse::<i32>().map_err(|e| e.to_string())?;
    if source < 0 {
        return Ok(value.into());
    }
    bones
        .get(&(source as usize))
        .map(usize::to_string)
        .ok_or_else(|| format!("unbound bow modifier bone {source}"))
}

fn bone_map(
    source: &SkeletonRecord,
    target: &SkeletonRecord,
) -> Result<BTreeMap<usize, usize>, String> {
    let source_pose = super::target_reference_pose(source)?;
    let target_pose = super::target_reference_pose(target)?;
    let names = super::actor_bone_map(&source_pose, &target_pose, true)?;
    Ok(source
        .bone_names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            let mapped = names.get(name).unwrap_or(name);
            target
                .bone_names
                .iter()
                .position(|candidate| candidate.eq_ignore_ascii_case(mapped))
                .map(|target| (index, target))
        })
        .collect())
}

fn merge_variables(
    source: &roxmltree::Document<'_>,
    target: &roxmltree::Document<'_>,
    closure: &BTreeSet<String>,
    objects: &BTreeMap<&str, roxmltree::Node<'_, '_>>,
    edits: &mut Vec<(Range<usize>, String)>,
) -> Result<BTreeMap<String, usize>, String> {
    let source_names = strings(source, "variableNames")?;
    let mut names = strings(target, "variableNames")?;
    let original = names.len();
    let infos = rows(source, "variableInfos")?;
    let target_infos = rows(target, "variableInfos")?;
    let values = rows(source, "wordVariableValues")?;
    let mut indices = BTreeSet::new();
    for id in closure {
        for node in objects[id.as_str()]
            .descendants()
            .filter(roxmltree::Node::is_element)
        {
            if node.attribute("name") == Some("expression") {
                let tokens: BTreeSet<_> = node
                    .text()
                    .unwrap_or("")
                    .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .collect();
                indices.extend(
                    source_names.iter().enumerate().filter_map(|(index, name)| {
                        tokens.contains(name.as_str()).then_some(index)
                    }),
                );
            }
            if matches!(
                node.attribute("name"),
                Some("variableIndex" | "syncVariableIndex" | "assignmentVariableIndex")
            ) {
                if node.parent().is_some_and(|row| {
                    text(row, "bindingType") == Some("BINDING_TYPE_CHARACTER_PROPERTY")
                }) {
                    continue;
                }
                let index = node
                    .text()
                    .unwrap_or("-1")
                    .parse::<i32>()
                    .map_err(|e| e.to_string())?;
                if index >= 0 {
                    indices.insert(index as usize);
                }
            }
            if node.attribute("name") == Some("bindingType")
                && node.text() == Some("BINDING_TYPE_CHARACTER_PROPERTY")
            {
                let row = node.parent().ok_or("bow property binding has no row")?;
                if text(row, "memberPath") != Some("enable")
                    || text(row, "variableIndex") != Some("0")
                    || strings(source, "characterPropertyNames")?
                        .first()
                        .map(String::as_str)
                        != Some("IsThirdPerson")
                {
                    return Err("source bow requires an unsupported character property".into());
                }
            }
        }
    }
    let (mut new_names, mut new_infos, mut new_values) =
        (String::new(), String::new(), String::new());
    for index in indices {
        let name = source_names
            .get(index)
            .ok_or("bow variable index out of range")?;
        if let Some(existing) = names
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(name))
        {
            if text(
                *infos.get(index).ok_or("bow variable type missing")?,
                "type",
            ) != text(
                *target_infos
                    .get(existing)
                    .ok_or("target bow variable type missing")?,
                "type",
            ) {
                return Err(format!(
                    "source and target bow variable {name} have different types"
                ));
            }
            continue;
        }
        let info = *infos.get(index).ok_or("bow variable type missing")?;
        let value = *values
            .get(index)
            .ok_or("bow variable initial value missing")?;
        if !matches!(
            text(info, "type"),
            Some(
                "VARIABLE_TYPE_BOOL"
                    | "VARIABLE_TYPE_INT8"
                    | "VARIABLE_TYPE_INT16"
                    | "VARIABLE_TYPE_INT32"
                    | "VARIABLE_TYPE_REAL"
            )
        ) {
            return Err(format!("unsupported source bow variable {name}"));
        }
        names.push(name.clone());
        new_names.push_str(&format!("<hkcstring>{}</hkcstring>", escape(name)));
        new_infos.push_str(&source.input_text()[info.range()]);
        new_values.push_str(&source.input_text()[value.range()]);
    }
    names.push("B21SourceBowActive".into());
    new_names.push_str("<hkcstring>B21SourceBowActive</hkcstring>");
    new_infos.push_str("<hkobject><hkparam name=\"role\"><hkobject><hkparam name=\"role\">ROLE_DEFAULT</hkparam><hkparam name=\"flags\">FLAG_NONE</hkparam></hkobject></hkparam><hkparam name=\"type\">VARIABLE_TYPE_BOOL</hkparam></hkobject>");
    new_values.push_str("<hkobject><hkparam name=\"value\">0</hkparam></hkobject>");
    for (name, extra) in [
        ("variableNames", new_names),
        ("variableInfos", new_infos),
        ("wordVariableValues", new_values),
    ] {
        append_array(target, name, names.len() - original, &extra, edits)?;
    }
    let mut mapping: BTreeMap<_, _> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();
    for source in source_names {
        if let Some(i) = names
            .iter()
            .position(|name| name.eq_ignore_ascii_case(&source))
        {
            mapping.insert(source, i);
        }
    }
    Ok(mapping)
}

fn merge_events(
    source: &roxmltree::Document<'_>,
    target: &roxmltree::Document<'_>,
    edits: &mut Vec<(Range<usize>, String)>,
) -> Result<BTreeMap<String, usize>, String> {
    let mut names = strings(target, "eventNames")?;
    let original = names.len();
    let mut extra = String::new();
    let source_names = strings(source, "eventNames")?;
    let source_flags = rows(source, "eventInfos")?
        .into_iter()
        .map(event_flags)
        .collect::<Result<Vec<_>, _>>()?;
    let mut flags = rows(target, "eventInfos")?
        .into_iter()
        .map(event_flags)
        .collect::<Result<Vec<_>, _>>()?;
    if flags.len() != names.len() || source_flags.len() != source_names.len() {
        return Err("bow event flags disagree with names".into());
    }
    for (source_name, source_flag) in source_names
        .iter()
        .map(String::as_str)
        .zip(source_flags)
        .chain(
            [
                "bowReset",
                "unEquip",
                "staggerStart",
                "recoilStart",
                "forceIdle",
            ]
            .map(|name| (name, 0)),
        )
    {
        let name = event_name(source_name);
        if let Some(index) = names
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(name))
        {
            flags[index] |= source_flag;
        } else {
            names.push(name.into());
            extra.push_str(&format!("<hkcstring>{}</hkcstring>", escape(name)));
            flags.push(source_flag);
        }
    }
    append_array(target, "eventNames", names.len() - original, &extra, edits)?;
    edits.push((
        array(target, "eventInfos")?.range(),
        format!(
            "<hkparam name=\"eventInfos\" numelements=\"{}\">{}</hkparam>",
            flags.len(),
            flags
                .iter()
                .map(|flag| format!(
                    "<hkobject><hkparam name=\"flags\">{flag}</hkparam></hkobject>"
                ))
                .collect::<String>()
        ),
    ));
    let mut mapping: BTreeMap<_, _> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();
    for name in source_names {
        if let Some(i) = names
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(event_name(&name)))
        {
            mapping.insert(name, i);
        }
    }
    for name in [
        "bowAttackStart",
        "attackStop",
        "weaponSheathe",
        "unEquip",
        "staggerStart",
        "recoilStart",
        "forceIdle",
        "bowReset",
    ] {
        let index = names
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("bow event {name} missing"))?;
        mapping.insert(name.into(), index);
    }
    Ok(mapping)
}

fn event_name(name: &str) -> &str {
    match name {
        "bowDrawn" => "BowDrawn",
        "bowZoomStart" => "BowZoomStart",
        "bowZoomStop" => "BowZoomStop",
        _ => name,
    }
}

fn event_flags(node: roxmltree::Node<'_, '_>) -> Result<u32, String> {
    let value = text(node, "flags").ok_or("bow event flags missing")?;
    if let Ok(number) = value.parse::<u32>() {
        return Ok(number);
    }
    value
        .split('|')
        .map(str::trim)
        .try_fold(0, |bits, flag| match flag {
            "FLAG_SILENT" => Ok(bits | 1),
            "FLAG_SYNC_POINT" => Ok(bits | 2),
            _ => Err(format!("unsupported bow event flag {flag}")),
        })
}

fn replace_equipment_clips(
    objects: &BTreeMap<&str, roxmltree::Node<'_, '_>>,
    clips: &[StagedSkyrimActorClip],
    first: bool,
    edits: &mut Vec<(Range<usize>, String)>,
) -> Result<(), String> {
    let mut counts = [0, 0, 0];
    for node in objects
        .values()
        .filter(|n| n.attribute("class") == Some("hkbClipGenerator"))
    {
        let name = text(*node, "animationName")
            .unwrap_or("")
            .replace('\\', "/")
            .to_ascii_lowercase();
        let stem = name
            .rsplit('/')
            .next()
            .unwrap_or("")
            .split('.')
            .next()
            .unwrap_or("");
        let selected = if stem.starts_with("wpnidleready")
            || stem == "wpnidlesighted"
            || stem == "sneakwpnidleready"
        {
            Some((0, "Animations/Bow_IdleHeld.hkx"))
        } else if matches!(stem, "wpnequip" | "wpnequipfast") {
            Some((1, "Animations/Bow_Equip.hkx"))
        } else if stem == "wpnunequip" {
            Some((2, "Animations/Bow_Unequip.hkx"))
        } else {
            None
        };
        let Some((kind, path)) = selected else {
            continue;
        };
        counts[kind] += 1;
        let p = param(*node, "animationName").ok_or("donor bow slot has no animation path")?;
        edits.push((
            p.range(),
            field("animationName", &clip_path(clips, path, first)?),
        ));
        for (name, value) in [
            ("animationBindingIndex", "-1"),
            ("cropStartAmountLocalTime", "0"),
            ("cropEndAmountLocalTime", "0"),
            ("enforcedDuration", "0"),
        ] {
            if let Some(p) = param(*node, name) {
                edits.push((p.range(), field(name, value)));
            }
        }
    }
    if counts.contains(&0) {
        return Err(format!(
            "bow donor lacks ready/equip/unequip slots: {counts:?}"
        ));
    }
    Ok(())
}

fn clip_path(clips: &[StagedSkyrimActorClip], source: &str, first: bool) -> Result<String, String> {
    let root = if first {
        "Actors/Character/_1stPerson/"
    } else {
        "Actors/Character/"
    };
    let source = format!("{root}{source}");
    let clip = clips
        .iter()
        .find(|clip| clip.contract.source_path.eq_ignore_ascii_case(&source))
        .ok_or_else(|| format!("source bow clip not staged: {source}"))?;
    Ok(clip
        .contract
        .output_path
        .strip_prefix(root)
        .ok_or("bow output clip escapes actor root")?
        .replace('/', "\\"))
}

fn upper_weights(
    character: &str,
    source: &SkeletonRecord,
    target: &SkeletonRecord,
) -> Result<Vec<f32>, String> {
    let doc = roxmltree::Document::parse(character).map_err(|e| e.to_string())?;
    let names = strings(&doc, "characterPropertyNames")?;
    let index = names
        .iter()
        .position(|name| name == "UpperBody")
        .ok_or("source bow character has no UpperBody mask")?;
    let values = rows(&doc, "wordVariableValues")?;
    let variant = text(
        *values
            .get(index)
            .ok_or("source bow property value missing")?,
        "value",
    )
    .ok_or("source bow property is empty")?
    .parse::<usize>()
    .map_err(|e| e.to_string())?;
    let refs = array(&doc, "variantVariableValues")?
        .text()
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>();
    let id = *refs.get(variant).ok_or("source bow mask variant missing")?;
    let obs = objects(&doc);
    let weights = param(
        *obs.get(id).ok_or("source bow mask object missing")?,
        "boneWeights",
    )
    .ok_or("source bow mask values missing")?
    .text()
    .unwrap_or("")
    .split_whitespace()
    .map(str::parse::<f32>)
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| e.to_string())?;
    if weights.len() > source.bone_names.len()
        || weights
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err("source bow upper-body weights are invalid".into());
    }
    let source_pose = super::target_reference_pose(source)?;
    let target_pose = super::target_reference_pose(target)?;
    let mapping = super::actor_bone_map(&source_pose, &target_pose, true)?;
    let inverse: BTreeMap<_, _> = mapping
        .into_iter()
        .filter_map(|(from, to)| {
            Some((
                target
                    .bone_names
                    .iter()
                    .position(|n| n.eq_ignore_ascii_case(&to))?,
                source
                    .bone_names
                    .iter()
                    .position(|n| n.eq_ignore_ascii_case(&from))?,
            ))
        })
        .collect();
    let mut result = vec![0.0; target.bone_names.len()];
    for (index, weight) in result.iter_mut().enumerate() {
        let mut ancestor = index as i32;
        while ancestor >= 0 {
            if let Some(source) = inverse.get(&(ancestor as usize)) {
                *weight = weights.get(*source).copied().unwrap_or(0.0);
                break;
            }
            ancestor = i32::from(target.parent_indices[ancestor as usize]);
        }
    }
    Ok(result)
}

fn validate(xml: &str) -> Result<(), String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let obs = objects(&doc);
    for name in [
        "B21SourceBowProfile",
        "B21SourceBowLayers",
        "Bow_AttackStandingBehavior",
        "BowDrawBehavior",
        "Bow_DrawingBehavior",
        "Bow_ReleaseType_Behavior",
    ] {
        named(&obs, name)?;
    }
    let variables = strings(&doc, "variableNames")?;
    if !variables.iter().any(|n| n == "B21SourceBowActive") {
        return Err("source bow profile has no active producer".into());
    }
    for node in obs.values() {
        for leaf in node
            .descendants()
            .filter(|n| n.is_element() && !n.children().any(|c| c.is_element()))
        {
            for id in leaf
                .text()
                .unwrap_or("")
                .split_whitespace()
                .filter(|t| t.starts_with('#'))
            {
                if !obs.contains_key(id) {
                    return Err(format!("source bow graph has unresolved {id}"));
                }
            }
        }
    }
    Ok(())
}

fn closure(
    objects: &BTreeMap<&str, roxmltree::Node<'_, '_>>,
    root: &str,
) -> Result<BTreeSet<String>, String> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root.to_string()];
    while let Some(id) = pending.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let node = objects
            .get(id.as_str())
            .ok_or_else(|| format!("missing source bow object {id}"))?;
        for leaf in node
            .descendants()
            .filter(|n| n.is_element() && !n.children().any(|c| c.is_element()))
        {
            for token in leaf
                .text()
                .unwrap_or("")
                .split_whitespace()
                .filter(|v| v.starts_with('#'))
            {
                pending.push(token.into());
            }
        }
    }
    Ok(seen)
}
fn remap(
    value: &str,
    names: &[String],
    mapped: &BTreeMap<String, usize>,
) -> Result<String, String> {
    let index = value.parse::<i32>().map_err(|e| e.to_string())?;
    if index < 0 {
        return Ok(value.into());
    }
    let name = names
        .get(index as usize)
        .ok_or("bow graph index out of range")?;
    mapped
        .get(name)
        .map(usize::to_string)
        .ok_or_else(|| format!("bow graph index {name} was not mapped"))
}
fn objects<'a, 'i>(doc: &'a roxmltree::Document<'i>) -> BTreeMap<&'a str, roxmltree::Node<'a, 'i>> {
    doc.descendants()
        .filter(|n| n.has_tag_name("hkobject") && n.attribute("class").is_some())
        .filter_map(|n| n.attribute("name").map(|id| (id, n)))
        .collect()
}
fn named<'a>(
    objects: &BTreeMap<&'a str, roxmltree::Node<'_, '_>>,
    name: &str,
) -> Result<&'a str, String> {
    objects
        .iter()
        .find(|(_, n)| text(**n, "name") == Some(name))
        .map(|(id, _)| *id)
        .ok_or_else(|| format!("bow graph lacks {name}"))
}
fn param<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    node.children().find(|c| c.attribute("name") == Some(name))
}
fn text<'a>(node: roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    param(node, name).and_then(|p| p.text()).map(str::trim)
}
fn array<'a, 'i>(
    doc: &'a roxmltree::Document<'i>,
    name: &str,
) -> Result<roxmltree::Node<'a, 'i>, String> {
    doc.descendants()
        .find(|n| n.attribute("name") == Some(name))
        .ok_or_else(|| format!("bow graph has no {name}"))
}
fn rows<'a, 'i>(
    doc: &'a roxmltree::Document<'i>,
    name: &str,
) -> Result<Vec<roxmltree::Node<'a, 'i>>, String> {
    Ok(array(doc, name)?
        .children()
        .filter(roxmltree::Node::is_element)
        .collect())
}
fn strings(doc: &roxmltree::Document<'_>, name: &str) -> Result<Vec<String>, String> {
    Ok(rows(doc, name)?
        .iter()
        .map(|n| n.text().unwrap_or("").into())
        .collect())
}
fn append_array(
    doc: &roxmltree::Document<'_>,
    name: &str,
    count: usize,
    extra: &str,
    edits: &mut Vec<(Range<usize>, String)>,
) -> Result<(), String> {
    if count == 0 {
        return Ok(());
    }
    let node = array(doc, name)?;
    let original = node
        .attribute("numelements")
        .ok_or("bow array count missing")?
        .parse::<usize>()
        .map_err(|e| e.to_string())?;
    let old = &doc.input_text()[node.range()];
    let inside = if node.children().any(|c| c.is_element()) {
        let a = old.find('>').unwrap() + 1;
        let b = old.rfind("</hkparam>").ok_or("bow array is not closed")?;
        &old[a..b]
    } else {
        node.text().unwrap_or("")
    };
    edits.push((
        node.range(),
        format!(
            "<hkparam name=\"{name}\" numelements=\"{}\">{inside}{extra}</hkparam>",
            original + count
        ),
    ));
    Ok(())
}
fn apply_edits(xml: &str, mut edits: Vec<(Range<usize>, String)>) -> Result<String, String> {
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut previous = xml.len();
    let mut output = xml.to_string();
    for (range, replacement) in edits {
        if range.end > previous {
            return Err("overlapping source bow graph edits".into());
        }
        previous = range.start;
        output.replace_range(range, &replacement);
    }
    Ok(output)
}
fn take_id(next: &mut usize) -> String {
    let id = format!("#{:04}", *next);
    *next += 1;
    id
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn field(name: &str, value: &str) -> String {
    format!("<hkparam name=\"{name}\">{}</hkparam>", escape(value))
}
fn modifier_generator(id: &str, name: &str, modifier: &str, generator: &str) -> String {
    format!(
        "<hkobject name=\"{id}\" class=\"hkbModifierGenerator\" signature=\"0xc499fc9e\">{}{}{}{}{}</hkobject>",
        field("variableBindingSet", "null"),
        field("userData", "0"),
        field("name", name),
        field("modifier", modifier),
        field("generator", generator)
    )
}
fn state(id: &str, name: &str, index: i32, generator: &str, transitions: &str) -> String {
    format!(
        "<hkobject name=\"{id}\" class=\"hkbStateMachineStateInfo\" signature=\"0x39d76713\">{}<hkparam name=\"listeners\" numelements=\"0\"></hkparam>{}{}{}{}{}{}{}{}</hkobject>",
        field("variableBindingSet", "null"),
        field("enterNotifyEvents", "null"),
        field("exitNotifyEvents", "null"),
        field("transitions", transitions),
        field("generator", generator),
        field("name", name),
        field("stateId", &index.to_string()),
        field("probability", "1"),
        field("enable", "true")
    )
}
fn state_machine(id: &str, idle: &str, attack: &str) -> String {
    format!(
        "<hkobject name=\"{id}\" class=\"hkbStateMachine\" signature=\"0xa5896bcf\">{}{}{}<hkparam name=\"eventToSendWhenStateOrTransitionChanges\"><hkobject>{}{}</hkobject></hkparam>{}{}{}{}{}{}{}{}{}{}{}<hkparam name=\"states\" numelements=\"2\">{idle} {attack}</hkparam>{}</hkobject>",
        field("variableBindingSet", "null"),
        field("userData", "0"),
        field("name", "B21SourceBowAction"),
        field("id", "-1"),
        field("payload", "null"),
        field("startStateIdSelector", "null"),
        field("startStateId", "0"),
        field("returnToPreviousStateEventId", "-1"),
        field("randomTransitionEventId", "-1"),
        field("transitionToNextHigherStateEventId", "-1"),
        field("transitionToNextLowerStateEventId", "-1"),
        field("syncVariableIndex", "-1"),
        field("wrapAroundStateId", "false"),
        field("maxSimultaneousTransitions", "32"),
        field("startStateMode", "START_STATE_MODE_DEFAULT"),
        field("selfTransitionMode", "SELF_TRANSITION_MODE_NO_TRANSITION"),
        field("wildcardTransitions", "null")
    )
}
fn transition(event: usize, state: i32) -> String {
    let interval = format!(
        "<hkobject>{}{}{}{}</hkobject>",
        field("enterEventId", "-1"),
        field("exitEventId", "-1"),
        field("enterTime", "0"),
        field("exitTime", "0")
    );
    format!(
        "<hkobject><hkparam name=\"triggerInterval\">{interval}</hkparam><hkparam name=\"initiateInterval\">{interval}</hkparam>{}{}{}{}{}{}{}{}</hkobject>",
        field("transition", "null"),
        field("condition", "null"),
        field("eventId", &event.to_string()),
        field("toStateId", &state.to_string()),
        field("fromNestedStateId", "-1"),
        field("toNestedStateId", "-1"),
        field("priority", "0"),
        field("flags", "FLAG_DISABLE_CONDITION")
    )
}
fn transition_array(id: &str, transitions: &[String]) -> String {
    format!(
        "<hkobject name=\"{id}\" class=\"hkbStateMachineTransitionInfoArray\" signature=\"0x704a19af\"><hkparam name=\"transitions\" numelements=\"{}\">{}</hkparam></hkobject>",
        transitions.len(),
        transitions.join("")
    )
}
fn weights(id: &str, weights: &[f32]) -> String {
    format!(
        "<hkobject name=\"{id}\" class=\"hkbBoneWeightArray\" signature=\"0x2c9b5751\">{}<hkparam name=\"boneWeights\" numelements=\"{}\">{}</hkparam></hkobject>",
        field("variableBindingSet", "null"),
        weights.len(),
        weights
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    )
}
fn blend_child(id: &str, generator: &str, weights: &str, world: i32) -> String {
    format!(
        "<hkobject name=\"{id}\" class=\"hkbBlenderGeneratorChild\" signature=\"0xb35bbfd3\">{}{}{}{}{}</hkobject>",
        field("variableBindingSet", "null"),
        field("generator", generator),
        field("boneWeights", weights),
        field("weight", "1"),
        field("worldFromModelWeight", &world.to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires Skyrim and FO4 extracted assets"]
    fn actual_onfoot_bow_profiles_preserve_source_charge_aim_and_equipment() {
        let root = std::env::var("MODKIT_REPO_ROOT").expect("MODKIT_REPO_ROOT");
        let root = Path::new(&root);
        let source = root.join("extracted/skyrimse/meshes");
        let target = root.join("extracted/fo4/meshes");
        for first in [false, true] {
            let donor = read(&target.join(if first {
                "Actors/Character/_1stPerson/Behaviors/GunBehavior.hkx"
            } else {
                "Actors/Character/Behaviors/WeaponBehavior.hkx"
            }))
            .unwrap();
            let profile = stage_profile(&source, &target, &donor, first).unwrap();
            assert_eq!(profile.clips.len(), 9);
            let xml = havok_native::api::havok_hkx_to_xml(&profile.graph_bytes).unwrap();
            let doc = roxmltree::Document::parse(&xml).unwrap();
            let objects = objects(&doc);
            for name in [
                "BowAttackModifierList",
                "BowAttackSpineTwistModifier",
                "BSDirectAtModifier_Bow",
                "bowZoomStart_EEM",
                "bowReset_OnDeactivateMod",
                "arrowDetach_OnDeactivateMod",
            ] {
                named(&objects, name).unwrap();
            }
            let aim = objects[named(&objects, "BSDirectAtModifier_Bow").unwrap()];
            assert_eq!(text(aim, "target"), Some("AppSupplied"));
            assert_eq!(text(aim, "context"), Some("BowAttack"));
            let speeds: BTreeSet<_> = objects
                .values()
                .filter(|node| {
                    node.attribute("class") == Some("hkbClipGenerator")
                        && text(**node, "animationName")
                            .is_some_and(|name| name.ends_with("bow_drawlight.hkx"))
                })
                .map(|node| {
                    (text(*node, "playbackSpeed")
                        .unwrap()
                        .parse::<f32>()
                        .unwrap()
                        * 100.0)
                        .round() as i32
                })
                .collect();
            assert!(speeds.contains(&100) && speeds.contains(&200));
            let names = strings(&doc, "eventNames").unwrap();
            for event in [
                "arrowRelease",
                "BowDrawn",
                "BowZoomStart",
                "BowZoomStop",
                "weaponDraw",
                "weaponSheathe",
            ] {
                assert!(names.iter().any(|name| name == event), "missing {event}");
            }
            assert!(xml.contains("bowZoomAmt = IsAttacking"));
        }
    }
}
