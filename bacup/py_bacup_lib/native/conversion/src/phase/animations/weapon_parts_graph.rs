use havok_native::hkx::HkxFile;

pub(super) struct PartGraphState {
    pub(super) sequence: String,
    pub(super) triggers: Vec<String>,
    pub(super) exits: Vec<String>,
    pub(super) exit_sequence: String,
}

pub(super) fn build_behavior(states: &[PartGraphState]) -> Result<Vec<u8>, String> {
    if states.is_empty() || states[0].sequence != "LegacyIdle" {
        return Err("weapon part graph requires an idle sequence".into());
    }
    let mut events = Vec::<String>::new();
    for state in states {
        for event in state.triggers.iter().chain(&state.exits) {
            if !events.contains(event) {
                events.push(event.clone());
            }
        }
    }
    let mut xml = String::from(
        r##"<?xml version="1.0" encoding="ASCII"?>
<hkpackfile classversion="11" contentsversion="hk_2014.1.0-r1"><hksection name="__data__">
<hkobject name="#0001" class="hkRootLevelContainer" signature="0x2772c11e"><hkparam name="namedVariants" numelements="1"><hkobject><hkparam name="name">hkbBehaviorGraph</hkparam><hkparam name="className">hkbBehaviorGraph</hkparam><hkparam name="variant">#0002</hkparam></hkobject></hkparam></hkobject>
<hkobject name="#0002" class="hkbBehaviorGraph" signature="0xfdedb83b"><hkparam name="variableBindingSet">null</hkparam><hkparam name="userData">0</hkparam><hkparam name="name">LegacyWeaponParts</hkparam><hkparam name="variableMode">VARIABLE_MODE_DISCARD_WHEN_INACTIVE</hkparam><hkparam name="rootGenerator">#0003</hkparam><hkparam name="data">#0004</hkparam></hkobject>
<hkobject name="#0003" class="hkbStateMachine" signature="0xa5896bcf"><hkparam name="variableBindingSet">null</hkparam><hkparam name="userData">0</hkparam><hkparam name="name">LegacyWeaponPartsStates</hkparam><hkparam name="eventToSendWhenStateOrTransitionChanges"><hkobject><hkparam name="id">-1</hkparam><hkparam name="payload">null</hkparam></hkobject></hkparam><hkparam name="startStateIdSelector">null</hkparam><hkparam name="startStateId">0</hkparam><hkparam name="returnToPreviousStateEventId">-1</hkparam><hkparam name="randomTransitionEventId">-1</hkparam><hkparam name="transitionToNextHigherStateEventId">-1</hkparam><hkparam name="transitionToNextLowerStateEventId">-1</hkparam><hkparam name="syncVariableIndex">-1</hkparam><hkparam name="wrapAroundStateId">false</hkparam><hkparam name="maxSimultaneousTransitions">32</hkparam><hkparam name="startStateMode">START_STATE_MODE_DEFAULT</hkparam><hkparam name="selfTransitionMode">SELF_TRANSITION_MODE_NO_TRANSITION</hkparam>"##,
    );
    xml.push_str(&format!(
        "<hkparam name=\"states\" numelements=\"{}\">",
        states.len()
    ));
    for index in 0..states.len() {
        xml.push_str(&format!("#{} ", 10 + index * 3));
    }
    xml.push_str("</hkparam><hkparam name=\"wildcardTransitions\">#0008</hkparam></hkobject>");
    xml.push_str(r##"<hkobject name="#0004" class="hkbBehaviorGraphData" signature="0x907a8222"><hkparam name="attributeDefaults" numelements="0"/><hkparam name="variableInfos" numelements="0"/><hkparam name="characterPropertyInfos" numelements="0"/>"##);
    xml.push_str(&format!(
        "<hkparam name=\"eventInfos\" numelements=\"{}\">",
        events.len()
    ));
    for _ in &events {
        xml.push_str("<hkobject><hkparam name=\"flags\">0</hkparam></hkobject>");
    }
    xml.push_str(r##"</hkparam><hkparam name="variableBounds" numelements="0"/><hkparam name="variableInitialValues">#0005</hkparam><hkparam name="stringData">#0006</hkparam></hkobject>
<hkobject name="#0005" class="hkbVariableValueSet" signature="0xeb5f7e25"><hkparam name="wordVariableValues" numelements="0"/><hkparam name="quadVariableValues" numelements="0"/><hkparam name="variantVariableValues" numelements="0"/></hkobject>
<hkobject name="#0006" class="hkbBehaviorGraphStringData" signature="0x1bd27f38">"##);
    xml.push_str(&format!(
        "<hkparam name=\"eventNames\" numelements=\"{}\">",
        events.len()
    ));
    for event in &events {
        xml.push_str(&format!("<hkcstring>{}</hkcstring>", escape(event)));
    }
    xml.push_str(r##"</hkparam><hkparam name="attributeNames" numelements="0"/><hkparam name="variableNames" numelements="0"/><hkparam name="characterPropertyNames" numelements="0"/></hkobject>
<hkobject name="#0007" class="hkbBlendingTransitionEffect" signature="0x14e54c5c"><hkparam name="variableBindingSet">null</hkparam><hkparam name="userData">0</hkparam><hkparam name="name">LegacyWeaponPartsInstant</hkparam><hkparam name="selfTransitionMode">SELF_TRANSITION_MODE_CONTINUE_IF_CYCLIC_BLEND_IF_ACYCLIC</hkparam><hkparam name="eventMode">EVENT_MODE_DEFAULT</hkparam><hkparam name="duration">0</hkparam><hkparam name="toGeneratorStartTimeFraction">0</hkparam><hkparam name="flags">FLAG_NONE</hkparam><hkparam name="endMode">END_MODE_NONE</hkparam><hkparam name="blendCurve">BLEND_CURVE_SMOOTH</hkparam><hkparam name="alignmentBone">-1</hkparam></hkobject>"##);
    let trigger_count: usize = states.iter().map(|state| state.triggers.len()).sum();
    xml.push_str(&format!("<hkobject name=\"#0008\" class=\"hkbStateMachineTransitionInfoArray\" signature=\"0x704a19af\"><hkparam name=\"transitions\" numelements=\"{trigger_count}\">"));
    for (index, state) in states.iter().enumerate() {
        for event in &state.triggers {
            xml.push_str(&transition(
                events.iter().position(|name| name == event).unwrap(),
                index,
                "FLAG_IS_LOCAL_WILDCARD",
            ));
        }
    }
    xml.push_str("</hkparam></hkobject>");
    for (index, state) in states.iter().enumerate() {
        let info = 10 + index * 3;
        let generator = info + 1;
        let transitions = info + 2;
        xml.push_str(&format!(r##"<hkobject name="#{info}" class="hkbStateMachineStateInfo" signature="0x39d76713"><hkparam name="variableBindingSet">null</hkparam><hkparam name="listeners" numelements="0"/><hkparam name="enterNotifyEvents">null</hkparam><hkparam name="exitNotifyEvents">null</hkparam><hkparam name="transitions">#{transitions}</hkparam><hkparam name="generator">#{generator}</hkparam><hkparam name="name">{name}</hkparam><hkparam name="stateId">{index}</hkparam><hkparam name="probability">1</hkparam><hkparam name="enable">true</hkparam></hkobject>
<hkobject name="#{generator}" class="BGSGamebryoSequenceGenerator" signature="0x4e708fb6"><hkparam name="variableBindingSet">null</hkparam><hkparam name="userData">0</hkparam><hkparam name="name">{name}Generator</hkparam><hkparam name="pSequence">{name}</hkparam><hkparam name="eBlendModeFunction">BMF_NONE</hkparam><hkparam name="fPercent">1</hkparam><hkparam name="eUseTimePercentage">NOT_USING_TIME_PERCENTAGE</hkparam><hkparam name="fTimePercent">0</hkparam></hkobject>
<hkobject name="#{transitions}" class="hkbStateMachineTransitionInfoArray" signature="0x704a19af"><hkparam name="transitions" numelements="{exit_count}">"##, name=escape(&state.sequence), exit_count=state.exits.len()));
        let destination = states
            .iter()
            .position(|candidate| candidate.sequence == state.exit_sequence)
            .ok_or_else(|| {
                format!(
                    "weapon part graph exit sequence is missing: {}",
                    state.exit_sequence
                )
            })?;
        for event in &state.exits {
            xml.push_str(&transition(
                events.iter().position(|name| name == event).unwrap(),
                destination,
                "0",
            ));
        }
        xml.push_str("</hkparam></hkobject>");
    }
    xml.push_str("</hksection></hkpackfile>");
    let bytes = havok_native::api::havok_xml_to_hkx(&xml).map_err(|error| error.to_string())?;
    HkxFile::read(&bytes)
        .map_err(|error| format!("generated weapon part graph cannot reload: {error}"))?;
    Ok(bytes)
}

fn transition(event: usize, destination: usize, flags: &str) -> String {
    format!(
        r##"<hkobject><hkparam name="triggerInterval"><hkobject><hkparam name="enterEventId">-1</hkparam><hkparam name="exitEventId">-1</hkparam><hkparam name="enterTime">0</hkparam><hkparam name="exitTime">0</hkparam></hkobject></hkparam><hkparam name="initiateInterval"><hkobject><hkparam name="enterEventId">-1</hkparam><hkparam name="exitEventId">-1</hkparam><hkparam name="enterTime">0</hkparam><hkparam name="exitTime">0</hkparam></hkobject></hkparam><hkparam name="transition">#0007</hkparam><hkparam name="condition">null</hkparam><hkparam name="eventId">{event}</hkparam><hkparam name="toStateId">{destination}</hkparam><hkparam name="fromNestedStateId">0</hkparam><hkparam name="toNestedStateId">0</hkparam><hkparam name="priority">0</hkparam><hkparam name="flags">{flags}</hkparam></hkobject>"##
    )
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
    fn generated_part_graph_routes_engine_events_to_exact_nif_sequences() {
        let bytes = build_behavior(&[
            PartGraphState {
                sequence: "LegacyIdle".into(),
                triggers: vec![],
                exits: vec![],
                exit_sequence: "LegacyIdle".into(),
            },
            PartGraphState {
                sequence: "LegacyFire".into(),
                triggers: vec!["WeaponFire".into()],
                exits: vec!["end".into()],
                exit_sequence: "LegacyIdle".into(),
            },
            PartGraphState {
                sequence: "LegacyReload".into(),
                triggers: vec!["reloadStart".into(), "ReloadStart".into()],
                exits: vec!["end".into()],
                exit_sequence: "LegacyIdle".into(),
            },
        ])
        .unwrap();
        let xml = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let sequences = doc
            .descendants()
            .filter(|node| node.attribute("name") == Some("pSequence"))
            .filter_map(|node| node.text())
            .collect::<Vec<_>>();
        assert_eq!(sequences, vec!["LegacyIdle", "LegacyFire", "LegacyReload"]);
        assert!(
            xml.contains("WeaponFire")
                && xml.contains("reloadStart")
                && xml.contains("ReloadStart")
        );
        assert_eq!(
            doc.descendants()
                .filter(|node| node.attribute("class") == Some("hkbStateMachineStateInfo"))
                .count(),
            3
        );
    }
}
