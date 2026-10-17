use std::collections::{HashMap, HashSet};

use havok_native::animation::parsers::SkeletonRecord;
use havok_native::animation::pose::{
    Pose, PoseSkeleton, QsTransform, quat_conjugate, quat_mul, quat_normalize, quat_rotate,
};

use super::{AnimationClip, AnimationKeyframe, BoneChannel, Interpolation};

pub(crate) fn target_reference_pose(record: &SkeletonRecord) -> Result<PoseSkeleton, String> {
    let pose = PoseSkeleton {
        bone_names: record.bone_names.clone(),
        parent_indices: record.parent_indices.clone(),
        reference_local: record
            .reference_pose
            .iter()
            .map(|bone| QsTransform {
                translation: bone.t,
                rotation: bone.q,
                scale: bone.s,
            })
            .collect(),
    };
    validate_skeleton(&pose)?;
    Ok(pose)
}

pub(crate) fn humanoid_bone_map() -> Result<HashMap<String, String>, String> {
    #[derive(serde::Deserialize)]
    struct Table {
        bones: HashMap<String, String>,
    }

    let table: Table = serde_saphyr::from_str(include_str!(
        "../../embedded/translation_maps/skeleton_fo3_to_fo4.yaml"
    ))
    .map_err(|error| error.to_string())?;
    Ok(table.bones)
}

pub(super) fn skyrim_actor_bone_map(
    source: &PoseSkeleton,
    target: &PoseSkeleton,
) -> Result<HashMap<String, String>, String> {
    #[derive(serde::Deserialize)]
    struct Table {
        bones: HashMap<String, String>,
    }
    let table: Table = serde_saphyr::from_str(include_str!(
        "../../embedded/translation_maps/skeleton_skyrimse_to_fo4.yaml"
    ))
    .map_err(|error| error.to_string())?;
    let mut result = HashMap::new();
    let mut targets = HashSet::new();
    for name in &source.bone_names {
        // Weight aliases for skirts and holsters cannot share an animation track.
        if !name.starts_with("NPC ") {
            continue;
        }
        let Some((_, target_name)) = table
            .bones
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
        else {
            continue;
        };
        let target_name = match name.as_str() {
            "NPC Spine [Spn0]" => "Spine1",
            "NPC Spine1 [Spn1]" => "Spine2",
            "NPC Spine2 [Spn2]" => "Chest",
            _ => target_name.strip_suffix("_skin").unwrap_or(target_name),
        };
        let Some(target_name) = target
            .bone_names
            .iter()
            .find(|bone| bone.eq_ignore_ascii_case(target_name))
        else {
            continue;
        };
        if !targets.insert(target_name.to_ascii_lowercase()) {
            return Err(format!("ambiguous Skyrim actor target bone {target_name}"));
        }
        result.insert(name.clone(), target_name.clone());
    }
    Ok(result)
}

