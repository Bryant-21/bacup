use super::*;

#[cfg(test)]
#[path = "locomotion_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "wendigo_tests.rs"]
mod wendigo_tests;

fn extracted_motion(file: &HkxFile) -> Option<(f32, f32)> {
    let frame = file
        .objects()
        .iter()
        .find(|o| o.class_name == "hkaDefaultAnimatedReferenceFrame")?;
    let samples: Vec<f32> = match value(frame, "referenceFrameSamples")? {
        HkxValue::F32List(values) => values.clone(),
        HkxValue::Array(values) => values
            .iter()
            .flat_map(|v| match v {
                HkxValue::F32List(values) => values.clone(),
                HkxValue::Array(values) => values.iter().filter_map(number).collect(),
                _ => vec![],
            })
            .collect(),
        _ => return None,
    };
    if samples.len() < 8 {
        return None;
    }
    let last = samples.len() - 4;
    let dx = samples[last] - samples[0];
    let dy = samples[last + 1] - samples[1];
    let distance = dx.hypot(dy);
    let duration = value(frame, "duration").and_then(number)?;
    if duration <= 0.0 {
        return None;
    }
    if distance <= 1.0 {
        return None;
    }
    Some((dx.atan2(dy) / std::f32::consts::TAU, distance / duration))
}

fn motion(file: &HkxFile, name: &str) -> Option<(f32, f32)> {
    // FO76 in-place clips can omit extractedMotion entirely.
    let (mut direction, speed) = extracted_motion(file).or_else(|| {
        let name = clean(name);
        let x = i32::from(name.contains("right")) - i32::from(name.contains("left"));
        let y = i32::from(name.contains("forward"))
            - i32::from(name.contains("backward") || name.contains("backpedal"));
        if x == 0 && y == 0 {
            return None;
        }
        Some(((x as f32).atan2(y as f32) / std::f32::consts::TAU, 0.0))
    })?;
    direction = direction.rem_euclid(1.0);
    if direction.min(1.0 - direction) < 0.00001 {
        direction = 0.0;
    }
    Some((direction, speed))
}

fn template(file: &HkxFile, name: &str) -> Result<HkxObject, String> {
    file.objects()
        .iter()
        .find(|o| string(o, "name") == name)
        .cloned()
        .ok_or_else(|| format!("missing FO4 blender template {name}"))
}

fn forward_speed(
    file: &HkxFile,
    generator: usize,
    source: &Path,
    animations: &BTreeMap<String, String>,
) -> Result<Option<f32>, String> {
    let mut pending = vec![generator];
    let mut visited = std::collections::BTreeSet::new();
    let mut speeds = Vec::new();
    while let Some(index) = pending.pop() {
        if !visited.insert(index) {
            continue;
        }
        let object = &file.objects()[index];
        if object.class_name == "hkbClipGenerator" {
            let name = string(object, "animationName");
            let origin = animations
                .get(&name)
                .ok_or_else(|| format!("unresolved Wendigo locomotion clip {name}"))?;
            if let Some((direction, speed)) = extracted_motion(&read(&source.join(origin))?) {
                if direction.abs() < 0.01 {
                    speeds.push(speed);
                }
            }
        }
        if let Some(child) = pointer(object, "generator") {
            pending.push(child);
        }
        for field in ["states", "children", "generators"] {
            pending.extend(pointers(object, field));
        }
    }
    Ok((!speeds.is_empty()).then(|| speeds.iter().sum::<f32>() / speeds.len() as f32))
}

