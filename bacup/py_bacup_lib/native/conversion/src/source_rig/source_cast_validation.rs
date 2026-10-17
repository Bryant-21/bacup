fn validate_source_clip_node(
    manifest: &CreatureManifest,
    node: &CapabilityGeneratorNode,
    errors: &mut ValidationErrors,
) {
    let CapabilityGeneratorNode::SourceClip {
        source,
        clip_name,
        bindings,
        triggers,
        mode,
        flags,
        crop_start_bits,
        crop_end_bits,
        start_time_bits,
        playback_speed_bits,
        enforced_duration_bits,
        user_controlled_time_fraction_bits,
        ..
    } = node
    else {
        unreachable!()
    };
    if source.class_name != "hkbClipGenerator" {
        errors.push(
            "capability_generator_class",
            "source clip requires hkbClipGenerator",
        );
    }
    validate_identifier("capability.generator.tree.clip", clip_name, errors);
    for binding in bindings {
        if !matches!(
            binding.member_path.as_str(),
            "cropStartAmountLocalTime"
                | "cropEndAmountLocalTime"
                | "startTime"
                | "playbackSpeed"
                | "enforcedDuration"
                | "userControlledTimeFraction"
        ) {
            errors.push(
                "source_clip_binding",
                format!(
                    "source clip field {:?} has no exact scalar binding",
                    binding.member_path
                ),
            );
        }
    }
    validate_generator_bindings(manifest, bindings, None, errors);
    for trigger in triggers {
        validate_generator_event_property(manifest, &trigger.event, errors);
        if !f32::from_bits(trigger.local_time_bits).is_finite() {
            errors.push(
                "source_clip_trigger_time",
                "source clip trigger times must be finite",
            );
        }
    }
    if !(0..=4).contains(mode) || *flags > u8::MAX as u32 {
        errors.push(
            "source_clip_policy",
            "source clip mode or flags is outside the target storage domain",
        );
    }
    if [
        crop_start_bits,
        crop_end_bits,
        start_time_bits,
        playback_speed_bits,
        enforced_duration_bits,
        user_controlled_time_fraction_bits,
    ]
    .into_iter()
    .any(|bits| !f32::from_bits(*bits).is_finite())
        || f32::from_bits(*crop_start_bits) < 0.0
        || f32::from_bits(*crop_end_bits) < 0.0
        || f32::from_bits(*enforced_duration_bits) < 0.0
    {
        errors.push(
            "source_clip_numeric_contract",
            "source clip timing must be finite with nonnegative cropping and enforced duration",
        );
    }
}

fn validate_source_bone_weights(
    manifest: &CreatureManifest,
    weights: &CapabilityGeneratorBoneWeights,
    owned_objects: &mut BTreeSet<(String, u32)>,
    errors: &mut ValidationErrors,
) {
    if owned_objects.contains(&(
        weights.source.behavior_path.to_ascii_lowercase(),
        weights.source.object_index,
    )) {
        return;
    }
    validate_generator_anonymous_source_object(&weights.source, owned_objects, errors);
    if weights.source.class_name != "hkbBoneWeightArray" {
        errors.push(
            "capability_bone_weights_class",
            "bone weights require their exact anonymous source class",
        );
    }
    if weights.weights_bits.len() != manifest.animation_skeleton.bones.len()
        || weights.weights_bits.iter().any(|bits| {
            !f32::from_bits(*bits).is_finite() || !(0.0..=1.0).contains(&f32::from_bits(*bits))
        })
    {
        errors.push(
            "capability_bone_weights",
            "source bone weights must cover the retained source skeleton with finite 0..=1 weights",
        );
    }
    for binding in &weights.bindings {
        let index = binding
            .member_path
            .strip_prefix("boneWeights:")
            .and_then(|index| index.parse::<usize>().ok());
        if index.is_none_or(|index| index >= weights.weights_bits.len())
            || binding.source_variable_type != CapabilitySourceVariableType::Real
        {
            errors.push("capability_bone_weight_binding", "bone weight bindings require an existing source bone index and a real source variable");
        }
    }
    validate_generator_bindings(manifest, &weights.bindings, None, errors);
}

fn validate_source_expression_condition(
    manifest: &CreatureManifest,
    condition: &CapabilityGeneratorExpressionCondition,
    owned_objects: &mut BTreeSet<(String, u32)>,
    errors: &mut ValidationErrors,
) {
    if owned_objects.contains(&(
        condition.source.behavior_path.to_ascii_lowercase(),
        condition.source.object_index,
    )) {
        return;
    }
    validate_generator_anonymous_source_object(&condition.source, owned_objects, errors);
    if condition.source.class_name != "hkbExpressionCondition" {
        errors.push(
            "capability_expression_condition_class",
            "transition conditions require their exact anonymous source class",
        );
    }
    validate_name(
        "capability.generator.condition.expression",
        &condition.expression,
        errors,
    );
    let mut variables = BTreeSet::new();
    for variable in &condition.referenced_variables {
        if !variables.insert(variable.variable_index) {
            errors.push(
                "duplicate_capability_expression_variable",
                "condition repeats a source variable receipt",
            );
        }
        validate_generator_source_variable(
            manifest,
            variable.variable_index,
            &variable.variable_name,
            variable.source_variable_type,
            variable.initial_word_value,
            errors,
        );
        if !contains_identifier(&condition.expression, &variable.variable_name) {
            errors.push(
                "capability_expression_variable_boundary",
                "condition receipt does not identify an expression variable",
            );
        }
    }
    for (index, variable) in manifest.core.variables.iter().enumerate() {
        if contains_identifier(&condition.expression, &variable.name)
            && !variables.contains(&(index as i32))
        {
            errors.push(
                "missing_capability_expression_variable_receipt",
                format!(
                    "condition variable {:?} has no source receipt",
                    variable.name
                ),
            );
        }
    }
}

fn validate_shared_source_receipts(root: &CapabilityGeneratorNode, errors: &mut ValidationErrors) {
    fn visit(
        value: &serde_json::Value,
        receipts: &mut BTreeMap<(String, u64), serde_json::Value>,
        errors: &mut ValidationErrors,
    ) {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(source) = object.get("source") {
                    if let (Some(path), Some(index)) = (
                        source
                            .get("behavior_path")
                            .and_then(serde_json::Value::as_str),
                        source
                            .get("object_index")
                            .and_then(serde_json::Value::as_u64),
                    ) {
                        let key = (path.to_ascii_lowercase(), index);
                        if let Some(previous) = receipts.get(&key) {
                            if previous != value {
                                errors.push("conflicting_generator_source_object", format!("source object {path}#{index} has conflicting repeated payloads"));
                            }
                        } else {
                            receipts.insert(key, value.clone());
                        }
                    }
                }
                for child in object.values() {
                    visit(child, receipts, errors);
                }
            }
            serde_json::Value::Array(values) => {
                for child in values {
                    visit(child, receipts, errors);
                }
            }
            _ => {}
        }
    }
    let value =
        serde_json::to_value(root).expect("source graph contains serializable scalar receipts");
    visit(&value, &mut BTreeMap::new(), errors);
}
