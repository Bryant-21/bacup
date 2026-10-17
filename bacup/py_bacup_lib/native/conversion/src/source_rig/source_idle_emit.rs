use super::*;

pub(super) fn emit(
    manifest: &CreatureManifest,
    role: &CapabilityClipRole,
    state_index: usize,
    declarations: &GraphDeclarations,
) -> String {
    let CapabilityRoleGenerator::SourceIdle {
        duration_bits,
        loop_start_bits,
        loop_end_bits,
        has_loop_markers,
        looping_min,
        looping_max,
        complete_event,
        ..
    } = &role.generator
    else {
        unreachable!()
    };
    let duration = f32::from_bits(*duration_bits);
    let start = f32::from_bits(*loop_start_bits);
    let end = f32::from_bits(*loop_end_bits);
    let complete = declarations
        .events
        .iter()
        .position(|event| &event.name == complete_event)
        .unwrap();
    let stop = declarations
        .events
        .iter()
        .position(|event| event.name == "IdleStop")
        .unwrap();
    let clip = manifest
        .clips
        .iter()
        .find(|clip| clip.name == role.clip_name)
        .unwrap();
    let mut output = String::new();
    let mut next_object = 0;
    let mut allocate = || {
        let id = format!("#source_idle_{state_index}_{next_object}");
        next_object += 1;
        id
    };
    let counts = if *has_loop_markers {
        source_loop_passes(*looping_min, *looping_max)
    } else {
        vec![(!clip.looping).then_some(1)]
    };
    let mut variants = Vec::new();
    for count in counts {
        let mut segments = Vec::new();
        if start > 0.0 {
            segments.push((0.0, start, false));
        }
        for _ in 0..count.unwrap_or(1) {
            segments.push((start, end, count.is_none()));
        }
        let outro = (end < duration).then(|| {
            let index = segments.len();
            segments.push((end, duration, false));
            index
        });
        let mut states = Vec::new();
        for (index, &(from, to, looping)) in segments.iter().enumerate() {
            let generator = allocate();
            let mut segment_clip = clip.clone();
            segment_clip.looping = looping;
            let mut xml = emit_clip_generator_cropped(
                manifest,
                &generator,
                &format!("{}_segment_{index}", role.state_name),
                &segment_clip,
                from,
                duration - to,
            );
            if index + 1 == segments.len() && !looping {
                let trigger = allocate();
                xml = xml.replace(
                    "<hkparam name=\"triggers\">null</hkparam>",
                    &format!("<hkparam name=\"triggers\">{trigger}</hkparam>"),
                );
                xml.push_str(&format!("<hkobject name=\"{trigger}\" class=\"hkbClipTriggerArray\" signature=\"0xf757cd66\"><hkparam name=\"triggers\" numelements=\"1\"><hkobject><hkparam name=\"localTime\">0.0</hkparam><hkparam name=\"event\"><hkobject><hkparam name=\"id\">{complete}</hkparam><hkparam name=\"payload\">null</hkparam></hkobject></hkparam><hkparam name=\"relativeToEndOfClip\">true</hkparam><hkparam name=\"acyclic\">true</hkparam><hkparam name=\"isAnnotation\">false</hkparam></hkobject></hkparam></hkobject>\n"));
            }
            output.push_str(&xml);
            let mut transitions = Vec::new();
            if !looping && index + 1 < segments.len() {
                transitions.push((0, (index + 1) as i32, true));
            }
            if let Some(outro) = outro
                && index != outro
            {
                transitions.push((stop, outro as i32, false));
            }
            let transition_id = allocate();
            output.push_str(&emit_transition_array(&transition_id, &transitions));
            let state = allocate();
            output.push_str(&emit_state_info(
                &state,
                &format!("SourceIdleSegment{index}"),
                index as i32,
                &transition_id,
                &generator,
                None,
                None,
            ));
            states.push(state);
        }
        let machine = allocate();
        output.push_str(&state_machine(&machine, "SourceIdlePasses", &states, false));
        variants.push(machine);
    }
    let mut states = Vec::new();
    for (index, generator) in variants.iter().enumerate() {
        let state = allocate();
        output.push_str(&emit_state_info(
            &state,
            &format!("SourceIdleVariant{index}"),
            index as i32,
            "null",
            generator,
            None,
            None,
        ));
        states.push(state);
    }
    output.push_str(&state_machine(
        &format!("#010{state_index}"),
        &role.state_name,
        &states,
        states.len() > 1,
    ));
    output
}

fn source_loop_passes(min: u8, max: u8) -> Vec<Option<usize>> {
    if min == 0 || max == 0 {
        vec![Some(1)]
    } else if min == 255 {
        vec![None]
    } else if max <= min {
        vec![Some(usize::from(max))]
    } else {
        (min..max).map(|count| Some(usize::from(count))).collect()
    }
}

fn state_machine(id: &str, name: &str, states: &[String], random: bool) -> String {
    format!(
        "<hkobject name=\"{id}\" class=\"hkbStateMachine\" signature=\"0xa5896bcf\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"eventToSendWhenStateOrTransitionChanges\"><hkobject><hkparam name=\"id\">-1</hkparam><hkparam name=\"payload\">null</hkparam></hkobject></hkparam><hkparam name=\"startStateIdSelector\">null</hkparam><hkparam name=\"startStateId\">0</hkparam><hkparam name=\"returnToPreviousStateEventId\">-1</hkparam><hkparam name=\"randomTransitionEventId\">-1</hkparam><hkparam name=\"transitionToNextHigherStateEventId\">-1</hkparam><hkparam name=\"transitionToNextLowerStateEventId\">-1</hkparam><hkparam name=\"syncVariableIndex\">-1</hkparam><hkparam name=\"wrapAroundStateId\">false</hkparam><hkparam name=\"maxSimultaneousTransitions\">1</hkparam><hkparam name=\"startStateMode\">{mode}</hkparam><hkparam name=\"selfTransitionMode\">SELF_TRANSITION_MODE_NO_TRANSITION</hkparam><hkparam name=\"states\" numelements=\"{count}\">{states}</hkparam><hkparam name=\"wildcardTransitions\">null</hkparam></hkobject>\n",
        name = escape_xml(name),
        mode = if random {
            "START_STATE_MODE_RANDOM"
        } else {
            "START_STATE_MODE_DEFAULT"
        },
        count = states.len(),
        states = states.join(" ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_loop_counts_preserve_native_zero_infinite_and_exclusive_maximum() {
        assert_eq!(source_loop_passes(0, 8), [Some(1)]);
        assert_eq!(source_loop_passes(8, 0), [Some(1)]);
        assert_eq!(source_loop_passes(255, 1), [None]);
        assert_eq!(
            source_loop_passes(4, 8),
            [Some(4), Some(5), Some(6), Some(7)]
        );
        assert_eq!(source_loop_passes(8, 4), [Some(4)]);
    }
}
