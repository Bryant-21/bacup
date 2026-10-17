use std::collections::HashMap;
use std::path::Path;

pub(super) struct StagedWeaponActorGraphs {
    pub(super) wrapper_path: String,
    pub(super) assets: Vec<(String, Vec<u8>)>,
    pub(super) removed_draw_triggers: usize,
    pub(super) removed_sheathe_triggers: usize,
}

pub(super) fn stage_source_attachment_graphs(
    donor_graph_path: &str,
    donor_bytes: &[u8],
    target_meshes_root: &Path,
    key: &str,
    mapped_events: &[&str],
    emitted_clip_paths: &[String],
) -> Result<StagedWeaponActorGraphs, String> {
    let mut result = StagedWeaponActorGraphs {
        wrapper_path: String::new(),
        assets: Vec::new(),
        removed_draw_triggers: 0,
        removed_sheathe_triggers: 0,
    };
    let mut visited = HashMap::new();
    result.wrapper_path = clone_graph(
        &normalize_path(donor_graph_path)?,
        donor_bytes,
        target_meshes_root,
        key,
        mapped_events,
        emitted_clip_paths,
        &mut visited,
        &mut result,
    )?;
    Ok(result)
}

fn clone_graph(
    path: &str,
    bytes: &[u8],
    target: &Path,
    key: &str,
    mapped_events: &[&str],
    emitted_clip_paths: &[String],
    visited: &mut HashMap<String, String>,
    result: &mut StagedWeaponActorGraphs,
) -> Result<String, String> {
    let lookup = path.to_ascii_lowercase();
    if let Some(custom) = visited.get(&lookup) {
        return Ok(custom.clone());
    }
    let (directory, filename) = path
        .rsplit_once('/')
        .ok_or("donor graph has no directory")?;
    let custom = format!("{directory}/{key}_{filename}");
    visited.insert(lookup, custom.clone());
    let xml = havok_native::api::havok_hkx_to_xml(bytes).map_err(|error| error.to_string())?;
    let doc = roxmltree::Document::parse(&xml).map_err(|error| error.to_string())?;
    let mut edits = Vec::new();
    for animation in doc
        .descendants()
        .filter(|node| node.attribute("name") == Some("animationName"))
    {
        let Some(name) = animation.text() else {
            continue;
        };
        let start = name.rfind(['/', '\\']).map_or(0, |index| index + 1);
        let stem = name[start..]
            .rsplit_once('.')
            .map_or(&name[start..], |(stem, _)| stem);
        if let Some(filename) = emitted_clip_paths
            .iter()
            .filter_map(|path| path.rsplit('/').next())
            .find(|filename| {
                filename
                    .rsplit_once('.')
                    .is_some_and(|(candidate, _)| candidate.eq_ignore_ascii_case(stem))
            })
        {
            edits.push((
                animation.range(),
                format!(
                    "<hkparam name=\"animationName\">{}{}</hkparam>",
                    escape(&name[..start]),
                    escape(filename)
                ),
            ));
        }
    }
    let event_names: Vec<_> = doc
        .descendants()
        .find(|node| node.attribute("name") == Some("eventNames"))
        .into_iter()
        .flat_map(|node| node.children().filter(roxmltree::Node::is_element))
        .filter_map(|node| node.text())
        .collect();
    let mapped_ids: Vec<_> = event_names
        .iter()
        .enumerate()
        .filter(|(_, name)| mapped_events.contains(name))
        .collect();
    let new_events: Vec<_> = mapped_events
        .iter()
        .copied()
        .filter(|name| name.starts_with("B21LegacyWeaponSpin") && !event_names.contains(name))
        .collect();
    if !new_events.is_empty() {
        for name in ["eventNames", "eventInfos"] {
            let array = doc
                .descendants()
                .find(|node| node.attribute("name") == Some(name))
                .ok_or_else(|| format!("source weapon graph has no {name}"))?;
            let elements: Vec<_> = array
                .children()
                .filter(roxmltree::Node::is_element)
                .collect();
            if elements.len() != event_names.len() {
                return Err("weapon graph event-name and metadata counts disagree".into());
            }
            let mut contents = elements
                .iter()
                .map(|node| &xml[node.range()])
                .collect::<String>();
            for event in &new_events {
                if name == "eventNames" {
                    contents.push_str(&format!("<hkcstring>{}</hkcstring>", escape(event)));
                } else {
                    contents.push_str("<hkobject><hkparam name=\"flags\">0</hkparam></hkobject>");
                }
            }
            edits.push((
                array.range(),
                format!(
                    "<hkparam name=\"{name}\" numelements=\"{}\">{contents}</hkparam>",
                    elements.len() + new_events.len()
                ),
            ));
        }
    }
    let source_charge_arrays: Vec<_> =
        if mapped_events.contains(&super::weapon_events::SOURCE_SPIN_STOP_EVENT) {
            doc.descendants()
                .filter(|node| node.attribute("class") == Some("hkbClipGenerator"))
                .filter(|node| {
                    node.children()
                        .find(|child| child.attribute("name") == Some("animationName"))
                        .and_then(|child| child.text())
                        .is_some_and(|name| {
                            let name = name.replace('\\', "/");
                            let leaf = name.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
                            leaf.starts_with("wpnchargedown") || leaf.starts_with("wpnchargeup")
                        })
                })
                .filter_map(|node| {
                    node.children()
                        .find(|child| child.attribute("name") == Some("triggers"))
                        .and_then(|child| child.text())
                })
                .collect()
        } else {
            Vec::new()
        };
    for object in doc
        .descendants()
        .filter(|node| node.has_tag_name("hkobject"))
    {
        let array_name = match object.attribute("class") {
            Some("hkbClipTriggerArray") => "triggers",
            Some("hkbStateMachineEventPropertyArray") => "events",
            _ => continue,
        };
        let Some(array) = object
            .children()
            .find(|node| node.attribute("name") == Some(array_name))
        else {
            continue;
        };
        let mut children = String::new();
        let mut count = 0;
        let mut removed = false;
        for child in array.children().filter(roxmltree::Node::is_element) {
            let id = child
                .descendants()
                .find(|node| node.attribute("name") == Some("id"))
                .and_then(|node| node.text())
                .and_then(|value| value.parse::<usize>().ok());
            let mapped_name = mapped_ids
                .iter()
                .find(|(mapped, _)| Some(*mapped) == id)
                .map(|(_, name)| **name);
            let source_charge_owns_event = source_charge_arrays
                .contains(&object.attribute("name").unwrap_or(""))
                && id.and_then(|id| event_names.get(id)).is_some_and(|name| {
                    matches!(
                        *name,
                        "attackRelease"
                            | "idleStop"
                            | "AttackEnd"
                            | "triggerEnd"
                            | "attackReleaseSlave"
                            | "attackReleaseCharge"
                            | "attackEndSlave"
                            | "attackReleaseChargeSlave"
                            | "initiateStart"
                            | "attackStartAuto"
                            | "vatsSpinningStart"
                            | "00NextClip"
                    )
                });
            if mapped_name.is_some() {
                if mapped_name == Some("weaponDraw") {
                    result.removed_draw_triggers += 1;
                }
                if mapped_name == Some("weaponSheathe") {
                    result.removed_sheathe_triggers += 1;
                }
                removed = true;
            } else {
                if source_charge_owns_event {
                    let range = child.range();
                    let mut trigger = xml[range.clone()].to_string();
                    let mut timing_edits = Vec::new();
                    for name in ["localTime", "relativeToEndOfClip"] {
                        let parameter = child
                            .children()
                            .find(|node| node.attribute("name") == Some(name))
                            .ok_or_else(|| format!("charge completion trigger has no {name}"))?;
                        let parameter_range = parameter.range();
                        timing_edits.push((
                            parameter_range.start - range.start..parameter_range.end - range.start,
                            format!(
                                "<hkparam name=\"{name}\">{}</hkparam>",
                                if name == "localTime" { "-0.00001" } else { "1" }
                            ),
                        ));
                    }
                    timing_edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
                    for (range, replacement) in timing_edits {
                        trigger.replace_range(range, &replacement);
                    }
                    children.push_str(&trigger);
                    removed = true;
                } else {
                    children.push_str(&xml[child.range()]);
                }
                count += 1;
            }
        }
        if removed {
            edits.push((
                array.range(),
                format!(
                    "<hkparam name=\"{array_name}\" numelements=\"{count}\">{children}</hkparam>"
                ),
            ));
        }
    }
    for reference in doc
        .descendants()
        .filter(|node| node.attribute("name") == Some("behaviorName"))
    {
        let Some(name) = reference.text().filter(|name| !name.trim().is_empty()) else {
            continue;
        };
        let referenced = resolve_reference(path, name)?;
        let child_custom = if let Some(custom) = visited.get(&referenced.to_ascii_lowercase()) {
            custom.clone()
        } else {
            let child = std::fs::read(target.join(&referenced))
                .map_err(|error| format!("referenced weapon graph {referenced}: {error}"))?;
            clone_graph(
                &referenced,
                &child,
                target,
                key,
                mapped_events,
                emitted_clip_paths,
                visited,
                result,
            )?
        };
        let new_name = replace_reference_filename(name, &child_custom)?;
        edits.push((
            reference.range(),
            format!(
                "<hkparam name=\"behaviorName\">{}</hkparam>",
                escape(&new_name)
            ),
        ));
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut changed = xml;
    for (range, replacement) in edits {
        changed.replace_range(range, &replacement);
    }
    let packed =
        havok_native::api::havok_xml_to_hkx(&changed).map_err(|error| error.to_string())?;
    havok_native::hkx::HkxFile::read(&packed).map_err(|error| error.to_string())?;
    result.assets.push((custom.clone(), packed));
    Ok(custom)
}

fn resolve_reference(path: &str, reference: &str) -> Result<String, String> {
    let reference = reference.replace('\\', "/");
    if reference.to_ascii_lowercase().starts_with("actors/") {
        return normalize_path(&reference);
    }
    let lower = path.to_ascii_lowercase();
    let actor_end = lower
        .find("/behaviors/")
        .ok_or("weapon graph is outside a Behaviors folder")?;
    normalize_path(&format!("{}/{}", &path[..actor_end], reference))
}

fn normalize_path(path: &str) -> Result<String, String> {
    let mut parts = Vec::new();
    let normalized = path.replace('\\', "/");
    if Path::new(&normalized).is_absolute() || normalized.contains(':') {
        return Err(format!("invalid mesh-relative graph path: {path}"));
    }
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop().ok_or("weapon graph path escapes meshes")?;
            }
            _ => parts.push(part),
        }
    }
    Ok(parts.join("/"))
}

