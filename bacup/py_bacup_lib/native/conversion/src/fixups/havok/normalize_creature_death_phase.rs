//! Adapt FO76 creature death branches to FO4's active-modifier contract.

use std::path::{Path, PathBuf};

use havok_native::hkx::types::HkxValue;
use havok_native::hkx::{HkxFile, HkxMember, HkxObject, read_packfile};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

const ANIMATION_DRIVEN: &str = "bAnimationDriven";
const DEATH_POSE_START: &str = "DeathPoseStart";
const RAGDOLL: &str = "Ragdoll";
const DEATH_MODIFIER_LIST: &str = "DeathAnimation_ML";
const DEATH_STATE_MACHINES: [&str; 2] = ["DeathAnimation_SM", "DeathAnimationRunning_SM"];

pub struct NormalizeCreatureDeathPhaseFixup;

impl Fixup for NormalizeCreatureDeathPhaseFixup {
    fn name(&self) -> &'static str {
        "normalize_creature_death_phase"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::AssetOnly
    }

    fn asset_phase_allowed(&self, phases: &AssetPhaseFlags) -> bool {
        phases.havok
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, config: &FixupConfig) -> bool {
        let source_game = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref());
        let target_game = session.target_slot().parsed.game.as_deref();
        config.mod_path.is_some() && source_game == Some("fo76") && target_game == Some("fo4")
    }

    fn run_with_session(
        &self,
        _session: &mut PluginSession,
        _mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let Some(mod_path) = config.mod_path.as_deref() else {
            return Ok(FixupReport::empty());
        };
        normalize_creature_death_phase_in_mod_path(mod_path)
    }
}

pub fn normalize_creature_death_phase_in_mod_path(
    mod_path: &Path,
) -> Result<FixupReport, FixupError> {
    let mut changed = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        walk_behavior_dirs(&meshes_root, &mut |behavior_dir| {
            for root_path in root_behavior_files(behavior_dir) {
                if normalize_creature_death_phase_file(&root_path).unwrap_or(false) {
                    changed += 1;
                }
            }
        });
    }
    Ok(FixupReport {
        records_changed: changed,
        ..FixupReport::empty()
    })
}

fn normalize_creature_death_phase_file(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let mut behavior = read_packfile(&std::fs::read(path)?)?;
    if !normalize_creature_death_phase(&mut behavior).map_err(std::io::Error::other)? {
        return Ok(false);
    }
    std::fs::write(path, behavior.save())?;
    Ok(true)
}

struct DeathPhaseLayout {
    strings: usize,
    state_machines: [usize; 2],
    modifier_list: usize,
    contact_listener: usize,
    active_modifier_template: usize,
    active_binding_template: usize,
    death_pose_event_id: usize,
    modifier_count: usize,
}

fn normalize_creature_death_phase(behavior: &mut HkxFile) -> Result<bool, String> {
    let Some(layout) = death_phase_layout(behavior) else {
        return Ok(false);
    };

    let mut active_binding = behavior.objects()[layout.active_binding_template].clone();
    active_binding.name = None;
    let active_binding_index = behavior.push_object(active_binding);

    let mut active_modifier = behavior.objects()[layout.active_modifier_template].clone();
    active_modifier.name = None;
    set_pointer_member(
        &mut active_modifier,
        "variableBindingSet",
        Some(active_binding_index),
    )?;
    set_string_member(&mut active_modifier, "name", "BSIsActiveModifier")?;
    set_integer_member(
        &mut active_modifier,
        "userData",
        i64::try_from(layout.modifier_count + 1).map_err(|error| error.to_string())?,
    )?;
    let active_modifier_index = behavior.push_object(active_modifier);

    let objects = behavior.objects_mut();
    array_member_mut(&mut objects[layout.strings], "eventNames")?.push(HkxValue::String {
        value: DEATH_POSE_START.to_string(),
        is_null: false,
    });

    for state_machine in layout.state_machines {
        set_pointer_member(&mut objects[state_machine], "variableBindingSet", None)?;
    }

    let contact_event = object_member_mut(&mut objects[layout.contact_listener], "contactEvent")?;
    set_inline_integer_member(
        contact_event,
        "id",
        i64::try_from(layout.death_pose_event_id).map_err(|error| error.to_string())?,
    )?;

    array_member_mut(&mut objects[layout.modifier_list], "modifiers")?
        .push(HkxValue::Pointer(Some(active_modifier_index)));
    Ok(true)
}

