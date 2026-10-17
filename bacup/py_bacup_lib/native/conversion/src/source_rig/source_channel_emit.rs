use super::*;

#[path = "source_channel_plan.rs"]
mod plan;

fn variable(declarations: &GraphDeclarations, name: &str) -> usize {
    declarations
        .variables
        .iter()
        .position(|item| item.name == name)
        .unwrap()
}

fn default_state(plan: &plan::Plan, members: &[usize]) -> usize {
    members
        .iter()
        .copied()
        .find(|index| *index == 0)
        .map_or(0, |index| index + 1)
}

fn assignments(plan: &plan::Plan) -> Vec<(String, String)> {
    let mut result = plan.assignments.clone();
    for (channel, members) in &plan.channels {
        for bone in 0..plan.role_weights[0].len() {
            result.push((
                format!("B21SourceChannelWeight{channel}_{bone}"),
                members
                    .iter()
                    .map(|role| plan.role_weights[*role][bone].clone())
                    .collect::<Vec<_>>()
                    .join(" + "),
            ));
        }
        let motion = plan
            .samples
            .iter()
            .enumerate()
            .filter(|(_, sample)| members.contains(&sample.role))
            .map(|(index, _)| plan.motion_weights[index].clone())
            .collect::<Vec<_>>();
        result.push((
            format!("B21SourceChannelMotion{channel}"),
            motion.join(" + "),
        ));
    }
    let attack = plan
        .roles
        .iter()
        .enumerate()
        .filter(|(_, role)| is_attack(role))
        .map(|(index, _)| {
            format!(
                "({} == {}) * {}",
                plan::state_variable(4),
                index + 1,
                index + 1
            )
        })
        .collect::<Vec<_>>();
    result.push((
        "B21SourceAttackActive".into(),
        if attack.is_empty() {
            "0".into()
        } else {
            attack.join(" + ")
        },
    ));
    result.push((
        "B21SourceMovementActive".into(),
        if plan.channels.contains_key(&1) {
            format!("({} != 0)", plan::state_variable(1))
        } else {
            "0".into()
        },
    ));
    let gates = plan
        .roles
        .iter()
        .filter_map(|role| role.generator.source_attack())
        .map(|contract| (contract.gate_variable.clone(), contract))
        .collect::<BTreeMap<_, _>>();
    for (name, contract) in gates {
        let opening = contract.opening_variable.as_ref().map_or(
            contract.opening_group.to_string(),
            |variable| {
                format!(
                    "B21SourceMovementActive * {variable} + (B21SourceMovementActive == 0) * {}",
                    contract.opening_group
                )
            },
        );
        result.push((
            name,
            format!("(B21SourceAttackActive == 0) * ({opening}) - (B21SourceAttackActive != 0)"),
        ));
    }
    result
}

pub(super) fn add_declarations(
    manifest: &CreatureManifest,
    graph: &CapabilityGraphManifest,
    declarations: &mut GraphDeclarations,
) {
    let plan = plan::build(manifest, graph);
    let mut push = |name: String, variable_type, initial_value| {
        if !declarations
            .variables
            .iter()
            .any(|variable| variable.name == name)
        {
            declarations.variables.push(VariableDecl {
                name,
                variable_type,
                initial_value,
            });
        }
    };
    push(
        "B21SourceChannels".into(),
        VariableType::Bool,
        VariableValue::Bool(true),
    );
    push(
        "B21SourcePoseBoneCount".into(),
        VariableType::Int32,
        VariableValue::Int32(manifest.animation_skeleton.bones.len() as i32),
    );
    for (channel, members) in &plan.channels {
        push(
            plan::state_variable(*channel),
            VariableType::Int32,
            VariableValue::Int32(default_state(&plan, members) as i32),
        );
    }
    for name in ["B21SourceAttackActive", "B21SourceMovementActive"] {
        push(name.into(), VariableType::Int32, VariableValue::Int32(0));
    }
    for (name, _) in assignments(&plan) {
        push(name, VariableType::Real, VariableValue::Real(0.0));
    }
}