fn replace_reference_filename(reference: &str, custom: &str) -> Result<String, String> {
    let filename = custom
        .rsplit('/')
        .next()
        .ok_or("custom graph filename is missing")?;
    let offset = reference.rfind(['/', '\\']).map_or(0, |index| index + 1);
    Ok(format!("{}{filename}", &reference[..offset]))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_relative_graph_resolution_preserves_pov_and_directory_depth() {
        assert_eq!(
            resolve_reference(
                "Actors/Character/_1stPerson/Behaviors/pistol.hkx",
                r"Behaviors\GunBehavior.hkx"
            )
            .unwrap(),
            "Actors/Character/_1stPerson/Behaviors/GunBehavior.hkx"
        );
        assert_eq!(
            resolve_reference(
                "Actors/Character/Behaviors/wrapper.hkx",
                r"Behaviors\WeaponBehavior.hkx"
            )
            .unwrap(),
            "Actors/Character/Behaviors/WeaponBehavior.hkx"
        );
        assert_eq!(
            replace_reference_filename(
                r"Behaviors\GunBehavior.hkx",
                "Actors/Character/_1stPerson/Behaviors/Test_GunBehavior.hkx"
            )
            .unwrap(),
            r"Behaviors\Test_GunBehavior.hkx"
        );
        assert!(normalize_path("../../outside.hkx").is_err());
    }

    #[test]
    #[ignore = "requires extracted vanilla FO4 actor behavior graphs"]
    fn real_actor_graphs_preserve_references_and_remove_only_source_attachment_events() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../extracted/fo4/meshes");
        for donor in [
            "Actors/Character/Behaviors/WeaponBehavior.hkx",
            "Actors/Character/_1stPerson/Behaviors/pistol_gunwrappingbehavior.hkx",
            "Actors/Character/_1stPerson/Behaviors/BigGuns_GunWrappingBehavior.hkx",
        ] {
            let bytes = std::fs::read(root.join(donor)).unwrap();
            let staged = stage_source_attachment_graphs(
                donor,
                &bytes,
                &root,
                "TestSource",
                &[
                    "weaponDraw",
                    "weaponSheathe",
                    super::super::weapon_events::SOURCE_SPIN_START_EVENT,
                    super::super::weapon_events::SOURCE_SPIN_STOP_EVENT,
                ],
                &[
                    "Actors/Character/Animations/Test/WPNChargeUp.hkx".into(),
                    "Actors/Character/Animations/Test/WPNChargeDown.hkx".into(),
                    "Actors/Character/Animations/Test/WPNFireAutoReady.hkx".into(),
                ],
            )
            .unwrap();
            assert!(staged.removed_draw_triggers > 0);
            assert!(
                staged
                    .assets
                    .iter()
                    .any(|(path, _)| path == &staged.wrapper_path)
            );
            assert_eq!(std::fs::read(root.join(donor)).unwrap(), bytes);
            for (path, bytes) in &staged.assets {
                let xml = havok_native::api::havok_hkx_to_xml(bytes).unwrap();
                let doc = roxmltree::Document::parse(&xml).unwrap();
                let names: Vec<_> = doc
                    .descendants()
                    .find(|node| node.attribute("name") == Some("eventNames"))
                    .unwrap()
                    .children()
                    .filter(roxmltree::Node::is_element)
                    .map(|node| node.text().unwrap())
                    .collect();
                assert!(names.contains(&super::super::weapon_events::SOURCE_SPIN_START_EVENT));
                assert!(names.contains(&super::super::weapon_events::SOURCE_SPIN_STOP_EVENT));
                let infos = doc
                    .descendants()
                    .find(|node| node.attribute("name") == Some("eventInfos"))
                    .unwrap()
                    .children()
                    .filter(roxmltree::Node::is_element)
                    .count();
                assert_eq!(names.len(), infos);
                for animation in doc
                    .descendants()
                    .filter(|node| node.attribute("name") == Some("animationName"))
                {
                    let name = animation.text().unwrap();
                    if ["WPNChargeUp", "WPNChargeDown", "WPNFireAutoReady"]
                        .iter()
                        .any(|stem| {
                            name.rsplit(['/', '\\']).next() == Some(format!("{stem}.hkt").as_str())
                        })
                    {
                        panic!("emitted alias still references obsolete extension: {name}");
                    }
                }
                for generator in doc
                    .descendants()
                    .filter(|node| node.attribute("class") == Some("hkbClipGenerator"))
                {
                    let animation = generator
                        .children()
                        .find(|node| node.attribute("name") == Some("animationName"))
                        .and_then(|node| node.text())
                        .unwrap_or("");
                    if !animation.contains("WPNChargeUp") && !animation.contains("WPNChargeDown") {
                        continue;
                    }
                    let triggers = generator
                        .children()
                        .find(|node| node.attribute("name") == Some("triggers"))
                        .and_then(|node| node.text())
                        .unwrap_or("");
                    let Some(array) = doc
                        .descendants()
                        .find(|node| node.attribute("name") == Some(triggers))
                    else {
                        continue;
                    };
                    for trigger in array
                        .children()
                        .filter(|node| node.attribute("name") == Some("triggers"))
                        .flat_map(|node| node.children().filter(roxmltree::Node::is_element))
                    {
                        let id = trigger
                            .descendants()
                            .find(|node| node.attribute("name") == Some("id"))
                            .and_then(|node| node.text())
                            .and_then(|id| id.parse::<usize>().ok());
                        if id.and_then(|id| names.get(id)).is_some_and(|name| {
                            matches!(
                                *name,
                                "attackStartAuto"
                                    | "triggerEnd"
                                    | "00NextClip"
                                    | "vatsSpinningStart"
                            )
                        }) {
                            let parameter = |name| {
                                trigger
                                    .children()
                                    .find(|node| node.attribute("name") == Some(name))
                                    .and_then(|node| node.text())
                                    .unwrap()
                            };
                            assert_eq!(parameter("relativeToEndOfClip"), "1");
                            assert!(
                                (parameter("localTime").parse::<f64>().unwrap() + 0.00001).abs()
                                    < 1e-7
                            );
                        }
                    }
                }
                for reference in doc
                    .descendants()
                    .filter(|node| node.attribute("name") == Some("behaviorName"))
                {
                    let resolved = resolve_reference(path, reference.text().unwrap()).unwrap();
                    assert!(
                        staged
                            .assets
                            .iter()
                            .any(|(path, _)| path.eq_ignore_ascii_case(&resolved)),
                        "{resolved}"
                    );
                }
            }
            eprintln!(
                "{donor}: {} cloned graphs, removed draw {}, sheathe {}",
                staged.assets.len(),
                staged.removed_draw_triggers,
                staged.removed_sheathe_triggers
            );
        }
    }
}