fn death_phase_layout(behavior: &HkxFile) -> Option<DeathPhaseLayout> {
    let data = unique_class_object(behavior, "hkbBehaviorGraphData")?;
    let strings = pointer_member(&behavior.objects()[data], "stringData")?;
    let string_data = behavior.objects().get(strings)?;
    if string_data.class_name != "hkbBehaviorGraphStringData" {
        return None;
    }

    let event_names = string_array(string_data, "eventNames")?;
    if string_index(&event_names, DEATH_POSE_START).is_some() {
        return None;
    }
    let variable_names = string_array(string_data, "variableNames")?;
    let animation_driven = string_index(&variable_names, ANIMATION_DRIVEN)?;

    let state_machines = [
        unique_named_object(behavior, "hkbStateMachine", DEATH_STATE_MACHINES[0])?,
        unique_named_object(behavior, "hkbStateMachine", DEATH_STATE_MACHINES[1])?,
    ];
    for state_machine in state_machines {
        let binding = pointer_member(&behavior.objects()[state_machine], "variableBindingSet")?;
        if !binding_set_targets(behavior, binding, "isActive", animation_driven) {
            return None;
        }
    }

    let modifier_list = unique_named_object(behavior, "hkbModifierList", DEATH_MODIFIER_LIST)?;
    let modifiers = pointer_array(&behavior.objects()[modifier_list], "modifiers")?;
    if modifiers.len() != 2 {
        return None;
    }
    let contact_listeners: Vec<usize> = modifiers
        .iter()
        .copied()
        .filter(|index| {
            behavior
                .objects()
                .get(*index)
                .is_some_and(|object| object.class_name == "BSRagdollContactListenerModifier")
        })
        .collect();
    if contact_listeners.len() != 1
        || !modifiers.iter().any(|index| {
            behavior
                .objects()
                .get(*index)
                .is_some_and(|object| object.class_name == "hkbKeyframeBonesModifier")
        })
    {
        return None;
    }
    let contact_listener = contact_listeners[0];
    let contact_event = object_member(&behavior.objects()[contact_listener], "contactEvent")?;
    let contact_event_id = inline_integer_member(contact_event, "id")?;
    let contact_event_id = usize::try_from(contact_event_id).ok()?;
    if event_names
        .get(contact_event_id)
        .is_none_or(|name| !name.eq_ignore_ascii_case(RAGDOLL))
    {
        return None;
    }

    let templates: Vec<(usize, usize)> = behavior
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, object)| object.class_name == "BSIsActiveModifier")
        .filter_map(|(modifier, object)| {
            let binding = pointer_member(object, "variableBindingSet")?;
            binding_set_targets(behavior, binding, "bIsActive0", animation_driven)
                .then_some((modifier, binding))
        })
        .collect();
    if templates.len() != 1 {
        return None;
    }

    Some(DeathPhaseLayout {
        strings,
        state_machines,
        modifier_list,
        contact_listener,
        active_modifier_template: templates[0].0,
        active_binding_template: templates[0].1,
        death_pose_event_id: event_names.len(),
        modifier_count: modifiers.len(),
    })
}

