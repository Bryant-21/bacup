use std::path::{Path, PathBuf};

use havok_native::hkx::types::HkxValue;
use havok_native::hkx::{HkxFile, HkxObject, read_packfile};

use crate::fixups::{FixupError, FixupReport};

const ENTER_FULLY_RAGDOLL_EVENT: &str = "EnterFullyRagdoll";
const BEE_SWARM_DEATH_EVENT: &str = "beeSwarmDeath";
const BEE_SWARM_DEATH_STATE: &str = "SwarmFXDeathState";

pub fn wire_honeybeast_swarm_death_in_mod_path(mod_path: &Path) -> Result<FixupReport, FixupError> {
    let mut changed = 0;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        let Some(path) = honeybeast_swarm_behavior_path(&meshes_root) else {
            continue;
        };
        if wire_honeybeast_swarm_death_file(&path)
            .map_err(|error| FixupError::Other(format!("{}: {error}", path.display())))?
        {
            changed += 1;
        }
    }
    Ok(FixupReport {
        records_changed: changed,
        ..FixupReport::empty()
    })
}

fn wire_honeybeast_swarm_death_file(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let mut behavior = read_packfile(&std::fs::read(path)?)?;
    if !wire_honeybeast_swarm_death_transition(&mut behavior)? {
        return Ok(false);
    }
    std::fs::write(path, behavior.save())?;
    Ok(true)
}

fn wire_honeybeast_swarm_death_transition(behavior: &mut HkxFile) -> Result<bool, String> {
    let enter_ragdoll_event = event_id(behavior, ENTER_FULLY_RAGDOLL_EVENT)
        .ok_or_else(|| format!("missing {ENTER_FULLY_RAGDOLL_EVENT} event"))?;
    let bee_swarm_death_event = event_id(behavior, BEE_SWARM_DEATH_EVENT)
        .ok_or_else(|| format!("missing {BEE_SWARM_DEATH_EVENT} event"))?;
    let death_state = state_id(behavior, BEE_SWARM_DEATH_STATE)
        .ok_or_else(|| format!("missing {BEE_SWARM_DEATH_STATE} state"))?;

    for object in behavior.objects_mut() {
        if object.class_name != "hkbStateMachineTransitionInfoArray" {
            continue;
        }
        let Some(transitions) = array_member_mut(object, "transitions") else {
            continue;
        };
        if transitions
            .iter()
            .any(|transition| transition_targets(transition, enter_ragdoll_event, death_state))
        {
            return Ok(false);
        }
        let Some(mut transition) = transitions
            .iter()
            .find(|transition| transition_targets(transition, bee_swarm_death_event, death_state))
            .cloned()
        else {
            continue;
        };
        let event_id = inline_member_mut(&mut transition, "eventId")
            .ok_or_else(|| "bee swarm death transition has no eventId".to_string())?;
        set_integer(event_id, enter_ragdoll_event)?;
        transitions.push(transition);
        return Ok(true);
    }

    Err("missing bee swarm death transition".to_string())
}

fn event_id(behavior: &HkxFile, expected: &str) -> Option<i64> {
    behavior
        .objects()
        .iter()
        .find(|object| object.class_name == "hkbBehaviorGraphStringData")
        .and_then(|object| array_member(object, "eventNames"))
        .and_then(|events| {
            events.iter().position(|event| {
                matches!(
                    event,
                    HkxValue::String {
                        value,
                        is_null: false,
                    } if value.eq_ignore_ascii_case(expected)
                )
            })
        })
        .map(|index| index as i64)
}

fn state_id(behavior: &HkxFile, expected: &str) -> Option<i64> {
    behavior.objects().iter().find_map(|object| {
        if object.class_name != "hkbStateMachineStateInfo"
            || string_member(object, "name").is_none_or(|name| !name.eq_ignore_ascii_case(expected))
        {
            return None;
        }
        integer_member(object, "stateId")
    })
}

fn transition_targets(transition: &HkxValue, event_id: i64, state_id: i64) -> bool {
    inline_integer_member(transition, "eventId") == Some(event_id)
        && inline_integer_member(transition, "toStateId") == Some(state_id)
}

