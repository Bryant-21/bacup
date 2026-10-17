use super::*;

#[path = "automatic_graph_tests.rs"]
mod automatic_graph_tests;

#[path = "source_attack_graph_tests.rs"]
mod source_attack_graph_tests;
#[path = "source_idle_overlay_tests.rs"]
mod source_idle_overlay_tests;
#[path = "property_sequence_tests.rs"]
mod property_sequence_tests;
use crate::source_rig::{
    BoneDecl, CapabilityBlenderChild, CapabilityLocomotionDirection, Capsule, ClipBinding,
    CreatureControllerDecl, ScaffoldPaths, SkeletonDecl, SourceOwnedRagdollReceipt,
    deterministic_standard_event, pack_capability_scaffold,
};

fn fixture() -> (CreatureManifest, CapabilityGraphManifest) {
    let mut graph = CapabilityGraphManifest {
        source_channels: Vec::new(),
        template: CreatureGraphTemplate::GroundMelee,
        roles: Vec::new(),
        idle_event: "Idle".to_string(),
        explicit_events: Vec::new(),
        candidate_attack_bindings: Vec::new(),
        source_inert_attack_receipts: Vec::new(),
        overlays: Vec::new(),
    };
    for role in [
        CreatureClipRole::Idle,
        CreatureClipRole::GroundForward,
        CreatureClipRole::TurnLeft,
        CreatureClipRole::TurnRight,
        CreatureClipRole::MeleeAttack,
        CreatureClipRole::Hurt,
        CreatureClipRole::Death,
    ] {
        let name = format!("{role:?}");
        let event = deterministic_standard_event(role, "fixture", 1);
        graph.explicit_events.push(EventDecl {
            name: event.clone(),
            usage: if role == CreatureClipRole::MeleeAttack {
                EventUsage::MeleeAttack
            } else {
                EventUsage::Generic
            },
            flags: 0,
        });
        graph.roles.push(CapabilityClipRole {
            equipment_index: None,
            role,
            state_name: name.clone(),
            clip_name: name,
            generator: CapabilityRoleGenerator::Single,
            trigger_event: (role != CreatureClipRole::Idle).then_some(event),
            trigger_aliases: Vec::new(),
            motion: Default::default(),
        });
    }
    graph.explicit_events.push(EventDecl {
        name: "B21_DeathAnimationComplete".to_string(),
        usage: EventUsage::Generic,
        flags: 0,
    });
    let core = GraphDeclarations {
        events: graph.explicit_events.clone(),
        variables: vec![VariableDecl {
            name: "bGraphDriven".to_string(),
            variable_type: VariableType::Bool,
            initial_value: VariableValue::Bool(true),
        }],
        character_properties: Vec::new(),
    };
    let mut root = core.clone();
    root.events.extend(
        RAGDOLL_TRANSITION_EVENTS
            .into_iter()
            .chain(RAGDOLL_ENTER_EVENTS)
            .map(|name| EventDecl {
                name: name.to_string(),
                usage: EventUsage::Generic,
                flags: 0,
            }),
    );
    let binding = ClipBinding {
        skeleton_path: "CharacterAssets\\Skeleton.hkx".to_string(),
        original_skeleton_name: "SourceRig".to_string(),
        declared_transform_tracks: 1,
        transform_track_to_bone_indices: vec![0],
        declared_float_tracks: 0,
        float_track_to_float_slot_indices: Vec::new(),
    };
    let manifest = CreatureManifest {
        property_sequences: Vec::new(),
        creature_name: "SourceCreature".to_string(),
        visual_skeleton_nif: "CharacterAssets\\Skeleton.nif".to_string(),
        animation_skeleton: SkeletonDecl {
            path: binding.skeleton_path.clone(),
            runtime_name: binding.original_skeleton_name.clone(),
            bones: vec![BoneDecl {
                name: "Root".to_string(),
                parent_index: None,
            }],
            float_slots: Vec::new(),
        },
        controller: CreatureControllerDecl {
            collision_filter_info: 1,
            rigid_body_type: 255,
            model_up_ms: [0.0, 0.0, 1.0, 0.0],
            model_forward_ms: [0.0, 1.0, 0.0, 0.0],
            model_right_ms: [1.0, 0.0, 0.0, 0.0],
            model_scale: 1.0,
        },
        ragdoll: RagdollDisposition::SourceOwned {
            runtime_path: "CharacterAssets\\Ragdoll.hkx".to_string(),
            receipt: SourceOwnedRagdollReceipt {
                byte_len: 1,
                blake3: "a".repeat(64),
                powered_ragdoll: None,
            },
        },
        clips: graph
            .roles
            .iter()
            .map(|role| ClipDecl {
                name: role.clip_name.clone(),
                path: format!("Animations\\{}.hkx", role.clip_name),
                binding: binding.clone(),
                looping: matches!(
                    role.role,
                    CreatureClipRole::Idle
                        | CreatureClipRole::GroundForward
                        | CreatureClipRole::TurnLeft
                        | CreatureClipRole::TurnRight
                ),
            })
            .collect(),
        idle_clip: "Idle".to_string(),
        capsule: Capsule {
            height: 1.0,
            radius: 0.2,
        },
        paths: ScaffoldPaths {
            project: "SourceCreatureProject.hkx".to_string(),
            character: "Characters\\SourceCreatureCharacter.hkx".to_string(),
            root_behavior: "Behaviors\\SourceCreatureRootBehavior.hkx".to_string(),
            core_behavior: "Behaviors\\SourceCreatureCoreBehavior.hkx".to_string(),
        },
        root,
        core,
    };
    (manifest, graph)
}