pub(super) fn repair(
    file: &mut HkxFile,
    source: &Path,
    animations: &BTreeMap<String, String>,
    route_source: &str,
    vanilla: &HkxFile,
) -> Result<(), String> {
    // Wendigo's authored gait machines need the same speed driver as converted
    // locomotion generators: FO4 does not supply their WalkStart/RunStart protocol.
    let is_wendigo = clean(route_source) == "actors/wendigo/behaviors/wendigocorebehavior.hkx";
    let manual: Vec<_> = file
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            is_wendigo
                && o.class_name == "hkbStateMachine"
                && matches!(
                    string(o, "name").as_str(),
                    "Locomotion_SM" | "CombatLocomotion_SM"
                )
        })
        .map(|(i, _)| i)
        .collect();
    let cyclics: Vec<_> = file
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            o.class_name == "BSCyclicBlendTransitionGenerator"
                && string(o, "name").contains("LocomotionBlendGenerator")
        })
        .map(|(i, _)| i)
        .collect();
    if cyclics.is_empty() && manual.is_empty() {
        return Ok(());
    }
    let mut speeds = BTreeMap::new();
    for cyclic in &cyclics {
        let blend = pointer(&file.objects()[*cyclic], "pBlenderGenerator")
            .ok_or("locomotion wrapper has no blender")?;
        let children = pointers(&file.objects()[blend], "children");
        let mut positioned = Vec::new();
        let mut measured = Vec::new();
        for child in children {
            let clip = pointer(&file.objects()[child], "generator")
                .ok_or("locomotion child has no generator")?;
            let name = string(&file.objects()[clip], "animationName");
            let origin = animations
                .get(&name)
                .ok_or_else(|| format!("{route_source}: unresolved locomotion clip {name}"))?;
            let (direction, speed) = motion(&read(&source.join(origin))?, &name)
                .ok_or_else(|| format!("{origin}: cannot determine locomotion direction"))?;
            set(
                &mut file.objects_mut()[child],
                "weight",
                HkxValue::F32(direction),
            );
            positioned.push((direction, child));
            if speed > 0.0 {
                measured.push(speed);
            }
        }
        positioned.sort_by(|a, b| a.0.total_cmp(&b.0));
        let blend = &mut file.objects_mut()[blend];
        set(blend, "flags", HkxValue::I16(49));
        set(blend, "minCyclicBlendParameter", HkxValue::F32(0.0));
        set(blend, "maxCyclicBlendParameter", HkxValue::F32(1.0));
        set(blend, "indexOfSyncMasterChild", HkxValue::I16(-1));
        set(
            blend,
            "children",
            HkxValue::Array(
                positioned
                    .into_iter()
                    .map(|(_, i)| HkxValue::Pointer(Some(i)))
                    .collect(),
            ),
        );
        if !measured.is_empty() {
            speeds.insert(
                *cyclic,
                measured.iter().sum::<f32>() / measured.len() as f32,
            );
        }
    }
    for machine in &manual {
        for state in pointers(&file.objects()[*machine], "states") {
            let generator = pointer(&file.objects()[state], "generator")
                .ok_or("Wendigo gait state has no generator")?;
            if let Some(speed) = forward_speed(file, generator, source, animations)? {
                speeds.insert(generator, speed);
            }
        }
    }
    let strings = file
        .objects()
        .iter()
        .find(|o| o.class_name == "hkbBehaviorGraphStringData")
        .ok_or("missing graph strings")?;
    let speed_index = array(strings, "variableNames")
        .iter()
        .position(|v| text(v).is_some_and(|s| s.eq_ignore_ascii_case("Speed")))
        .ok_or("locomotion graph has no Speed variable")?;
    let mut machines: Vec<_> = file
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            o.class_name == "hkbStateMachine"
                && string(o, "name").contains("LocomotionBlendGenerator")
        })
        .map(|(i, _)| i)
        .collect();
    machines.extend(&manual);
    let blend_template = template(vanilla, "ReadyTurnRightSlowBlend")?;
    let child_template = vanilla.objects()[pointers(&blend_template, "children")[0]].clone();
    let mut speed_blends = Vec::new();
    for index in machines {
        let mut tiers = pointers(&file.objects()[index], "states")
            .iter()
            .filter_map(|state| pointer(&file.objects()[*state], "generator"))
            .map(|cyclic| speeds.get(&cyclic).map(|speed| (*speed, cyclic)))
            .collect::<Option<Vec<_>>>();
        // In-place locomotion has no measurable speed knots. Retain its authored
        // tier selector instead of inventing physical speeds from clip names.
        let Some(ref mut tiers) = tiers else { continue };
        tiers.sort_by(|a, b| a.0.total_cmp(&b.0));
        tiers.dedup_by(|a, b| (a.0 - b.0).abs() < 0.01);
        let mut children = Vec::new();
        for (speed, cyclic) in tiers {
            let mut child = child_template.clone();
            set(&mut child, "generator", HkxValue::Pointer(Some(*cyclic)));
            set(&mut child, "weight", HkxValue::F32(*speed));
            children.push(HkxValue::Pointer(Some(add(file, child))));
        }
        let bound = HkxObject {
            name: None,
            offset: 0,
            signature: 2,
            class_name: "hkbVariableBindingSet".into(),
            members: vec![
                HkxMember {
                    name: "bindings".into(),
                    value: HkxValue::Array(vec![HkxValue::Object(vec![
                        HkxMember {
                            name: "memberPath".into(),
                            value: HkxValue::String {
                                value: "blendParameter".into(),
                                is_null: false,
                            },
                        },
                        HkxMember {
                            name: "variableIndex".into(),
                            value: HkxValue::I32(speed_index as i32),
                        },
                        HkxMember {
                            name: "bitIndex".into(),
                            value: HkxValue::I8(-1),
                        },
                        HkxMember {
                            name: "bindingType".into(),
                            value: HkxValue::I8(0),
                        },
                    ])]),
                },
                HkxMember {
                    name: "indexOfBindingToEnable".into(),
                    value: HkxValue::I32(-1),
                },
            ],
        };
        let binding = add(file, bound);
        let mut replacement = blend_template.clone();
        replacement.name = file.objects()[index].name.clone();
        let name = string(&file.objects()[index], "name");
        set_string(
            &mut replacement,
            "name",
            &if manual.contains(&index) {
                format!("B21_Wendigo_{name}_SpeedBlend")
            } else {
                name
            },
        );
        set(
            &mut replacement,
            "variableBindingSet",
            HkxValue::Pointer(Some(binding)),
        );
        set(&mut replacement, "children", HkxValue::Array(children));
        file.objects_mut()[index] = replacement;
        speed_blends.push(index);
    }
    smooth_speed(file, &speed_blends, vanilla)?;
    Ok(())
}

