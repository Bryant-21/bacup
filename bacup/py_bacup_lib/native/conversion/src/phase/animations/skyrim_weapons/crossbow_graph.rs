use std::ops::Range;

use super::{SkyrimActorConsumer, SkyrimWeaponAction, StagedSkyrimActorClip};

pub(crate) struct StagedCrossbowActorProfile {
    pub graph_path: String,
    pub graph_bytes: Vec<u8>,
    pub clips: Vec<StagedSkyrimActorClip>,
    pub replaced_fire_clips: usize,
    pub replaced_reload_clips: usize,
}

pub(crate) fn stage_crossbow_actor_profile(
    donor_graph: &[u8],
    fire: StagedSkyrimActorClip,
    reload: StagedSkyrimActorClip,
    support: Vec<StagedSkyrimActorClip>,
) -> Result<StagedCrossbowActorProfile, String> {
    if fire.consumer != SkyrimActorConsumer::Fo4CrossbowGun
        || reload.consumer != SkyrimActorConsumer::Fo4CrossbowGun
        || fire.contract.action != SkyrimWeaponAction::CrossbowRelease
        || reload.contract.action != SkyrimWeaponAction::CrossbowReload
        || fire.contract.first_person != reload.contract.first_person
        || fire.contract.target_skeleton_path != reload.contract.target_skeleton_path
        || fire.weapon_fire_time.is_none()
        || reload.weapon_fire_time.is_some()
    {
        return Err("crossbow graph requires matching FO4 gun fire and reload actions".into());
    }
    if support.iter().any(|clip| {
        clip.contract.first_person != fire.contract.first_person
            || clip.contract.target_skeleton_path != fire.contract.target_skeleton_path
            || clip.consumer != SkyrimActorConsumer::SourceAnnotations
            || !matches!(
                clip.contract.action,
                SkyrimWeaponAction::CrossbowIdleHeld
                    | SkyrimWeaponAction::CrossbowIdleDrawn
                    | SkyrimWeaponAction::CrossbowEquip
                    | SkyrimWeaponAction::CrossbowUnequip
            )
    }) {
        return Err("crossbow support clips disagree with the actor profile".into());
    }
    let actor_root = if fire.contract.first_person {
        "Actors/Character/_1stPerson/"
    } else {
        "Actors/Character/"
    };
    let fire_path = fire
        .contract
        .output_path
        .strip_prefix(actor_root)
        .ok_or("crossbow fire clip is outside its actor project")?;
    let reload_path = reload
        .contract
        .output_path
        .strip_prefix(actor_root)
        .ok_or("crossbow reload clip is outside its actor project")?;
    let xml =
        havok_native::api::havok_hkx_to_xml(donor_graph).map_err(|error| error.to_string())?;
    let xml = declare_profile(&xml)?;
    let (xml, replaced_fire_clips, replaced_reload_clips) =
        connect_actions(&xml, fire_path, reload_path)?;
    let xml = connect_support(&xml, actor_root, &support)?;
    let paths = |action| {
        support
            .iter()
            .filter(|clip| clip.contract.action == action)
            .map(|clip| {
                clip.contract
                    .output_path
                    .strip_prefix(actor_root)
                    .unwrap_or("")
                    .to_string()
            })
            .collect::<Vec<_>>()
    };
    let xml = super::equipment_graph::retime_equipment(
        &xml,
        &paths(SkyrimWeaponAction::CrossbowEquip),
        &paths(SkyrimWeaponAction::CrossbowUnequip),
        true,
    )?;
    crate::source_rig::validate_fo4_havok_xml_signatures(&xml)
        .map_err(|error| error.to_string())?;
    let graph_bytes =
        havok_native::api::havok_xml_to_hkx(&xml).map_err(|error| error.to_string())?;
    Ok(StagedCrossbowActorProfile {
        graph_path: format!("{actor_root}Behaviors/B21_SkyrimCrossbow.hkx"),
        graph_bytes,
        clips: [fire, reload].into_iter().chain(support).collect(),
        replaced_fire_clips,
        replaced_reload_clips,
    })
}

