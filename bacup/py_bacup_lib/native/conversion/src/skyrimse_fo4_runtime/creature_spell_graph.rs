use super::*;

pub(super) fn ordinary_casts(
    family: &SkyrimFamilyMotionSet,
    inventory: &SkyrimCreatureFamilyInventory,
    clips: &[&SkyrimCatalogClip],
    roles: &mut Vec<CapabilityClipRole>,
    events: &mut BTreeMap<String, EventDecl>,
) -> Result<(), SkyrimCreatureMotionError> {
    let source_names: &[&str] = match family.family_id.as_str() {
        "hagraven" => &[
            "FFCastState_MG",
            "ConCastState_MG",
            "FFSelfCastState_MG",
            "ConCastSelfState_MG",
        ],
        "atronach_flame" => &["FF_MG", "Concentration_MG"],
        "draugr" => &["ModifierGenerator00"],
        _ => return Ok(()),
    };
    let mut replacements = Vec::new();
    for name in source_names {
        let mut owners = BTreeMap::new();
        for group in &inventory.behavior_groups {
            let SkyrimBehaviorGeneratorGroupEvidence::StateMachine(group) = group else {
                continue;
            };
            for state in &group.machine.states {
                let Some(node) = state.generator.as_ref() else {
                    continue;
                };
                if source(node).and_then(|source| source.name.as_deref()) == Some(*name) {
                    owners.insert(
                        (group.behavior_path.clone(), state.source.object_index),
                        (&group.machine, state, node),
                    );
                }
            }
        }
        if owners.len() != 1 {
            return adapter_unavailable(
                family,
                "ordinary casting",
                format!("source generator {name} has {} owning states", owners.len()),
            );
        }
        let ((path, _), (machine, state, node)) = owners.into_iter().next().unwrap();
        let stop_events = state
            .transitions
            .iter()
            .flat_map(|array| &array.transitions)
            .filter(|transition| !transition.flags.is_some_and(|flags| flags & 32 != 0))
            .filter(|transition| transition.to_state_id != state.state_id)
            .filter_map(|transition| transition.event_name.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if stop_events.is_empty() {
            return adapter_unavailable(
                family,
                "ordinary casting",
                format!("source generator {name} has no outer stop transition"),
            );
        }
        if family.family_id == "draugr" {
            let mut upper = Vec::new();
            find_machine(node, "MLh_Cast_Behavior", &mut upper);
            let [upper] = upper.as_slice() else {
                return adapter_unavailable(
                    family,
                    "ordinary casting",
                    "missing unique source MLh_Cast_Behavior".into(),
                );
            };
            let entries = upper
                .wildcard_transitions
                .iter()
                .flat_map(|array| &array.transitions)
                .filter(|transition| !transition.flags.is_some_and(|flags| flags & 32 != 0))
                .filter_map(|transition| transition.event_name.as_ref().zip(transition.to_state_id))
                .map(|(event, id)| (event.clone(), id))
                .collect::<BTreeSet<_>>();
            for (event, state_id) in entries {
                let mut branch = node.clone();
                select_upper_branch(&mut branch, "MLh_Cast_Behavior", state_id);
                replacements.push(role(
                    family,
                    &path,
                    state,
                    &branch,
                    &event,
                    &stop_events,
                    clips,
                )?);
            }
        } else {
            let entries = machine
                .states
                .iter()
                .filter_map(|state| state.transitions.as_ref())
                .chain(machine.wildcard_transitions.as_ref())
                .flat_map(|array| &array.transitions)
                .filter(|transition| !transition.flags.is_some_and(|flags| flags & 32 != 0))
                .filter(|transition| transition.to_state_id == state.state_id)
                .filter_map(|transition| transition.event_name.clone())
                .collect::<BTreeSet<_>>();
            if entries.is_empty() {
                return adapter_unavailable(
                    family,
                    "ordinary casting",
                    format!("source generator {name} has no entry event"),
                );
            }
            for event in entries {
                replacements.push(role(
                    family,
                    &path,
                    state,
                    node,
                    &event,
                    &stop_events,
                    clips,
                )?);
            }
        }
    }
    let replaced = replacements
        .iter()
        .flat_map(|role| role.generator.clip_names(&role.clip_name))
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    roles.retain(|role| {
        !matches!(
            role.role,
            CreatureClipRole::ProjectileAttack
                | CreatureClipRole::ContinuousAttackStart
                | CreatureClipRole::ContinuousAttackLoop
                | CreatureClipRole::ContinuousAttackStop
        ) || !replaced.contains(&role.clip_name)
    });
    for role in replacements {
        graph_selection::collect_events(&role.generator, events);
        if let CapabilityRoleGenerator::SourceCasting { observations, .. } = &role.generator {
            for observation in observations {
                events
                    .entry(observation.target_event_name.to_ascii_lowercase())
                    .or_insert(EventDecl {
                        name: observation.target_event_name.clone(),
                        usage: EventUsage::Generic,
                        flags: 0,
                    });
            }
        }
        if let CapabilityRoleGenerator::SourceCasting {
            enter_events,
            exit_events,
            ..
        } = &role.generator
        {
            for event in enter_events.iter().chain(exit_events) {
                if let Some(name) = &event.event.event_name {
                    events
                        .entry(name.to_ascii_lowercase())
                        .or_insert(EventDecl {
                            name: name.clone(),
                            usage: EventUsage::Generic,
                            flags: 0,
                        });
                }
            }
        }
        for name in role.trigger_event.iter().chain(match &role.generator {
            CapabilityRoleGenerator::SourceCasting { stop_events, .. } => stop_events.iter(),
            _ => unreachable!(),
        }) {
            events
                .entry(name.to_ascii_lowercase())
                .or_insert(EventDecl {
                    name: name.clone(),
                    usage: EventUsage::Generic,
                    flags: 0,
                });
        }
        roles.push(role);
    }
    Ok(())
}

fn role(
    family: &SkyrimFamilyMotionSet,
    path: &str,
    owner: &SkyrimBehaviorStateEvidence,
    node: &SkyrimBehaviorGeneratorNodeEvidence,
    event: &str,
    stop_events: &[String],
    clips: &[&SkyrimCatalogClip],
) -> Result<CapabilityClipRole, SkyrimCreatureMotionError> {
    let root = capability_generator_node(family, path, node, clips)?;
    let enter_events = owner
        .enter_notify_events
        .iter()
        .map(|event| capability_event_property(family, path, event, "casting owner enter"))
        .collect::<Result<Vec<_>, _>>()?;
    let exit_events = owner
        .exit_notify_events
        .iter()
        .map(|event| capability_event_property(family, path, event, "casting owner exit"))
        .collect::<Result<Vec<_>, _>>()?;
    let names = root.clip_names();
    let release_clips = clips
        .iter()
        .filter(|clip| {
            if !names.contains(&clip.clip_id.as_str()) {
                return false;
            }
            let (triggers, enters) = node_events(&root, &clip.clip_id);
            clip.annotations
                .iter()
                .any(|annotation| is_release(&annotation.name))
                || enters.iter().any(|event| is_release(event))
                || triggers
                    .iter()
                    .filter_map(|trigger| trigger.event.event.event_name.as_deref())
                    .any(is_release)
        })
        .copied()
        .collect::<Vec<_>>();
    let anchor =
        release_clips
            .first()
            .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "ordinary casting",
                detail: format!("source entry {event} has no spell release annotation"),
            })?;
    let release_names = release_clips
        .iter()
        .map(|clip| clip.clip_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut observations = Vec::new();
    observe_branches(&root, &family.family_id, &release_names, &mut observations);
    observations.sort_by(|left, right| {
        (&left.source_behavior_path, left.source_state_object_index)
            .cmp(&(&right.source_behavior_path, right.source_state_object_index))
    });
    observations.dedup();
    if release_names.len() > 1
        && release_names.iter().any(|name| {
            !observations
                .iter()
                .any(|observation| observation.release_clip_name == *name)
        })
    {
        return adapter_unavailable(
            family,
            "ordinary casting",
            format!("source entry {event} cannot observe all distinct release branches"),
        );
    }
    Ok(CapabilityClipRole {
        equipment_index: None,
        role: CreatureClipRole::ProjectileAttack,
        state_name: format!("Source{event}"),
        clip_name: anchor.clip_id.clone(),
        generator: CapabilityRoleGenerator::SourceCasting {
            root,
            stop_events: stop_events.into(),
            observations,
            enter_events,
            exit_events,
        },
        trigger_event: Some(event.into()),
        trigger_aliases: Vec::new(),
        motion: anchor.root_motion.policy(),
    })
}