pub(crate) fn retarget_weapon_clip(
    clip: &AnimationClip,
    source: &PoseSkeleton,
    target: &PoseSkeleton,
    bone_map: &HashMap<String, String>,
    sample_rate: f64,
    playback_speed: f64,
) -> Result<(AnimationClip, Vec<String>), String> {
    validate_skeleton(source)?;
    validate_skeleton(target)?;
    if !sample_rate.is_finite()
        || sample_rate <= 0.0
        || !playback_speed.is_finite()
        || playback_speed <= 0.0
        || !clip.duration.is_finite()
        || clip.duration < 0.0
    {
        return Err("invalid weapon animation duration or playback rate".into());
    }
    let source_by_name: HashMap<_, _> = source
        .bone_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.to_ascii_lowercase(), index))
        .collect();
    let target_by_name: HashMap<_, _> = target
        .bone_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.to_ascii_lowercase(), index))
        .collect();
    let map: HashMap<_, _> = bone_map
        .iter()
        .map(|(source, target)| (source.to_ascii_lowercase(), target.to_ascii_lowercase()))
        .collect();
    let mut target_to_source = vec![None; target.bone_count()];
    for (source_index, name) in source.bone_names.iter().enumerate() {
        let normalized = name.to_ascii_lowercase();
        // Skin aliases collapse weights; animation needs the actual joint, including
        // motion inherited through Neck1, rather than two tracks for the same bone.
        if matches!(
            normalized.as_str(),
            "bip01 neck1" | "bip01 lpauldron" | "bip01 rpauldron"
        ) || (normalized == "bip" && source_by_name.contains_key("bip01"))
        {
            continue;
        }
        let target_name = map.get(&normalized).unwrap_or(&normalized);
        let Some(&target_index) = target_by_name.get(target_name) else {
            continue;
        };
        if target_to_source[target_index]
            .replace(source_index)
            .is_some()
        {
            return Err(format!(
                "ambiguous animation mapping for {}",
                target.bone_names[target_index]
            ));
        }
    }
    let mut channels_by_source = HashMap::new();
    let mut omitted = Vec::new();
    for channel in &clip.channels {
        let Some(&index) = source_by_name.get(&channel.bone_name.to_ascii_lowercase()) else {
            omitted.push(channel.bone_name.clone());
            continue;
        };
        if channels_by_source.insert(index, channel).is_some() {
            return Err(format!(
                "duplicate source animation channel {}",
                channel.bone_name
            ));
        }
    }
    if !target_to_source.iter().flatten().any(|index| {
        let mut ancestor = *index as i32;
        while ancestor >= 0 {
            if channels_by_source.contains_key(&(ancestor as usize)) {
                return true;
            }
            ancestor = source.parent_indices[ancestor as usize];
        }
        false
    }) {
        return Err("weapon animation has no mapped actor channels".into());
    }
    let source_reference = Pose::from_reference(source.clone()).all_model();
    let target_reference = Pose::from_reference(target.clone()).all_model();
    let mapped_source: HashSet<_> = target_to_source.iter().flatten().copied().collect();
    let source_parents: Vec<_> = (0..source.bone_count())
        .map(|index| {
            let mut parent = source.parent_indices[index];
            while parent >= 0 && !mapped_source.contains(&(parent as usize)) {
                parent = source.parent_indices[parent as usize];
            }
            parent
        })
        .collect();
    let duration = clip.duration / playback_speed;
    let frame_count = super::target_sample_count(duration, sample_rate);
    let mut output = clip.clone();
    output.duration = duration;
    output.float_channels.clear();
    output.channels = target
        .bone_names
        .iter()
        .map(|name| BoneChannel {
            bone_name: name.clone(),
            rotations: Vec::with_capacity(frame_count),
            translations: Vec::with_capacity(frame_count),
            scales: Vec::with_capacity(frame_count),
        })
        .collect();
    for event in &mut output.events {
        event.time /= playback_speed;
    }

    for frame in 0..frame_count {
        let time = if frame_count > 1 {
            duration * frame as f64 / (frame_count - 1) as f64
        } else {
            0.0
        };
        let mut source_local = source.reference_local.clone();
        for (&index, channel) in &channels_by_source {
            source_local[index] =
                sample_channel(channel, &source_local[index], time * playback_speed);
        }
        let source_model = Pose::from_local(source.clone(), source_local.clone()).all_model();
        let mut target_model: Vec<QsTransform> = Vec::with_capacity(target.bone_count());
        for (target_index, source_index) in target_to_source.iter().enumerate() {
            let parent_index = target.parent_indices[target_index];
            let parent = if parent_index < 0 {
                &QsTransform::IDENTITY
            } else {
                &target_model[parent_index as usize]
            };
            let mut local = target.reference_local[target_index].clone();
            if let Some(source_index) = *source_index {
                // Transfer motion relative to the source bind pose. Copying absolute
                // source rotations would rotate FO4's different bone axes at rest.
                let delta = quat_mul(
                    &source_model[source_index].rotation,
                    &quat_conjugate(&source_reference[source_index].rotation),
                );
                let desired = quat_mul(&delta, &target_reference[target_index].rotation);
                local.rotation =
                    quat_normalize(&quat_mul(&quat_conjugate(&parent.rotation), &desired));
                let source_parent = source_parents[source_index];
                let (animated_parent, reference_parent) = if source_parent < 0 {
                    (&QsTransform::IDENTITY, &QsTransform::IDENTITY)
                } else {
                    (
                        &source_model[source_parent as usize],
                        &source_reference[source_parent as usize],
                    )
                };
                // First-person Looking/Translate/Rotate/Bip nodes have no FO4
                // counterpart. Their motion belongs to the next mapped child.
                let animated_translation =
                    relative_translation(&source_model[source_index], animated_parent);
                let reference_translation =
                    relative_translation(&source_reference[source_index], reference_parent);
                let translation_delta = std::array::from_fn(|axis| {
                    (animated_translation[axis] - reference_translation[axis])
                        * animated_parent.scale[axis]
                });
                let model_delta = quat_rotate(&animated_parent.rotation, &translation_delta);
                let local_delta = quat_rotate(&quat_conjugate(&parent.rotation), &model_delta);
                for axis in 0..3 {
                    local.translation[axis] += local_delta[axis] / parent.scale[axis];
                }
                for axis in 0..3 {
                    local.scale[axis] *= source_local[source_index].scale[axis]
                        / source.reference_local[source_index].scale[axis];
                }
            }
            if local
                .translation
                .iter()
                .chain(local.rotation.iter())
                .chain(local.scale.iter())
                .any(|value| !value.is_finite())
            {
                return Err(format!(
                    "nonfinite retargeted pose on {}",
                    target.bone_names[target_index]
                ));
            }
            let channel = &mut output.channels[target_index];
            channel.rotations.push(key(
                time,
                local.rotation.iter().map(|v| *v as f64).collect(),
            ));
            channel.translations.push(key(
                time,
                local.translation.iter().map(|v| *v as f64).collect(),
            ));
            channel
                .scales
                .push(key(time, local.scale.iter().map(|v| *v as f64).collect()));
            target_model.push(QsTransform::compose(parent, &local));
        }
    }
    output.channels = output
        .channels
        .into_iter()
        .zip(target_to_source)
        .filter_map(|(channel, source)| source.map(|_| channel))
        .collect();
    Ok((output, omitted))
}