fn connect_support(
    xml: &str,
    actor_root: &str,
    support: &[StagedSkyrimActorClip],
) -> Result<String, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let mut edits = Vec::new();
    for clip in doc
        .descendants()
        .filter(|node| node.attribute("class") == Some("hkbClipGenerator"))
    {
        let animation =
            parameter(clip, "animationName").ok_or("crossbow donor clip has no animation")?;
        let path = animation
            .text()
            .unwrap_or("")
            .replace('\\', "/")
            .to_ascii_lowercase();
        let stem = path
            .rsplit('/')
            .next()
            .unwrap_or("")
            .split('.')
            .next()
            .unwrap_or("");
        let action = match stem {
            "wpnidleready" | "wpnidlegundown" => SkyrimWeaponAction::CrossbowIdleHeld,
            "wpnidlesighted" | "sneakwpnidleready" => SkyrimWeaponAction::CrossbowIdleDrawn,
            "wpnequip" | "wpnequipfast" => SkyrimWeaponAction::CrossbowEquip,
            "wpnunequip" => SkyrimWeaponAction::CrossbowUnequip,
            _ => continue,
        };
        let Some(source) = support.iter().find(|source| {
            source.contract.action == action
                && source.contract.source_path.contains("sneakcrossbow")
                    == stem.starts_with("sneak")
        }) else {
            continue;
        };
        let path = source
            .contract
            .output_path
            .strip_prefix(actor_root)
            .ok_or("crossbow support clip is outside actor project")?;
        edits.push((
            animation.range(),
            format!(
                "<hkparam name=\"animationName\">{}</hkparam>",
                escape(&path.replace('/', "\\"))
            ),
        ));
        if let Some(binding) = parameter(clip, "animationBindingIndex") {
            edits.push((
                binding.range(),
                "<hkparam name=\"animationBindingIndex\">-1</hkparam>".into(),
            ));
        }
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut result = xml.to_string();
    for (range, value) in edits {
        result.replace_range(range, &value);
    }
    Ok(result)
}

pub(super) fn stage_source_unequip(
    source_root: &std::path::Path,
    target_root: &std::path::Path,
    first_person: bool,
) -> Result<StagedSkyrimActorClip, String> {
    let actor = if first_person {
        "Actors/Character/_1stPerson"
    } else {
        "Actors/Character"
    };
    let graph_path = source_root.join(actor).join("Behaviors/0_Master.hkx");
    let graph = super::unpack(
        &std::fs::read(&graph_path).map_err(|e| format!("{}: {e}", graph_path.display()))?,
    )?;
    let doc = roxmltree::Document::parse(&graph).map_err(|e| e.to_string())?;
    let node = doc
        .descendants()
        .find(|node| {
            node.attribute("class") == Some("hkbClipGenerator")
                && parameter(*node, "name").and_then(|node| node.text()) == Some("CrossBow_Unequip")
        })
        .ok_or("source crossbow unequip generator is absent")?;
    let scalar = |name: &str| {
        parameter(node, name)
            .and_then(|node| node.text())
            .ok_or_else(|| format!("source crossbow unequip lacks {name}"))?
            .parse::<f32>()
            .map_err(|e| e.to_string())
    };
    if scalar("playbackSpeed")? != -1.0
        || [
            "cropStartAmountLocalTime",
            "cropEndAmountLocalTime",
            "enforcedDuration",
        ]
        .into_iter()
        .any(|name| scalar(name) != Ok(0.0))
    {
        return Err("source crossbow unequip has an unsupported playback contract".into());
    }
    let path = parameter(node, "animationName")
        .and_then(|node| node.text())
        .ok_or("source crossbow unequip path is absent")?
        .replace('\\', "/");
    let mut contract = super::actor_clip_contract(&format!("{actor}/{path}"))
        .ok_or("unrecognized source crossbow unequip asset")?;
    if contract.action != SkyrimWeaponAction::CrossbowEquip {
        return Err("source crossbow unequip is not the verified reversed equip".into());
    }
    contract.action = SkyrimWeaponAction::CrossbowUnequip;
    contract.output_path =
        format!("{actor}/Animations/B21_SkyrimWeapons/dlc01/crossbow_unequip.hkx");
    let read = |path: &std::path::Path| {
        std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
    };
    let source_xml = super::unpack(&read(&source_root.join(&contract.source_path))?)?;
    let source_skeleton = havok_native::animation::parsers::parse_skeleton_xml(&super::unpack(
        &read(&source_root.join(&contract.source_skeleton_path))?,
    )?)
    .map_err(|e| e.to_string())?;
    let target_skeleton = havok_native::animation::parsers::parse_skeleton_xml(&super::unpack(
        &read(&target_root.join(&contract.target_skeleton_path))?,
    )?)
    .map_err(|e| e.to_string())?;
    let mut clip = super::decode_actor_clip(&source_xml, &source_skeleton)?;
    for channel in &mut clip.channels {
        for keys in [
            &mut channel.translations,
            &mut channel.rotations,
            &mut channel.scales,
        ] {
            for key in keys.iter_mut() {
                key.time = clip.duration - key.time;
            }
            keys.reverse();
        }
    }
    let mut floats = super::decode_float_tracks(&source_xml, &source_skeleton)?;
    for track in &mut floats {
        track.values.reverse();
    }
    let names: Vec<_> = doc
        .descendants()
        .find(|node| node.attribute("name") == Some("eventNames"))
        .ok_or("source master event names are absent")?
        .children()
        .filter_map(|node| node.is_element().then(|| node.text()).flatten())
        .collect();
    let trigger_id = parameter(node, "triggers")
        .and_then(|node| node.text())
        .ok_or("source crossbow unequip triggers are absent")?;
    let triggers = doc
        .descendants()
        .find(|node| node.attribute("name") == Some(trigger_id))
        .and_then(|node| parameter(node, "triggers"))
        .ok_or("source crossbow unequip trigger array is absent")?;
    clip.events.clear();
    for row in triggers.children().filter(roxmltree::Node::is_element) {
        let local = parameter(row, "localTime")
            .and_then(|node| node.text())
            .ok_or("source unequip trigger has no time")?
            .parse::<f64>()
            .map_err(|e| e.to_string())?;
        let relative = matches!(
            parameter(row, "relativeToEndOfClip").and_then(|node| node.text()),
            Some("1" | "true")
        );
        let time = if relative {
            -local
        } else {
            clip.duration - local
        };
        if time < -1e-5 || time > clip.duration + 1e-5 {
            return Err("source unequip trigger falls outside the reversed clip".into());
        }
        let id = parameter(row, "event")
            .and_then(|node| node.children().find(roxmltree::Node::is_element))
            .and_then(|node| parameter(node, "id"))
            .and_then(|node| node.text())
            .ok_or("source unequip trigger has no event")?
            .parse::<usize>()
            .map_err(|e| e.to_string())?;
        let text = names
            .get(id)
            .ok_or("source unequip event is out of range")?
            .to_string();
        clip.events.push(super::AnimationEvent {
            time: time.clamp(0.0, clip.duration),
            source_time: local,
            text,
        });
    }
    clip.events.sort_by(|a, b| a.time.total_cmp(&b.time));
    if clip
        .events
        .iter()
        .filter(|event| event.text == "weaponSheathe")
        .count()
        != 1
    {
        return Err("source unequip has no unique weaponSheathe marker".into());
    }
    super::stage_decoded_actor_clip(
        contract,
        SkyrimActorConsumer::SourceAnnotations,
        clip,
        &source_skeleton,
        &target_skeleton,
        floats,
    )
}

