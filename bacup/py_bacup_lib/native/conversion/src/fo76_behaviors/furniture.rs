use super::*;

const WORKBENCH: &str = "actors/character/behaviors/workbenchfurniturebehavior.hkx";
const GAS_PUMP_EXIT: &str =
    "/atx/actors/character/animation/furniture/atx_gaspump_distillingstation/exittostand.hkx";

fn named(file: &HkxFile, class: &str, name: &str) -> Result<usize, String> {
    file.objects()
        .iter()
        .position(|o| o.class_name == class && string(o, "name") == name)
        .ok_or_else(|| format!("Gas Pump exit: missing {class} {name}"))
}

fn event_names(file: &HkxFile) -> Result<Vec<String>, String> {
    let strings = file
        .objects()
        .iter()
        .find(|o| o.class_name == "hkbBehaviorGraphStringData")
        .ok_or("Gas Pump exit: missing event names")?;
    Ok(array(strings, "eventNames")
        .iter()
        .filter_map(text)
        .map(str::to_owned)
        .collect())
}

pub(super) fn repair_gas_pump_exit(
    file: &mut HkxFile,
    source_graph: &str,
    target: &Path,
) -> Result<bool, String> {
    if clean(source_graph) != WORKBENCH {
        return Ok(false);
    }
    let clip = named(file, "hkbClipGenerator", "ExitToStand")?;
    if !clean(&string(&file.objects()[clip], "animationName")).ends_with(GAS_PUMP_EXIT) {
        return Ok(false);
    }
    let state = named(file, "hkbStateMachineStateInfo", "Exit_To_Stand")?;
    let modifier = pointer(&file.objects()[state], "generator")
        .ok_or("Gas Pump exit: missing state generator")?;
    let generator = pointer(&file.objects()[modifier], "generator")
        .ok_or("Gas Pump exit: missing modifier generator")?;
    if generator == clip {
        return Ok(false);
    }
    if file.objects()[generator].class_name != "hkbBlenderGenerator"
        || string(&file.objects()[generator], "name") != "Exit_To_Stand Blend"
        || value(&file.objects()[clip], "playbackSpeed").and_then(number) != Some(-0.1)
        || pointer(&file.objects()[clip], "triggers").is_some()
    {
        return Err("Gas Pump exit: source exit layout changed".into());
    }

    let donor = read(&target.join(WORKBENCH))?;
    let donor_clip = named(&donor, "hkbClipGenerator", "ExitToStand")?;
    let donor_triggers = pointer(&donor.objects()[donor_clip], "triggers")
        .ok_or("Gas Pump exit: FO4 exit has no triggers")?;
    let names = event_names(file)?;
    let donor_names = event_names(&donor)?;
    let exit_event = names
        .iter()
        .position(|name| name == "furnitureExitSlave")
        .ok_or("Gas Pump exit: missing furnitureExitSlave")? as i32;
    let mut triggers = donor.objects()[donor_triggers].clone();
    let mut entries = array(&triggers, "triggers");
    for trigger in &mut entries {
        let event = trigger
            .as_object_members_mut()
            .and_then(|m| m.iter_mut().find(|m| m.name == "event"))
            .and_then(|m| m.value.as_object_members_mut())
            .ok_or("Gas Pump exit: invalid FO4 trigger event")?;
        if event
            .iter()
            .any(|m| m.name == "payload" && !matches!(m.value, HkxValue::Pointer(None)))
        {
            return Err("Gas Pump exit: unexpected FO4 trigger payload".into());
        }
        let id = event
            .iter_mut()
            .find(|m| m.name == "id")
            .ok_or("Gas Pump exit: missing FO4 trigger event ID")?;
        let donor_id = number(&id.value).ok_or("Gas Pump exit: invalid FO4 event ID")?;
        let name = donor_names
            .get(donor_id as usize)
            .ok_or("Gas Pump exit: unknown FO4 trigger event")?;
        let mapped = names
            .iter()
            .position(|n| n == name)
            .ok_or_else(|| format!("Gas Pump exit: missing event {name}"))?;
        id.value = HkxValue::I32(mapped as i32);
    }
    set(&mut triggers, "triggers", HkxValue::Array(entries));
    let entry = pointer(&file.objects()[state], "enterNotifyEvents")
        .ok_or("Gas Pump exit: missing entry notifications")?;
    let mut notifications = file.objects()[entry].clone();
    let events = array(&notifications, "events")
        .into_iter()
        .filter(|event| {
            !event.as_object_members().is_some_and(|members| {
                members
                    .iter()
                    .any(|m| m.name == "id" && number(&m.value) == Some(exit_event as f32))
            })
        })
        .collect();
    set(&mut notifications, "events", HkxValue::Array(events));

    // FO4 needs completion on the forward body clip, not FO76's 60x root-only blend.
    let triggers = add(file, triggers);
    let notifications = add(file, notifications);
    set(
        &mut file.objects_mut()[clip],
        "triggers",
        HkxValue::Pointer(Some(triggers)),
    );
    set(
        &mut file.objects_mut()[clip],
        "playbackSpeed",
        HkxValue::F32(1.0),
    );
    set(
        &mut file.objects_mut()[modifier],
        "generator",
        HkxValue::Pointer(Some(clip)),
    );
    set(
        &mut file.objects_mut()[state],
        "enterNotifyEvents",
        HkxValue::Pointer(Some(notifications)),
    );
    Ok(true)
}

#[cfg(test)]
#[path = "furniture_exit_tests.rs"]
mod tests;