fn binding_set_targets(
    behavior: &HkxFile,
    binding_index: usize,
    member_path: &str,
    variable_index: usize,
) -> bool {
    let Some(binding_set) = behavior.objects().get(binding_index) else {
        return false;
    };
    if binding_set.class_name != "hkbVariableBindingSet" {
        return false;
    }
    let Some(bindings) = array_member(binding_set, "bindings") else {
        return false;
    };
    if bindings.len() != 1 {
        return false;
    }
    let Some(binding) = bindings[0].as_object_members() else {
        return false;
    };
    inline_string_member(binding, "memberPath")
        .is_some_and(|value| value.eq_ignore_ascii_case(member_path))
        && inline_integer_member(binding, "variableIndex") == i64::try_from(variable_index).ok()
        && inline_member(binding, "bindingType").is_some_and(is_variable_binding)
}

fn unique_class_object(behavior: &HkxFile, class_name: &str) -> Option<usize> {
    unique_object(behavior, |object| object.class_name == class_name)
}

fn unique_named_object(behavior: &HkxFile, class_name: &str, name: &str) -> Option<usize> {
    unique_object(behavior, |object| {
        object.class_name == class_name
            && string_member(object, "name").is_some_and(|value| value.eq_ignore_ascii_case(name))
    })
}

fn unique_object(behavior: &HkxFile, predicate: impl Fn(&HkxObject) -> bool) -> Option<usize> {
    let mut matches = behavior
        .objects()
        .iter()
        .enumerate()
        .filter_map(|(index, object)| predicate(object).then_some(index));
    let index = matches.next()?;
    matches.next().is_none().then_some(index)
}

fn pointer_member(object: &HkxObject, name: &str) -> Option<usize> {
    object
        .members
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| match member.value {
            HkxValue::Pointer(index) => index,
            _ => None,
        })
}

fn set_pointer_member(
    object: &mut HkxObject,
    name: &str,
    value: Option<usize>,
) -> Result<(), String> {
    let member = object
        .members
        .iter_mut()
        .find(|member| member.name == name)
        .ok_or_else(|| format!("missing pointer member {name}"))?;
    if !matches!(member.value, HkxValue::Pointer(_)) {
        return Err(format!("{name} is not a pointer"));
    }
    member.value = HkxValue::Pointer(value);
    Ok(())
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

fn array_member_mut<'a>(
    object: &'a mut HkxObject,
    name: &str,
) -> Result<&'a mut Vec<HkxValue>, String> {
    object
        .members
        .iter_mut()
        .find(|member| member.name == name)
        .and_then(|member| match &mut member.value {
            HkxValue::Array(values) => Some(values),
            _ => None,
        })
        .ok_or_else(|| format!("missing array member {name}"))
}

fn pointer_array(object: &HkxObject, name: &str) -> Option<Vec<usize>> {
    array_member(object, name)?
        .iter()
        .map(|value| match value {
            HkxValue::Pointer(Some(index)) => Some(*index),
            _ => None,
        })
        .collect()
}

fn string_array<'a>(object: &'a HkxObject, name: &str) -> Option<Vec<&'a str>> {
    array_member(object, name)?
        .iter()
        .map(|value| match value {
            HkxValue::String {
                value,
                is_null: false,
            } => Some(value.as_str()),
            _ => None,
        })
        .collect()
}

fn string_index(values: &[&str], expected: &str) -> Option<usize> {
    values
        .iter()
        .position(|value| value.eq_ignore_ascii_case(expected))
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

fn set_string_member(object: &mut HkxObject, name: &str, value: &str) -> Result<(), String> {
    let member = object
        .members
        .iter_mut()
        .find(|member| member.name == name)
        .ok_or_else(|| format!("missing string member {name}"))?;
    if !matches!(member.value, HkxValue::String { .. }) {
        return Err(format!("{name} is not a string"));
    }
    member.value = HkxValue::String {
        value: value.to_string(),
        is_null: false,
    };
    Ok(())
}

fn object_member<'a>(object: &'a HkxObject, name: &str) -> Option<&'a [HkxMember]> {
    object
        .members
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| member.value.as_object_members())
}