fn relative_translation(child: &QsTransform, parent: &QsTransform) -> [f32; 3] {
    let difference = std::array::from_fn(|axis| child.translation[axis] - parent.translation[axis]);
    let rotated = quat_rotate(&quat_conjugate(&parent.rotation), &difference);
    std::array::from_fn(|axis| rotated[axis] / parent.scale[axis])
}

fn key(time: f64, value: Vec<f64>) -> AnimationKeyframe {
    AnimationKeyframe {
        time,
        value,
        interpolation: Interpolation::Linear,
    }
}

pub(super) fn sample_channel(
    channel: &BoneChannel,
    reference: &QsTransform,
    time: f64,
) -> QsTransform {
    let mut pose = reference.clone();
    if !channel.translations.is_empty() {
        let (x, y, z) = super::sample_translation(&channel.translations, time);
        pose.translation = [x as f32, y as f32, z as f32];
    }
    if !channel.rotations.is_empty() {
        let (x, y, z, w) = super::sample_rotation(&channel.rotations, time);
        pose.rotation = [x as f32, y as f32, z as f32, w as f32];
    }
    if !channel.scales.is_empty() {
        pose.scale = super::sample_scale_axes(&channel.scales, time).map(|value| value as f32);
    }
    pose
}

pub(super) fn preserve_weapon_attachments(
    source_clip: &AnimationClip,
    target_clip: &mut AnimationClip,
    source: &PoseSkeleton,
    target: &PoseSkeleton,
    bindings: &[(&str, &str, &str, &str)],
    sample_rate: f64,
) -> Result<(), String> {
    let bone = |skeleton: &PoseSkeleton, name: &str| {
        skeleton
            .bone_names
            .iter()
            .position(|bone| bone == name)
            .ok_or_else(|| format!("weapon requires attachment bone {name}"))
    };
    let mut attachments = Vec::new();
    for &(source_name, target_name, source_hand, target_hand) in bindings {
        let target_index = bone(target, target_name)?;
        let parent = usize::try_from(target.parent_indices[target_index])
            .map_err(|_| format!("weapon attachment {target_name} has no parent"))?;
        let channel = target_clip
            .channels
            .iter()
            .position(|channel| channel.bone_name == target_name)
            .ok_or_else(|| format!("weapon has no {target_name} output track"))?;
        attachments.push((
            bone(source, source_name)?,
            channel,
            parent,
            bone(source, source_hand)?,
            bone(target, target_hand)?,
        ));
    }
    let samples = super::target_sample_count(target_clip.duration, sample_rate);
    for frame in 0..samples {
        let time = if samples > 1 {
            target_clip.duration * frame as f64 / (samples - 1) as f64
        } else {
            0.0
        };
        let source_pose = sample_pose(source_clip, source, time).all_model();
        let target_pose = sample_pose(target_clip, target, time).all_model();
        for &(source_bone, channel, parent, source_hand, target_hand) in &attachments {
            let attachment = &source_pose[source_bone];
            let parent = &target_pose[parent];
            // Source-owned weapon geometry retains its original attachment axes.
            let position = std::array::from_fn(|axis| {
                target_pose[target_hand].translation[axis] + attachment.translation[axis]
                    - source_pose[source_hand].translation[axis]
                    - parent.translation[axis]
            });
            let translation = quat_rotate(&quat_conjugate(&parent.rotation), &position);
            let rotation = quat_normalize(&quat_mul(
                &quat_conjugate(&parent.rotation),
                &attachment.rotation,
            ));
            let track = &mut target_clip.channels[channel];
            for axis in 0..3 {
                track.translations[frame].value[axis] =
                    (translation[axis] / parent.scale[axis]) as f64;
                track.scales[frame].value[axis] =
                    (attachment.scale[axis] / parent.scale[axis]) as f64;
            }
            track.rotations[frame].value = rotation.into_iter().map(f64::from).collect();
        }
    }
    Ok(())
}