fn smooth_speed(file: &mut HkxFile, blends: &[usize], vanilla: &HkxFile) -> Result<(), String> {
    if blends.is_empty()
        || file
            .objects()
            .iter()
            .any(|o| string(o, "name") == "B21_LocomotionSpeedSmoothing")
    {
        return Ok(());
    }
    let graph = file
        .objects()
        .iter()
        .position(|o| o.class_name == "hkbBehaviorGraph")
        .ok_or("missing locomotion behavior graph")?;
    let data = pointer(&file.objects()[graph], "data").ok_or("missing locomotion graph data")?;
    let strings =
        pointer(&file.objects()[data], "stringData").ok_or("missing locomotion strings")?;
    let values = pointer(&file.objects()[data], "variableInitialValues")
        .ok_or("missing locomotion values")?;
    let mut names = array(&file.objects()[strings], "variableNames");
    let speed = names
        .iter()
        .position(|v| text(v) == Some("Speed"))
        .ok_or("missing Speed variable")?;
    let locomotion = names
        .iter()
        .position(|v| text(v) == Some("iSyncIdleLocomotion"));
    let mut infos = array(&file.objects()[data], "variableInfos");
    let mut initial = array(&file.objects()[values], "wordVariableValues");
    if names.len() != infos.len() || names.len() != initial.len() {
        return Err("locomotion variable tables differ in length".into());
    }
    let smoothed = names.len();
    names.push(HkxValue::String {
        value: "B21_LocomotionSpeed".into(),
        is_null: false,
    });
    infos.push(infos[speed].clone());
    initial.push(HkxValue::Object(vec![HkxMember {
        name: "value".into(),
        value: HkxValue::I32(0),
    }]));
    set(
        &mut file.objects_mut()[strings],
        "variableNames",
        HkxValue::Array(names),
    );
    set(
        &mut file.objects_mut()[data],
        "variableInfos",
        HkxValue::Array(infos),
    );
    set(
        &mut file.objects_mut()[values],
        "wordVariableValues",
        HkxValue::Array(initial),
    );
    let source_binding = pointer(&file.objects()[blends[0]], "variableBindingSet")
        .ok_or("missing locomotion speed binding")?;
    let binding_template = file.objects()[source_binding].clone();
    let entry = array(&binding_template, "bindings")
        .into_iter()
        .next()
        .ok_or("empty speed binding")?;
    let bound_entry = |field: &str, index: usize| {
        let mut entry = entry.clone();
        if let HkxValue::Object(fields) = &mut entry {
            for member in fields {
                match member.name.as_str() {
                    "memberPath" => {
                        member.value = HkxValue::String {
                            value: field.into(),
                            is_null: false,
                        }
                    }
                    "variableIndex" => member.value = HkxValue::I32(index as i32),
                    _ => {}
                }
            }
        }
        entry
    };
    let mut bound = binding_template.clone();
    set(
        &mut bound,
        "bindings",
        HkxValue::Array(vec![
            bound_entry("source", smoothed),
            bound_entry("target", speed),
            bound_entry("result", smoothed),
        ]),
    );
    let binding = add(file, bound);
    let mut modifier = template(vanilla, "InterpRagdollToAnim")?;
    set_string(&mut modifier, "name", "B21_LocomotionSpeedSmoothing");
    set(
        &mut modifier,
        "variableBindingSet",
        HkxValue::Pointer(Some(binding)),
    );
    // Raw Speed reaches zero before the outgoing run pose finishes fading to idle.
    set(&mut modifier, "gain", HkxValue::F32(0.15));
    let modifier = add(file, modifier);
    for blend in blends {
        let mut bound = binding_template.clone();
        set(
            &mut bound,
            "bindings",
            HkxValue::Array(vec![bound_entry("blendParameter", smoothed)]),
        );
        let binding = add(file, bound);
        set(
            &mut file.objects_mut()[*blend],
            "variableBindingSet",
            HkxValue::Pointer(Some(binding)),
        );
    }
    if let Some(locomotion) = locomotion {
        let full_mask_speed = blends
            .iter()
            .flat_map(|i| pointers(&file.objects()[*i], "children"))
            .filter_map(|i| value(&file.objects()[i], "weight").and_then(number))
            .reduce(f32::min)
            .ok_or("locomotion blend has no speed knots")?;
        smooth_masks(file, blends[0], locomotion, full_mask_speed);
    }
    let root = pointer(&file.objects()[graph], "rootGenerator").ok_or("missing locomotion root")?;
    let mut wrapper = file
        .objects()
        .iter()
        .find(|o| o.class_name == "hkbModifierGenerator")
        .cloned()
        .ok_or("missing modifier generator template")?;
    set_string(&mut wrapper, "name", "B21_LocomotionSpeedRoot");
    set(&mut wrapper, "variableBindingSet", HkxValue::Pointer(None));
    set(&mut wrapper, "modifier", HkxValue::Pointer(Some(modifier)));
    set(&mut wrapper, "generator", HkxValue::Pointer(Some(root)));
    let wrapper = add(file, wrapper);
    set(
        &mut file.objects_mut()[graph],
        "rootGenerator",
        HkxValue::Pointer(Some(wrapper)),
    );
    Ok(())
}

