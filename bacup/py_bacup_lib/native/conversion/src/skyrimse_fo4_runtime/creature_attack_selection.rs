use super::*;

pub(super) fn event_references(file: &HkxFile, event_names: &[String]) -> BTreeSet<String> {
    fn value(
        atom: &HkxValue,
        event_context: bool,
        names: &[String],
        references: &mut BTreeSet<String>,
    ) {
        match atom {
            HkxValue::Array(values) => {
                for child in values {
                    value(child, event_context, names, references);
                }
            }
            HkxValue::Object(members) | HkxValue::TypedObject { members, .. } => {
                for member in members {
                    if member.name.eq_ignore_ascii_case("eventNames") {
                        continue;
                    }
                    value(
                        &member.value,
                        event_context || member.name.to_ascii_lowercase().contains("event"),
                        names,
                        references,
                    );
                }
            }
            HkxValue::String { value, .. } => {
                for name in names {
                    if !name.is_empty()
                        && value
                            .to_ascii_lowercase()
                            .contains(&name.to_ascii_lowercase())
                    {
                        references.insert(name.to_ascii_lowercase());
                    }
                }
            }
            value if event_context => {
                let index = match value {
                    HkxValue::I8(v) => Some(i64::from(*v)),
                    HkxValue::U8(v) => Some(i64::from(*v)),
                    HkxValue::I16(v) => Some(i64::from(*v)),
                    HkxValue::U16(v) => Some(i64::from(*v)),
                    HkxValue::I32(v) => Some(i64::from(*v)),
                    HkxValue::U32(v) => Some(i64::from(*v)),
                    HkxValue::I64(v) => Some(*v),
                    _ => None,
                };
                if let Some(name) = index
                    .and_then(|index| usize::try_from(index).ok())
                    .and_then(|index| names.get(index))
                {
                    references.insert(name.to_ascii_lowercase());
                }
            }
            _ => {}
        }
    }
    let mut references = BTreeSet::new();
    for object in file.objects() {
        let context = object.class_name.to_ascii_lowercase().contains("event");
        for member in &object.members {
            if member.name.eq_ignore_ascii_case("eventNames") {
                continue;
            }
            value(
                &member.value,
                context || member.name.to_ascii_lowercase().contains("event"),
                event_names,
                &mut references,
            );
        }
    }
    references
}

pub(super) fn exact_clip_bindings(
    inventory: Option<&SkyrimCreatureFamilyInventory>,
    clips: &[&SkyrimCatalogClip],
    bindings: &[CapabilityCandidateAttackBinding],
) -> BTreeMap<String, Vec<String>> {
    let Some(inventory) = inventory else {
        return BTreeMap::new();
    };
    let mut result = BTreeMap::<String, Vec<String>>::new();
    let events = bindings
        .iter()
        .map(|binding| (binding.event.clone(), binding.role))
        .collect::<BTreeSet<_>>();
    for (event, role) in events {
        let mut paths = BTreeSet::new();
        for group in &inventory.behavior_groups {
            let SkyrimBehaviorGeneratorGroupEvidence::StateMachine(group) = group else {
                continue;
            };
            for (state, nested) in destinations(&group.machine, &event) {
                if let Some(generator) = &state.generator {
                    selected_paths_at(generator, &event, nested, &mut BTreeSet::new(), &mut paths);
                }
            }
        }
        let matches = clips.iter().filter(|clip| paths.contains(&path_key(&clip.clip_path)))
            .filter(|clip| matches!(&clip.disposition, SkyrimClipDisposition::Role{role:source_role,..} if motion_attack_role(*source_role)==Some(role)))
            .collect::<Vec<_>>();
        if let [clip] = matches.as_slice() {
            result.entry(clip.clip_id.clone()).or_default().push(event);
        }
    }
    result
}

fn destinations<'a>(
    machine: &'a SkyrimBehaviorStateMachineEvidence,
    event: &str,
) -> Vec<(&'a SkyrimBehaviorStateEvidence, Option<i32>)> {
    let ids = machine
        .states
        .iter()
        .filter_map(|state| state.transitions.as_ref())
        .chain(machine.wildcard_transitions.as_ref())
        .flat_map(|array| &array.transitions)
        .filter(|transition| !transition.flags.is_some_and(|flags| flags & 32 != 0))
        .filter(|transition| {
            transition
                .event_name
                .as_ref()
                .is_some_and(|name| name.eq_ignore_ascii_case(event))
        })
        .filter_map(|transition| {
            transition.to_state_id.map(|id| {
                (
                    id,
                    if transition.flags.is_some_and(|flags| flags & 8192 != 0) {
                        transition.to_nested_state_id
                    } else {
                        None
                    },
                )
            })
        })
        .collect::<BTreeSet<_>>();
    machine
        .states
        .iter()
        .filter(|state| state.enable != Some(false))
        .flat_map(|state| {
            ids.iter()
                .filter(move |(id, _)| state.state_id == Some(*id))
                .map(move |(_, nested)| (state, *nested))
        })
        .collect()
}