fn array_member<'a>(object: &'a HkxObject, name: &str) -> Option<&'a Vec<HkxValue>> {
    object
        .members
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| match &member.value {
            HkxValue::Array(values) => Some(values),
            _ => None,
        })
}

fn array_member_mut<'a>(object: &'a mut HkxObject, name: &str) -> Option<&'a mut Vec<HkxValue>> {
    object
        .members
        .iter_mut()
        .find(|member| member.name == name)
        .and_then(|member| match &mut member.value {
            HkxValue::Array(values) => Some(values),
            _ => None,
        })
}

fn string_member<'a>(object: &'a HkxObject, name: &str) -> Option<&'a str> {
    object
        .members
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| match &member.value {
            HkxValue::String {
                value,
                is_null: false,
            } => Some(value.as_str()),
            _ => None,
        })
}

fn integer_member(object: &HkxObject, name: &str) -> Option<i64> {
    object
        .members
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| integer(&member.value))
}

fn inline_integer_member(value: &HkxValue, name: &str) -> Option<i64> {
    value
        .as_object_members()?
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| integer(&member.value))
}

fn inline_member_mut<'a>(value: &'a mut HkxValue, name: &str) -> Option<&'a mut HkxValue> {
    value
        .as_object_members_mut()?
        .iter_mut()
        .find(|member| member.name == name)
        .map(|member| &mut member.value)
}

fn integer(value: &HkxValue) -> Option<i64> {
    match value {
        HkxValue::I8(value) => Some(i64::from(*value)),
        HkxValue::U8(value) => Some(i64::from(*value)),
        HkxValue::I16(value) => Some(i64::from(*value)),
        HkxValue::U16(value) => Some(i64::from(*value)),
        HkxValue::I32(value) => Some(i64::from(*value)),
        HkxValue::U32(value) => Some(i64::from(*value)),
        HkxValue::I64(value) => Some(*value),
        HkxValue::U64(value) => i64::try_from(*value).ok(),
        _ => None,
    }
}

fn set_integer(target: &mut HkxValue, value: i64) -> Result<(), String> {
    *target = match target {
        HkxValue::I8(_) => HkxValue::I8(i8::try_from(value).map_err(|e| e.to_string())?),
        HkxValue::U8(_) => HkxValue::U8(u8::try_from(value).map_err(|e| e.to_string())?),
        HkxValue::I16(_) => HkxValue::I16(i16::try_from(value).map_err(|e| e.to_string())?),
        HkxValue::U16(_) => HkxValue::U16(u16::try_from(value).map_err(|e| e.to_string())?),
        HkxValue::I32(_) => HkxValue::I32(i32::try_from(value).map_err(|e| e.to_string())?),
        HkxValue::U32(_) => HkxValue::U32(u32::try_from(value).map_err(|e| e.to_string())?),
        HkxValue::I64(_) => HkxValue::I64(value),
        HkxValue::U64(_) => HkxValue::U64(u64::try_from(value).map_err(|e| e.to_string())?),
        _ => return Err("transition eventId is not an integer".to_string()),
    };
    Ok(())
}

fn mesh_roots_for_mod_path(mod_path: &Path) -> Vec<PathBuf> {
    [
        mod_path.join("data").join("Meshes"),
        mod_path.join("meshes"),
    ]
    .into_iter()
    .filter(|path| path.is_dir())
    .collect()
}

fn honeybeast_swarm_behavior_path(meshes_root: &Path) -> Option<PathBuf> {
    ["UniqueBehaviors", "HoneyBeastSwarmFX", "Behaviors"]
        .into_iter()
        .try_fold(meshes_root.to_path_buf(), |path, component| {
            directory_named(&path, component)
        })
        .and_then(|path| file_named(&path, "Behavior.hkx"))
}

fn directory_named(path: &Path, expected: &str) -> Option<PathBuf> {
    std::fs::read_dir(path).ok()?.flatten().find_map(|entry| {
        let candidate = entry.path();
        (candidate.is_dir()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(expected)))
        .then_some(candidate)
    })
}

