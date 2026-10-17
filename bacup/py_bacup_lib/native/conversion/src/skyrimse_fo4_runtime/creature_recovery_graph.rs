use super::*;
use crate::source_rig::{
    SourceRecoveryGraph, SourceRecoveryNode, SourceRecoveryValue as Value, SourceRecoveryVariable,
};

pub(crate) fn extract(
    file: &HkxFile,
    actors: &Path,
    family_root: &Path,
    behavior_path: &Path,
) -> Option<Result<SourceRecoveryGraph, String>> {
    let machines = file
        .objects()
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            if object.class_name != "hkbStateMachine" {
                return None;
            }
            let start = int_member(object, "startStateId")?;
            let states = array_member(object, "states")?;
            let living = states
                .iter()
                .filter_map(pointer_value)
                .filter_map(|index| file.objects().get(index))
                .find(|state| int_member(state, "stateId") == Some(start))
                .and_then(|state| pointer_member(state, "generator"))?;
            let recovery = states
                .iter()
                .filter_map(pointer_value)
                .filter_map(|index| file.objects().get(index))
                .filter(|state| int_member(state, "stateId") != Some(start))
                .filter_map(|state| pointer_member(state, "generator"))
                .any(|index| reaches_getup(file, index, &mut BTreeSet::new()));
            (recovery && !reaches_getup(file, living, &mut BTreeSet::new()))
                .then_some((index, living))
        })
        .collect::<Vec<_>>();
    if machines.is_empty() {
        return file
            .objects()
            .iter()
            .any(|object| object.class_name == "hkbGetUpModifier")
            .then(|| {
                Err(
                    "source GetUp modifier has no supported living/recovery state-machine boundary"
                        .into(),
                )
            });
    }
    Some((|| {
        let [(root, living)] = machines.as_slice() else {
            return Err(format!(
                "ambiguous source recovery state machines: {machines:?}"
            ));
        };
        let strings = file
            .objects()
            .iter()
            .find(|object| object.class_name == "hkbBehaviorGraphStringData")
            .ok_or("missing source graph strings")?;
        let events = string_array(strings, "eventNames");
        let variables = string_array(strings, "variableNames");
        let (types, words) = behavior_variable_data(file);
        let mut graph = SourceRecoveryGraph {
            behavior_path: runtime_asset_path(actors, behavior_path),
            root: *root as u32,
            living_generator: *living as u32,
            nodes: BTreeMap::new(),
            variables: Vec::new(),
        };
        let mut pending = vec![*root];
        let mut used_variables = BTreeSet::new();
        while let Some(index) = pending.pop() {
            if index == *living || graph.nodes.contains_key(&(index as u32)) {
                continue;
            }
            let object = file
                .objects()
                .get(index)
                .ok_or_else(|| format!("missing recovery source object {index}"))?;
            if !crate::source_rig::recovery::allowed_class(&object.class_name) {
                return Err(format!(
                    "source recovery object {index} has unsupported class {}",
                    object.class_name
                ));
            }
            let mut members = BTreeMap::new();
            for member in &object.members {
                let value =
                    if object.class_name == "hkbClipGenerator" && member.name == "animationName" {
                        let name = string_member(object, "animationName")
                            .ok_or("source recovery clip has no animation name")?;
                        let resolved = resolve_reference(actors, family_root, behavior_path, &name)
                            .ok_or_else(|| format!("missing source recovery animation {name}"))?;
                        Value::Clip(runtime_asset_path(actors, &resolved))
                    } else if object.class_name == "hkbClipGenerator"
                        && member.name == "animationBindingIndex"
                    {
                        Value::Int(-1)
                    } else {
                        convert_value(
                            &member.value,
                            &member.name,
                            &events,
                            &variables,
                            &mut pending,
                            &mut used_variables,
                        )?
                    };
                members.insert(member.name.clone(), value);
            }
            graph.nodes.insert(
                index as u32,
                SourceRecoveryNode {
                    class_name: object.class_name.clone(),
                    members,
                },
            );
        }
        for index in used_variables {
            let variable_type = match types.get(index).copied().flatten() {
                Some(0) => VariableType::Bool,
                Some(1..=3) => VariableType::Int32,
                Some(4) => VariableType::Real,
                other => {
                    return Err(format!(
                        "unsupported recovery variable {} type {other:?}",
                        variables[index]
                    ));
                }
            };
            graph.variables.push(SourceRecoveryVariable {
                name: variables[index].clone(),
                variable_type,
                initial_word: words
                    .get(index)
                    .copied()
                    .flatten()
                    .ok_or("missing recovery variable initial word")?,
            });
        }
        Ok(graph)
    })())
}

fn pointer_value(value: &HkxValue) -> Option<usize> {
    if let HkxValue::Pointer(index) = value {
        *index
    } else {
        None
    }
}

fn reaches_getup(file: &HkxFile, index: usize, seen: &mut BTreeSet<usize>) -> bool {
    if !seen.insert(index) {
        return false;
    }
    let Some(object) = file.objects().get(index) else {
        return false;
    };
    object.class_name == "hkbGetUpModifier"
        || object.members.iter().any(|member| {
            let mut refs = Vec::new();
            references(&member.value, &mut refs);
            refs.into_iter()
                .any(|index| reaches_getup(file, index, seen))
        })
}

fn references(value: &HkxValue, out: &mut Vec<usize>) {
    match value {
        HkxValue::Pointer(Some(index)) => out.push(*index),
        HkxValue::Array(values) => values.iter().for_each(|value| references(value, out)),
        HkxValue::Object(members) | HkxValue::TypedObject { members, .. } => members
            .iter()
            .for_each(|member| references(&member.value, out)),
        _ => {}
    }
}

