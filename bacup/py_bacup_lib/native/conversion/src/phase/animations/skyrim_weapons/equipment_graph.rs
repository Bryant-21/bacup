use std::collections::BTreeMap;

pub(super) fn retime_equipment(
    xml: &str,
    equip_paths: &[String],
    unequip_paths: &[String],
    disable_left_hand_ik: bool,
) -> Result<String, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let objects: BTreeMap<_, _> = doc
        .descendants()
        .filter(|node| node.attribute("class").is_some())
        .filter_map(|node| node.attribute("name").map(|id| (id, node)))
        .collect();
    let events: Vec<_> = doc
        .descendants()
        .find(|node| node.attribute("name") == Some("eventNames"))
        .ok_or("equipment graph has no events")?
        .children()
        .filter(roxmltree::Node::is_element)
        .map(|node| node.text().unwrap_or(""))
        .collect();
    let mut next = objects
        .keys()
        .filter_map(|id| id.strip_prefix('#')?.parse::<usize>().ok())
        .max()
        .ok_or("equipment graph has no objects")?
        + 1;
    let mut edits = Vec::new();
    let mut appended = String::new();
    for clip in objects
        .values()
        .filter(|node| node.attribute("class") == Some("hkbClipGenerator"))
    {
        let path = parameter(*clip, "animationName")
            .and_then(|node| node.text())
            .unwrap_or("");
        let matches = |candidate: &String| {
            candidate
                .replace('/', "\\")
                .eq_ignore_ascii_case(&path.replace('/', "\\"))
        };
        if !equip_paths.iter().any(matches) && !unequip_paths.iter().any(matches) {
            continue;
        }
        let trigger_param =
            parameter(*clip, "triggers").ok_or("equipment clip has no trigger field")?;
        let trigger_id = trigger_param.text().unwrap_or("null").trim();
        if trigger_id == "null" {
            continue;
        }
        let triggers = objects
            .get(trigger_id)
            .and_then(|node| parameter(*node, "triggers"))
            .ok_or("equipment clip trigger array is missing")?;
        let mut rows = Vec::new();
        for trigger in triggers.children().filter(roxmltree::Node::is_element) {
            let event = parameter(trigger, "event")
                .and_then(|node| node.children().find(roxmltree::Node::is_element))
                .and_then(|node| parameter(node, "id"))
                .and_then(|node| node.text())
                .ok_or("equipment trigger event is missing")?
                .parse::<usize>()
                .map_err(|e| e.to_string())?;
            let event_name = *events
                .get(event)
                .ok_or("equipment trigger event is out of range")?;
            if event_name.eq_ignore_ascii_case("weaponDraw")
                || event_name.eq_ignore_ascii_case("weaponSheathe")
                || (disable_left_hand_ik && event_name == "EnableLeftHandIK")
            {
                continue;
            }
            let mut row = xml[trigger.range()].to_string();
            if matches!(event_name, "00NextClip" | "g_archetypeBaseStateStart") {
                for (name, value) in [("localTime", "0"), ("relativeToEndOfClip", "true")] {
                    let parameter =
                        parameter(trigger, name).ok_or("equipment completion lacks timing")?;
                    row = row.replace(
                        &xml[parameter.range()],
                        &format!("<hkparam name=\"{name}\">{value}</hkparam>"),
                    );
                }
            }
            rows.push(row);
        }
        let id = format!("#{next:04}");
        next += 1;
        appended.push_str(&format!("<hkobject name=\"{id}\" class=\"hkbClipTriggerArray\" signature=\"0xf757cd66\"><hkparam name=\"triggers\" numelements=\"{}\">{}</hkparam></hkobject>", rows.len(), rows.join("")));
        edits.push((
            trigger_param.range(),
            format!("<hkparam name=\"triggers\">{id}</hkparam>"),
        ));
    }
    let end = xml
        .rfind("</hksection>")
        .ok_or("equipment graph has no section end")?;
    edits.push((end..end, appended));
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut result = xml.to_string();
    for (range, replacement) in edits {
        result.replace_range(range, &replacement);
    }
    Ok(result)
}

fn parameter<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    node.children()
        .find(|node| node.attribute("name") == Some(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_equipment_keeps_native_completion_and_removes_duplicate_visibility() {
        let trigger = |event, time| {
            format!(
                "<hkobject><hkparam name=\"localTime\">{time}</hkparam><hkparam name=\"event\"><hkobject><hkparam name=\"id\">{event}</hkparam></hkobject></hkparam><hkparam name=\"relativeToEndOfClip\">false</hkparam></hkobject>"
            )
        };
        let xml = format!(
            "<hkpackfile><hksection><hkobject name=\"#0001\" class=\"hkbBehaviorGraphStringData\"><hkparam name=\"eventNames\"><hkcstring>weaponDraw</hkcstring><hkcstring>00NextClip</hkcstring><hkcstring>EnableLeftHandIK</hkcstring></hkparam></hkobject><hkobject name=\"#0002\" class=\"hkbClipGenerator\"><hkparam name=\"animationName\">Animations/Bow_Equip.hkx</hkparam><hkparam name=\"triggers\">#0003</hkparam></hkobject><hkobject name=\"#0003\" class=\"hkbClipTriggerArray\"><hkparam name=\"triggers\" numelements=\"3\">{}{}{}</hkparam></hkobject></hksection></hkpackfile>",
            trigger(0, "0.201"),
            trigger(1, "-0.5"),
            trigger(2, "0.5")
        );
        let result =
            retime_equipment(&xml, &["Animations/Bow_Equip.hkx".into()], &[], true).unwrap();
        let doc = roxmltree::Document::parse(&result).unwrap();
        let clone = doc
            .descendants()
            .find(|node| node.attribute("name") == Some("#0004"))
            .unwrap();
        let triggers = parameter(clone, "triggers").unwrap();
        assert_eq!(triggers.attribute("numelements"), Some("1"));
        let row = triggers
            .children()
            .find(roxmltree::Node::is_element)
            .unwrap();
        assert_eq!(parameter(row, "localTime").unwrap().text(), Some("0"));
        assert_eq!(
            parameter(row, "relativeToEndOfClip").unwrap().text(),
            Some("true")
        );
        assert!(result.contains("0.201"));
    }
}
