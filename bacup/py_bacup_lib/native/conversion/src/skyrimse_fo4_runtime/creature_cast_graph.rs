use super::*;

pub(super) fn dragon_shouts(
    family: &SkyrimFamilyMotionSet,
    inventory: &SkyrimCreatureFamilyInventory,
    clips: &[&SkyrimCatalogClip],
    roles: &mut Vec<CapabilityClipRole>,
    events: &mut BTreeMap<String, EventDecl>,
) -> Result<(), SkyrimCreatureMotionError> {
    if family.family_id != "dragon" {
        return Ok(());
    }
    let groups = inventory
        .behavior_groups
        .iter()
        .filter_map(|group| match group {
            SkyrimBehaviorGeneratorGroupEvidence::StateMachine(group)
                if group.machine.source.name.as_deref() == Some("BHR_Shout") =>
            {
                Some(group)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [group] = groups.as_slice() else {
        return adapter_unavailable(
            family,
            "dragon shouts",
            "missing unique source BHR_Shout state machine".into(),
        );
    };
    let mut replacements = Vec::new();
    for (entry, exit, event) in [(1, 0, "shoutStart"), (2, 3, "shoutStartFireball")] {
        let mut machine = group.machine.clone();
        machine.start_state_id = Some(entry);
        machine
            .states
            .retain(|state| matches!(state.state_id, Some(id) if id == entry || id == exit));
        let valid = machine.states.iter().any(|state| {
            state.state_id == Some(entry)
                && state.transitions.as_ref().is_some_and(|array| {
                    array.transitions.iter().any(|transition| {
                        transition.event_name.as_deref() == Some("shoutRelease")
                            && transition.to_state_id == Some(exit)
                    })
                })
        });
        if machine.states.len() != 2 || !valid {
            return adapter_unavailable(
                family,
                "dragon shouts",
                format!("source {event} has no exact inhale/release/exhale path"),
            );
        }
        let root = capability_state_machine_node(family, &group.behavior_path, &machine, clips)?;
        let generator = CapabilityRoleGenerator::Tree { root };
        let clip_names = generator.clip_names("");
        let anchor = clips
            .iter()
            .find(|clip| {
                clip_names.contains(&clip.clip_id.as_str())
                    && clip
                        .annotations
                        .iter()
                        .any(|annotation| annotation.name == "Voice_SpellFire_Event")
            })
            .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "dragon shouts",
                detail: format!("source {event} has no voice release annotation"),
            })?;
        replacements.push(CapabilityClipRole {
            equipment_index: None,
            role: CreatureClipRole::ProjectileAttack,
            state_name: format!("Source{event}"),
            clip_name: anchor.clip_id.clone(),
            generator,
            trigger_event: Some(event.into()),
            trigger_aliases: Vec::new(),
            motion: anchor.root_motion.policy(),
        });
    }
    let replaced = replacements
        .iter()
        .flat_map(|role| role.generator.clip_names(&role.clip_name))
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    roles.retain(|role| {
        !role
            .generator
            .clip_names(&role.clip_name)
            .iter()
            .any(|name| replaced.contains(*name))
    });
    for role in replacements {
        graph_selection::collect_events(&role.generator, events);
        roles.push(role);
    }
    for name in [
        "shoutStart",
        "shoutStartFireball",
        "shoutRelease",
        "shoutStop",
    ] {
        events
            .entry(name.to_ascii_lowercase())
            .or_insert(EventDecl {
                name: name.into(),
                usage: EventUsage::Generic,
                flags: 0,
            });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_rig::{ClipDecl, CreatureManifest, pack_capability_scaffold};

    #[test]
    #[ignore = "requires the extracted Skyrim Dragon graph and prior source-rig recipe"]
    fn source_dragon_shout_paths_pack_and_reload_without_physical_attack_aliases() {
        let root = PathBuf::from(std::env::var("MODKIT_SKYRIM_MESH_ROOT").unwrap());
        let data = load_animation_data_catalog(&root.join("animationdata")).unwrap();
        let family = load_family_evidence(
            &root.join("actors").canonicalize().unwrap(),
            &data["dragonproject"],
            "dragon",
            "dragon/dragonproject.hkx",
            Vec::new(),
        )
        .unwrap();
        let catalog = build_skyrim_creature_motion_catalog(&[family]).unwrap();
        let (graph, variables) = catalog.capability_graph_bundle("dragon").unwrap();
        let idle = graph
            .roles
            .iter()
            .find(|role| role.role == CreatureClipRole::Idle)
            .unwrap();
        assert!(
            catalog
                .clip(&idle.clip_name)
                .unwrap()
                .clip_path
                .to_ascii_lowercase()
                .contains("combatidle")
        );
        for (event, start, end, suffix) in [
            ("shoutStart", 1, 0, "ground_exhalefirebreath.hkx"),
            ("shoutStartFireball", 2, 3, "ground_exhalefireball.hkx"),
        ] {
            let role = graph
                .roles
                .iter()
                .find(|role| role.trigger_event.as_deref() == Some(event))
                .unwrap();
            assert!(role.trigger_aliases.is_empty());
            assert!(
                catalog
                    .clip(&role.clip_name)
                    .unwrap()
                    .clip_path
                    .to_ascii_lowercase()
                    .ends_with(suffix)
            );
            let CapabilityRoleGenerator::Tree {
                root:
                    CapabilityGeneratorNode::StateMachine {
                        start_state_id,
                        states,
                        ..
                    },
            } = &role.generator
            else {
                panic!("source shout tree missing")
            };
            assert_eq!(*start_state_id, start);
            assert_eq!(states.len(), 2);
            let inhale = states.iter().find(|state| state.state_id == start).unwrap();
            assert!(
                inhale
                    .transitions
                    .as_ref()
                    .unwrap()
                    .transitions
                    .iter()
                    .any(|transition| transition.event.event_name.as_deref()
                        == Some("shoutRelease")
                        && transition.to_state_id == end)
            );
        }
        for role in graph
            .roles
            .iter()
            .filter(|role| role.role == CreatureClipRole::MeleeAttack)
        {
            let path = catalog
                .clip(&role.clip_name)
                .unwrap()
                .clip_path
                .to_ascii_lowercase();
            assert!(!path.contains("fireball") && !path.contains("firebreath"));
        }
        let source: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var("MODKIT_SKYRIM_DRAGON_RECIPE").unwrap()).unwrap(),
        )
        .unwrap();
        let mut rig: CreatureManifest = serde_json::from_value(source["rig"].clone()).unwrap();
        let original = rig.clips[0].clone();
        rig.clips.clear();
        let mut seen = BTreeSet::new();
        for role in &graph.roles {
            for name in role.generator.clip_names(&role.clip_name) {
                if seen.insert(name.to_string()) {
                    rig.clips.push(ClipDecl {
                        name: name.into(),
                        path: format!("{}\\{name}.hkx", original.path.rsplit_once('\\').unwrap().0),
                        binding: original.binding.clone(),
                        looping: matches!(role.role, CreatureClipRole::Idle | CreatureClipRole::GroundForward
                            | CreatureClipRole::SwimIdle | CreatureClipRole::SwimForward | CreatureClipRole::FlyIdle
                            | CreatureClipRole::FlyForward | CreatureClipRole::StationaryIdle | CreatureClipRole::TurnLeft
                            | CreatureClipRole::TurnRight | CreatureClipRole::ContinuousAttackLoop),
                    });
                }
            }
        }
        rig.idle_clip = idle.clip_name.clone();
        for declarations in [&mut rig.root, &mut rig.core] {
            declarations.variables = variables.clone();
            for event in &graph.explicit_events {
                declarations
                    .events
                    .retain(|old| !old.name.eq_ignore_ascii_case(&event.name));
                declarations.events.push(event.clone());
            }
        }
        let output =
            std::env::temp_dir().join(format!("b21-dragon-shout-graph-{}", std::process::id()));
        let report = pack_capability_scaffold(&rig, &graph, &output).unwrap();
        for artifact in report.artifacts {
            let bytes = std::fs::read(&artifact.hkx_path).unwrap();
            let file = HkxFile::read(&bytes).unwrap();
            assert_eq!(file.save(), bytes);
        }
    }
}
