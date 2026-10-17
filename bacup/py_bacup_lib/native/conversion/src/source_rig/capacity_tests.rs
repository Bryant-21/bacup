use super::*;

#[test]
fn symbolic_graph_ids_are_numbered_without_rewriting_names_or_payloads() {
    let source = r##"<hkpackfile toplevelobject="#root"><hksection>
        <hkobject name="#0042" class="hkbStringEventPayload"><hkparam name="data">#tree_1</hkparam></hkobject>
        <hkobject name="#root"><hkparam name="children">#tree_1 #tree_10 #0042</hkparam></hkobject>
        <hkobject name="#tree_1"><hkparam name="name">#tree_1</hkparam></hkobject>
        <hkobject name="#tree_10"><hkparam name="names"><hkcstring>#tree_10</hkcstring></hkparam></hkobject>
        </hksection></hkpackfile>"##;
    let output = super::super::xml::number_generated_object_ids(source).unwrap();
    let document = roxmltree::Document::parse(&output).unwrap();
    let objects = document
        .descendants()
        .filter(|node| node.has_tag_name("hkobject"))
        .map(|node| node.attribute("name").unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(objects.len(), 4);
    assert!(objects.contains("#0042"));
    assert!(objects.iter().all(|id| id[1..].parse::<u64>().is_ok()));
    assert!(objects.contains(document.root_element().attribute("toplevelobject").unwrap()));
    let children = document
        .descendants()
        .find(|node| node.attribute("name") == Some("children"))
        .unwrap();
    assert!(
        children
            .text()
            .unwrap()
            .split_whitespace()
            .all(|id| objects.contains(id))
    );
    assert!(output.contains("<hkparam name=\"data\">#tree_1</hkparam>"));
    assert!(output.contains("<hkparam name=\"name\">#tree_1</hkparam>"));
    assert!(output.contains("<hkcstring>#tree_10</hkcstring>"));
    assert_eq!(
        super::super::xml::number_generated_object_ids(&output).unwrap(),
        output
    );
}

#[test]
fn more_than_one_hundred_states_pack_with_distinct_nested_generator_ids() {
    let (mut manifest, mut graph) = ranged_capability();
    let attack = graph
        .roles
        .iter()
        .find(|role| role.role == CreatureClipRole::ProjectileAttack)
        .unwrap()
        .clone();
    while graph.roles.len() < 210 {
        let index = graph.roles.len();
        let event = format!("attackVariant{index}");
        let mut role = attack.clone();
        let mut clip = manifest
            .clips
            .iter()
            .find(|clip| clip.name == role.clip_name)
            .unwrap()
            .clone();
        clip.name = format!("attack_variant_{index}");
        clip.path = format!("Actors\\B21_SkyrimWolf\\Animations\\attack_variant_{index}.hkx");
        role.clip_name = clip.name.clone();
        role.generator = CapabilityRoleGenerator::Single;
        manifest.clips.push(clip);
        role.state_name = format!("AttackVariant{index}");
        role.trigger_event = Some(event.clone());
        graph.roles.push(role);
        graph.explicit_events.push(generic_event(&event));
    }
    graph.roles.swap(1, 10);
    graph.roles[10].generator = CapabilityRoleGenerator::Locomotion {
        directions: vec![CapabilityLocomotionDirection {
            direction_bits: 0.0f32.to_bits(),
            speeds: vec![CapabilityBlenderChild {
                clip_name: graph.roles[10].clip_name.clone(),
                weight_bits: 1.0f32.to_bits(),
                world_from_model_weight_bits: 1.0f32.to_bits(),
            }],
        }],
    };
    graph.roles[100].generator = CapabilityRoleGenerator::Tree {
        root: CapabilityGeneratorNode::ManualSelector {
            source: recursive_source_object(900, "hkbManualSelectorGenerator", "AttackSelector"),
            bindings: Vec::new(),
            children: vec![recursive_clip(901, &graph.roles[100].clip_name)],
            selected_generator_index: 0,
            index_selector: None,
            selected_index_can_change_after_activate: false,
            generator_changed_transition_effect: None,
        },
    };
    graph.roles[0].generator = CapabilityRoleGenerator::MountSynchronization {
        state_index: 0,
        generator: Box::new(CapabilityRoleGenerator::Single),
        speed_sampler_source: recursive_source_object(
            910,
            "BSSpeedSamplerModifier",
            "SpeedSampler",
        ),
        speed_sampler_bindings: Vec::new(),
        turn_damping: CapabilityModifierNode::Damping {
            source: recursive_source_object(911, "hkbDampingModifier", "TurnDamping"),
            bindings: Vec::new(),
            user_data: 0,
            enable: true,
            k_p_bits: 0.2f32.to_bits(),
            k_i_bits: 0,
            k_d_bits: 0,
            enable_scalar_damping: true,
            enable_vector_damping: false,
            raw_value_bits: 0,
            damped_value_bits: 0,
            raw_vector_bits: [0; 4],
            damped_vector_bits: [0; 4],
            vector_error_sum_bits: [0; 4],
            vector_previous_error_bits: [0; 4],
            error_sum_bits: 0,
            previous_error_bits: 0,
        },
    };
    for (name, variable_type, initial_value) in [
        ("Speed", VariableType::Real, VariableValue::Real(0.0)),
        ("Direction", VariableType::Real, VariableValue::Real(0.0)),
        (
            "SyncLocomotionState",
            VariableType::Int32,
            VariableValue::Int32(0),
        ),
        (
            "B21MountRequiresDismount",
            VariableType::Bool,
            VariableValue::Bool(false),
        ),
    ] {
        let variable = VariableDecl {
            name: name.into(),
            variable_type,
            initial_value,
        };
        if !manifest
            .core
            .variables
            .iter()
            .any(|value| value.name == name)
        {
            manifest.core.variables.push(variable.clone());
        }
        if !manifest
            .root
            .variables
            .iter()
            .any(|value| value.name == name)
        {
            manifest.root.variables.push(variable);
        }
    }
    manifest.validate_capability_graph(&graph).unwrap();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    let document = roxmltree::Document::parse(&core.xml).unwrap();
    let mut names = std::collections::HashSet::new();
    for name in document
        .descendants()
        .filter(|node| node.has_tag_name("hkobject"))
        .filter_map(|node| node.attribute("name"))
    {
        assert!(names.insert(name), "duplicate graph object {name}");
    }
    assert_capability_pack_roundtrip(&manifest, &graph);
}