fn observe_branches(
    node: &CapabilityGeneratorNode,
    family: &str,
    releases: &BTreeSet<&str>,
    observations: &mut Vec<crate::source_rig::SourceCastingObservation>,
) {
    match node {
        CapabilityGeneratorNode::StateMachine { states, .. } => {
            for state in states {
                let leaves = state
                    .generator
                    .clip_names()
                    .into_iter()
                    .filter(|name| releases.contains(name))
                    .collect::<BTreeSet<_>>();
                if leaves.len() == 1 {
                    observations.push(crate::source_rig::SourceCastingObservation {
                        source_behavior_path: state.source.behavior_path.clone(),
                        source_state_object_index: state.source.object_index,
                        target_event_name: format!(
                            "B21_CastBranch_{}_{}_{}",
                            identifier_fragment(family),
                            identifier_fragment(&state.source.behavior_path),
                            state.source.object_index
                        ),
                        release_clip_name: (*leaves.first().unwrap()).to_string(),
                    });
                } else if !leaves.is_empty() {
                    observe_branches(&state.generator, family, releases, observations);
                }
            }
        }
        CapabilityGeneratorNode::ModifierGenerator { generator, .. }
        | CapabilityGeneratorNode::CyclicBlendTransition { generator, .. } => {
            observe_branches(generator, family, releases, observations)
        }
        CapabilityGeneratorNode::Blender { children, .. } => {
            for child in children {
                observe_branches(&child.generator, family, releases, observations);
            }
        }
        CapabilityGeneratorNode::ManualSelector { children, .. } => {
            for child in children {
                observe_branches(child, family, releases, observations);
            }
        }
        _ => {}
    }
}