pub(super) fn sample_pose(clip: &AnimationClip, skeleton: &PoseSkeleton, time: f64) -> Pose {
    let mut locals = skeleton.reference_local.clone();
    for channel in &clip.channels {
        if let Some(index) = skeleton
            .bone_names
            .iter()
            .position(|name| name == &channel.bone_name)
        {
            locals[index] = sample_channel(channel, &locals[index], time);
        }
    }
    Pose::from_local(skeleton.clone(), locals)
}

fn validate_skeleton(skeleton: &PoseSkeleton) -> Result<(), String> {
    let count = skeleton.bone_count();
    if count == 0
        || skeleton.parent_indices.len() != count
        || skeleton.reference_local.len() != count
    {
        return Err("invalid retarget skeleton dimensions".into());
    }
    let mut names = HashSet::new();
    for (index, name) in skeleton.bone_names.iter().enumerate() {
        if name.is_empty() || !names.insert(name.to_ascii_lowercase()) {
            return Err(format!("duplicate or empty skeleton bone {name:?}"));
        }
        let parent = skeleton.parent_indices[index];
        if parent < -1 || parent >= index as i32 {
            return Err(format!("invalid parent for skeleton bone {name}"));
        }
        let pose = &skeleton.reference_local[index];
        if pose
            .translation
            .iter()
            .chain(pose.rotation.iter())
            .chain(pose.scale.iter())
            .any(|value| !value.is_finite())
            || pose.scale.iter().any(|value| value.abs() < 1.0e-8)
        {
            return Err(format!(
                "invalid reference transform for skeleton bone {name}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig(rotation: [f32; 4], length: f32) -> PoseSkeleton {
        PoseSkeleton {
            bone_names: vec!["Root".into(), "Hand".into()],
            parent_indices: vec![-1, 0],
            reference_local: vec![
                QsTransform::IDENTITY,
                QsTransform {
                    rotation,
                    translation: [length, 0.0, 0.0],
                    scale: [1.0; 3],
                },
            ],
        }
    }

    fn clip(rotation: [f32; 4]) -> AnimationClip {
        AnimationClip {
            duration: 1.0,
            channels: vec![BoneChannel {
                bone_name: "Hand".into(),
                rotations: vec![key(0.0, rotation.iter().map(|v| *v as f64).collect())],
                translations: vec![],
                scales: vec![],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn source_bind_pose_becomes_target_bind_pose_with_target_lengths() {
        let source_rotation =
            havok_native::animation::pose::quat_from_axis_angle(&[0.0, 0.0, 1.0], 1.0);
        let target_rotation =
            havok_native::animation::pose::quat_from_axis_angle(&[1.0, 0.0, 0.0], 0.7);
        let (converted, _) = retarget_weapon_clip(
            &clip(source_rotation),
            &rig(source_rotation, 2.0),
            &rig(target_rotation, 4.0),
            &HashMap::new(),
            30.0,
            1.0,
        )
        .unwrap();
        let pose = sample_channel(&converted.channels[1], &QsTransform::IDENTITY, 0.0);
        assert_eq!(pose.translation, [4.0, 0.0, 0.0]);
        for (actual, expected) in pose.rotation.iter().zip(target_rotation) {
            assert!((actual - expected).abs() < 1.0e-5);
        }
    }

    #[test]
    fn playback_speed_retimes_pose_and_events_together() {
        let mut source = clip(QsTransform::IDENTITY.rotation);
        source.events.push(super::super::AnimationEvent {
            time: 0.4,
            source_time: 0.4,
            text: "weaponFire".into(),
        });
        let rig = rig(QsTransform::IDENTITY.rotation, 2.0);
        let (converted, _) =
            retarget_weapon_clip(&source, &rig, &rig, &HashMap::new(), 30.0, 2.0).unwrap();
        assert_eq!(converted.duration, 0.5);
        assert_eq!(converted.events[0].time, 0.2);
        assert_eq!(converted.channels[1].rotations.last().unwrap().time, 0.5);
    }

    #[test]
    fn target_only_bones_are_absent_from_weapon_binding() {
        let source = rig(QsTransform::IDENTITY.rotation, 2.0);
        let mut target = source.clone();
        target
            .bone_names
            .extend(["Face".into(), "HandHelper".into()]);
        target.parent_indices.extend([0, 1]);
        target.reference_local.extend([QsTransform::IDENTITY; 2]);
        let (converted, _) = retarget_weapon_clip(
            &clip(QsTransform::IDENTITY.rotation),
            &source,
            &target,
            &HashMap::new(),
            30.0,
            1.0,
        )
        .unwrap();
        let binding = super::super::bind_fnv_kf_clip(
            &target.bone_names,
            &converted,
            "Actors/Character/CharacterAssets/Skeleton.hkx",
            "Actors/Test/Animations/Test.hkx",
            "Human",
            &[],
        )
        .unwrap();
        assert_eq!(binding.binding.transform_track_to_bone_indices, vec![0, 1]);
        assert_eq!(converted.channels.len(), 2);
    }

    #[test]
    fn unmapped_intermediate_bone_motion_reaches_mapped_child() {
        let source = PoseSkeleton {
            bone_names: vec!["Root".into(), "NeckHelper".into(), "Hand".into()],
            parent_indices: vec![-1, 0, 1],
            reference_local: vec![QsTransform::IDENTITY; 3],
        };
        let target = rig(QsTransform::IDENTITY.rotation, 4.0);
        let quarter_turn = havok_native::animation::pose::quat_from_axis_angle(
            &[0.0, 0.0, 1.0],
            std::f32::consts::FRAC_PI_2,
        );
        let mut animation = clip(QsTransform::IDENTITY.rotation);
        let mut helper = animation.channels[0].clone();
        helper.bone_name = "NeckHelper".into();
        helper.rotations[0].value = quarter_turn.iter().map(|v| *v as f64).collect();
        animation.channels.push(helper);
        let (converted, _) =
            retarget_weapon_clip(&animation, &source, &target, &HashMap::new(), 30.0, 1.0).unwrap();
        let actual = sample_channel(&converted.channels[1], &QsTransform::IDENTITY, 0.0).rotation;
        for (actual, expected) in actual.iter().zip(quarter_turn) {
            assert!((actual - expected).abs() < 1.0e-5);
        }
    }

    #[test]
    fn nested_first_person_bip_motion_reaches_pelvis_without_duplicate_root() {
        let source = PoseSkeleton {
            bone_names: vec![
                "Bip01".into(),
                "Bip01 NonAccum".into(),
                "Bip".into(),
                "Bip01 Pelvis".into(),
            ],
            parent_indices: vec![-1, 0, 1, 2],
            reference_local: vec![QsTransform::IDENTITY; 4],
        };
        let target = PoseSkeleton {
            bone_names: vec!["Root".into(), "COM".into(), "Pelvis".into()],
            parent_indices: vec![-1, 0, 1],
            reference_local: vec![QsTransform::IDENTITY; 3],
        };
        let animation = AnimationClip {
            duration: 1.0,
            channels: vec![BoneChannel {
                bone_name: "Bip".into(),
                rotations: vec![],
                translations: vec![key(0.0, vec![2.0, 3.0, 4.0])],
                scales: vec![],
            }],
            ..Default::default()
        };
        let (converted, _) = retarget_weapon_clip(
            &animation,
            &source,
            &target,
            &humanoid_bone_map().unwrap(),
            30.0,
            1.0,
        )
        .unwrap();
        assert_eq!(converted.channels[0].translations[0].value, vec![0.0; 3]);
        assert_eq!(
            converted.channels[2].translations[0].value,
            vec![2.0, 3.0, 4.0]
        );
    }

    #[test]
    #[ignore = "requires locally extracted FNV and Fallout 4 assets"]
    fn real_pistol_clips_retarget_and_pack_for_both_perspectives() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        for (source_dir, target_dir) in [
            ("characters/_male", "actors/character"),
            ("characters/_1stperson", "actors/character/_1stperson"),
        ] {
            let source_rig = root
                .join("extracted/fnv/meshes")
                .join(source_dir)
                .join("skeleton.nif");
            let nif = nif_core_native::model::NifFile::from_bytes(
                &std::fs::read(source_rig).unwrap(),
                None,
            )
            .unwrap();
            let source = crate::phase::skeleton::nif_reference_pose(&nif).unwrap();
            let bytes = std::fs::read(
                root.join("extracted/fo4/meshes")
                    .join(target_dir)
                    .join("characterassets/skeleton.hkx"),
            )
            .unwrap();
            let xml = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
            let target_record = havok_native::animation::parsers::parse_skeleton_xml(&xml).unwrap();
            let target = target_reference_pose(&target_record).unwrap();
            for filename in ["1hpattackright.kf", "1hpreloadl.kf"] {
                let bytes = std::fs::read(
                    root.join("extracted/fnv/meshes")
                        .join(source_dir)
                        .join(filename),
                )
                .unwrap();
                let parsed = super::super::parse_kf_bytes(&bytes, filename).unwrap();
                let (clip, _) = retarget_weapon_clip(
                    &parsed,
                    &source,
                    &target,
                    &humanoid_bone_map().unwrap(),
                    30.0,
                    1.0,
                )
                .unwrap();
                let declaration = super::super::bind_fnv_kf_clip(
                    &target.bone_names,
                    &clip,
                    "Actors/Character/CharacterAssets/Skeleton.hkx",
                    "Actors/Test/Animations/Test.hkx",
                    &target_record.name,
                    &[],
                )
                .unwrap();
                let (xml, _) = super::super::clip_to_havok_xml_with_float_tracks(
                    &clip,
                    &declaration,
                    &[],
                    &target_record.name,
                    30.0,
                    super::super::FnvExtractedMotionPolicy::RejectNonzero,
                )
                .unwrap();
                let hkx = havok_native::api::havok_xml_to_hkx(&xml).unwrap();
                super::super::validate_staged_fo4_animation(&hkx).unwrap();
                assert!(clip.channels.len() < target.bone_count());
                assert!(clip.duration > 0.0);
                assert!(
                    clip.channels
                        .iter()
                        .all(|channel| !channel.rotations.is_empty())
                );
            }
        }
    }
}