#[test]
fn hurt_and_death_interrupt_actions_and_death_never_returns_to_idle() {
    let (mut manifest, graph) = fixture();
    manifest.core.events.sort_by_key(|event| event.name.clone());
    let death_event = graph
        .roles
        .iter()
        .find(|role| role.role == CreatureClipRole::Death)
        .unwrap()
        .trigger_event
        .as_ref()
        .unwrap();
    let death_event_index = manifest
        .core
        .events
        .iter()
        .position(|event| &event.name == death_event)
        .unwrap();
    manifest.core.events.swap(0, death_event_index);
    let roles = ordered_capability_roles(&graph);
    let transitions = capability_state_transitions(&graph, &roles, &manifest.core);
    let index = |role| roles.iter().position(|entry| entry.role == role).unwrap();
    let death = index(CreatureClipRole::Death);
    let hurt = index(CreatureClipRole::Hurt);
    for (state, transitions) in transitions.iter().enumerate() {
        if state == death {
            assert!(transitions.is_empty());
            continue;
        }
        assert!(
            transitions
                .iter()
                .any(|(_, target, _)| *target == death as i32)
        );
        if state != hurt {
            assert!(
                transitions
                    .iter()
                    .any(|(_, target, _)| *target == hurt as i32)
            );
        }
    }
    assert!(transitions[hurt].contains(&(0, 0, true)));
    for role in [CreatureClipRole::TurnLeft, CreatureClipRole::TurnRight] {
        assert!(
            transitions[index(role)]
                .iter()
                .any(|(_, target, completion)| *target == 0 && !completion)
        );
    }
}

#[test]
fn source_speed_direction_blends_and_death_handoff_pack_as_fo4_graphs() {
    let (mut manifest, mut graph) = fixture();
    for declarations in [&mut manifest.root, &mut manifest.core] {
        for name in ["Speed", "Direction"] {
            declarations.variables.push(VariableDecl {
                name: name.to_string(),
                variable_type: VariableType::Real,
                initial_value: VariableValue::Real(0.0),
            });
        }
    }
    let walk = manifest
        .clips
        .iter()
        .find(|clip| clip.name == "GroundForward")
        .unwrap()
        .clone();
    for name in ["ForwardRun", "Backward"] {
        manifest.clips.push(ClipDecl {
            name: name.to_string(),
            path: format!("Animations\\{name}.hkx"),
            ..walk.clone()
        });
    }
    let speed = |name: &str, value: f32| CapabilityBlenderChild {
        clip_name: name.to_string(),
        weight_bits: value.to_bits(),
        world_from_model_weight_bits: 1.0_f32.to_bits(),
    };
    graph
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::GroundForward)
        .unwrap()
        .generator = CapabilityRoleGenerator::Locomotion {
        directions: vec![
            CapabilityLocomotionDirection {
                direction_bits: 0.0_f32.to_bits(),
                speeds: vec![speed("GroundForward", 50.0), speed("ForwardRun", 150.0)],
            },
            CapabilityLocomotionDirection {
                direction_bits: 0.5_f32.to_bits(),
                speeds: vec![speed("Backward", 30.0)],
            },
        ],
    };
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = &scaffold
        .artifact(&manifest.paths.core_behavior)
        .unwrap()
        .xml;
    let root = &scaffold
        .artifact(&manifest.paths.root_behavior)
        .unwrap()
        .xml;
    assert!(core.contains("<hkparam name=\"weight\">150</hkparam>"));
    assert!(core.contains("<hkparam name=\"relativeToEndOfClip\">true</hkparam>"));
    assert!(core.contains("B21_DeathAnimationComplete"));
    assert!(root.contains("B21_DeathAnimationComplete"));
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(
        pack_capability_scaffold(&manifest, &graph, temp.path())
            .unwrap()
            .artifacts
            .len(),
        4
    );
}