fn is_release(event: &str) -> bool {
    event.to_ascii_lowercase().contains("spellfire")
}

pub(crate) fn source_clip_triggers(
    role: &CapabilityRoleGenerator,
    clip_name: &str,
) -> Vec<crate::source_rig::CapabilityGeneratorClipTrigger> {
    match role.base_generator() {
        CapabilityRoleGenerator::SourceCasting { root, .. }
        | CapabilityRoleGenerator::Tree { root } => node_events(root, clip_name).0,
        _ => Vec::new(),
    }
}

pub(crate) fn source_clip_entry_events(
    role: &CapabilityRoleGenerator,
    clip_name: &str,
) -> Vec<String> {
    match role.base_generator() {
        CapabilityRoleGenerator::SourceCasting { root, .. }
        | CapabilityRoleGenerator::Tree { root } => node_events(root, clip_name).1,
        _ => Vec::new(),
    }
}

fn node_events(
    node: &CapabilityGeneratorNode,
    clip_name: &str,
) -> (
    Vec<crate::source_rig::CapabilityGeneratorClipTrigger>,
    Vec<String>,
) {
    let mut triggers = Vec::new();
    let mut events = Vec::new();
    match node {
        CapabilityGeneratorNode::SourceClip {
            clip_name: name,
            triggers: source_triggers,
            ..
        } if name == clip_name => triggers.extend(source_triggers.clone()),
        CapabilityGeneratorNode::StateMachine { states, .. } => {
            for state in states {
                if state.generator.clip_names().contains(&clip_name) {
                    events.extend(
                        state
                            .enter_notify_events
                            .iter()
                            .filter_map(|event| event.event.event_name.clone()),
                    );
                    let child = node_events(&state.generator, clip_name);
                    triggers.extend(child.0);
                    events.extend(child.1);
                }
            }
        }
        CapabilityGeneratorNode::ModifierGenerator { generator, .. }
        | CapabilityGeneratorNode::CyclicBlendTransition { generator, .. } => {
            return node_events(generator, clip_name)
        }
        CapabilityGeneratorNode::Blender { children, .. } => {
            for child in children {
                let child = node_events(&child.generator, clip_name);
                triggers.extend(child.0);
                events.extend(child.1);
            }
        }
        CapabilityGeneratorNode::ManualSelector { children, .. } => {
            for child in children {
                let child = node_events(child, clip_name);
                triggers.extend(child.0);
                events.extend(child.1);
            }
        }
        _ => {}
    }
    events.sort();
    events.dedup();
    (triggers, events)
}

