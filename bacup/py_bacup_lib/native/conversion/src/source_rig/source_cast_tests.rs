use super::*;

fn source_cast_fixture() -> (CreatureManifest, CapabilityGraphManifest) {
    let (mut manifest, mut graph) = ranged_capability();
    let variable_index = manifest.core.variables.len() as i32;
    let active = VariableDecl {
        name: "bMagicActive".into(),
        variable_type: VariableType::Bool,
        initial_value: VariableValue::Bool(false),
    };
    manifest.core.variables.push(active.clone());
    manifest.root.variables.push(active);
    for name in ["CastStop", "FreezeBlend", "CrossBlend"] {
        manifest.core.events.push(generic_event(name));
        manifest.root.events.push(generic_event(name));
        graph.explicit_events.push(generic_event(name));
    }
    let source_clip = CapabilityGeneratorNode::SourceClip {
        source: recursive_source_object(801, "hkbClipGenerator", "MagicCharge"),
        clip_name: graph.roles[3].clip_name.clone(),
        user_data: 123,
        bindings: Vec::new(),
        triggers: vec![CapabilityGeneratorClipTrigger {
            local_time_bits: (-0.003f32).to_bits(),
            event: CapabilityGeneratorEventProperty {
                event: CapabilityGeneratorEventRef {
                    source_id: 9,
                    event_name: Some("CastStop".into()),
                },
                payload: None,
            },
            relative_to_end_of_clip: true,
            acyclic: false,
            is_annotation: false,
        }],
        mode: 1,
        flags: 8,
        crop_start_bits: 0.1f32.to_bits(),
        crop_end_bits: 0.2f32.to_bits(),
        start_time_bits: 0.3f32.to_bits(),
        playback_speed_bits: 1.5f32.to_bits(),
        enforced_duration_bits: 0.0f32.to_bits(),
        user_controlled_time_fraction_bits: 0.0f32.to_bits(),
    };
    let child = CapabilityGeneratorBlenderChild {
        generator: source_clip,
        bone_weights: Some(CapabilityGeneratorBoneWeights {
            source: recursive_anonymous_source(802, "hkbBoneWeightArray"),
            bindings: Vec::new(),
            weights_bits: (0..manifest.animation_skeleton.bones.len())
                .map(|index| {
                    if index % 2 == 0 {
                        0.25f32.to_bits()
                    } else {
                        1.0f32.to_bits()
                    }
                })
                .collect(),
        }),
        weight_bits: 1.0f32.to_bits(),
        world_from_model_weight_bits: 0.0f32.to_bits(),
    };
    let blender = CapabilityGeneratorNode::Blender {
        source: recursive_source_object(803, "hkbBlenderGenerator", "MagicDirectionalBlend"),
        bindings: Vec::new(),
        children: vec![child.clone(), child],
        reference_pose_weight_threshold_bits: 0.001f32.to_bits(),
        blend_parameter_bits: 0.4f32.to_bits(),
        min_cyclic_blend_parameter_bits: (-180.0f32).to_bits(),
        max_cyclic_blend_parameter_bits: 180.0f32.to_bits(),
        index_of_sync_master_child: 0,
        flags: 0x11,
        subtract_last_child: false,
    };
    let event = |name: Option<&str>| CapabilityGeneratorEventProperty {
        event: CapabilityGeneratorEventRef {
            source_id: if name.is_some() { 4 } else { -1 },
            event_name: name.map(str::to_string),
        },
        payload: None,
    };
    let cyclic = CapabilityGeneratorNode::CyclicBlendTransition {
        source: recursive_source_object(804, "BSCyclicBlendTransitionGenerator", "MagicCyclic"),
        bindings: Vec::new(),
        user_data: 7,
        generator: Box::new(blender),
        event_to_freeze_blend_value: event(Some("FreezeBlend")),
        event_to_cross_blend: event(Some("CrossBlend")),
        transition_in_event: event(None),
        transition_out_event: event(None),
        f_blend_parameter_bits: 0.4f32.to_bits(),
        f_transition_duration_bits: 0.2f32.to_bits(),
        e_blend_curve: 1,
    };
    let interval = CapabilityGeneratorInterval {
        enter_event: no_generator_event(),
        exit_event: no_generator_event(),
        enter_time_bits: 0,
        exit_time_bits: 0,
    };
    let state_machine = CapabilityGeneratorNode::StateMachine {
        source: recursive_source_object(805, "hkbStateMachine", "MagicLowerBody"),
        bindings: Vec::new(),
        event_to_send_when_state_or_transition_changes: event(None),
        start_state_id_selector: None,
        start_state_id: 0,
        return_to_previous_state_event: no_generator_event(),
        random_transition_event: no_generator_event(),
        transition_to_next_higher_state_event: no_generator_event(),
        transition_to_next_lower_state_event: no_generator_event(),
        sync_variable_index: -1,
        wrap_around_state_id: false,
        max_simultaneous_transitions: 32,
        start_state_mode: 0,
        self_transition_mode: 0,
        states: vec![CapabilityGeneratorState {
            source: recursive_source_object(806, "hkbStateMachineStateInfo", "CastLoop"),
            listeners: Vec::new(),
            enter_notify_events: Vec::new(),
            exit_notify_events: Vec::new(),
            transitions: Some(CapabilityGeneratorTransitionArray {
                source: recursive_anonymous_source(807, "hkbStateMachineTransitionInfoArray"),
                has_eventless_transitions: Some(true),
                has_time_bounded_transitions: Some(false),
                transitions: vec![CapabilityGeneratorTransition {
                    trigger_interval: interval.clone(),
                    initiate_interval: interval,
                    transition_effect: None,
                    condition: Some(CapabilityGeneratorExpressionCondition {
                        source: recursive_anonymous_source(808, "hkbExpressionCondition"),
                        expression: "bMagicActive == false".into(),
                        referenced_variables: vec![
                            CapabilityGeneratorExpressionVariableReference {
                                variable_index,
                                variable_name: "bMagicActive".into(),
                                source_variable_type: CapabilitySourceVariableType::Bool,
                                initial_word_value: Some(0),
                            },
                        ],
                    }),
                    event: no_generator_event(),
                    to_state_id: 0,
                    from_nested_state_id: 0,
                    to_nested_state_id: 0,
                    priority: 3,
                    flags: 0,
                }],
            }),
            generator: Box::new(cyclic),
            name: "CastLoop".into(),
            state_id: 0,
            probability_bits: 1.0f32.to_bits(),
            enable: true,
            has_eventless_transitions: true,
        }],
        wildcard_transitions: None,
    };
    graph.roles[3].generator = CapabilityRoleGenerator::SourceCasting {
        root: CapabilityGeneratorNode::ModifierGenerator {
            source: recursive_source_object(809, "hkbModifierGenerator", "MagicActive"),
            bindings: Vec::new(),
            user_data: 2,
            modifier: Box::new(CapabilityModifierNode::IsActive {
                source: recursive_source_object(810, "BSIsActiveModifier", "MagicActiveFlag"),
                user_data: 2,
                enable: true,
                active: [false; 5],
                invert: [true, false, true, false, true],
                bindings: vec![CapabilityGeneratorVariableBinding {
                    member_path: "bIsActive0".into(),
                    variable_index,
                    variable_name: "bMagicActive".into(),
                    bit_index: -1,
                    binding_type: CapabilityVariableBindingType::Variable,
                    source_variable_type: CapabilitySourceVariableType::Bool,
                    initial_word_value: Some(0),
                }],
            }),
            generator: Box::new(state_machine),
        },
        stop_events: vec!["CastStop".into()],
        observations: Vec::new(),
        enter_events: Vec::new(),
        exit_events: Vec::new(),
    };
    (manifest, graph)
}