#[cfg(test)]
fn selected_paths(
    node: &SkyrimBehaviorGeneratorNodeEvidence,
    event: &str,
    visiting: &mut BTreeSet<usize>,
    paths: &mut BTreeSet<String>,
) {
    selected_paths_at(node, event, None, visiting, paths);
}

fn selected_paths_at(
    node: &SkyrimBehaviorGeneratorNodeEvidence,
    event: &str,
    nested: Option<i32>,
    visiting: &mut BTreeSet<usize>,
    paths: &mut BTreeSet<String>,
) {
    match node {
        SkyrimBehaviorGeneratorNodeEvidence::Clip {
            resolved_clip_path: Some(path),
            ..
        } => {
            paths.insert(path_key(path));
        }
        SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine) => {
            if !visiting.insert(machine.source.object_index) {
                return;
            }
            let mut states = if let Some(id) = nested {
                machine
                    .states
                    .iter()
                    .filter(|state| state.enable != Some(false) && state.state_id == Some(id))
                    .map(|state| (state, None))
                    .collect()
            } else {
                destinations(machine, event)
            };
            if states.is_empty() && nested.is_none() {
                let dynamic = machine.start_state_mode == Some(1)
                    || machine.start_state_id_selector.is_some()
                    || machine
                        .variable_bindings
                        .iter()
                        .any(|binding| binding.member_path == "startStateId");
                states = machine
                    .states
                    .iter()
                    .filter(|state| state.enable != Some(false))
                    .filter(|state| dynamic || state.state_id == machine.start_state_id)
                    .map(|state| (state, None))
                    .collect();
            }
            for (state, nested) in states {
                if let Some(generator) = &state.generator {
                    selected_paths_at(generator, event, nested, visiting, paths);
                }
            }
            visiting.remove(&machine.source.object_index);
        }
        SkyrimBehaviorGeneratorNodeEvidence::ModifierGenerator {
            generator: Some(node),
            ..
        } => selected_paths_at(node, event, nested, visiting, paths),
        SkyrimBehaviorGeneratorNodeEvidence::CyclicBlendTransition(node) => {
            if let Some(node) = &node.generator {
                selected_paths_at(node, event, nested, visiting, paths);
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::Blender { children, .. } => {
            for child in children {
                selected_paths_at(&child.generator, event, nested, visiting, paths);
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::ManualSelector { children, .. } => {
            for child in children {
                selected_paths_at(child, event, nested, visiting, paths);
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::Unsupported { child_members, .. } => {
            for node in child_members.iter().flat_map(|member| &member.children) {
                selected_paths_at(node, event, nested, visiting, paths);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_events_are_not_treated_as_source_executable_references() {
        let names: Vec<String> = vec![
            "unusedAttack".into(),
            "nestedEvent".into(),
            "modifierEvent".into(),
        ];
        let file = HkxFile::from_tagxml(
            8,
            SKYRIM_CONTENTS_VERSION,
            vec![
                HkxObject {
                    name: None,
                    offset: 0,
                    signature: 0,
                    class_name: "hkbBehaviorGraphStringData".into(),
                    members: vec![HkxMember {
                        name: "eventNames".into(),
                        value: HkxValue::Array(
                            names
                                .iter()
                                .map(|name| HkxValue::String {
                                    value: name.clone(),
                                    is_null: false,
                                })
                                .collect(),
                        ),
                    }],
                },
                HkxObject {
                    name: None,
                    offset: 0,
                    signature: 0,
                    class_name: "hkbClipTriggerArray".into(),
                    members: vec![HkxMember {
                        name: "triggers".into(),
                        value: HkxValue::Array(vec![HkxValue::Object(vec![HkxMember {
                            name: "event".into(),
                            value: HkxValue::Object(vec![HkxMember {
                                name: "id".into(),
                                value: HkxValue::I32(1),
                            }]),
                        }])]),
                    }],
                },
                HkxObject {
                    name: None,
                    offset: 0,
                    signature: 0,
                    class_name: "hkbEventDrivenModifier".into(),
                    members: vec![HkxMember {
                        name: "activateEventId".into(),
                        value: HkxValue::I32(2),
                    }],
                },
            ],
        );
        assert_eq!(
            event_references(&file, &names),
            BTreeSet::from(["nestedevent".into(), "modifierevent".into()])
        );
    }

    fn machine() -> SkyrimBehaviorStateMachineEvidence {
        let member = |name: &str, value| HkxMember {
            name: name.into(),
            value,
        };
        let object = |class_name: &str, members| HkxObject {
            name: None,
            offset: 0,
            signature: 0,
            class_name: class_name.into(),
            members,
        };
        let string = |value: &str| HkxValue::String {
            value: value.into(),
            is_null: false,
        };
        let state = |id, generator| {
            object(
                "hkbStateMachineStateInfo",
                vec![
                    member("stateId", HkxValue::I32(id)),
                    member("generator", HkxValue::Pointer(Some(generator))),
                    member("enable", HkxValue::Bool(true)),
                ],
            )
        };
        let transition = |event_id, to_state, flags| {
            HkxValue::Object(vec![
                member("eventId", HkxValue::I32(event_id)),
                member("toStateId", HkxValue::I32(to_state)),
                member("flags", HkxValue::I16(flags)),
            ])
        };
        let file = HkxFile::from_tagxml(
            8,
            SKYRIM_CONTENTS_VERSION,
            vec![
                object(
                    "hkbStateMachine",
                    vec![
                        member("startStateId", HkxValue::I32(0)),
                        member("startStateMode", HkxValue::I32(0)),
                        member(
                            "states",
                            HkxValue::Array(vec![
                                HkxValue::Pointer(Some(1)),
                                HkxValue::Pointer(Some(2)),
                            ]),
                        ),
                        member("wildcardTransitions", HkxValue::Pointer(Some(5))),
                    ],
                ),
                state(0, 3),
                state(1, 4),
                object(
                    "hkbClipGenerator",
                    vec![member("animationName", string("Idle.hkx"))],
                ),
                object(
                    "hkbClipGenerator",
                    vec![member("animationName", string("AttackLeft.hkx"))],
                ),
                object(
                    "hkbStateMachineTransitionInfoArray",
                    vec![member(
                        "transitions",
                        HkxValue::Array(vec![transition(0, 1, 0), transition(1, 0, 32)]),
                    )],
                ),
            ],
        );
        let SkyrimBehaviorGeneratorNodeEvidence::StateMachine(mut machine) =
            behavior_generator_tree(
                &file,
                0,
                &["attackStartRight".into(), "disabledAttack".into()],
                &[],
                &[],
                &[],
                &mut BTreeSet::new(),
            )
        else {
            panic!("expected fixture machine")
        };
        for state in &mut machine.states {
            let Some(SkyrimBehaviorGeneratorNodeEvidence::Clip {
                animation_name,
                resolved_clip_path,
                ..
            }) = &mut state.generator
            else {
                panic!("expected fixture clip")
            };
            *resolved_clip_path = Some(animation_name.clone());
        }
        machine
    }

    #[test]
    fn exact_transition_uses_destination_instead_of_event_filename_similarity() {
        let machine = machine();
        let mut paths = BTreeSet::new();
        for (state, nested) in destinations(&machine, "attackStartRight") {
            selected_paths_at(
                state.generator.as_ref().unwrap(),
                "attackStartRight",
                nested,
                &mut BTreeSet::new(),
                &mut paths,
            );
        }
        assert_eq!(paths, BTreeSet::from(["attackleft.hkx".into()]));
        assert!(destinations(&machine, "disabledAttack").is_empty());
    }

    #[test]
    fn nested_entry_preserves_default_and_random_source_selection() {
        let mut machine = machine();
        let mut paths = BTreeSet::new();
        selected_paths(
            &SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine.clone()),
            "outerEntry",
            &mut BTreeSet::new(),
            &mut paths,
        );
        assert_eq!(paths, BTreeSet::from(["idle.hkx".into()]));
        paths.clear();
        selected_paths_at(
            &SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine.clone()),
            "outerEntry",
            Some(1),
            &mut BTreeSet::new(),
            &mut paths,
        );
        assert_eq!(
            paths,
            BTreeSet::from(["attackleft.hkx".into()]),
            "explicit nested destination overrides the default state"
        );
        machine.start_state_mode = Some(1);
        paths.clear();
        selected_paths(
            &SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine),
            "outerEntry",
            &mut BTreeSet::new(),
            &mut paths,
        );
        assert_eq!(
            paths,
            BTreeSet::from(["idle.hkx".into(), "attackleft.hkx".into()])
        );
    }
}
