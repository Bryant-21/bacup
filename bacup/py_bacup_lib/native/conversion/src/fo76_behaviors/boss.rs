use super::*;

const TITAN_CORE: &str =
    "actors/ultraciteabomination/behaviors/ultraciteabominationcorebehavior.hkx";
const STORM_CORE: &str = "actors/stormboss/behaviors/stormbosscorebehavior.hkx";
const SHARED_AMBUSH: &str = "actors/shared/behaviors/ambushbehavior.hkx";

fn field_mut<'a>(value: &'a mut HkxValue, name: &str) -> Result<&'a mut HkxValue, String> {
    value
        .as_object_members_mut()
        .and_then(|fields| fields.iter_mut().find(|field| field.name == name))
        .map(|field| &mut field.value)
        .ok_or_else(|| format!("Titan graph: missing {name}"))
}

fn event_id(file: &HkxFile, name: &str) -> Result<i32, String> {
    let strings = file
        .objects()
        .iter()
        .find(|object| object.class_name == "hkbBehaviorGraphStringData")
        .ok_or("Titan graph: missing event table")?;
    array(strings, "eventNames")
        .iter()
        .position(|value| text(value) == Some(name))
        .map(|index| index as i32)
        .ok_or_else(|| format!("Titan graph: missing event {name}"))
}

fn named(file: &HkxFile, class: &str, name: &str) -> Result<usize, String> {
    file.objects()
        .iter()
        .position(|object| object.class_name == class && string(object, "name") == name)
        .ok_or_else(|| format!("Titan graph: missing {class} {name}"))
}

pub(super) fn repair(
    file: &mut HkxFile,
    source: &Path,
    graph: &str,
    project: &str,
) -> Result<(), String> {
    let graph = clean(graph);
    let boss_ambush = graph == SHARED_AMBUSH
        && matches!(
            clean(project).as_str(),
            "actors/b21_fo76/stormboss" | "actors/b21_fo76/ultraciteabomination"
        );
    if graph != TITAN_CORE && graph != STORM_CORE && !boss_ambush {
        return Ok(());
    }
    let original = read(&source.join(&graph))?;
    let clips: Vec<_> = original
        .objects()
        .iter()
        .filter(|object| object.class_name == "hkbClipGenerator")
        .map(|object| (string(object, "name"), string(object, "animationName")))
        .collect();
    let converted: Vec<_> = file
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, object)| object.class_name == "hkbClipGenerator")
        .map(|(index, object)| (index, string(object, "name")))
        .collect();
    if clips.len() != converted.len()
        || clips
            .iter()
            .zip(&converted)
            .any(|((source_name, _), (_, converted_name))| source_name != converted_name)
    {
        return Err("Titan graph: converted clip order changed".into());
    }
    for ((_, animation), (index, _)) in clips.iter().zip(converted) {
        set_string(&mut file.objects_mut()[index], "animationName", animation);
    }
    if graph != TITAN_CORE {
        return Ok(());
    }

    let core = named(file, "hkbStateMachine", "Core_SM")?;
    let tunnel = named(file, "hkbStateMachine", "Tunnel_SM")?;
    let states = pointers(&file.objects()[core], "states");
    let tunnel_state = states
        .iter()
        .find(|index| string(&file.objects()[**index], "name") == "Tunnel")
        .ok_or("Titan graph: Core_SM has no Tunnel state")?;
    if value(&file.objects()[*tunnel_state], "stateId").and_then(number) != Some(15.0)
        || pointers(&file.objects()[tunnel], "states").len() != 4
    {
        return Err("Titan graph: tunnel state layout changed".into());
    }
    let wildcard = pointer(&file.objects()[core], "wildcardTransitions")
        .ok_or("Titan graph: missing Core_SM transitions")?;
    let mut transitions = array(&file.objects()[wildcard], "transitions");
    if transitions.iter().any(|transition| {
        transition.as_object_members().is_some_and(|fields| {
            fields
                .iter()
                .any(|field| field.name == "toStateId" && number(&field.value) == Some(15.0))
        })
    }) {
        return Err("Titan graph: Tunnel state already has a parent transition".into());
    }
    let template = transitions
        .iter()
        .find(|transition| {
            transition.as_object_members().is_some_and(|fields| {
                fields
                    .iter()
                    .any(|field| field.name == "toStateId" && number(&field.value) == Some(19.0))
            })
        })
        .cloned()
        .ok_or("Titan graph: missing wildcard transition template")?;
    for (name, nested) in [("TunnelEnter", 0), ("TunnelExit", 3)] {
        let mut transition = template.clone();
        *field_mut(&mut transition, "eventId")? = HkxValue::I32(event_id(file, name)?);
        *field_mut(&mut transition, "toStateId")? = HkxValue::I32(15);
        if nested != 0 {
            *field_mut(&mut transition, "toNestedStateId")? = HkxValue::I32(nested);
            let flags = number(field_mut(&mut transition, "flags")?)
                .ok_or("Titan graph: invalid wildcard flags")? as i32;
            *field_mut(&mut transition, "flags")? = HkxValue::I32(flags | (1 << 13));
        }
        transitions.push(transition);
    }
    set(
        &mut file.objects_mut()[wildcard],
        "transitions",
        HkxValue::Array(transitions),
    );

    for name in ["TunnelExit"] {
        let clip = named(file, "hkbClipGenerator", name)?;
        let trigger_index = pointer(&file.objects()[clip], "triggers")
            .ok_or_else(|| format!("Titan graph: {name} has no trigger array"))?;
        let mut triggers = array(&file.objects()[trigger_index], "triggers");
        let mut completion = triggers
            .first()
            .cloned()
            .ok_or_else(|| format!("Titan graph: {name} has no trigger template"))?;
        *field_mut(&mut completion, "localTime")? = HkxValue::F32(0.0);
        *field_mut(&mut completion, "relativeToEndOfClip")? = HkxValue::Bool(true);
        let event = field_mut(&mut completion, "event")?;
        *field_mut(event, "id")? = HkxValue::I32(event_id(file, "ReturnToDefault")?);
        *field_mut(event, "payload")? = HkxValue::Pointer(None);
        triggers.push(completion);
        set(
            &mut file.objects_mut()[trigger_index],
            "triggers",
            HkxValue::Array(triggers),
        );
    }
    Ok(())
}