#[test]
fn source_cast_observation_uses_target_event_index_without_mutating_source_receipts() {
    let (mut manifest, mut graph) = source_cast_fixture();
    let event_name = "B21_CastBranch_Fixture_806";
    for events in [&mut manifest.core.events, &mut manifest.root.events, &mut graph.explicit_events] {
        events.push(generic_event(event_name));
    }
    let release_clip_name = graph.roles[3].clip_name.clone();
    let CapabilityRoleGenerator::SourceCasting { root, observations, enter_events, exit_events, .. } = &mut graph.roles[3].generator else { unreachable!() };
    for (events, name) in [(enter_events, "FreezeBlend"), (exit_events, "CrossBlend")] {
        events.push(CapabilityGeneratorEventProperty {
            event: CapabilityGeneratorEventRef { source_id: 999, event_name: Some(name.into()) },
            payload: None,
        });
    }
    let CapabilityGeneratorNode::ModifierGenerator { generator, .. } = root else { unreachable!() };
    let CapabilityGeneratorNode::StateMachine { states, .. } = generator.as_mut() else { unreachable!() };
    let state = &mut states[0];
    state.enter_notify_events.push(CapabilityGeneratorEventProperty {
        event: CapabilityGeneratorEventRef { source_id: 777, event_name: Some("CastStop".into()) },
        payload: None,
    });
    observations.push(SourceCastingObservation {
        source_behavior_path: state.source.behavior_path.clone(),
        source_state_object_index: state.source.object_index,
        target_event_name: event_name.into(),
        release_clip_name,
    });
    let before = graph.clone();
    let output = tempfile::tempdir().unwrap();
    let report = pack_capability_scaffold(&manifest, &graph, output.path()).unwrap();
    assert_eq!(graph, before);
    let xml = reread_packed_xml(&report, &manifest.paths.core_behavior);
    let document = roxmltree::Document::parse(&xml).unwrap();
    let parameter = |node: roxmltree::Node<'_, '_>, name: &str| node.children()
        .find(|child| child.attribute("name") == Some(name)).and_then(|child| child.text()).unwrap_or("").to_string();
    let state = document.descendants().find(|node| node.attribute("class") == Some("hkbStateMachineStateInfo") && parameter(*node, "name") == "CastLoop").unwrap();
    let array_id = parameter(state, "enterNotifyEvents");
    let array = document.descendants().find(|node| node.attribute("name") == Some(array_id.as_str())).unwrap();
    let event_ids = array.descendants().filter(|node| node.attribute("name") == Some("id"))
        .map(|node| node.text().unwrap().parse::<usize>().unwrap()).collect::<Vec<_>>();
    let names = document.descendants().find(|node| node.attribute("name") == Some("eventNames")).unwrap()
        .children().filter(|node| node.has_tag_name("hkcstring")).map(|node| node.text().unwrap_or("")).collect::<Vec<_>>();
    assert_eq!(event_ids.iter().map(|index| names[*index]).collect::<Vec<_>>(), [event_name, "CastStop"]);
    assert!(!event_ids.contains(&777));
    let outer = document.descendants().find(|node| node.attribute("class") == Some("hkbStateMachineStateInfo") && parameter(*node, "name") == graph.roles[3].state_name).unwrap();
    for (property, expected) in [("enterNotifyEvents", "FreezeBlend"), ("exitNotifyEvents", "CrossBlend")] {
        let id = parameter(outer, property);
        let array = document.descendants().find(|node| node.attribute("name") == Some(id.as_str())).unwrap();
        let ids = array.descendants().filter(|node| node.attribute("name") == Some("id"))
            .map(|node| node.text().unwrap().parse::<usize>().unwrap()).collect::<Vec<_>>();
        assert_eq!(names[*ids.last().unwrap()], expected);
        assert!(!ids.contains(&999));
    }
}