fn connect_actions(
    xml: &str,
    fire_path: &str,
    reload_path: &str,
) -> Result<(String, usize, usize), String> {
    let doc = roxmltree::Document::parse(xml).map_err(|error| error.to_string())?;
    let objects: Vec<_> = doc
        .descendants()
        .filter(|node| node.has_tag_name("hkobject") && node.attribute("class").is_some())
        .collect();
    let names = objects
        .iter()
        .find(|node| node.attribute("class") == Some("hkbBehaviorGraphStringData"))
        .and_then(|node| parameter(*node, "eventNames"))
        .ok_or("crossbow donor has no graph event names")?;
    let events: Vec<_> = names
        .children()
        .filter_map(|node| node.is_element().then(|| node.text()).flatten())
        .collect();
    let event = |name: &str| {
        events
            .iter()
            .position(|candidate| *candidate == name)
            .ok_or_else(|| format!("crossbow donor has no {name} event"))
    };
    let reload_start = event("reloadStart")?;
    let reload_end = event("reloadEnd")?;
    let initiate_start = event("initiateStart")?;
    let reload_state = event("reloadState")?;
    event("weaponFire")?;
    event("reloadComplete")?;
    let has_reload_handler = objects.iter().any(|node| {
        node.attribute("class") == Some("hkbStateMachineStateInfo")
            && parameter(*node, "enterNotifyEvents")
                .and_then(|value| value.text())
                .and_then(|id| {
                    objects
                        .iter()
                        .find(|candidate| candidate.attribute("name") == Some(id))
                })
                .is_some_and(|array| {
                    array.descendants().any(|value| {
                        value.attribute("name") == Some("id")
                            && value.text().and_then(|value| value.parse::<usize>().ok())
                                == Some(reload_state)
                    })
                })
    });
    if !has_reload_handler {
        return Err("crossbow donor has no engine reload-state handler".into());
    }
    let mut next_id = objects
        .iter()
        .filter_map(|node| node.attribute("name"))
        .filter_map(|name| name.strip_prefix('#')?.parse::<u32>().ok())
        .max()
        .ok_or("crossbow donor has no object IDs")?
        + 1;
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut added_objects = String::new();
    let mut fire_count = 0;
    let mut reload_count = 0;
    for clip in objects
        .iter()
        .filter(|node| node.attribute("class") == Some("hkbClipGenerator"))
    {
        let animation =
            parameter(*clip, "animationName").ok_or("donor clip has no animation path")?;
        let normalized = animation
            .text()
            .unwrap_or("")
            .replace('\\', "/")
            .to_ascii_lowercase();
        let leaf = normalized.rsplit('/').next().unwrap_or("");
        let stem = leaf.rsplit_once('.').map_or(leaf, |(stem, _)| stem);
        let is_fire = !stem.contains("slave")
            && (stem.starts_with("wpnfiresingleready")
                || stem.starts_with("wpnfiresinglesighted")
                || stem == "sneakwpnfiresingleready");
        let is_reload = stem == "wpnreload";
        if !is_fire && !is_reload {
            continue;
        }
        let path = if is_fire {
            fire_count += 1;
            fire_path
        } else {
            reload_count += 1;
            reload_path
        };
        let trigger_id = format!("#{next_id:04}");
        next_id += 1;
        for (name, value) in [
            ("animationName", path.replace('/', "\\")),
            ("animationBindingIndex", "-1".into()),
            ("triggers", trigger_id.clone()),
            ("mode", "MODE_SINGLE_PLAY".into()),
            ("cropStartAmountLocalTime", "0.0".into()),
            ("cropEndAmountLocalTime", "0.0".into()),
            ("enforcedDuration", "0.0".into()),
        ] {
            let parameter =
                parameter(*clip, name).ok_or_else(|| format!("donor clip has no {name}"))?;
            edits.push((
                parameter.range(),
                format!("<hkparam name=\"{name}\">{}</hkparam>", escape(&value)),
            ));
        }
        // The short source shot cannot inherit the donor's negative-time bolt
        // triggers. Its completion opens the existing genuine reload state.
        added_objects.push_str(&format!(
            "<hkobject name=\"{trigger_id}\" class=\"hkbClipTriggerArray\" signature=\"0xf757cd66\"><hkparam name=\"triggers\" numelements=\"3\">{}{}{}</hkparam></hkobject>",
            trigger(event(if is_fire { "BowRelease" } else { "Reload" })?, "0.0").replace("<hkparam name=\"relativeToEndOfClip\">1", "<hkparam name=\"relativeToEndOfClip\">0"),
            trigger(initiate_start, "-0.00002"),
            trigger(if is_fire { reload_start } else { reload_end }, "-0.00001"),
        ));
    }
    if fire_count == 0 || reload_count == 0 {
        return Err("crossbow donor lacks both normal gun fire and reload slots".into());
    }
    let section = doc
        .descendants()
        .find(|node| node.has_tag_name("hksection"))
        .ok_or("crossbow donor has no data section")?;
    let end = section.range().end - "</hksection>".len();
    edits.push((end..end, added_objects));
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut result = xml.to_string();
    for (range, value) in edits {
        result.replace_range(range, &value);
    }
    Ok((result, fire_count, reload_count))
}

