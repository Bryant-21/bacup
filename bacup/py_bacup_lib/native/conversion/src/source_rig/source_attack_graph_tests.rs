use super::*;
use crate::source_rig::{SourceAttackContract, SourceAttackWindow};

#[test]
fn source_movement_channel_controls_opening_gate_and_idle_resets_it() {
    let (mut manifest, mut graph) = fixture();
    for declarations in [&mut manifest.root, &mut manifest.core] {
        for name in [
            "B21SourceAttack_Test",
            "B21SourceMovementAttackGroup",
            "B21SourceAttackActive",
            "B21SourceMovementActive",
        ] {
            declarations.variables.push(VariableDecl {
                name: name.into(),
                variable_type: VariableType::Int32,
                initial_value: VariableValue::Int32(if name.ends_with("Active") { 0 } else { 32 }),
            });
        }
    }
    for role in &mut graph.roles {
        if role.role == CreatureClipRole::MeleeAttack {
            role.generator = CapabilityRoleGenerator::SourceAttack {
                contract: SourceAttackContract {
                    gate_variable: "B21SourceAttack_Test".into(),
                    group: 32,
                    opening_group: 32,
                    opening_variable: Some("B21SourceMovementAttackGroup".into()),
                    window: None,
                    aim_children: Vec::new(),
                },
                generator: Box::new(CapabilityRoleGenerator::Single),
            };
        } else if role.role == CreatureClipRole::Hurt {
            role.generator = CapabilityRoleGenerator::SourceMovement {
                clips: vec![crate::source_rig::SourceMovementClip {
                    clip_name: role.clip_name.clone(),
                    source_order: 0,
                    keys: vec![crate::source_rig::SourceMovementKey {
                        time_bits: 0.5_f32.to_bits(),
                        group: 26,
                    }],
                }],
                generator: Box::new(CapabilityRoleGenerator::Single),
            };
        }
    }
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let xml = &scaffold
        .artifacts
        .iter()
        .find(|artifact| artifact.runtime_path == manifest.paths.core_behavior)
        .unwrap()
        .xml;
    let document = roxmltree::Document::parse(xml).unwrap();
    let parameter = |node: roxmltree::Node<'_, '_>, name: &str| {
        node.children()
            .find(|child| child.attribute("name") == Some(name))
            .unwrap()
            .text()
            .unwrap()
            .to_string()
    };
    let object = |id: &str| {
        document
            .descendants()
            .find(|node| node.attribute("name") == Some(id))
            .unwrap()
    };
    for role in ordered_capability_roles(&graph) {
        let state = document
            .descendants()
            .find(|node| {
                node.attribute("class") == Some("hkbStateMachineStateInfo")
                    && parameter(*node, "name") == role.state_name
            })
            .unwrap();
        let scope = object(&parameter(state, "generator"));
        let modifiers = object(&parameter(scope, "modifier"));
        let modifier_ids = parameter(modifiers, "modifiers");
        let reset = object(&parameter(
            object(modifier_ids.split_whitespace().next().unwrap()),
            "expressions",
        ));
        let expression = reset
            .descendants()
            .find(|node| node.attribute("name") == Some("expression"))
            .unwrap()
            .text()
            .unwrap();
        assert_eq!(
            expression,
            match role.role {
                CreatureClipRole::Hurt => "B21SourceMovementAttackGroup",
                CreatureClipRole::MeleeAttack => "-1",
                _ => "32",
            }
        );
    }
    pack_capability_scaffold(&manifest, &graph, tempfile::tempdir().unwrap().path()).unwrap();
}

