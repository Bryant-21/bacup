use super::*;
use crate::source_rig::source_priority::{ExpressionContributor, expressions};

pub(super) struct Sample {
    pub role: usize,
    pub clip: String,
    pub generator: String,
    pub scalar: String,
    pub active: String,
}

pub(super) struct Plan {
    pub roles: Vec<CapabilityClipRole>,
    pub channels: BTreeMap<u8, Vec<usize>>,
    pub samples: Vec<Sample>,
    pub assignments: Vec<(String, String)>,
    pub weights: Vec<Vec<String>>,
    pub role_weights: Vec<Vec<String>>,
    pub motion_weights: Vec<String>,
}

pub(super) fn state_variable(channel: u8) -> String {
    format!("B21SourceChannel{channel}")
}

pub(super) fn channel(graph: &CapabilityGraphManifest, role: &CapabilityClipRole) -> u8 {
    let native = graph
        .source_channels
        .iter()
        .find(|clip| clip.clip_name == role.clip_name)
        .unwrap()
        .channel;
    if matches!(native, 4..=6) { 4 } else { native }
}

pub(super) fn build(manifest: &CreatureManifest, graph: &CapabilityGraphManifest) -> Plan {
    let mut roles = ordered_capability_roles(graph)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    roles.extend(graph.overlays.iter().map(|overlay| CapabilityClipRole {
        role: CreatureClipRole::ScriptedLoop,
        state_name: overlay.name.clone(),
        clip_name: overlay.clip_name.clone(),
        generator: CapabilityRoleGenerator::Single,
        trigger_event: Some(overlay.start_event.clone()),
        trigger_aliases: Vec::new(),
        motion: overlay.motion,
        equipment_index: None,
    }));
    let mut channels = BTreeMap::<u8, Vec<usize>>::new();
    let mut samples = Vec::new();
    for (index, role) in roles.iter().enumerate() {
        let channel = channel(graph, role);
        channels.entry(channel).or_default().push(index);
        let active = format!("({} == {})", state_variable(channel), index + 1);
        let aim_children = match &role.generator {
            CapabilityRoleGenerator::SourceAttack { contract, .. } => Some(&contract.aim_children),
            CapabilityRoleGenerator::SourceAim { aim_children, .. } => Some(aim_children),
            _ => None,
        }
        .filter(|children| !children.is_empty());
        if let Some(aim_children) = aim_children {
            let anchors = aim_children
                .iter()
                .map(|child| f32::from_bits(child.weight_bits))
                .collect::<Vec<_>>();
            for (child, sample) in aim_children.iter().enumerate() {
                samples.push(Sample {
                    role: index,
                    clip: sample.clip_name.clone(),
                    generator: capability_child_id(3, index, child),
                    scalar: linear_weight("B21SourceWeaponPitch", &anchors, child, false),
                    active: active.clone(),
                });
            }
        } else if let CapabilityRoleGenerator::Locomotion { directions } =
            role.generator.base_generator()
        {
            let angles = directions
                .iter()
                .map(|direction| f32::from_bits(direction.direction_bits))
                .collect::<Vec<_>>();
            for (direction_index, direction) in directions.iter().enumerate() {
                let speeds = direction
                    .speeds
                    .iter()
                    .map(|speed| f32::from_bits(speed.weight_bits))
                    .collect::<Vec<_>>();
                for (speed_index, speed) in direction.speeds.iter().enumerate() {
                    let scalar = format!(
                        "({}) * ({})",
                        linear_weight("Direction", &angles, direction_index, true),
                        linear_weight("Speed", &speeds, speed_index, false)
                    );
                    samples.push(Sample {
                        role: index,
                        clip: speed.clip_name.clone(),
                        generator: format!(
                            "#locomotion_clip_{index}_{direction_index}_{speed_index}"
                        ),
                        active: format!("{active} * (({scalar}) > 0)"),
                        scalar,
                    });
                }
            }
        } else {
            samples.push(Sample {
                role: index,
                clip: role.clip_name.clone(),
                generator: format!("#010{index}"),
                scalar: "1".into(),
                active,
            });
        }
    }
    let priorities = samples
        .iter()
        .map(|sample| {
            &graph
                .source_channels
                .iter()
                .find(|clip| clip.clip_name == sample.clip)
                .unwrap()
                .transform_priorities
        })
        .collect::<Vec<_>>();
    let mut assignments = Vec::new();
    let mut weights = vec![Vec::new(); samples.len()];
    let mut role_weights = vec![Vec::new(); roles.len()];
    let mut profiles = BTreeMap::<Vec<Option<u8>>, Vec<String>>::new();
    for (bone_index, bone) in manifest.animation_skeleton.bones.iter().enumerate() {
        let profile = priorities
            .iter()
            .map(|priorities| priorities.get(&bone.name).copied())
            .collect::<Vec<_>>();
        let normalized = if let Some(existing) = profiles.get(&profile) {
            existing.clone()
        } else {
            let inputs = samples
                .iter()
                .zip(&profile)
                .map(|(sample, priority)| ExpressionContributor {
                    active: sample.active.clone(),
                    priority: priority.map_or("-1".into(), |value| value.to_string()),
                    weight: sample.scalar.clone(),
                    ease: "1".into(),
                })
                .collect::<Vec<_>>();
            let generated = expressions(&format!("B21SourcePriority{}", profiles.len()), &inputs);
            assignments.extend(generated.assignments);
            profiles.insert(profile, generated.weights.clone());
            generated.weights
        };
        for (sample, weight) in normalized.iter().enumerate() {
            weights[sample].push(weight.clone());
        }
        for (role, values) in role_weights.iter_mut().enumerate() {
            let terms = samples
                .iter()
                .enumerate()
                .filter(|(_, sample)| sample.role == role)
                .map(|(sample, _)| normalized[sample].clone())
                .collect::<Vec<_>>();
            let name = format!("B21SourceRoleWeight{role}_{bone_index}");
            assignments.push((name.clone(), terms.join(" + ")));
            values.push(name);
        }
        assignments.push((
            format!("B21SourcePoseValid_{bone_index}"),
            format!("(({}) > 0)", normalized.join(" + ")),
        ));
    }
    let root_inputs = samples
        .iter()
        .map(|sample| {
            let source = graph
                .source_channels
                .iter()
                .find(|source| source.clip_name == sample.clip)
                .unwrap();
            ExpressionContributor {
                active: sample.active.clone(),
                priority: source
                    .accumulation_root
                    .as_ref()
                    .and_then(|root| source.transform_priorities.get(root))
                    .map_or("-1".into(), |value| value.to_string()),
                weight: sample.scalar.clone(),
                ease: "1".into(),
            }
        })
        .collect::<Vec<_>>();
    let root = expressions("B21SourceRootPriority", &root_inputs);
    assignments.extend(root.assignments);
    Plan {
        roles,
        channels,
        samples,
        assignments,
        weights,
        role_weights,
        motion_weights: root.weights,
    }
}