#[test]
fn source_cast_observation_rejects_state_identity_from_another_behavior() {
    let (mut manifest, mut graph) = source_cast_fixture();
    let event_name = "B21_CastBranch_Fixture_806";
    manifest.core.events.push(generic_event(event_name));
    let release_clip_name = graph.roles[3].clip_name.clone();
    let CapabilityRoleGenerator::SourceCasting { observations, .. } = &mut graph.roles[3].generator else { unreachable!() };
    observations.push(SourceCastingObservation {
        source_behavior_path: "OtherBehavior.hkx".into(),
        source_state_object_index: 806,
        target_event_name: event_name.into(),
        release_clip_name,
    });
    let errors = emit_capability_scaffold(&manifest, &graph).unwrap_err();
    assert!(errors.to_string().contains("casting_observation_state"), "{errors}");
}

#[test]
fn source_casting_graph_preserves_masks_conditions_clip_timing_and_stop_events_after_pack() {
    let (manifest, graph) = source_cast_fixture();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    let transitions = object_containing(&core.xml, "name=\"#0113\"");
    assert!(
        !transitions.contains("<hkparam name=\"eventId\">-1</hkparam>"),
        "{transitions}"
    );
    assert!(
        !transitions.contains("<hkparam name=\"flags\">256</hkparam>"),
        "{transitions}"
    );
    let output = tempfile::tempdir().unwrap();
    let report = pack_capability_scaffold(&manifest, &graph, output.path()).unwrap();
    let xml = reread_packed_xml(&report, &manifest.paths.core_behavior);
    let document = roxmltree::Document::parse(&xml).unwrap();
    let objects = |class: &str| {
        document
            .descendants()
            .filter(|node| node.attribute("class") == Some(class))
            .collect::<Vec<_>>()
    };
    let parameter = |node: roxmltree::Node<'_, '_>, name: &str| {
        node.children()
            .find(|child| child.attribute("name") == Some(name))
            .and_then(|child| child.text())
            .unwrap_or("")
            .to_string()
    };
    let clips = objects("hkbClipGenerator")
        .into_iter()
        .filter(|node| parameter(*node, "name") == "MagicCharge")
        .collect::<Vec<_>>();
    assert_eq!(clips.len(), 2);
    for clip in clips {
        assert_eq!(parameter(clip, "mode"), "MODE_LOOPING");
        assert_eq!(
            parameter(clip, "playbackSpeed").parse::<f32>().unwrap(),
            1.5
        );
        assert_eq!(
            parameter(clip, "cropStartAmountLocalTime")
                .parse::<f32>()
                .unwrap(),
            0.1
        );
        assert_eq!(parameter(clip, "userData"), "123");
    }
    let weights = objects("hkbBoneWeightArray");
    assert_eq!(weights.len(), 2);
    for weights in weights {
        let values = parameter(weights, "boneWeights")
            .split_whitespace()
            .map(|word| word.parse::<f32>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(values.len(), manifest.animation_skeleton.bones.len());
        assert_eq!(&values[..2], &[0.25, 1.0]);
    }
    let cyclic = objects("BSCyclicBlendTransitionGenerator")[0];
    let trigger_arrays = objects("hkbClipTriggerArray");
    assert_eq!(trigger_arrays.len(), 2);
    for array in trigger_arrays {
        let trigger = array
            .children()
            .find(|child| child.attribute("name") == Some("triggers"))
            .unwrap()
            .children()
            .find(|child| child.has_tag_name("hkobject"))
            .unwrap();
        assert_eq!(
            parameter(trigger, "localTime").parse::<f32>().unwrap(),
            -0.003
        );
        assert_eq!(parameter(trigger, "relativeToEndOfClip"), "true");
    }
    assert_eq!(
        parameter(cyclic, "fTransitionDuration")
            .parse::<f32>()
            .unwrap(),
        0.2
    );
    assert_eq!(
        parameter(objects("BSIsActiveModifier")[0], "bInvertActive4"),
        "true"
    );
    assert_eq!(
        parameter(objects("hkbExpressionCondition")[0], "expression"),
        "bMagicActive == false"
    );
}

#[test]
fn source_casting_rejects_conflicting_shared_source_nodes() {
    let (manifest, mut graph) = source_cast_fixture();
    let CapabilityRoleGenerator::SourceCasting {
        root: CapabilityGeneratorNode::ModifierGenerator { generator, .. },
        ..
    } = &mut graph.roles[3].generator
    else {
        unreachable!()
    };
    let CapabilityGeneratorNode::StateMachine { states, .. } = generator.as_mut() else {
        unreachable!()
    };
    let CapabilityGeneratorNode::CyclicBlendTransition { generator, .. } =
        states[0].generator.as_mut()
    else {
        unreachable!()
    };
    let CapabilityGeneratorNode::Blender { children, .. } = generator.as_mut() else {
        unreachable!()
    };
    let CapabilityGeneratorNode::SourceClip {
        playback_speed_bits,
        ..
    } = &mut children[1].generator
    else {
        unreachable!()
    };
    *playback_speed_bits = 2.0f32.to_bits();
    assert!(
        error_codes(manifest.validate_capability_graph(&graph))
            .contains(&"conflicting_generator_source_object")
    );
}

#[test]
fn source_casting_outputs_bind_root_visible_variable_slots_after_pack() {
    let (mut manifest, graph) = source_cast_fixture();
    manifest.root.variables.insert(
        0,
        VariableDecl {
            name: "RootOnlyProbe".into(),
            variable_type: VariableType::Int32,
            initial_value: VariableValue::Int32(42),
        },
    );
    let output = tempfile::tempdir().unwrap();
    let report = pack_capability_scaffold(&manifest, &graph, output.path()).unwrap();
    let root_xml = reread_packed_xml(&report, &manifest.paths.root_behavior);
    let core_xml = reread_packed_xml(&report, &manifest.paths.core_behavior);
    let root = roxmltree::Document::parse(&root_xml).unwrap();
    let core = roxmltree::Document::parse(&core_xml).unwrap();
    let variable_names = |document: &roxmltree::Document<'_>| {
        document
            .descendants()
            .find(|node| node.attribute("name") == Some("variableNames"))
            .unwrap()
            .children()
            .filter(|node| node.has_tag_name("hkcstring"))
            .map(|node| node.text().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    let root_names = variable_names(&root);
    let core_names = variable_names(&core);
    let root_index = root_names
        .iter()
        .position(|name| name == "bMagicActive")
        .unwrap();
    let core_index = core_names
        .iter()
        .position(|name| name == "bMagicActive")
        .unwrap();
    assert_ne!(root_index, core_index);
    for names in [&root_names, &core_names] {
        assert_eq!(
            names
                .iter()
                .filter(|name| name.as_str() == "bMagicActive")
                .count(),
            1
        );
    }
    let parameter = |node: roxmltree::Node<'_, '_>, name: &str| {
        node.children()
            .find(|child| child.attribute("name") == Some(name))
            .and_then(|child| child.text())
            .unwrap_or("")
            .to_string()
    };
    let active = core
        .descendants()
        .find(|node| node.attribute("class") == Some("BSIsActiveModifier"))
        .unwrap();
    let binding_id = parameter(active, "variableBindingSet");
    let binding_set = core
        .descendants()
        .find(|node| node.attribute("name") == Some(binding_id.as_str()))
        .unwrap();
    let binding = binding_set
        .descendants()
        .find(|node| {
            node.has_tag_name("hkobject") && parameter(*node, "memberPath") == "bIsActive0"
        })
        .unwrap();
    assert_eq!(
        parameter(binding, "variableIndex")
            .parse::<usize>()
            .unwrap(),
        core_index
    );
    assert_eq!(root_names[root_index], core_names[core_index]);
}