fn smooth_masks(file: &mut HkxFile, speed_blend: usize, locomotion: usize, full_mask_speed: f32) {
    let blend_template = file.objects()[speed_blend].clone();
    let child_template = file.objects()[pointers(&blend_template, "children")[0]].clone();
    let masks: Vec<_> = file
        .objects()
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            if object.class_name != "hkbManualSelectorGenerator"
                || !string(object, "name").starts_with("FO76_BoneMaskSelector_")
            {
                return None;
            }
            let binding = pointer(object, "variableBindingSet")?;
            let entries = array(&file.objects()[binding], "bindings");
            let [HkxValue::Object(fields)] = entries.as_slice() else {
                return None;
            };
            let bound = fields
                .iter()
                .any(|m| m.name == "variableIndex" && number(&m.value) == Some(locomotion as f32));
            let selected = fields.iter().any(|m| {
                m.name == "memberPath" && text(&m.value) == Some("selectedGeneratorIndex")
            });
            let variable = fields
                .iter()
                .any(|m| m.name == "bindingType" && number(&m.value) == Some(0.0));
            (bound && selected && variable).then_some(index)
        })
        .collect();
    for mask in masks {
        let choices = pointers(&file.objects()[mask], "generators");
        if choices.len() < 2 {
            continue;
        }
        let mut children = Vec::new();
        for (generator, weight) in choices.into_iter().zip([0.0, full_mask_speed]) {
            let mut child = child_template.clone();
            set(&mut child, "variableBindingSet", HkxValue::Pointer(None));
            set(&mut child, "generator", HkxValue::Pointer(Some(generator)));
            set(&mut child, "boneWeights", HkxValue::Pointer(None));
            set(&mut child, "weight", HkxValue::F32(weight));
            set(&mut child, "worldFromModelWeight", HkxValue::F32(1.0));
            children.push(HkxValue::Pointer(Some(add(file, child))));
        }
        let mut blend = blend_template.clone();
        blend.name = file.objects()[mask].name.clone();
        set_string(&mut blend, "name", &string(&file.objects()[mask], "name"));
        set(&mut blend, "children", HkxValue::Array(children));
        // Both masks keep their shared action alive while its lower-body weight fades.
        set(&mut blend, "flags", HkxValue::I16(24));
        set(&mut blend, "blendParameter", HkxValue::F32(0.0));
        set(&mut blend, "indexOfSyncMasterChild", HkxValue::I16(-1));
        file.objects_mut()[mask] = blend;
    }
}