fn declare_profile(xml: &str) -> Result<String, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|error| error.to_string())?;
    let array = |name: &str| {
        doc.descendants()
            .find(|node| node.attribute("name") == Some(name))
            .ok_or_else(|| format!("crossbow donor has no {name}"))
    };
    let mut edits = Vec::new();
    let append = |name: &str,
                  extra: &str,
                  count: usize,
                  edits: &mut Vec<(Range<usize>, String)>|
     -> Result<(), String> {
        let node = array(name)?;
        let original = node
            .attribute("numelements")
            .ok_or("missing donor array length")?
            .parse::<usize>()
            .map_err(|e| e.to_string())?;
        let old = &xml[node.range()];
        let start = old.find('>').ok_or("invalid donor array")? + 1;
        let end = old.rfind("</hkparam>").ok_or("invalid donor array end")?;
        edits.push((
            node.range(),
            format!(
                "<hkparam name=\"{name}\" numelements=\"{}\">{}{extra}</hkparam>",
                original + count,
                &old[start..end]
            ),
        ));
        Ok(())
    };
    let names = array("variableNames")?;
    let active_index = names.children().filter(roxmltree::Node::is_element).count();
    if names
        .children()
        .any(|node| node.text() == Some("B21SourceCrossbowActive"))
    {
        return Err("crossbow donor already has source profile admission".into());
    }
    append(
        "variableNames",
        "<hkcstring>B21SourceCrossbowActive</hkcstring>",
        1,
        &mut edits,
    )?;
    append(
        "variableInfos",
        "<hkobject><hkparam name=\"role\"><hkobject><hkparam name=\"role\">ROLE_DEFAULT</hkparam><hkparam name=\"flags\">FLAG_NONE</hkparam></hkobject></hkparam><hkparam name=\"type\">VARIABLE_TYPE_BOOL</hkparam></hkobject>",
        1,
        &mut edits,
    )?;
    append(
        "wordVariableValues",
        "<hkobject><hkparam name=\"value\">0</hkparam></hkobject>",
        1,
        &mut edits,
    )?;
    let events = array("eventNames")?;
    let missing: Vec<_> = ["BowRelease", "Reload"]
        .into_iter()
        .filter(|name| !events.children().any(|node| node.text() == Some(name)))
        .collect();
    if !missing.is_empty() {
        append(
            "eventNames",
            &missing
                .iter()
                .map(|name| format!("<hkcstring>{name}</hkcstring>"))
                .collect::<String>(),
            missing.len(),
            &mut edits,
        )?;
        append(
            "eventInfos",
            &"<hkobject><hkparam name=\"flags\">0</hkparam></hkobject>".repeat(missing.len()),
            missing.len(),
            &mut edits,
        )?;
    }
    let next = doc
        .descendants()
        .filter_map(|node| node.attribute("name"))
        .filter_map(|name| name.strip_prefix('#')?.parse::<u32>().ok())
        .max()
        .ok_or("crossbow donor has no object IDs")?
        + 1;
    let binding = format!("#{next:04}");
    let active = format!("#{:04}", next + 1);
    let wrapper = format!("#{:04}", next + 2);
    let root = doc
        .descendants()
        .find(|node| node.attribute("class") == Some("hkbBehaviorGraph"))
        .and_then(|node| parameter(node, "rootGenerator"))
        .ok_or("crossbow donor has no root generator")?;
    let generator = root.text().ok_or("crossbow donor root is empty")?;
    edits.push((
        root.range(),
        format!("<hkparam name=\"rootGenerator\">{wrapper}</hkparam>"),
    ));
    let objects = format!(
        "<hkobject name=\"{binding}\" class=\"hkbVariableBindingSet\" signature=\"0xe942f339\"><hkparam name=\"bindings\" numelements=\"1\"><hkobject><hkparam name=\"memberPath\">bIsActive0</hkparam><hkparam name=\"variableIndex\">{active_index}</hkparam><hkparam name=\"bitIndex\">-1</hkparam><hkparam name=\"bindingType\">BINDING_TYPE_VARIABLE</hkparam></hkobject></hkparam><hkparam name=\"indexOfBindingToEnable\">-1</hkparam></hkobject><hkobject name=\"{active}\" class=\"BSIsActiveModifier\" signature=\"0x5bf1f5cf\"><hkparam name=\"variableBindingSet\">{binding}</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">B21SourceCrossbowActive</hkparam><hkparam name=\"enable\">true</hkparam><hkparam name=\"bIsActive0\">false</hkparam><hkparam name=\"bInvertActive0\">false</hkparam></hkobject><hkobject name=\"{wrapper}\" class=\"hkbModifierGenerator\" signature=\"0xc499fc9e\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">B21SourceCrossbowProfile</hkparam><hkparam name=\"modifier\">{active}</hkparam><hkparam name=\"generator\">{generator}</hkparam></hkobject>"
    );
    let end = xml
        .rfind("</hksection>")
        .ok_or("crossbow donor has no section end")?;
    edits.push((end..end, objects));
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut result = xml.to_string();
    for (range, replacement) in edits {
        result.replace_range(range, &replacement);
    }
    Ok(result)
}

fn parameter<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children()
        .find(|child| child.attribute("name") == Some(name))
}

fn trigger(event: usize, time: &str) -> String {
    format!(
        "<hkobject><hkparam name=\"localTime\">{time}</hkparam><hkparam name=\"event\"><hkobject><hkparam name=\"id\">{event}</hkparam><hkparam name=\"payload\">null</hkparam></hkobject></hkparam><hkparam name=\"relativeToEndOfClip\">1</hkparam><hkparam name=\"acyclic\">1</hkparam><hkparam name=\"isAnnotation\">0</hkparam></hkobject>"
    )
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