fn is_attack(role: &CapabilityClipRole) -> bool {
    matches!(
        role.role,
        CreatureClipRole::MeleeAttack
            | CreatureClipRole::ProjectileAttack
            | CreatureClipRole::ContinuousAttackStart
            | CreatureClipRole::ContinuousAttackLoop
            | CreatureClipRole::ContinuousAttackStop
    )
}

#[derive(Clone)]
struct Transition {
    event: i32,
    target: usize,
    completion: bool,
    condition: Option<String>,
}

fn channel_transitions(
    graph: &CapabilityGraphManifest,
    plan: &plan::Plan,
    channel: u8,
    current: Option<usize>,
    legacy: &[Vec<(usize, i32, bool)>],
    declarations: &GraphDeclarations,
) -> Vec<Transition> {
    let event = |name: &str| {
        declarations
            .events
            .iter()
            .position(|item| item.name == name)
            .unwrap() as i32
    };
    let members = &plan.channels[&channel];
    let fallback = default_state(plan, members);
    if current.is_some_and(|index| plan.roles[index].role == CreatureClipRole::Death) {
        return Vec::new();
    }
    let mut transitions = Vec::new();
    let mut push = |event: i32, target: usize, completion: bool, condition: Option<String>| {
        if !transitions.iter().any(|row: &Transition| {
            row.event == event && row.completion == completion && row.condition == condition
        }) {
            transitions.push(Transition {
                event,
                target,
                completion,
                condition,
            });
        }
    };
    let source = current
        .map(|index| legacy[index].clone())
        .unwrap_or_else(|| {
            members
                .iter()
                .copied()
                .filter(|index| {
                    plan.roles[*index].generator.is_attack_entry()
                        || !is_attack(&plan.roles[*index])
                })
                .flat_map(|index| {
                    plan.roles[index]
                        .trigger_event
                        .iter()
                        .chain(&plan.roles[index].trigger_aliases)
                        .map(move |name| (event(name) as usize, index as i32, false))
                })
                .collect()
        });
    for (requested, target, completion) in source {
        let target = target as usize;
        if completion {
            push(
                -1,
                if members.contains(&target) {
                    target + 1
                } else {
                    fallback
                },
                true,
                None,
            );
            continue;
        }
        if members.contains(&target) {
            let condition = plan.roles[target]
                .generator
                .source_attack()
                .map(|contract| format!("{} == {}", contract.gate_variable, contract.group));
            push(requested as i32, target + 1, false, condition);
        } else if target == 0 {
            push(requested as i32, fallback, false, None);
        }
    }
    for role in &plan.roles {
        let clear_group = match role.generator.base_generator() {
            CapabilityRoleGenerator::SourceIdle { source_group, .. } => Some(*source_group),
            _ => (role.role == CreatureClipRole::Death).then_some(20),
        };
        let clear = match clear_group {
            Some(20) => channel <= 7 && plan::channel(graph, role) != channel,
            Some(21) => matches!(channel, 2 | 3),
            _ => false,
        };
        if clear {
            for name in role.trigger_event.iter().chain(&role.trigger_aliases) {
                push(event(name), 0, false, None);
            }
        }
    }
    if let Some(index) = current {
        let role = &plan.roles[index];
        if let Some(overlay) = graph
            .overlays
            .iter()
            .find(|overlay| overlay.name == role.state_name)
        {
            push(event(&overlay.stop_event), fallback, false, None);
        }
        if is_attack(role)
            && !matches!(
                role.generator.base_generator(),
                CapabilityRoleGenerator::ContinuousAttackPhase { .. }
            )
        {
            if let Some(stop) = declarations
                .events
                .iter()
                .position(|item| item.name == "attackStop")
            {
                push(stop as i32, fallback, false, None);
            }
        }
    }
    let blocked = plan
        .roles
        .iter()
        .enumerate()
        .filter(|(_, role)| {
            matches!(
                role.generator.base_generator(),
                CapabilityRoleGenerator::SourceIdle {
                    source_group: 20,
                    ..
                }
            ) || role.role == CreatureClipRole::Death
        })
        .map(|(index, role)| {
            format!(
                "({} != {})",
                plan::state_variable(plan::channel(graph, role)),
                index + 1
            )
        })
        .collect::<Vec<_>>();
    let unblocked = if blocked.is_empty() {
        "1".into()
    } else {
        blocked.join(" * ")
    };
    if current.is_none() && channel == 0 && fallback != 0 {
        push(-1, fallback, false, Some(unblocked.clone()));
    }
    if channel == 4
        && current.is_none_or(|index| {
            matches!(
                plan.roles[index].generator.base_generator(),
                CapabilityRoleGenerator::SourceAim { .. }
            )
        })
    {
        for &index in members {
            if let CapabilityRoleGenerator::SourceAim { bank_index, .. } =
                plan.roles[index].generator.base_generator()
            {
                if current != Some(index) {
                    push(
                        -1,
                        index + 1,
                        false,
                        Some(format!(
                            "({unblocked}) * (B21SourceWeaponBank == {bank_index})"
                        )),
                    );
                }
            }
        }
        if current.is_some() {
            push(-1, 0, false, Some("B21SourceWeaponBank == 0".into()));
        }
    }
    transitions
}

