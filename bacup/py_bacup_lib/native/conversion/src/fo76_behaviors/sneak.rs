use super::*;

const FO76_SYNC: &str = "iSyncReadyRelaxSneak";
const FO4_SNEAK: &str = "iIsInSneak";
const SNEAK_STATE: &str = "SneakState";
const START_STATE_MODE_DEFAULT: i32 = 0;

fn renumbered(value: &HkxValue, n: i32) -> HkxValue {
    match value {
        HkxValue::I8(_) => HkxValue::I8(n as i8),
        HkxValue::U8(_) => HkxValue::U8(n as u8),
        HkxValue::I16(_) => HkxValue::I16(n as i16),
        HkxValue::U16(_) => HkxValue::U16(n as u16),
        HkxValue::U32(_) => HkxValue::U32(n as u32),
        _ => HkxValue::I32(n),
    }
}

fn int(object: &HkxObject, name: &str) -> Option<i32> {
    value(object, name).and_then(number).map(|n| n as i32)
}

fn swap_id(value: &mut HkxValue, a: i32, b: i32) {
    match number(value).map(|n| n as i32) {
        Some(id) if id == a => *value = renumbered(value, b),
        Some(id) if id == b => *value = renumbered(value, a),
        _ => {}
    }
}

fn swap_member(object: &mut HkxObject, name: &str, a: i32, b: i32) {
    if let Some(member) = object.members.iter_mut().find(|m| m.name == name) {
        swap_id(&mut member.value, a, b);
    }
}

/// FO76's engine writes `iSyncReadyRelaxSneak`; FO4's never does. A state
/// machine that starts in SYNC mode on it therefore restores the state it last
/// wrote itself, so after a stance change in first person the reactivated 3P
/// graph comes back crouched (or standing). FO4's weapon graphs instead bind
/// `startStateId` to the engine-driven `iIsInSneak` with stand = 0 and
/// sneak = 1, so the sneak state is renumbered to 1 to match.
pub(super) fn repair(file: &mut HkxFile) -> Result<usize, String> {
    let Some(strings) = file
        .objects()
        .iter()
        .find(|o| o.class_name == "hkbBehaviorGraphStringData")
    else {
        return Ok(0);
    };
    let variables = array(strings, "variableNames");
    let position = |name| variables.iter().position(|v| text(v) == Some(name));
    let Some(sync) = position(FO76_SYNC) else {
        return Ok(0);
    };
    let machines: Vec<_> = file
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            o.class_name == "hkbStateMachine" && int(o, "syncVariableIndex") == Some(sync as i32)
        })
        .map(|(i, _)| i)
        .collect();
    if machines.is_empty() {
        return Ok(0);
    }
    let sneak_variable =
        position(FO4_SNEAK).ok_or("FO76 sneak sync: graph does not declare iIsInSneak")?;
    let template = file
        .objects()
        .iter()
        .find(|o| o.class_name == "hkbVariableBindingSet" && !array(o, "bindings").is_empty())
        .cloned()
        .ok_or("FO76 sneak sync: no binding set to template from")?;

    for &machine in &machines {
        let states = pointers(&file.objects()[machine], "states");
        let sneak_state = states
            .iter()
            .copied()
            .find(|&i| string(&file.objects()[i], "name") == SNEAK_STATE)
            .ok_or("FO76 sneak sync: machine has no SneakState")?;
        let old_id =
            int(&file.objects()[sneak_state], "stateId").ok_or("FO76 sneak sync: bad stateId")?;
        let new_id = 1;

        let mut transition_arrays: Vec<_> = states
            .iter()
            .filter_map(|&i| pointer(&file.objects()[i], "transitions"))
            .collect();
        transition_arrays.extend(pointer(&file.objects()[machine], "wildcardTransitions"));
        for &state in &states {
            swap_member(&mut file.objects_mut()[state], "stateId", old_id, new_id);
        }
        for index in transition_arrays {
            let object = &mut file.objects_mut()[index];
            let mut transitions = array(object, "transitions");
            for transition in &mut transitions {
                if let Some(to) = transition
                    .as_object_members_mut()
                    .and_then(|m| m.iter_mut().find(|m| m.name == "toStateId"))
                {
                    swap_id(&mut to.value, old_id, new_id);
                }
            }
            set(object, "transitions", HkxValue::Array(transitions));
        }

        let object = &mut file.objects_mut()[machine];
        swap_member(object, "startStateId", old_id, new_id);
        swap_member(object, "wrapAroundStateId", old_id, new_id);
        let mode = value(object, "startStateMode")
            .cloned()
            .unwrap_or(HkxValue::I8(0));
        set(
            object,
            "startStateMode",
            renumbered(&mode, START_STATE_MODE_DEFAULT),
        );
        let unsynced = renumbered(value(object, "syncVariableIndex").unwrap(), -1);
        set(object, "syncVariableIndex", unsynced);

        let mut binding = array(&template, "bindings").swap_remove(0);
        let fields = binding
            .as_object_members_mut()
            .ok_or("FO76 sneak sync: bad binding template")?;
        for field in fields.iter_mut() {
            match field.name.as_str() {
                "memberPath" => {
                    field.value = HkxValue::String {
                        value: "startStateId".into(),
                        is_null: false,
                    }
                }
                "variableIndex" => field.value = renumbered(&field.value, sneak_variable as i32),
                "bitIndex" => field.value = renumbered(&field.value, -1),
                "bindingType" => field.value = renumbered(&field.value, 0),
                _ => {}
            }
        }
        match pointer(&file.objects()[machine], "variableBindingSet") {
            Some(existing) => {
                let set_object = &mut file.objects_mut()[existing];
                let mut bindings = array(set_object, "bindings");
                bindings.push(binding);
                set(set_object, "bindings", HkxValue::Array(bindings));
            }
            None => {
                let mut set_object = template.clone();
                set(&mut set_object, "bindings", HkxValue::Array(vec![binding]));
                let no_enable = renumbered(
                    value(&set_object, "indexOfBindingToEnable").unwrap_or(&HkxValue::I32(0)),
                    -1,
                );
                set(&mut set_object, "indexOfBindingToEnable", no_enable);
                let index = add(file, set_object);
                set(
                    &mut file.objects_mut()[machine],
                    "variableBindingSet",
                    HkxValue::Pointer(Some(index)),
                );
            }
        }
    }
    Ok(machines.len())
}

#[cfg(test)]
#[path = "sneak_tests.rs"]
mod tests;
