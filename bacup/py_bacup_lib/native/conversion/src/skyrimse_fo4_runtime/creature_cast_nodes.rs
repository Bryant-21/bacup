use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoneWeights {
    pub source: SkyrimBehaviorObjectEvidence,
    pub bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
    pub weights_bits: Option<Vec<u32>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionCondition {
    pub source: SkyrimBehaviorObjectEvidence,
    pub expression: Option<String>,
    pub referenced_variables: Vec<SkyrimBehaviorExpressionVariableReferenceEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsActive {
    pub source: SkyrimBehaviorObjectEvidence,
    pub bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
    pub user_data: Option<u64>,
    pub enable: Option<bool>,
    pub active: [Option<bool>; 5],
    pub invert: [Option<bool>; 5],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CyclicBlendTransition {
    pub source: SkyrimBehaviorObjectEvidence,
    pub bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
    pub user_data: Option<u64>,
    pub generator: Option<Box<SkyrimBehaviorGeneratorNodeEvidence>>,
    pub freeze: Option<SkyrimBehaviorEventPropertyEvidence>,
    pub cross_blend: Option<SkyrimBehaviorEventPropertyEvidence>,
    pub blend_parameter_bits: Option<u32>,
    pub transition_duration_bits: Option<u32>,
    pub blend_curve: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipPlayback {
    pub bindings: Vec<SkyrimBehaviorVariableBindingEvidence>,
    pub user_data: Option<u64>,
    pub mode: Option<i32>,
    pub flags: Option<i32>,
    pub crop_start_bits: Option<u32>,
    pub crop_end_bits: Option<u32>,
    pub start_time_bits: Option<u32>,
    pub playback_speed_bits: Option<u32>,
    pub enforced_duration_bits: Option<u32>,
    pub user_controlled_time_fraction_bits: Option<u32>,
    pub triggers: Vec<ClipTrigger>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipTrigger {
    pub local_time_bits: Option<u32>,
    pub event: Option<SkyrimBehaviorEventPropertyEvidence>,
    pub relative_to_end_of_clip: Option<bool>,
    pub acyclic: Option<bool>,
    pub is_annotation: Option<bool>,
}

fn bool_members(members: &[HkxMember], name: &str) -> Option<bool> {
    members.iter().find_map(|member| {
        (member.name == name).then(|| match member.value {
            HkxValue::Bool(value) => Some(value),
            HkxValue::U8(value) if value <= 1 => Some(value != 0),
            HkxValue::I8(value) if (0..=1).contains(&value) => Some(value != 0),
            _ => None,
        })?
    })
}

pub(super) fn clip_triggers(
    file: &HkxFile,
    object: &HkxObject,
    events: &[String],
) -> Vec<ClipTrigger> {
    pointer_member(object, "triggers")
        .and_then(|index| file.objects().get(index))
        .and_then(|object| array_member(object, "triggers"))
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_object_members())
        .map(|members| ClipTrigger {
            local_time_bits: float_members(members, "localTime").map(f32::to_bits),
            event: member_object(members, "event")
                .map(|members| behavior_event_property(file, members, events)),
            relative_to_end_of_clip: bool_members(members, "relativeToEndOfClip"),
            acyclic: bool_members(members, "acyclic"),
            is_annotation: bool_members(members, "isAnnotation"),
        })
        .collect()
}

pub(super) fn bone_weights(
    file: &HkxFile,
    index: usize,
    variable_names: &[String],
    variable_types: &[Option<i32>],
    initial_words: &[Option<u32>],
) -> Option<BoneWeights> {
    let object = file.objects().get(index)?;
    let member = object
        .members
        .iter()
        .find(|member| member.name == "boneWeights");
    let weights_bits = match member.map(|member| &member.value) {
        Some(HkxValue::F32List(values)) => Some(values.iter().map(|v| v.to_bits()).collect()),
        Some(HkxValue::Array(values)) => values
            .iter()
            .map(|value| match value {
                HkxValue::F32(value) | HkxValue::Half(value) => Some(value.to_bits()),
                _ => None,
            })
            .collect(),
        _ => None,
    };
    Some(BoneWeights {
        source: behavior_object_evidence(file, index)?,
        bindings: behavior_variable_bindings(
            file,
            object,
            variable_names,
            variable_types,
            initial_words,
        ),
        weights_bits,
    })
}

pub(super) fn expression_condition(
    file: &HkxFile,
    index: usize,
    variable_names: &[String],
    variable_types: &[Option<i32>],
    initial_words: &[Option<u32>],
) -> Option<ExpressionCondition> {
    let object = file.objects().get(index)?;
    let expression = string_member(object, "expression");
    let referenced_variables = variable_names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            expression
                .as_deref()
                .is_some_and(|expression| expression_contains_identifier(expression, name))
                .then(|| SkyrimBehaviorExpressionVariableReferenceEvidence {
                    variable_index: index,
                    variable_name: name.clone(),
                    variable_type: variable_types.get(index).copied().flatten(),
                    initial_word_value: initial_words.get(index).copied().flatten(),
                })
        })
        .collect();
    Some(ExpressionCondition {
        source: behavior_object_evidence(file, index)?,
        expression,
        referenced_variables,
    })
}

pub(super) fn lower_bone_weights(
    family: &SkyrimFamilyMotionSet,
    path: &str,
    weights: &BoneWeights,
) -> Result<crate::source_rig::CapabilityGeneratorBoneWeights, SkyrimCreatureMotionError> {
    Ok(crate::source_rig::CapabilityGeneratorBoneWeights {
        source: capability_generator_anonymous_source(family, path, &weights.source)?,
        bindings: capability_generator_bindings(family, &weights.bindings)?,
        weights_bits: required_capability_value(
            family,
            weights.weights_bits.clone(),
            "boneWeights",
        )?,
    })
}

pub(super) fn lower_condition(
    family: &SkyrimFamilyMotionSet,
    path: &str,
    condition: &ExpressionCondition,
) -> Result<crate::source_rig::CapabilityGeneratorExpressionCondition, SkyrimCreatureMotionError> {
    Ok(crate::source_rig::CapabilityGeneratorExpressionCondition {
        source: capability_generator_anonymous_source(family, path, &condition.source)?,
        expression: required_capability_value(
            family,
            condition.expression.clone(),
            "condition.expression",
        )?,
        referenced_variables: condition
            .referenced_variables
            .iter()
            .map(|reference| capability_expression_variable_reference(family, reference))
            .collect::<Result<_, _>>()?,
    })
}

pub(super) fn lower_is_active(
    family: &SkyrimFamilyMotionSet,
    path: &str,
    modifier: &IsActive,
) -> Result<CapabilityModifierNode, SkyrimCreatureMotionError> {
    let mut active = [false; 5];
    let mut invert = [false; 5];
    for i in 0..5 {
        active[i] = required_capability_value(family, modifier.active[i], "isActive.bIsActive")?;
        invert[i] =
            required_capability_value(family, modifier.invert[i], "isActive.bInvertActive")?;
    }
    Ok(CapabilityModifierNode::IsActive {
        source: capability_generator_source(family, path, &modifier.source)?,
        bindings: capability_generator_bindings(family, &modifier.bindings)?,
        user_data: required_capability_value(family, modifier.user_data, "isActive.userData")?,
        enable: required_capability_value(family, modifier.enable, "isActive.enable")?,
        active,
        invert,
    })
}

pub(super) fn lower_cyclic(
    family: &SkyrimFamilyMotionSet,
    path: &str,
    node: &CyclicBlendTransition,
    clips: &[&SkyrimCatalogClip],
) -> Result<CapabilityGeneratorNode, SkyrimCreatureMotionError> {
    let event = |value: &Option<SkyrimBehaviorEventPropertyEvidence>, label| {
        capability_event_property(
            family,
            path,
            required_capability_value(family, value.as_ref(), label)?,
            label,
        )
    };
    let disabled = CapabilityGeneratorEventProperty {
        event: CapabilityGeneratorEventRef {
            source_id: -1,
            event_name: None,
        },
        payload: None,
    };
    Ok(CapabilityGeneratorNode::CyclicBlendTransition {
        source: capability_generator_source(family, path, &node.source)?,
        bindings: capability_generator_bindings(family, &node.bindings)?,
        user_data: required_capability_value(family, node.user_data, "cyclic.userData")?,
        generator: Box::new(capability_generator_node(
            family,
            path,
            required_capability_value(
                family,
                node.generator.as_deref(),
                "cyclic.pBlenderGenerator",
            )?,
            clips,
        )?),
        event_to_freeze_blend_value: event(&node.freeze, "cyclic.EventToFreezeBlendValue")?,
        event_to_cross_blend: event(&node.cross_blend, "cyclic.EventToCrossBlend")?,
        transition_out_event: disabled.clone(),
        transition_in_event: disabled,
        f_blend_parameter_bits: required_capability_value(
            family,
            node.blend_parameter_bits,
            "cyclic.fBlendParameter",
        )?,
        f_transition_duration_bits: required_capability_value(
            family,
            node.transition_duration_bits,
            "cyclic.fTransitionDuration",
        )?,
        e_blend_curve: required_i32(family, node.blend_curve, "cyclic.eBlendCurve")?,
    })
}

pub(super) fn lower_clip(
    family: &SkyrimFamilyMotionSet,
    path: &str,
    source: &SkyrimBehaviorObjectEvidence,
    clip_name: &str,
    playback: &ClipPlayback,
) -> Result<CapabilityGeneratorNode, SkyrimCreatureMotionError> {
    Ok(CapabilityGeneratorNode::SourceClip {
        source: capability_generator_source(family, path, source)?,
        clip_name: clip_name.into(),
        bindings: capability_generator_bindings(family, &playback.bindings)?,
        user_data: required_capability_value(family, playback.user_data, "clip.userData")?,
        mode: required_i32(family, playback.mode, "clip.mode")?,
        flags: u32::try_from(required_i32(family, playback.flags, "clip.flags")?).map_err(
            |_| SkyrimCreatureMotionError::AdapterUnavailable {
                family_id: family.family_id.clone(),
                target: "clip",
                detail: "negative source flags".into(),
            },
        )?,
        crop_start_bits: required_capability_value(
            family,
            playback.crop_start_bits,
            "clip.cropStartAmountLocalTime",
        )?,
        crop_end_bits: required_capability_value(
            family,
            playback.crop_end_bits,
            "clip.cropEndAmountLocalTime",
        )?,
        start_time_bits: required_capability_value(
            family,
            playback.start_time_bits,
            "clip.startTime",
        )?,
        playback_speed_bits: required_capability_value(
            family,
            playback.playback_speed_bits,
            "clip.playbackSpeed",
        )?,
        enforced_duration_bits: required_capability_value(
            family,
            playback.enforced_duration_bits,
            "clip.enforcedDuration",
        )?,
        user_controlled_time_fraction_bits: required_capability_value(
            family,
            playback.user_controlled_time_fraction_bits,
            "clip.userControlledTimeFraction",
        )?,
        triggers: playback
            .triggers
            .iter()
            .map(|trigger| {
                Ok(crate::source_rig::CapabilityGeneratorClipTrigger {
                    local_time_bits: required_capability_value(
                        family,
                        trigger.local_time_bits,
                        "clip.trigger.localTime",
                    )?,
                    event: capability_event_property(
                        family,
                        path,
                        required_capability_value(
                            family,
                            trigger.event.as_ref(),
                            "clip.trigger.event",
                        )?,
                        "clip.trigger.event",
                    )?,
                    relative_to_end_of_clip: required_capability_value(
                        family,
                        trigger.relative_to_end_of_clip,
                        "clip.trigger.relativeToEndOfClip",
                    )?,
                    acyclic: required_capability_value(
                        family,
                        trigger.acyclic,
                        "clip.trigger.acyclic",
                    )?,
                    is_annotation: required_capability_value(
                        family,
                        trigger.is_annotation,
                        "clip.trigger.isAnnotation",
                    )?,
                })
            })
            .collect::<Result<_, SkyrimCreatureMotionError>>()?,
    })
}