#[test]
fn passive_source_one_shots_are_event_selectable_and_return_to_idle() {
    let (mut manifest, mut graph) = fixture();
    graph.template = CreatureGraphTemplate::PassiveGround;
    graph.roles.retain(|role| matches!(role.role, CreatureClipRole::Idle | CreatureClipRole::Hurt | CreatureClipRole::Death));
    manifest.clips.retain(|clip| graph.roles.iter().any(|role| role.clip_name == clip.name));
    for name in ["SourceFlyAway", "SourceTurnLeft"] {
        let event = EventDecl { name: name.to_string(), usage: EventUsage::Generic, flags: 0 };
        graph.explicit_events.push(event.clone());
        manifest.core.events.push(event.clone());
        manifest.root.events.push(event);
        graph.roles.push(CapabilityClipRole {
            equipment_index: None, role: CreatureClipRole::ScriptedAction, state_name: name.to_string(), clip_name: name.to_string(), generator: CapabilityRoleGenerator::Single, trigger_event: Some(name.to_string()), trigger_aliases: Vec::new(), motion: Default::default() });
        manifest.clips.push(ClipDecl { name: name.to_string(), path: format!("Animations\\{name}.hkx"), binding: manifest.clips[0].binding.clone(), looping: false });
    }
    let roles = ordered_capability_roles(&graph);
    let transitions = capability_state_transitions(&graph, &roles, &manifest.core);
    for (index, role) in roles.iter().enumerate().filter(|(_, role)| role.role == CreatureClipRole::ScriptedAction) {
        let event = manifest.core.events.iter().position(|event| Some(&event.name) == role.trigger_event.as_ref()).unwrap();
        assert!(transitions[0].contains(&(event, index as i32, false)));
        assert!(transitions[index].contains(&(0, 0, true)));
    }
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(pack_capability_scaffold(&manifest, &graph, temp.path()).unwrap().artifacts.len(), 4);
}

#[test]
fn source_scripted_loop_waits_for_stop_or_a_new_actor_action() {
    let (mut manifest, mut graph) = fixture();
    for name in ["SourceScriptedLoop", "IdleStop", "SourceScriptedLoopComplete"] {
        let event = EventDecl { name: name.to_string(), usage: EventUsage::Generic, flags: 0 };
        graph.explicit_events.push(event.clone());
        manifest.core.events.push(event.clone());
        manifest.root.events.push(event);
    }
    graph.roles.push(CapabilityClipRole {
        equipment_index: None,
        role: CreatureClipRole::ScriptedLoop,
        state_name: "SourceScriptedLoop".to_string(),
        clip_name: "SourceScriptedLoop".to_string(),
        generator: CapabilityRoleGenerator::SourceIdle {
            source_group: 20,
            overlay: None, duration_bits: 2.0_f32.to_bits(), loop_start_bits: 0.0_f32.to_bits(),
            loop_end_bits: 2.0_f32.to_bits(), has_loop_markers: false, looping_min: 0, looping_max: 0,
            complete_event: "SourceScriptedLoopComplete".to_string(),
        },
        trigger_event: Some("SourceScriptedLoop".to_string()),
        trigger_aliases: Vec::new(),
        motion: Default::default(),
    });
    manifest.clips.push(ClipDecl {
        name: "SourceScriptedLoop".to_string(),
        path: "Animations\\SourceScriptedLoop.hkx".to_string(),
        binding: manifest.clips[0].binding.clone(),
        looping: true,
    });
    let roles = ordered_capability_roles(&graph);
    let index = roles.iter().position(|role| role.role == CreatureClipRole::ScriptedLoop).unwrap();
    let transitions = capability_state_transitions(&graph, &roles, &manifest.core);
    let source_xml = source_idle_emit::emit(&manifest, roles[index], index, &manifest.core);
    assert!(source_xml.contains("MODE_LOOPING"));
    assert!(!source_xml.contains("hkbClipTriggerArray"));
    assert!(transitions[index].iter().all(|(_, _, completion)| !completion));
    let stop = manifest.core.events.iter().position(|event| event.name == "IdleStop").unwrap();
    assert!(transitions[index].contains(&(stop, 0, false)));
    for target_role in [CreatureClipRole::GroundForward, CreatureClipRole::Hurt, CreatureClipRole::Death] {
        let target = roles.iter().position(|role| role.role == target_role).unwrap();
        assert!(transitions[index].iter().any(|(_, state, completion)| *state == target as i32 && !completion));
    }
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(pack_capability_scaffold(&manifest, &graph, temp.path()).unwrap().artifacts.len(), 4);
}

