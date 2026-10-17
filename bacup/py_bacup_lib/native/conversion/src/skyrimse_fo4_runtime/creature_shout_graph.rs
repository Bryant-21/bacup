use super::*;

pub(super) fn creature_shouts(
    family: &SkyrimFamilyMotionSet,
    inventory: &SkyrimCreatureFamilyInventory,
    clips: &[&SkyrimCatalogClip],
    roles: &mut Vec<CapabilityClipRole>,
    events: &mut BTreeMap<String, EventDecl>,
) -> Result<(), SkyrimCreatureMotionError> {
    if !matches!(family.family_id.as_str(), "draugr" | "dwarven_steam") {
        return Ok(());
    }
    let mut owners = BTreeMap::new();
    for group in &inventory.behavior_groups {
        let SkyrimBehaviorGeneratorGroupEvidence::StateMachine(group) = group else {
            continue;
        };
        for state in &group.machine.states {
            if state.name.as_deref() == Some("Shout_State") {
                owners.insert(
                    (group.behavior_path.clone(), state.source.object_index),
                    (&group.machine, state),
                );
            }
        }
    }
    if owners.len() != 1 {
        return adapter_unavailable(
            family,
            "creature shouts",
            format!("source Shout_State has {} owners", owners.len()),
        );
    }
    let ((path, _), (owner, state)) = owners.into_iter().next().unwrap();
    let entry = owner
        .wildcard_transitions
        .iter()
        .flat_map(|array| &array.transitions)
        .filter(|transition| transition.to_state_id == state.state_id)
        .collect::<Vec<_>>();
    if entry.len() != 1
        || entry[0].event_name.as_deref() != Some("shoutStart")
        || entry[0].condition.is_some()
    {
        return adapter_unavailable(
            family,
            "creature shouts",
            "source Shout_State lacks its exact shoutStart entry".into(),
        );
    }
    let stop_events = state
        .transitions
        .iter()
        .flat_map(|array| &array.transitions)
        .filter(|transition| transition.to_state_id != state.state_id)
        .filter_map(|transition| transition.event_name.clone())
        .collect::<Vec<_>>();
    if stop_events != ["shoutStop"]
        || state.transitions.as_ref().is_some_and(|array| {
            array
                .transitions
                .iter()
                .any(|transition| transition.condition.is_some())
        })
    {
        return adapter_unavailable(
            family,
            "creature shouts",
            "source Shout_State lacks its exact shoutStop exit".into(),
        );
    }
    let Some(node) = &state.generator else {
        return adapter_unavailable(
            family,
            "creature shouts",
            "source Shout_State lacks a generator".into(),
        );
    };
    let SkyrimBehaviorGeneratorNodeEvidence::ModifierGenerator {
        generator: Some(child),
        ..
    } = node
    else {
        return adapter_unavailable(
            family,
            "creature shouts",
            "source shout activity modifier is missing".into(),
        );
    };
    let SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine) = child.as_ref() else {
        return adapter_unavailable(
            family,
            "creature shouts",
            "source shout modifier does not own its state machine".into(),
        );
    };
    let start = machine
        .states
        .iter()
        .find(|state| state.state_id == machine.start_state_id);
    if machine.source.name.as_deref() != Some("Shout_Behavior")
        || machine.states.len() != 2
        || !start.is_some_and(|state| {
            state.transitions.as_ref().is_some_and(|array| {
                array.transitions.iter().any(|transition| {
                    transition.event_name.as_deref() == Some("shoutRelease")
                        && transition.to_state_id != state.state_id
                        && machine
                            .states
                            .iter()
                            .any(|target| target.state_id == transition.to_state_id)
                })
            })
        })
    {
        return adapter_unavailable(
            family,
            "creature shouts",
            "source shout has no exact inhale/hold/release/exhale path".into(),
        );
    }
    let root = capability_generator_node(family, &path, node, clips)?;
    let names = root.clip_names();
    let releases = clips
        .iter()
        .filter(|clip| {
            names.contains(&clip.clip_id.as_str())
                && clip
                    .annotations
                    .iter()
                    .any(|annotation| annotation.name == "Voice_SpellFire_Event")
        })
        .collect::<Vec<_>>();
    let [anchor] = releases.as_slice() else {
        return adapter_unavailable(
            family,
            "creature shouts",
            format!(
                "source shout has {} exact voice release clips",
                releases.len()
            ),
        );
    };
    let lower_events = |source: &[SkyrimBehaviorEventPropertyEvidence], label: &str| {
        source
            .iter()
            .map(|event| capability_event_property(family, &path, event, label))
            .collect::<Result<Vec<_>, _>>()
    };
    let generator = CapabilityRoleGenerator::SourceCasting {
        root,
        stop_events,
        observations: Vec::new(),
        enter_events: lower_events(&state.enter_notify_events, "shout.enter")?,
        exit_events: lower_events(&state.exit_notify_events, "shout.exit")?,
    };
    let replaced = generator
        .clip_names("")
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    roles.retain(|role| {
        !role
            .generator
            .clip_names(&role.clip_name)
            .iter()
            .any(|name| replaced.contains(*name))
    });
    graph_selection::collect_events(&generator, events);
    for name in ["shoutStart", "shoutRelease", "shoutStop", "InterruptCast"] {
        events
            .entry(name.to_ascii_lowercase())
            .or_insert(EventDecl {
                name: name.into(),
                usage: EventUsage::Generic,
                flags: 0,
            });
    }
    roles.push(CapabilityClipRole {
        equipment_index: None,
        role: CreatureClipRole::ProjectileAttack,
        state_name: "SourceShout".into(),
        clip_name: anchor.clip_id.clone(),
        generator,
        trigger_event: Some("shoutStart".into()),
        trigger_aliases: Vec::new(),
        motion: anchor.root_motion.policy(),
    });
    Ok(())
}

#[cfg(test)]
#[path = "creature_shout_graph_tests.rs"]
mod tests;