#[test]
fn source_attack_windows_guard_continuations_and_aim_channels_pack() {
    let (mut manifest, mut graph) = fixture();
    let gate = "B21SourceAttack_Test";
    for declarations in [&mut manifest.root, &mut manifest.core] {
        declarations.variables.push(VariableDecl {
            name: gate.into(),
            variable_type: VariableType::Int32,
            initial_value: VariableValue::Int32(32),
        });
        declarations.variables.push(VariableDecl {
            name: "B21SourceWeaponPitch".into(),
            variable_type: VariableType::Real,
            initial_value: VariableValue::Real(0.0),
        });
    }
    let base = graph
        .roles
        .iter()
        .position(|role| role.role == CreatureClipRole::MeleeAttack)
        .unwrap();
    let contract = SourceAttackContract {
        gate_variable: gate.into(),
        group: 32,
        opening_group: 32,
        opening_variable: None,
        window: Some(SourceAttackWindow {
            start_bits: 0.6_f32.to_bits(),
            end_bits: 1.0_f32.to_bits(),
            next_group: 26,
        }),
        aim_children: vec![
            CapabilityBlenderChild {
                clip_name: "AttackUp".into(),
                weight_bits: (-std::f32::consts::FRAC_PI_2).to_bits(),
                world_from_model_weight_bits: 1.0_f32.to_bits(),
            },
            CapabilityBlenderChild {
                clip_name: graph.roles[base].clip_name.clone(),
                weight_bits: 0.0_f32.to_bits(),
                world_from_model_weight_bits: 1.0_f32.to_bits(),
            },
            CapabilityBlenderChild {
                clip_name: "AttackDown".into(),
                weight_bits: std::f32::consts::FRAC_PI_2.to_bits(),
                world_from_model_weight_bits: 1.0_f32.to_bits(),
            },
        ],
    };
    graph.roles[base].generator = CapabilityRoleGenerator::SourceAttack {
        contract: contract.clone(),
        generator: Box::new(CapabilityRoleGenerator::Single),
    };
    let mut continuation = graph.roles[base].clone();
    continuation.state_name = "AttackLeft".into();
    continuation.clip_name = "AttackLeft".into();
    continuation.trigger_event = Some("meleeAttackLeftRequest".into());
    let mut next_contract = contract;
    next_contract.group = 26;
    next_contract.window = None;
    next_contract.aim_children.clear();
    continuation.generator = CapabilityRoleGenerator::SourceAttack {
        contract: next_contract,
        generator: Box::new(CapabilityRoleGenerator::Single),
    };
    graph.roles.push(continuation);
    for name in ["AttackUp", "AttackDown", "AttackLeft"] {
        manifest.clips.push(ClipDecl {
            name: name.into(),
            path: format!("Animations\\{name}.hkx"),
            binding: manifest.clips[0].binding.clone(),
            looping: false,
        });
    }
    for name in SourceAttackContract::window_events(&graph.roles[base].state_name)
        .into_iter()
        .chain(["meleeAttackLeftRequest".into()])
    {
        let event = EventDecl {
            usage: if name == "meleeAttackLeftRequest" {
                EventUsage::MeleeAttack
            } else {
                EventUsage::Generic
            },
            name,
            flags: 0,
        };
        graph.explicit_events.push(event.clone());
        manifest.root.events.push(event.clone());
        manifest.core.events.push(event);
    }
    let roles = ordered_capability_roles(&graph);
    let transitions = capability_state_transitions(&graph, &roles, &manifest.core);
    let right = roles
        .iter()
        .position(|role| role.state_name == "MeleeAttack")
        .unwrap();
    let left = roles
        .iter()
        .position(|role| role.state_name == "AttackLeft")
        .unwrap();
    let event = manifest
        .core
        .events
        .iter()
        .position(|event| event.name == "meleeAttackLeftRequest")
        .unwrap();
    assert!(transitions[right].contains(&(event, left as i32, false)));
    let xml = emit_capability_scaffold(&manifest, &graph)
        .unwrap()
        .artifacts
        .into_iter()
        .find(|artifact| artifact.runtime_path == manifest.paths.core_behavior)
        .unwrap()
        .xml;
    assert!(xml.contains("B21SourceAttack_Test == 26"));
    assert!(xml.contains("hkbEventDrivenModifier"));
    assert!(xml.contains("<hkparam name=\"flags\">25</hkparam>"));
    assert!(xml.contains("<hkparam name=\"localTime\">0.6</hkparam>"));
    assert_eq!(
        xml.matches("<hkparam name=\"flags\">16</hkparam>").count(),
        2
    );
    assert!(xml.contains("<hkparam name=\"indexOfSyncMasterChild\">1</hkparam>"));
    let temp = tempfile::tempdir().unwrap();
    let packed = pack_capability_scaffold(&manifest, &graph, temp.path()).unwrap();
    assert_eq!(packed.artifacts.len(), 4);
    for path in [&manifest.paths.root_behavior, &manifest.paths.core_behavior] {
        let bytes = std::fs::read(&packed.artifact(path).unwrap().hkx_path).unwrap();
        let xml = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
        let document = roxmltree::Document::parse(&xml).unwrap();
        let names = document
            .descendants()
            .find(|node| node.attribute("name") == Some("variableNames"))
            .unwrap()
            .children()
            .filter(|node| node.has_tag_name("hkcstring"))
            .map(|node| node.text().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(names.iter().filter(|name| **name == gate).count(), 1);
        let gate_index = names.iter().position(|name| *name == gate).unwrap();
        let info = document
            .descendants()
            .find(|node| node.attribute("name") == Some("variableInfos"))
            .unwrap()
            .children()
            .filter(|node| node.has_tag_name("hkobject"))
            .nth(gate_index)
            .unwrap();
        let variable_type = info
            .children()
            .find(|node| node.attribute("name") == Some("type"))
            .unwrap()
            .text()
            .unwrap();
        assert!(matches!(variable_type, "3" | "VARIABLE_TYPE_INT32"));
        if path == &manifest.paths.core_behavior {
            assert!(document.descendants().any(|node| node.attribute("class")
                == Some("hkbExpressionDataArray")
                && node.descendants().any(|value| value.attribute("name")
                    == Some("assignmentVariableIndex")
                    && value.text().and_then(|text| text.parse::<usize>().ok())
                        == Some(gate_index))));
        }
    }
    for clip_name in [
        graph.roles[base].clip_name.as_str(),
        "AttackUp",
        "AttackDown",
    ] {
        manifest
            .property_sequences
            .push(crate::source_rig::CreaturePropertySequence {
                clip_name: clip_name.into(),
                crop_start_bits: 0.0_f32.to_bits(),
                crop_end_bits: 0.0_f32.to_bits(),
                looping: false,
                sequence_names: vec![format!("B21CP_Test_{clip_name}")],
                source_float_slots: vec!["TestVisibility".into()],
            });
    }
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let xml = &scaffold
        .artifacts
        .iter()
        .find(|artifact| artifact.runtime_path == manifest.paths.core_behavior)
        .unwrap()
        .xml;
    let document = roxmltree::Document::parse(xml).unwrap();
    let objects = document
        .descendants()
        .filter(|node| node.has_tag_name("hkobject"));
    let suppressed = objects
        .clone()
        .filter(|node| {
            node.attribute("class") == Some("hkbClipGenerator")
                && node.children().any(|child| {
                    child.attribute("name") == Some("name")
                        && child
                            .text()
                            .is_some_and(|name| name.starts_with("B21PropertyClock_"))
                })
                && node.children().any(|child| {
                    child.attribute("name") == Some("flags") && child.text() == Some("16")
                })
        })
        .count();
    assert_eq!(suppressed, 2);
    assert!(
        objects
            .filter(|node| node.attribute("class") == Some("hkbBlenderGenerator"))
            .all(|node| !node.children().any(
                |child| child.attribute("name") == Some("flags") && child.text() == Some("16")
            ))
    );
    assert!(document.descendants().any(|node| {
        node.attribute("class") == Some("hkbClipGenerator")
            && node.children().any(|child| {
                child.attribute("name") == Some("triggers")
                    && child.text().is_some_and(|text| text.starts_with('#'))
            })
    }));
    pack_capability_scaffold(&manifest, &graph, temp.path()).unwrap();

    let CapabilityRoleGenerator::SourceAttack { contract, .. } = &mut graph.roles[base].generator
    else {
        unreachable!()
    };
    contract.aim_children.clear();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let xml = &scaffold
        .artifacts
        .iter()
        .find(|artifact| artifact.runtime_path == manifest.paths.core_behavior)
        .unwrap()
        .xml;
    let document = roxmltree::Document::parse(xml).unwrap();
    assert!(document.descendants().any(|node| {
        node.attribute("class") == Some("hkbClipGenerator")
            && node.children().any(|child| {
                child.attribute("name") == Some("name")
                    && child
                        .text()
                        .is_some_and(|name| name.starts_with("B21PropertyClock_010"))
            })
            && node.children().any(|child| {
                child.attribute("name") == Some("triggers")
                    && child.text().is_some_and(|text| text.starts_with('#'))
            })
    }));
    pack_capability_scaffold(&manifest, &graph, temp.path()).unwrap();
}