fn object_member_mut<'a>(
    object: &'a mut HkxObject,
    name: &str,
) -> Result<&'a mut Vec<HkxMember>, String> {
    object
        .members
        .iter_mut()
        .find(|member| member.name == name)
        .and_then(|member| member.value.as_object_members_mut())
        .ok_or_else(|| format!("missing object member {name}"))
}

fn inline_member<'a>(members: &'a [HkxMember], name: &str) -> Option<&'a HkxValue> {
    members
        .iter()
        .find(|member| member.name == name)
        .map(|member| &member.value)
}

fn inline_string_member<'a>(members: &'a [HkxMember], name: &str) -> Option<&'a str> {
    inline_member(members, name).and_then(|value| match value {
        HkxValue::String {
            value,
            is_null: false,
        } => Some(value.as_str()),
        _ => None,
    })
}

fn inline_integer_member(members: &[HkxMember], name: &str) -> Option<i64> {
    inline_member(members, name).and_then(integer)
}

fn set_inline_integer_member(
    members: &mut [HkxMember],
    name: &str,
    value: i64,
) -> Result<(), String> {
    let member = members
        .iter_mut()
        .find(|member| member.name == name)
        .ok_or_else(|| format!("missing integer member {name}"))?;
    set_integer(&mut member.value, value)
}

fn set_integer_member(object: &mut HkxObject, name: &str, value: i64) -> Result<(), String> {
    let member = object
        .members
        .iter_mut()
        .find(|member| member.name == name)
        .ok_or_else(|| format!("missing integer member {name}"))?;
    set_integer(&mut member.value, value)
}

fn is_variable_binding(value: &HkxValue) -> bool {
    integer(value) == Some(0)
        || matches!(
            value,
            HkxValue::String {
                value,
                is_null: false,
            } if value.eq_ignore_ascii_case("BINDING_TYPE_VARIABLE")
        )
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
        HkxValue::I8(_) => HkxValue::I8(i8::try_from(value).map_err(|error| error.to_string())?),
        HkxValue::U8(_) => HkxValue::U8(u8::try_from(value).map_err(|error| error.to_string())?),
        HkxValue::I16(_) => HkxValue::I16(i16::try_from(value).map_err(|error| error.to_string())?),
        HkxValue::U16(_) => HkxValue::U16(u16::try_from(value).map_err(|error| error.to_string())?),
        HkxValue::I32(_) => HkxValue::I32(i32::try_from(value).map_err(|error| error.to_string())?),
        HkxValue::U32(_) => HkxValue::U32(u32::try_from(value).map_err(|error| error.to_string())?),
        HkxValue::I64(_) => HkxValue::I64(value),
        HkxValue::U64(_) => HkxValue::U64(u64::try_from(value).map_err(|error| error.to_string())?),
        _ => return Err("value is not an integer".to_string()),
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

fn walk_behavior_dirs(dir: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("Behaviors"))
        {
            visit(&path);
        } else {
            walk_behavior_dirs(&path, visit);
        }
    }
}