fn file_named(path: &Path, expected: &str) -> Option<PathBuf> {
    std::fs::read_dir(path).ok()?.flatten().find_map(|entry| {
        let candidate = entry.path();
        (candidate.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(expected)))
        .then_some(candidate)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use havok_native::hkx::HkxMember;

    fn member(name: &str, value: HkxValue) -> HkxMember {
        HkxMember {
            name: name.to_string(),
            value,
        }
    }

    fn string(value: &str) -> HkxValue {
        HkxValue::String {
            value: value.to_string(),
            is_null: false,
        }
    }

    fn transition(event_id: i32, to_state_id: i32) -> HkxValue {
        HkxValue::Object(vec![
            member("eventId", HkxValue::I32(event_id)),
            member("toStateId", HkxValue::I32(to_state_id)),
        ])
    }

    fn behavior_fixture() -> HkxFile {
        HkxFile::from_tagxml(
            11,
            "hk_2014.1.0-r1",
            vec![
                HkxObject {
                    name: Some("#0001".to_string()),
                    offset: 0,
                    signature: 0,
                    class_name: "hkbBehaviorGraphStringData".to_string(),
                    members: vec![member(
                        "eventNames",
                        HkxValue::Array(vec![
                            string(ENTER_FULLY_RAGDOLL_EVENT),
                            string(BEE_SWARM_DEATH_EVENT),
                        ]),
                    )],
                },
                HkxObject {
                    name: Some("#0002".to_string()),
                    offset: 1,
                    signature: 0,
                    class_name: "hkbStateMachineStateInfo".to_string(),
                    members: vec![
                        member("name", string(BEE_SWARM_DEATH_STATE)),
                        member("stateId", HkxValue::I32(4)),
                    ],
                },
                HkxObject {
                    name: Some("#0003".to_string()),
                    offset: 2,
                    signature: 0,
                    class_name: "hkbStateMachineTransitionInfoArray".to_string(),
                    members: vec![member(
                        "transitions",
                        HkxValue::Array(vec![transition(1, 4)]),
                    )],
                },
            ],
        )
    }

    #[test]
    fn clones_fo76_death_transition_for_fo4_ragdoll_event() {
        let mut behavior = behavior_fixture();

        assert!(wire_honeybeast_swarm_death_transition(&mut behavior).unwrap());

        let transitions = behavior
            .objects()
            .iter()
            .find(|object| object.class_name == "hkbStateMachineTransitionInfoArray")
            .and_then(|object| array_member(object, "transitions"))
            .unwrap();
        assert_eq!(transitions.len(), 2);
        assert!(
            transitions
                .iter()
                .any(|value| transition_targets(value, 0, 4))
        );
    }

    #[test]
    fn existing_fo4_ragdoll_transition_is_unchanged() {
        let mut behavior = behavior_fixture();

        assert!(wire_honeybeast_swarm_death_transition(&mut behavior).unwrap());
        assert!(!wire_honeybeast_swarm_death_transition(&mut behavior).unwrap());
    }

    #[test]
    fn patches_converted_asset_path_once() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp
            .path()
            .join("data/Meshes/UniqueBehaviors/HoneyBeastSwarmFX/Behaviors/Behavior.hkx");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, behavior_fixture().save()).unwrap();

        let first = wire_honeybeast_swarm_death_in_mod_path(temp.path()).unwrap();
        let second = wire_honeybeast_swarm_death_in_mod_path(temp.path()).unwrap();

        assert_eq!(first.records_changed, 1);
        assert!(second.is_no_op());
        let behavior = read_packfile(&std::fs::read(path).unwrap()).unwrap();
        assert!(
            behavior
                .objects()
                .iter()
                .find(|object| object.class_name == "hkbStateMachineTransitionInfoArray")
                .and_then(|object| array_member(object, "transitions"))
                .unwrap()
                .iter()
                .any(|transition| transition_targets(transition, 0, 4))
        );
    }
}