fn source(node: &SkyrimBehaviorGeneratorNodeEvidence) -> Option<&SkyrimBehaviorObjectEvidence> {
    match node {
        SkyrimBehaviorGeneratorNodeEvidence::Clip { source, .. }
        | SkyrimBehaviorGeneratorNodeEvidence::ManualSelector { source, .. }
        | SkyrimBehaviorGeneratorNodeEvidence::Blender { source, .. }
        | SkyrimBehaviorGeneratorNodeEvidence::ModifierGenerator { source, .. }
        | SkyrimBehaviorGeneratorNodeEvidence::Unsupported { source, .. }
        | SkyrimBehaviorGeneratorNodeEvidence::Cycle { source } => Some(source),
        SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine) => Some(&machine.source),
        SkyrimBehaviorGeneratorNodeEvidence::CyclicBlendTransition(node) => Some(&node.source),
        SkyrimBehaviorGeneratorNodeEvidence::Missing { .. } => None,
    }
}

fn find_machine<'a>(
    node: &'a SkyrimBehaviorGeneratorNodeEvidence,
    name: &str,
    output: &mut Vec<&'a SkyrimBehaviorStateMachineEvidence>,
) {
    if let SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine) = node {
        if machine.source.name.as_deref() == Some(name) {
            output.push(machine);
            return;
        }
        for state in &machine.states {
            if let Some(node) = &state.generator {
                find_machine(node, name, output);
            }
        }
    } else {
        match node {
            SkyrimBehaviorGeneratorNodeEvidence::ModifierGenerator {
                generator: Some(node),
                ..
            } => find_machine(node, name, output),
            SkyrimBehaviorGeneratorNodeEvidence::CyclicBlendTransition(node) => {
                if let Some(node) = &node.generator {
                    find_machine(node, name, output);
                }
            }
            SkyrimBehaviorGeneratorNodeEvidence::Blender { children, .. } => {
                for child in children {
                    find_machine(&child.generator, name, output);
                }
            }
            SkyrimBehaviorGeneratorNodeEvidence::ManualSelector { children, .. } => {
                for child in children {
                    find_machine(child, name, output);
                }
            }
            _ => {}
        }
    }
}

fn select_upper_branch(node: &mut SkyrimBehaviorGeneratorNodeEvidence, name: &str, selected: i32) {
    match node {
        SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine)
            if machine.source.name.as_deref() == Some(name) =>
        {
            machine.start_state_id = Some(selected);
            machine
                .states
                .retain(|state| state.state_id == Some(selected));
            for array in machine
                .states
                .iter_mut()
                .filter_map(|state| state.transitions.as_mut())
                .chain(machine.wildcard_transitions.as_mut())
            {
                array
                    .transitions
                    .retain(|transition| transition.to_state_id == Some(selected));
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine) => {
            for state in &mut machine.states {
                if let Some(node) = &mut state.generator {
                    select_upper_branch(node, name, selected);
                }
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::ModifierGenerator {
            generator: Some(node),
            ..
        } => select_upper_branch(node, name, selected),
        SkyrimBehaviorGeneratorNodeEvidence::CyclicBlendTransition(node) => {
            if let Some(node) = &mut node.generator {
                select_upper_branch(node, name, selected);
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::Blender { children, .. } => {
            for child in children {
                select_upper_branch(&mut child.generator, name, selected);
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::ManualSelector { children, .. } => {
            for child in children {
                select_upper_branch(child, name, selected);
            }
        }
        _ => {}
    }
}
