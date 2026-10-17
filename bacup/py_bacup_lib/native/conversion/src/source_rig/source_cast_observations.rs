fn validate_cast_observations(
    manifest: &CreatureManifest,
    root: &CapabilityGeneratorNode,
    observations: &[SourceCastingObservation],
    errors: &mut ValidationErrors,
) {
    fn states<'a>(node: &'a CapabilityGeneratorNode, result: &mut BTreeMap<(&'a str, u32), &'a CapabilityGeneratorState>) {
        match node {
            CapabilityGeneratorNode::StateMachine { states: children, .. } => for state in children {
                result.insert((&state.source.behavior_path, state.source.object_index), state);
                states(&state.generator, result);
            },
            CapabilityGeneratorNode::ManualSelector { children, .. } => for child in children { states(child, result); },
            CapabilityGeneratorNode::Blender { children, .. } => for child in children { states(&child.generator, result); },
            CapabilityGeneratorNode::ModifierGenerator { generator, .. }
            | CapabilityGeneratorNode::CyclicBlendTransition { generator, .. } => states(generator, result),
            CapabilityGeneratorNode::Clip { .. } | CapabilityGeneratorNode::SourceClip { .. } => {},
        }
    }
    let mut state_map = BTreeMap::new();
    states(root, &mut state_map);
    let mut owned_states = BTreeSet::new();
    let mut owned_events = BTreeSet::new();
    for observation in observations {
        let key = (observation.source_behavior_path.as_str(), observation.source_state_object_index);
        if !owned_states.insert(key) || !owned_events.insert(&observation.target_event_name) {
            errors.push("casting_observation_identity", "source cast observations must have distinct states and target events");
        }
        if !observation.target_event_name.starts_with("B21_CastBranch_")
            || !manifest.core.events.iter().any(|event| event.name == observation.target_event_name)
        {
            errors.push("casting_observation_event", format!("source cast observation {:?} requires a declared generated target event", observation.target_event_name));
        }
        match state_map.get(&key) {
            Some(state) if state.generator.clip_names().contains(&observation.release_clip_name.as_str()) => {},
            _ => errors.push("casting_observation_state", format!("source cast observation {:?} does not identify a state containing release clip {:?}", observation.target_event_name, observation.release_clip_name)),
        }
    }
}
