use super::*;

pub(super) fn declarations(
    family: &SkyrimFamilyMotionSet,
    inventory: &SkyrimCreatureFamilyInventory,
    roles: &mut [CapabilityClipRole],
) -> Result<Vec<VariableDecl>, SkyrimCreatureMotionError> {
    let mut used = BTreeMap::<String, VariableDecl>::new();
    visit_roles(roles, &mut |path, index| {
        let evidence = evidence(family, inventory, path, *index)?;
        let declaration = capability_variable_declaration(family, evidence)?;
        if let Some(previous) = used.get(&declaration.name) {
            if previous != &declaration {
                return adapter_unavailable(
                    family,
                    "graph variables",
                    format!(
                        "source variable {:?} differs across used behavior tables",
                        declaration.name
                    ),
                );
            }
        } else {
            used.insert(declaration.name.clone(), declaration);
        }
        Ok(())
    })?;
    let declarations = used.into_values().collect::<Vec<_>>();
    visit_roles(roles, &mut |path, index| {
        let source = evidence(family, inventory, path, *index)?;
        let name = source
            .variable_name
            .as_deref()
            .expect("used source variable was validated");
        *index = declarations
            .iter()
            .position(|variable| variable.name == name)
            .expect("used source variable was collected") as i32;
        Ok(())
    })?;
    Ok(declarations)
}

fn evidence<'a>(
    family: &SkyrimFamilyMotionSet,
    inventory: &'a SkyrimCreatureFamilyInventory,
    path: &str,
    index: i32,
) -> Result<&'a SkyrimBehaviorVariableEvidence, SkyrimCreatureMotionError> {
    let matching = inventory
        .behavior_variables
        .iter()
        .filter(|variable| {
            variable.variable_index as i32 == index
                && path_key(&variable.behavior_path) == path_key(path)
        })
        .collect::<Vec<_>>();
    let [variable] = matching.as_slice() else {
        return adapter_unavailable(
            family,
            "graph variables",
            format!(
                "{path} variable {index} has {} exact source receipts",
                matching.len()
            ),
        );
    };
    Ok(*variable)
}

type VisitResult = std::result::Result<(), SkyrimCreatureMotionError>;
type Visitor<'a> = dyn FnMut(&str, &mut i32) -> VisitResult + 'a;

fn visit_roles(roles: &mut [CapabilityClipRole], visit: &mut Visitor<'_>) -> VisitResult {
    for role in roles {
        generator(&mut role.generator, visit)?;
    }
    Ok(())
}

fn generator(generator: &mut CapabilityRoleGenerator, visit: &mut Visitor<'_>) -> VisitResult {
    match generator {
        CapabilityRoleGenerator::Tree { root }
        | CapabilityRoleGenerator::SourceCasting { root, .. } => node(root, visit),
        CapabilityRoleGenerator::SourceAttack { generator, .. }
        | CapabilityRoleGenerator::SourceMovement { generator, .. }
        | CapabilityRoleGenerator::MountSynchronization { generator, .. } => {
            self::generator(generator, visit)
        }
        _ => Ok(()),
    }
}

fn bindings(
    path: &str,
    bindings: &mut [CapabilityGeneratorVariableBinding],
    visit: &mut Visitor<'_>,
) -> VisitResult {
    for binding in bindings {
        if binding.binding_type == CapabilityVariableBindingType::Variable {
            visit(path, &mut binding.variable_index)?;
        }
    }
    Ok(())
}

fn effect(
    effect: &mut CapabilityGeneratorBlendingTransitionEffect,
    visit: &mut Visitor<'_>,
) -> VisitResult {
    bindings(&effect.source.behavior_path, &mut effect.bindings, visit)
}

fn transitions(
    array: &mut CapabilityGeneratorTransitionArray,
    visit: &mut Visitor<'_>,
) -> VisitResult {
    for transition in &mut array.transitions {
        if let Some(value) = &mut transition.transition_effect {
            effect(value, visit)?;
        }
        if let Some(condition) = &mut transition.condition {
            for reference in &mut condition.referenced_variables {
                visit(
                    &condition.source.behavior_path,
                    &mut reference.variable_index,
                )?;
            }
        }
    }
    Ok(())
}

fn node(node: &mut CapabilityGeneratorNode, visit: &mut Visitor<'_>) -> VisitResult {
    let (source, node_bindings) = match node {
        CapabilityGeneratorNode::Clip { .. } => return Ok(()),
        CapabilityGeneratorNode::SourceClip {
            source, bindings, ..
        }
        | CapabilityGeneratorNode::ManualSelector {
            source, bindings, ..
        }
        | CapabilityGeneratorNode::Blender {
            source, bindings, ..
        }
        | CapabilityGeneratorNode::StateMachine {
            source, bindings, ..
        }
        | CapabilityGeneratorNode::CyclicBlendTransition {
            source, bindings, ..
        }
        | CapabilityGeneratorNode::ModifierGenerator {
            source, bindings, ..
        } => (source, bindings),
    };
    bindings(&source.behavior_path, node_bindings, visit)?;
    match node {
        CapabilityGeneratorNode::Clip { .. } | CapabilityGeneratorNode::SourceClip { .. } => {}
        CapabilityGeneratorNode::ManualSelector { children, .. } => {
            for child in children {
                self::node(child, visit)?;
            }
        }
        CapabilityGeneratorNode::Blender { children, .. } => {
            for child in children {
                if let Some(weights) = &mut child.bone_weights {
                    bindings(&weights.source.behavior_path, &mut weights.bindings, visit)?;
                }
                self::node(&mut child.generator, visit)?;
            }
        }
        CapabilityGeneratorNode::StateMachine {
            source,
            sync_variable_index,
            states,
            wildcard_transitions,
            ..
        } => {
            if *sync_variable_index >= 0 {
                visit(&source.behavior_path, sync_variable_index)?;
            }
            for state in states {
                self::node(&mut state.generator, visit)?;
                if let Some(array) = &mut state.transitions {
                    transitions(array, visit)?;
                }
            }
            if let Some(array) = wildcard_transitions {
                transitions(array, visit)?;
            }
        }
        CapabilityGeneratorNode::CyclicBlendTransition { generator, .. } => {
            self::node(generator, visit)?
        }
        CapabilityGeneratorNode::ModifierGenerator {
            modifier,
            generator,
            ..
        } => {
            self::modifier(modifier, visit)?;
            self::node(generator, visit)?;
        }
    }
    Ok(())
}