fn transition_array(id: &str, transitions: &[Transition], _: &[CapabilityClipRole]) -> String {
    let mut objects = String::new();
    let rows=transitions.iter().enumerate().map(|(index,row)|{
        let condition=row.condition.as_ref().map(|expression|{let name=format!("{id}_condition{index}");objects.push_str(&format!("<hkobject name=\"{name}\" class=\"hkbExpressionCondition\" signature=\"0x78a69526\"><hkparam name=\"expression\">{}</hkparam></hkobject>\n",escape_xml(expression)));name}).unwrap_or("null".into());
        format!("<hkobject><hkparam name=\"triggerInterval\"><hkobject><hkparam name=\"enterEventId\">-1</hkparam><hkparam name=\"exitEventId\">-1</hkparam><hkparam name=\"enterTime\">0</hkparam><hkparam name=\"exitTime\">0</hkparam></hkobject></hkparam><hkparam name=\"initiateInterval\"><hkobject><hkparam name=\"enterEventId\">-1</hkparam><hkparam name=\"exitEventId\">-1</hkparam><hkparam name=\"enterTime\">0</hkparam><hkparam name=\"exitTime\">0</hkparam></hkobject></hkparam><hkparam name=\"transition\">null</hkparam><hkparam name=\"condition\">{condition}</hkparam><hkparam name=\"eventId\">{event}</hkparam><hkparam name=\"toStateId\">{target}</hkparam><hkparam name=\"fromNestedStateId\">0</hkparam><hkparam name=\"toNestedStateId\">0</hkparam><hkparam name=\"priority\">0</hkparam><hkparam name=\"flags\">{flags}</hkparam></hkobject>",event=row.event,target=row.target,flags=if row.completion{16384}else{0})
    }).collect::<String>();
    format!(
        "<hkobject name=\"{id}\" class=\"hkbStateMachineTransitionInfoArray\" signature=\"0x704a19af\"><hkparam name=\"transitions\" numelements=\"{}\">{rows}</hkparam></hkobject>\n{objects}",
        transitions.len()
    )
}

