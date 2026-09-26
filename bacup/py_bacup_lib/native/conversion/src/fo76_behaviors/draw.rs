use super::*;

const THIRD_PERSON: &str = "actors/character/behaviors/gunbehavior.hkx";
const FIRST_PERSON: &str = "actors/character/_1stperson/behaviors/gunbehavior.hkx";

fn member<'a>(object: &'a HkxValue, name: &str) -> Option<&'a HkxValue> {
    object
        .as_object_members()?
        .iter()
        .find(|m| m.name == name)
        .map(|m| &m.value)
}

fn event_name<'a>(id: Option<&HkxValue>, names: &'a [HkxValue]) -> Option<&'a str> {
    let index = number(id?)?;
    (index >= 0.0)
        .then(|| names.get(index as usize).and_then(text))
        .flatten()
}

fn attack(transition: &HkxValue, names: &[HkxValue]) -> bool {
    matches!(
        event_name(member(transition, "eventId"), names),
        Some(
            "attackStart"
                | "attackStartAuto"
                | "attackStartAutoCharge"
                | "attackStartChargingHold"
                | "attackStartPreChageHold"
        )
    )
}

fn named(file: &HkxFile, class: &str, name: &str) -> Result<usize, String> {
    file.objects()
        .iter()
        .position(|o| o.class_name == class && string(o, "name") == name)
        .ok_or_else(|| format!("FO76 gun draw: missing {class} {name}"))
}

pub(super) fn repair(file: &mut HkxFile, source_graph: &str) -> Result<usize, String> {
    let path = clean(source_graph);
    if path != FIRST_PERSON && path != THIRD_PERSON {
        return Ok(0);
    }
    let strings = file
        .objects()
        .iter()
        .find(|o| o.class_name == "hkbBehaviorGraphStringData")
        .ok_or("FO76 gun draw: missing graph strings")?;
    let events = array(strings, "eventNames");
    let variables = array(strings, "variableNames");
    let mut changed = 0;
    if path == FIRST_PERSON {
        // FO4 queues an attack after drawing; FO76's global draw interrupts consume it as a shot.
        for object in file.objects_mut() {
            if object.class_name != "hkbStateMachineTransitionInfoArray" {
                continue;
            }
            let mut transitions = array(object, "transitions");
            let before = transitions.len();
            transitions.retain(|t| {
                let enter = member(t, "initiateInterval").and_then(|i| member(i, "enterEventId"));
                !(attack(t, &events) && event_name(enter, &events) == Some("initiateDrawStart"))
            });
            if before != transitions.len() {
                changed += before - transitions.len();
                set(object, "transitions", HkxValue::Array(transitions));
            }
        }
        return Ok(changed);
    }

    let machine = named(
        file,
        "hkbStateMachine",
        "INCLUDE_IN_QUERY_ActionStateMachine",
    )?;
    let states = pointers(&file.objects()[machine], "states");
    let state = |name| {
        states
            .iter()
            .copied()
            .find(|i| string(&file.objects()[*i], "name") == name)
            .ok_or_else(|| format!("FO76 gun draw: missing {name} state"))
    };
    let equip = state("Equip")?;
    let unequip = state("Unequip")?;
    let state_id = |index| {
        value(&file.objects()[index], "stateId")
            .and_then(number)
            .map(|id| id as i32)
            .ok_or("FO76 gun draw: missing state ID")
    };
    let binding = pointer(&file.objects()[machine], "variableBindingSet")
        .ok_or("FO76 gun draw: missing action state binding")?;
    let variable = array(&file.objects()[binding], "bindings")
        .iter()
        .find(|b| {
            member(b, "memberPath").and_then(text) == Some("currentStateId")
                && member(b, "bindingType").and_then(number) == Some(0.0)
        })
        .and_then(|b| event_name(member(b, "variableIndex"), &variables))
        .ok_or("FO76 gun draw: missing currentStateId variable")?;
    let gate = format!(
        "({variable} != {}) && ({variable} != {})",
        state_id(equip)?,
        state_id(unequip)?
    );
    let local = pointer(&file.objects()[equip], "transitions")
        .ok_or("FO76 gun draw: missing Equip transitions")?;
    let mut transitions = array(&file.objects()[local], "transitions");
    let before = transitions.len();
    transitions.retain(|t| !attack(t, &events));
    changed += before - transitions.len();
    if changed != 0 {
        set(
            &mut file.objects_mut()[local],
            "transitions",
            HkxValue::Array(transitions),
        );
    }

    let wildcard = pointer(&file.objects()[machine], "wildcardTransitions")
        .ok_or("FO76 gun draw: missing action wildcards")?;
    let mut transitions = array(&file.objects()[wildcard], "transitions");
    let template = file
        .objects()
        .iter()
        .find(|o| o.class_name == "hkbExpressionCondition")
        .cloned()
        .ok_or("FO76 gun draw: missing expression template")?;
    let mut conditions = BTreeMap::new();
    let mut guarded = 0;
    for transition in &mut transitions {
        if !attack(transition, &events) {
            continue;
        }
        let condition = transition
            .as_object_members_mut()
            .and_then(|fields| fields.iter_mut().find(|m| m.name == "condition"))
            .ok_or("FO76 gun draw: missing wildcard condition")?;
        let HkxValue::Pointer(original) = condition.value else {
            return Err("FO76 gun draw: invalid wildcard condition".into());
        };
        let expression = if let Some(index) = original {
            let object = &file.objects()[index];
            if object.class_name != "hkbExpressionCondition" {
                return Err("FO76 gun draw: unexpected wildcard condition class".into());
            }
            string(object, "expression")
        } else {
            String::new()
        };
        if expression.ends_with(&gate) {
            continue;
        }
        let replacement = *conditions.entry(original).or_insert_with(|| {
            let mut object = template.clone();
            let expression = if original.is_some() {
                format!("({expression}) && {gate}")
            } else {
                gate.clone()
            };
            set_string(&mut object, "expression", &expression);
            add(file, object)
        });
        condition.value = HkxValue::Pointer(Some(replacement));
        guarded += 1;
    }
    if guarded != 0 {
        set(
            &mut file.objects_mut()[wildcard],
            "transitions",
            HkxValue::Array(transitions),
        );
    }
    Ok(changed + guarded)
}

#[cfg(test)]
#[path = "draw_tests.rs"]
mod tests;