#[test]
fn source_idle_inner_loops_pack_and_stop_through_the_outro() {
    let (mut manifest, mut graph) = fixture();
    for name in ["IdleStop", "DogSit", "DogSitComplete", "DogSitInfinite", "DogSitInfiniteComplete"] {
        let event = EventDecl { name:name.to_string(), usage:EventUsage::Generic, flags:0 };
        graph.explicit_events.push(event.clone());
        manifest.core.events.push(event.clone());
        manifest.root.events.push(event);
    }
    for (name, min, max) in [("DogSit",4,8), ("DogSitInfinite",255,255)] {
        graph.roles.push(CapabilityClipRole {
            equipment_index:None, role:CreatureClipRole::ScriptedAction,
            state_name:name.to_string(), clip_name:name.to_string(), trigger_event:Some(name.to_string()), trigger_aliases:Vec::new(), motion:Default::default(),
            generator:CapabilityRoleGenerator::SourceIdle { source_group: 20, overlay: None, duration_bits:4.166667_f32.to_bits(), loop_start_bits:1.5_f32.to_bits(), loop_end_bits:2.833333_f32.to_bits(), has_loop_markers:true, looping_min:min, looping_max:max, complete_event:format!("{name}Complete") },
        });
        manifest.clips.push(ClipDecl { name:name.to_string(), path:format!("Animations\\{name}.hkx"), binding:manifest.clips[0].binding.clone(), looping:false });
    }
    let roles = ordered_capability_roles(&graph);
    let transitions = capability_state_transitions(&graph, &roles, &manifest.core);
    let stop = manifest.core.events.iter().position(|event| event.name == "IdleStop").unwrap();
    for (index, role) in roles.iter().enumerate().filter(|(_, role)| matches!(role.generator, CapabilityRoleGenerator::SourceIdle { .. })) {
        assert!(transitions[index].iter().all(|(event, _, completion)| *event != stop && !completion));
        let CapabilityRoleGenerator::SourceIdle { complete_event, .. } = &role.generator else { unreachable!() };
        let complete = manifest.core.events.iter().position(|event| &event.name == complete_event).unwrap();
        assert!(transitions[index].contains(&(complete, 0, false)));
    }
    let temp = tempfile::tempdir().unwrap();
    let report = pack_capability_scaffold(&manifest, &graph, temp.path()).unwrap();
    let artifact = report.artifact(&manifest.paths.core_behavior).unwrap();
    let packed = std::fs::read(&artifact.hkx_path).unwrap();
    let xml = havok_native::api::havok_hkx_to_xml(&packed).unwrap();
    assert!(xml.contains("<hkparam name=\"cropStartAmountLocalTime\">1.5</hkparam>"));
    assert!(xml.contains("DogSitInfiniteComplete"));
    assert!(xml.contains("<hkparam name=\"startStateMode\">2</hkparam>") || xml.contains("START_STATE_MODE_RANDOM"));
}