fn state_notifications(
    role: &CapabilityClipRole,
    index: usize,
    declarations: &GraphDeclarations,
) -> (String, Option<String>, Option<String>) {
    let event = |name: &str| {
        declarations
            .events
            .iter()
            .position(|item| item.name == name)
    };
    let row = |index: usize| {
        format!(
            "<hkobject><hkparam name=\"id\">{index}</hkparam><hkparam name=\"payload\">null</hkparam></hkobject>"
        )
    };
    let mut enter = Vec::new();
    let mut exit = Vec::new();
    if role.motion.animation_driven {
        if let Some(index) = event("startAnimationDriven") {
            enter.push(row(index));
        }
    }
    if role
        .generator
        .source_attack()
        .is_some_and(|contract| contract.window.is_some())
    {
        let close =
            crate::source_rig::SourceAttackContract::window_events(&role.state_name)[1].clone();
        exit.push(row(event(&close).unwrap()));
    }
    let enter_id = format!("#source_channel_enter{index}");
    let exit_id = format!("#source_channel_exit{index}");
    let mut xml = String::new();
    let ranged = matches!(
        role.role,
        CreatureClipRole::ProjectileAttack
            | CreatureClipRole::ContinuousAttackStart
            | CreatureClipRole::ContinuousAttackLoop
            | CreatureClipRole::ContinuousAttackStop
    );
    if ranged
        && let Some(handler) = event(
            &role
                .equipment_index
                .map_or("attackState".into(), |slot| format!("attackState{slot}")),
        )
    {
        let thrown = matches!(
            role.generator.base_generator(),
            CapabilityRoleGenerator::ThrownAttack
        )
        .then(|| {
            Some((
                event("weaponAttach")?,
                event("weaponDetach")?,
                role.equipment_index?,
            ))
        })
        .flatten();
        xml.push_str(&emit_attack_state_notifications(
            &enter_id,
            &exit_id,
            handler,
            None,
            thrown,
            (&enter, &exit),
        ));
        return (xml, Some(enter_id), Some(exit_id));
    }
    if role.role == CreatureClipRole::MeleeAttack
        && let Some(index) = event("attackStop")
    {
        exit.push(row(index));
    }
    for (id, rows) in [(&enter_id, &enter), (&exit_id, &exit)] {
        if !rows.is_empty() {
            xml.push_str(&format!("<hkobject name=\"{id}\" class=\"hkbStateMachineEventPropertyArray\" signature=\"0x71957c2d\"><hkparam name=\"events\" numelements=\"{}\">{}</hkparam></hkobject>\n",rows.len(),rows.join("")));
        }
    }
    (
        xml,
        (!enter.is_empty()).then_some(enter_id),
        (!exit.is_empty()).then_some(exit_id),
    )
}