fn clamp(value: String) -> String {
    format!("(({value}) >= 1) + (({value}) > 0) * (({value}) < 1) * ({value})")
}

fn linear_weight(variable: &str, anchors: &[f32], index: usize, cyclic: bool) -> String {
    if anchors.len() == 1 {
        return "1".into();
    }
    let at = anchors[index];
    if !cyclic {
        let rising = if index == 0 {
            "1".into()
        } else {
            clamp(format!(
                "({variable} - {}) / {}",
                anchors[index - 1],
                at - anchors[index - 1]
            ))
        };
        let falling = if index + 1 == anchors.len() {
            "1".into()
        } else {
            clamp(format!(
                "({} - {variable}) / {}",
                anchors[index + 1],
                anchors[index + 1] - at
            ))
        };
        return format!("({rising}) * ({falling})");
    }
    let previous = if index == 0 {
        anchors[anchors.len() - 1] - 1.0
    } else {
        anchors[index - 1]
    };
    let next = if index + 1 == anchors.len() {
        anchors[0] + 1.0
    } else {
        anchors[index + 1]
    };
    [-1.0_f32, 0.0, 1.0]
        .iter()
        .map(|shift| {
            let value = format!("({variable} + {shift})");
            format!(
                "({}) * ({})",
                clamp(format!("({value} - {previous}) / {}", at - previous)),
                clamp(format!("({next} - {value}) / {}", next - at))
            )
        })
        .collect::<Vec<_>>()
        .join(" + ")
}