fn convert_value(
    value: &HkxValue,
    name: &str,
    events: &[String],
    variables: &[String],
    pending: &mut Vec<usize>,
    used: &mut BTreeSet<usize>,
) -> Result<Value, String> {
    let integer = match value {
        HkxValue::I8(v) => Some(i64::from(*v)),
        HkxValue::U8(v) => Some(i64::from(*v)),
        HkxValue::I16(v) => Some(i64::from(*v)),
        HkxValue::U16(v) => Some(i64::from(*v)),
        HkxValue::I32(v) => Some(i64::from(*v)),
        HkxValue::U32(v) => Some(i64::from(*v)),
        HkxValue::I64(v) => Some(*v),
        HkxValue::U64(v) => {
            Some(i64::try_from(*v).map_err(|_| "source integer exceeds target range")?)
        }
        _ => None,
    };
    if let Some(value) = integer {
        if name == "bindingType" && value != 0 {
            return Err(
                "source recovery character-property binding requires explicit lowering".into(),
            );
        }
        if value >= 0 && matches!(name, "variableIndex" | "syncVariableIndex") {
            let name = variables
                .get(value as usize)
                .ok_or("source recovery variable index out of bounds")?;
            used.insert(value as usize);
            return Ok(Value::Variable(name.clone()));
        }
        if value >= 0 && (name == "id" || name == "eventId" || name.ends_with("EventId")) {
            return events
                .get(value as usize)
                .cloned()
                .map(Value::Event)
                .ok_or_else(|| format!("source recovery {name}={value} is outside event table"));
        }
        return Ok(Value::Int(value));
    }
    Ok(match value {
        HkxValue::Bool(v) => Value::Bool(*v),
        HkxValue::F32(v) | HkxValue::Half(v) => Value::Real(v.to_bits()),
        HkxValue::F32List(v) if name == "boneWeights" => {
            Value::Array(v.iter().map(|v| Value::Real(v.to_bits())).collect())
        }
        HkxValue::F32List(v) => Value::Vector(v.iter().map(|v| v.to_bits()).collect()),
        HkxValue::String { value, is_null } => Value::String(if *is_null {
            String::new()
        } else {
            value.clone()
        }),
        HkxValue::Pointer(index) => {
            pending.extend(index);
            Value::Reference(index.map(|v| v as u32))
        }
        HkxValue::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| convert_value(value, name, events, variables, pending, used))
                .collect::<Result<_, _>>()?,
        ),
        HkxValue::Object(members) | HkxValue::TypedObject { members, .. } => Value::Object(
            members
                .iter()
                .map(|member| {
                    Ok((
                        member.name.clone(),
                        convert_value(
                            &member.value,
                            &member.name,
                            events,
                            variables,
                            pending,
                            used,
                        )?,
                    ))
                })
                .collect::<Result<_, String>>()?,
        ),
        _ => {
            return Err(format!(
                "unsupported source recovery value for {name}: {value:?}"
            ));
        }
    })
}

pub(super) fn roles(
    family: &SkyrimFamilyMotionSet,
    inventory: &SkyrimCreatureFamilyInventory,
    clips: &[&SkyrimCatalogClip],
    roles: &mut Vec<CapabilityClipRole>,
    events: &mut BTreeMap<String, EventDecl>,
) -> Result<(), SkyrimCreatureMotionError> {
    if inventory.recovery_graphs.is_empty() {
        return Ok(());
    }
    if inventory.recovery_graphs.len() != 1 {
        return adapter_unavailable(
            family,
            "source recovery",
            "multiple recovery root graphs".into(),
        );
    }
    let mut graph = inventory.recovery_graphs[0].clone().map_err(|detail| {
        SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "source recovery",
            detail,
        }
    })?;
    fn lower(value: &mut Value, clips: &[&SkyrimCatalogClip]) -> Result<(), String> {
        match value {
            Value::Clip(path) => {
                *path = clips
                    .iter()
                    .find(|clip| path_key(&clip.clip_path) == path_key(path))
                    .ok_or_else(|| format!("recovery clip absent from family catalog: {path}"))?
                    .clip_id
                    .clone()
            }
            Value::Array(values) => {
                for value in values {
                    lower(value, clips)?;
                }
            }
            Value::Object(members) => {
                for value in members.values_mut() {
                    lower(value, clips)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    for node in graph.nodes.values_mut() {
        for value in node.members.values_mut() {
            lower(value, clips).map_err(|detail| {
                SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "source recovery",
                    detail,
                }
            })?;
        }
    }
    let Some(anchor) = graph.clip_names().first().map(|name| name.to_string()) else {
        return adapter_unavailable(
            family,
            "source recovery",
            "no source recovery animation".into(),
        );
    };
    let event = "GetUpBegin";
    if !graph.event_names().contains(&event) {
        return adapter_unavailable(
            family,
            "source recovery",
            "source graph does not consume GetUpBegin".into(),
        );
    }
    events.insert(
        event.to_ascii_lowercase(),
        EventDecl {
            name: event.into(),
            usage: EventUsage::Generic,
            flags: 0,
        },
    );
    roles.push(CapabilityClipRole {
        equipment_index: None,
        role: CreatureClipRole::GetUp,
        state_name: "SourceRecovery".into(),
        clip_name: anchor,
        generator: CapabilityRoleGenerator::SourceRecovery { graph },
        trigger_event: Some(event.into()),
        trigger_aliases: Vec::new(),
        motion: ClipMotionPolicy::default(),
    });
    Ok(())
}