pub(super) fn emit(
    manifest: &CreatureManifest,
    graph: &CapabilityGraphManifest,
    declarations: &GraphDeclarations,
) -> String {
    let plan = plan::build(manifest, graph);
    let mut xml = String::new();
    let mut window_modifiers = Vec::new();
    for (index, role) in plan.roles.iter().enumerate() {
        let samples = plan
            .samples
            .iter()
            .enumerate()
            .filter(|(_, sample)| sample.role == index)
            .collect::<Vec<_>>();
        let mut geometry = if samples.len() == 1 {
            emit_capability_role_generator(manifest, role, index, declarations)
        } else {
            let mut geometry = String::new();
            let mut children = Vec::new();
            for (sample_index, sample) in &samples {
                let clip = manifest
                    .clips
                    .iter()
                    .find(|clip| clip.name == sample.clip)
                    .unwrap();
                geometry.push_str(&emit_clip_generator(
                    manifest,
                    &sample.generator,
                    &sample.clip,
                    clip,
                ));
                let weights = format!("#source_channel_sample_weights{sample_index}");
                geometry.push_str(&bone_weights(
                    &weights,
                    &plan.weights[*sample_index],
                    declarations,
                ));
                let child = format!("#source_channel_sample{sample_index}");
                geometry.push_str(&blender_child(
                    &child,
                    &sample.generator,
                    &weights,
                    Some(&plan.motion_weights[*sample_index]),
                    declarations,
                ));
                children.push(child);
            }
            let master = samples
                .iter()
                .position(|(_, sample)| sample.clip == role.clip_name)
                .unwrap_or(0);
            geometry.push_str(&blender(
                &format!("#010{index}"),
                &role.state_name,
                &children,
                master as i16,
                9,
            ));
            source_attack_emit::suppress_aim_annotations(geometry, role, index)
        };
        if let Some(contract) = role.generator.source_attack()
            && let Some(window) = &contract.window
        {
            let [open, close] =
                crate::source_rig::SourceAttackContract::window_events(&role.state_name);
            let event = |name: &str| {
                declarations
                    .events
                    .iter()
                    .position(|event| event.name == name)
                    .unwrap()
            };
            let anchor = samples
                .iter()
                .find(|(_, sample)| sample.clip == role.clip_name)
                .unwrap()
                .1
                .generator
                .clone();
            let leaf = source_attack_emit::clip_clock_id(&geometry, &anchor);
            let start = geometry
                .find(&format!("<hkobject name=\"{leaf}\""))
                .unwrap();
            let end = start + geometry[start..].find("</hkobject>").unwrap();
            let trigger = format!("#source_channel_attack_triggers{index}");
            let replacement = geometry[start..end].replace(
                "<hkparam name=\"triggers\">null</hkparam>",
                &format!("<hkparam name=\"triggers\">{trigger}</hkparam>"),
            );
            geometry.replace_range(start..end, &replacement);
            let triggers=[(f32::from_bits(window.start_bits),event(&open)),(f32::from_bits(window.end_bits),event(&close))].iter().map(|(time,event)|format!("<hkobject><hkparam name=\"localTime\">{time}</hkparam><hkparam name=\"event\"><hkobject><hkparam name=\"id\">{event}</hkparam><hkparam name=\"payload\">null</hkparam></hkobject></hkparam><hkparam name=\"relativeToEndOfClip\">false</hkparam><hkparam name=\"acyclic\">true</hkparam><hkparam name=\"isAnnotation\">false</hkparam></hkobject>")).collect::<String>();
            geometry.push_str(&format!("<hkobject name=\"{trigger}\" class=\"hkbClipTriggerArray\" signature=\"0xf757cd66\"><hkparam name=\"triggers\" numelements=\"2\">{triggers}</hkparam></hkobject>\n"));
            let modifier = format!("#source_channel_window{index}");
            let assign = format!("{modifier}_assign");
            geometry.push_str(&source_attack_emit::assignment(
                &assign,
                &[(&contract.gate_variable, window.next_group.to_string())],
                declarations,
            ));
            geometry.push_str(&format!("<hkobject name=\"{modifier}\" class=\"hkbEventDrivenModifier\" signature=\"0x35ef356b\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">SourceAttackContinuation</hkparam><hkparam name=\"enable\">true</hkparam><hkparam name=\"modifier\">{assign}</hkparam><hkparam name=\"activateEventId\">{open}</hkparam><hkparam name=\"deactivateEventId\">{close}</hkparam><hkparam name=\"activeByDefault\">false</hkparam></hkobject>\n",open=event(&open),close=event(&close)));
            window_modifiers.push(modifier);
        }
        xml.push_str(&geometry);
    }
    let role_refs = plan.roles.iter().collect::<Vec<_>>();
    let legacy = capability_state_transitions(graph, &role_refs, declarations);
    let mut channel_children = Vec::new();
    for (channel, members) in &plan.channels {
        let prefix = format!("#source_channel{channel}");
        let off = format!("{prefix}_off");
        xml.push_str(&format!("<hkobject name=\"{off}\" class=\"hkbReferencePoseGenerator\" signature=\"0xbc1536ee\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">SourceChannelOff</hkparam></hkobject>\n"));
        let mut states = Vec::new();
        for current in std::iter::once(None).chain(members.iter().copied().map(Some)) {
            let state_id = current.map_or(0, |index| index + 1);
            let transitions =
                channel_transitions(graph, &plan, *channel, current, &legacy, declarations);
            let transition_id = format!("{prefix}_transitions{state_id}");
            xml.push_str(&transition_array(&transition_id, &transitions, &plan.roles));
            let state = format!("{prefix}_state{state_id}");
            let mut enter = None;
            let mut exit = None;
            if let Some(index) = current {
                let notifications = state_notifications(&plan.roles[index], index, declarations);
                xml.push_str(&notifications.0);
                enter = notifications.1;
                exit = notifications.2;
            }
            xml.push_str(&emit_state_info(
                &state,
                current.map_or("Off", |index| plan.roles[index].state_name.as_str()),
                state_id as i32,
                &transition_id,
                &current.map_or(off.clone(), |index| format!("#010{index}")),
                enter.as_deref(),
                exit.as_deref(),
            ));
            states.push(state);
        }
        xml.push_str(&machine(
            &prefix,
            &states,
            default_state(&plan, members),
            variable(declarations, &plan::state_variable(*channel)),
        ));
        let weights = (0..manifest.animation_skeleton.bones.len())
            .map(|bone| format!("B21SourceChannelWeight{channel}_{bone}"))
            .collect::<Vec<_>>();
        let weights_id = format!("{prefix}_weights");
        xml.push_str(&bone_weights(&weights_id, &weights, declarations));
        let child = format!("{prefix}_child");
        xml.push_str(&blender_child(
            &child,
            &prefix,
            &weights_id,
            Some(&format!("B21SourceChannelMotion{channel}")),
            declarations,
        ));
        channel_children.push(child);
    }
    xml.push_str(&blender(
        "#source_channels_blend",
        "SourceAnimationChannels",
        &channel_children,
        -1,
        8,
    ));
    let assignments = assignments(&plan);
    let assignments = assignments
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect::<Vec<_>>();
    xml.push_str(&source_attack_emit::assignment(
        "#source_channels_expressions",
        &assignments,
        declarations,
    ));
    let modifiers = std::iter::once("#source_channels_expressions".to_string())
        .chain(window_modifiers)
        .collect::<Vec<_>>();
    xml.push_str(&format!("<hkobject name=\"#source_channels_modifiers\" class=\"hkbModifierList\" signature=\"0x0ded564c\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">SourceChannelState</hkparam><hkparam name=\"enable\">true</hkparam><hkparam name=\"modifiers\" numelements=\"{}\">{}</hkparam></hkobject>\n",modifiers.len(),modifiers.join(" ")));
    let root = if graph.template == CreatureGraphTemplate::StationaryTurret {
        "#source_channels_root"
    } else {
        "#0093"
    };
    xml.push_str(&format!("<hkobject name=\"{root}\" class=\"hkbModifierGenerator\" signature=\"0xc499fc9e\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">SourceChannelPriority</hkparam><hkparam name=\"modifier\">#source_channels_modifiers</hkparam><hkparam name=\"generator\">#source_channels_blend</hkparam></hkobject>\n"));
    if graph.template == CreatureGraphTemplate::StationaryTurret {
        xml.push_str(&emit_stationary_turret_direct_at(
            declarations,
            root,
            "#0093",
        ));
    }
    emit_behavior_graph(
        &format!("{}CoreBehavior", manifest.creature_name),
        declarations,
        &xml,
    )
}

fn bone_weights(id: &str, weights: &[String], declarations: &GraphDeclarations) -> String {
    let binding = format!("{id}_bindings");
    let rows = weights
        .iter()
        .enumerate()
        .map(|(bone, name)| {
            emit_variable_binding(
                &format!("boneWeights:{bone}"),
                variable(declarations, name),
                "BINDING_TYPE_VARIABLE",
            )
        })
        .collect::<String>();
    format!(
        "{}<hkobject name=\"{id}\" class=\"hkbBoneWeightArray\" signature=\"0xcd902b77\"><hkparam name=\"variableBindingSet\">{binding}</hkparam><hkparam name=\"boneWeights\" numelements=\"{}\">{}</hkparam></hkobject>\n",
        binding_set(&binding, weights.len(), &rows),
        weights.len(),
        vec!["0"; weights.len()].join(" ")
    )
}

fn binding_set(id: &str, count: usize, rows: &str) -> String {
    format!(
        "<hkobject name=\"{id}\" class=\"hkbVariableBindingSet\" signature=\"0xe942f339\"><hkparam name=\"bindings\" numelements=\"{count}\">{rows}</hkparam><hkparam name=\"indexOfBindingToEnable\">-1</hkparam></hkobject>\n"
    )
}

fn blender_child(
    id: &str,
    generator: &str,
    weights: &str,
    motion: Option<&str>,
    declarations: &GraphDeclarations,
) -> String {
    let binding = format!("{id}_bindings");
    let mut xml = String::new();
    if let Some(motion) = motion {
        xml.push_str(&binding_set(
            &binding,
            1,
            &emit_variable_binding(
                "worldFromModelWeight",
                variable(declarations, motion),
                "BINDING_TYPE_VARIABLE",
            ),
        ));
    }
    xml.push_str(&format!("<hkobject name=\"{id}\" class=\"hkbBlenderGeneratorChild\" signature=\"0xb35bbfd3\"><hkparam name=\"variableBindingSet\">{binding}</hkparam><hkparam name=\"generator\">{generator}</hkparam><hkparam name=\"boneWeights\">{weights}</hkparam><hkparam name=\"weight\">1</hkparam><hkparam name=\"worldFromModelWeight\">0</hkparam></hkobject>\n",binding=if motion.is_some(){binding.as_str()}else{"null"}));
    xml
}

fn blender(id: &str, name: &str, children: &[String], master: i16, flags: u32) -> String {
    format!(
        "<hkobject name=\"{id}\" class=\"hkbBlenderGenerator\" signature=\"0xce45c088\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"referencePoseWeightThreshold\">0</hkparam><hkparam name=\"blendParameter\">0</hkparam><hkparam name=\"minCyclicBlendParameter\">0</hkparam><hkparam name=\"maxCyclicBlendParameter\">0</hkparam><hkparam name=\"indexOfSyncMasterChild\">{master}</hkparam><hkparam name=\"flags\">{flags}</hkparam><hkparam name=\"subtractLastChild\">false</hkparam><hkparam name=\"children\" numelements=\"{}\">{}</hkparam></hkobject>\n",
        children.len(),
        children.join(" "),
        name = escape_xml(name)
    )
}

fn machine(id: &str, states: &[String], start: usize, sync: usize) -> String {
    format!(
        "<hkobject name=\"{id}\" class=\"hkbStateMachine\" signature=\"0xa5896bcf\"><hkparam name=\"variableBindingSet\">null</hkparam><hkparam name=\"userData\">0</hkparam><hkparam name=\"name\">{name}</hkparam><hkparam name=\"eventToSendWhenStateOrTransitionChanges\"><hkobject><hkparam name=\"id\">-1</hkparam><hkparam name=\"payload\">null</hkparam></hkobject></hkparam><hkparam name=\"startStateIdSelector\">null</hkparam><hkparam name=\"startStateId\">{start}</hkparam><hkparam name=\"returnToPreviousStateEventId\">-1</hkparam><hkparam name=\"randomTransitionEventId\">-1</hkparam><hkparam name=\"transitionToNextHigherStateEventId\">-1</hkparam><hkparam name=\"transitionToNextLowerStateEventId\">-1</hkparam><hkparam name=\"syncVariableIndex\">{sync}</hkparam><hkparam name=\"wrapAroundStateId\">false</hkparam><hkparam name=\"maxSimultaneousTransitions\">1</hkparam><hkparam name=\"startStateMode\">START_STATE_MODE_DEFAULT</hkparam><hkparam name=\"selfTransitionMode\">SELF_TRANSITION_MODE_NO_TRANSITION</hkparam><hkparam name=\"states\" numelements=\"{}\">{}</hkparam><hkparam name=\"wildcardTransitions\">null</hkparam></hkobject>\n",
        states.len(),
        states.join(" "),
        name = id.trim_start_matches('#')
    )
}