fn root_behavior_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let is_root = path.is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.to_ascii_lowercase().ends_with("rootbehavior.hkx"));
            is_root.then_some(path)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn object(
        index: usize,
        class_name: &str,
        signature: u32,
        members: Vec<HkxMember>,
    ) -> HkxObject {
        HkxObject {
            name: Some(format!("#{index:04}")),
            offset: 0,
            signature,
            class_name: class_name.to_string(),
            members,
        }
    }

    fn binding(member_path: &str, variable_index: i32) -> HkxValue {
        HkxValue::Object(vec![
            member("memberPath", string(member_path)),
            member("variableIndex", HkxValue::I32(variable_index)),
            member("bitIndex", HkxValue::I8(-1)),
            member("bindingType", HkxValue::I32(0)),
        ])
    }

    fn binding_set(index: usize, member_path: &str) -> HkxObject {
        object(
            index,
            "hkbVariableBindingSet",
            0xe942_f339,
            vec![
                member("bindings", HkxValue::Array(vec![binding(member_path, 1)])),
                member("indexOfBindingToEnable", HkxValue::I32(-1)),
            ],
        )
    }

    fn state_machine(index: usize, name: &str, binding: usize) -> HkxObject {
        object(
            index,
            "hkbStateMachine",
            0xa589_6bcf,
            vec![
                member("variableBindingSet", HkxValue::Pointer(Some(binding))),
                member("name", string(name)),
            ],
        )
    }

    fn behavior_fixture() -> HkxFile {
        HkxFile::from_tagxml(
            11,
            "hk_2014.1.0-r1",
            vec![
                object(
                    1,
                    "hkbBehaviorGraphStringData",
                    0xc713_064e,
                    vec![
                        member("eventNames", HkxValue::Array(vec![string(RAGDOLL)])),
                        member(
                            "variableNames",
                            HkxValue::Array(vec![
                                string("selectedGeneratorIndex"),
                                string(ANIMATION_DRIVEN),
                            ]),
                        ),
                    ],
                ),
                object(
                    2,
                    "hkbBehaviorGraphData",
                    0x95ac_b079,
                    vec![
                        member("stringData", HkxValue::Pointer(Some(0))),
                        member(
                            "eventInfos",
                            HkxValue::Array(vec![HkxValue::Object(vec![member(
                                "flags",
                                HkxValue::I32(0),
                            )])]),
                        ),
                    ],
                ),
                state_machine(3, DEATH_STATE_MACHINES[0], 3),
                binding_set(4, "isActive"),
                state_machine(5, DEATH_STATE_MACHINES[1], 5),
                binding_set(6, "isActive"),
                object(
                    7,
                    "hkbModifierList",
                    0x0ded_564c,
                    vec![
                        member("name", string(DEATH_MODIFIER_LIST)),
                        member(
                            "modifiers",
                            HkxValue::Array(vec![
                                HkxValue::Pointer(Some(7)),
                                HkxValue::Pointer(Some(8)),
                            ]),
                        ),
                    ],
                ),
                object(8, "hkbKeyframeBonesModifier", 0x4e45_d0f7, vec![]),
                object(
                    9,
                    "BSRagdollContactListenerModifier",
                    0x7e6d_66c9,
                    vec![member(
                        "contactEvent",
                        HkxValue::Object(vec![
                            member("id", HkxValue::I32(0)),
                            member("payload", HkxValue::Pointer(None)),
                        ]),
                    )],
                ),
                object(
                    10,
                    "BSIsActiveModifier",
                    0x5bf1_f5cf,
                    vec![
                        member("variableBindingSet", HkxValue::Pointer(Some(10))),
                        member("userData", HkxValue::U64(2)),
                        member("name", string("IsActive_AnimDriven")),
                        member("enable", HkxValue::Bool(true)),
                        member("bIsActive0", HkxValue::Bool(false)),
                        member("bInvertActive0", HkxValue::Bool(false)),
                        member("bIsActive1", HkxValue::Bool(false)),
                        member("bInvertActive1", HkxValue::Bool(false)),
                        member("bIsActive2", HkxValue::Bool(false)),
                        member("bInvertActive2", HkxValue::Bool(false)),
                        member("bIsActive3", HkxValue::Bool(false)),
                        member("bInvertActive3", HkxValue::Bool(false)),
                        member("bIsActive4", HkxValue::Bool(false)),
                        member("bInvertActive4", HkxValue::Bool(false)),
                    ],
                ),
                binding_set(11, "bIsActive0"),
            ],
        )
    }

    fn event_names(behavior: &HkxFile) -> Vec<&str> {
        string_array(&behavior.objects()[0], "eventNames").unwrap()
    }

    fn modifier_indices(behavior: &HkxFile) -> Vec<usize> {
        pointer_array(&behavior.objects()[6], "modifiers").unwrap()
    }

    #[test]
    fn replaces_fo76_death_gates_with_fo4_active_modifier() {
        let mut behavior = behavior_fixture();

        assert!(normalize_creature_death_phase(&mut behavior).unwrap());

        assert_eq!(event_names(&behavior), vec![RAGDOLL, DEATH_POSE_START]);
        assert_eq!(
            array_member(&behavior.objects()[1], "eventInfos")
                .unwrap()
                .len(),
            1,
            "DeathPoseStart is an emitted terminal event, not a new input contract"
        );
        for state_machine in [2, 4] {
            assert!(matches!(
                behavior.objects()[state_machine]
                    .members
                    .iter()
                    .find(|member| member.name == "variableBindingSet")
                    .unwrap()
                    .value,
                HkxValue::Pointer(None)
            ));
        }
        let contact_event = object_member(&behavior.objects()[8], "contactEvent").unwrap();
        assert_eq!(inline_integer_member(contact_event, "id"), Some(1));

        let modifiers = modifier_indices(&behavior);
        assert_eq!(modifiers.len(), 3);
        let active_modifier = &behavior.objects()[modifiers[2]];
        assert_eq!(active_modifier.class_name, "BSIsActiveModifier");
        assert_eq!(
            string_member(active_modifier, "name"),
            Some("BSIsActiveModifier")
        );
        assert_eq!(
            integer(
                &active_modifier
                    .members
                    .iter()
                    .find(|member| member.name == "userData")
                    .unwrap()
                    .value
            ),
            Some(3)
        );
        let active_binding = pointer_member(active_modifier, "variableBindingSet").unwrap();
        assert!(binding_set_targets(
            &behavior,
            active_binding,
            "bIsActive0",
            1
        ));
    }

    #[test]
    fn repaired_graph_is_idempotent() {
        let mut behavior = behavior_fixture();
        assert!(normalize_creature_death_phase(&mut behavior).unwrap());
        let object_count = behavior.objects().len();

        assert!(!normalize_creature_death_phase(&mut behavior).unwrap());
        assert_eq!(behavior.objects().len(), object_count);
        assert_eq!(modifier_indices(&behavior).len(), 3);
    }

    #[test]
    fn near_match_without_both_death_state_gates_is_unchanged() {
        let mut behavior = behavior_fixture();
        set_pointer_member(&mut behavior.objects_mut()[4], "variableBindingSet", None).unwrap();
        let before = behavior.save();

        assert!(!normalize_creature_death_phase(&mut behavior).unwrap());
        assert_eq!(behavior.save(), before);
    }

    #[test]
    fn repaired_graph_round_trips_through_packfile() {
        let mut behavior = behavior_fixture();
        assert!(normalize_creature_death_phase(&mut behavior).unwrap());

        let decoded = read_packfile(&behavior.save()).unwrap();
        assert_eq!(event_names(&decoded), vec![RAGDOLL, DEATH_POSE_START]);
        assert_eq!(modifier_indices(&decoded).len(), 3);
    }

    #[test]
    fn mod_tree_patches_root_once() {
        let temp = tempfile::tempdir().unwrap();
        let behavior_dir = temp
            .path()
            .join("data/Meshes/Actors/Snallygaster/Behaviors");
        std::fs::create_dir_all(&behavior_dir).unwrap();
        let path = behavior_dir.join("SnallygasterRootBehavior.hkx");
        std::fs::write(&path, behavior_fixture().save()).unwrap();

        let first = normalize_creature_death_phase_in_mod_path(temp.path()).unwrap();
        let second = normalize_creature_death_phase_in_mod_path(temp.path()).unwrap();

        assert_eq!(first.records_changed, 1);
        assert!(second.is_no_op());
        let decoded = read_packfile(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(event_names(&decoded), vec![RAGDOLL, DEATH_POSE_START]);
    }
}