fn modifier(modifier: &mut CapabilityModifierNode, visit: &mut Visitor<'_>) -> VisitResult {
    let (source, node_bindings) = match modifier {
        CapabilityModifierNode::List {
            source, bindings, ..
        }
        | CapabilityModifierNode::Damping {
            source, bindings, ..
        }
        | CapabilityModifierNode::EvaluateExpression {
            source, bindings, ..
        }
        | CapabilityModifierNode::IsActive {
            source, bindings, ..
        } => (source, bindings),
    };
    bindings(&source.behavior_path, node_bindings, visit)?;
    match modifier {
        CapabilityModifierNode::List { modifiers, .. } => {
            for child in modifiers {
                self::modifier(child, visit)?;
            }
        }
        CapabilityModifierNode::EvaluateExpression { expressions, .. } => {
            for expression in &mut expressions.expressions {
                for reference in &mut expression.referenced_variables {
                    visit(
                        &expressions.source.behavior_path,
                        &mut reference.variable_index,
                    )?;
                }
                if expression.assignment_variable_index >= 0 {
                    visit(
                        &expressions.source.behavior_path,
                        &mut expression.assignment_variable_index,
                    )?;
                }
            }
        }
        CapabilityModifierNode::Damping { .. } | CapabilityModifierNode::IsActive { .. } => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn used_variables_are_remapped_without_importing_unused_vector_slots() {
        let family = SkyrimFamilyMotionSet {
            family_id: "fixture".into(),
            project_path: "fixture.hkx".into(),
            race_attacks: vec![],
            race_attack_sources: vec![],
            clip_ids: vec![],
        };
        let inventory = SkyrimCreatureFamilyInventory {
            behavior_variables: vec![
                SkyrimBehaviorVariableEvidence {
                    behavior_path: "fixture.hkx".into(),
                    variable_index: 0,
                    variable_name: Some("UnusedTargetLocation".into()),
                    variable_type: Some(7),
                    initial_word_value: Some(0),
                },
                SkyrimBehaviorVariableEvidence {
                    behavior_path: "fixture.hkx".into(),
                    variable_index: 2,
                    variable_name: Some("SelectedSourceState".into()),
                    variable_type: Some(3),
                    initial_word_value: Some(1),
                },
            ],
            ..Default::default()
        };
        let source = CapabilityGeneratorSourceObject {
            behavior_path: "fixture.hkx".into(),
            object_index: 1,
            class_name: "hkbManualSelectorGenerator".into(),
            name: "Selector".into(),
        };
        let mut roles = vec![CapabilityClipRole {
            role: CreatureClipRole::GroundForward,
            equipment_index: None,
            state_name: "Forward".into(),
            clip_name: "Forward".into(),
            trigger_event: Some("moveStart".into()),
            trigger_aliases: vec![],
            motion: ClipMotionPolicy::default(),
            generator: CapabilityRoleGenerator::Tree {
                root: CapabilityGeneratorNode::ManualSelector {
                    source,
                    bindings: vec![CapabilityGeneratorVariableBinding {
                        member_path: "selectedGeneratorIndex".into(),
                        variable_index: 2,
                        variable_name: "SelectedSourceState".into(),
                        bit_index: -1,
                        binding_type: CapabilityVariableBindingType::Variable,
                        source_variable_type: CapabilitySourceVariableType::Int32,
                        initial_word_value: Some(1),
                    }],
                    children: vec![],
                    selected_generator_index: 1,
                    index_selector: None,
                    selected_index_can_change_after_activate: true,
                    generator_changed_transition_effect: None,
                },
            },
        }];
        let variables = declarations(&family, &inventory, &mut roles).unwrap();
        assert_eq!(
            variables,
            vec![VariableDecl {
                name: "SelectedSourceState".into(),
                variable_type: VariableType::Int32,
                initial_value: VariableValue::Int32(1)
            }]
        );
        let CapabilityRoleGenerator::Tree {
            root: CapabilityGeneratorNode::ManualSelector { bindings, .. },
        } = &roles[0].generator
        else {
            panic!("expected source selector");
        };
        assert_eq!(bindings[0].variable_index, 0);
        assert_eq!(bindings[0].variable_name, "SelectedSourceState");
    }
}
