use super::*;
use crate::source_rig::ContinuousAttackPhase;

fn automatic_fixture() -> (CreatureManifest, CapabilityGraphManifest) {
    let (mut manifest, mut graph) = fixture();
    graph.template = CreatureGraphTemplate::GroundMeleeRanged;
    let binding = manifest.clips[0].binding.clone();
    for phase in [
        ContinuousAttackPhase::Start,
        ContinuousAttackPhase::Loop,
        ContinuousAttackPhase::Stop,
    ] {
        let name = format!("Automatic{phase:?}");
        let event = if phase == ContinuousAttackPhase::Start {
            "RangedAttack".to_string()
        } else {
            format!("B21_{name}_Phase")
        };
        graph.explicit_events.push(EventDecl {
            name: event.clone(),
            usage: EventUsage::Generic,
            flags: 0,
        });
        manifest.clips.push(ClipDecl {
            name: name.clone(),
            path: format!("Animations\\{name}.hkx"),
            binding: binding.clone(),
            looping: phase == ContinuousAttackPhase::Loop,
        });
        graph.roles.push(CapabilityClipRole {
            equipment_index: None,
            role: CreatureClipRole::ProjectileAttack,
            state_name: name.clone(),
            clip_name: name,
            generator: CapabilityRoleGenerator::ContinuousAttackPhase {
                phase,
                start_state: Some("AutomaticStart".into()),
                loop_state: "AutomaticLoop".into(),
                stop_state: Some("AutomaticStop".into()),
            },
            trigger_event: Some(event),
            trigger_aliases: vec![],
            motion: Default::default(),
        });
    }
    for name in ["attackStop", "attackRelease"] {
        graph.explicit_events.push(EventDecl {
            name: name.into(),
            usage: EventUsage::Generic,
            flags: 0,
        });
    }
    for declaration in [&mut manifest.core, &mut manifest.root] {
        for event in &graph.explicit_events {
            if !declaration
                .events
                .iter()
                .any(|entry| entry.name == event.name)
            {
                declaration.events.push(event.clone());
            }
        }
    }
    (manifest, graph)
}

#[test]
fn automatic_attack_start_loop_release_transitions_preserve_interrupts() {
    let (manifest, graph) = automatic_fixture();
    let roles = ordered_capability_roles(&graph);
    let transitions = capability_state_transitions(&graph, &roles, &manifest.core);
    let state = |name| {
        roles
            .iter()
            .position(|role| role.state_name == name)
            .unwrap()
    };
    let start = state("AutomaticStart");
    let repeating = state("AutomaticLoop");
    let stop = state("AutomaticStop");
    let event = |name| {
        manifest
            .core
            .events
            .iter()
            .position(|entry| entry.name == name)
            .unwrap()
    };
    assert!(transitions[start].contains(&(0, repeating as i32, true)));
    assert!(
        !transitions[repeating]
            .iter()
            .any(|(_, _, complete)| *complete)
    );
    assert!(transitions[stop].contains(&(0, 0, true)));
    for index in [start, repeating] {
        assert!(transitions[index].contains(&(event("attackRelease"), stop as i32, false)));
        assert!(transitions[index].contains(&(event("attackStop"), stop as i32, false)));
        assert!(transitions[index].contains(&(event("Idle"), stop as i32, false)));
    }
    for index in [start, repeating, stop] {
        for interrupt in [CreatureClipRole::Hurt, CreatureClipRole::Death] {
            let target = roles
                .iter()
                .position(|role| role.role == interrupt)
                .unwrap();
            assert!(
                transitions[index]
                    .iter()
                    .any(|(_, state, complete)| *state == target as i32 && !*complete)
            );
        }
    }
    let entry = graph
        .roles
        .iter()
        .filter(|role| {
            role.role == CreatureClipRole::ProjectileAttack && role.generator.is_attack_entry()
        })
        .collect::<Vec<_>>();
    assert_eq!(entry.len(), 1);
    assert_eq!(entry[0].state_name, "AutomaticStart");
}

#[test]
fn automatic_attack_phase_graph_packs_and_rejects_unlinked_release() {
    let (manifest, mut graph) = automatic_fixture();
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(
        pack_capability_scaffold(&manifest, &graph, temp.path())
            .unwrap()
            .artifacts
            .len(),
        4
    );
    let role = graph
        .roles
        .iter_mut()
        .find(|role| role.state_name == "AutomaticStop")
        .unwrap();
    role.generator = CapabilityRoleGenerator::Single;
    let errors = manifest.validate_capability_graph(&graph).unwrap_err();
    assert!(errors.to_string().contains("continuous_attack_phase_link"));
}
