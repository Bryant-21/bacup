use super::{
    AnimationClip, AnimationKeyframe, BoneChannel, Interpolation, normalize_fnv_bone_name,
};

pub(super) fn prepare(
    clip: &mut AnimationClip,
    bone_names: &[String],
    skeleton_hkx: &[u8],
) -> Result<(), String> {
    let root_name = normalize_fnv_bone_name(&clip.accum_root);
    let Some(bone_index) = bone_names
        .iter()
        .position(|name| normalize_fnv_bone_name(name) == root_name)
    else {
        return Ok(());
    };
    let xml = havok_native::api::havok_hkx_to_xml(skeleton_hkx)
        .map_err(|error| format!("read actor accumulator reference scale: {error}"))?;
    let scale = reference_scale(&xml, bone_index)?;
    prepare_with_scale(clip, &bone_names[bone_index], scale);
    Ok(())
}

fn reference_scale(xml: &str, bone_index: usize) -> Result<[f64; 3], String> {
    let skeleton = havok_native::animation::parsers::parse_skeleton_xml(xml)
        .map_err(|error| error.to_string())?;
    skeleton
        .reference_pose
        .get(bone_index)
        .map(|pose| pose.s.map(f64::from))
        .ok_or_else(|| format!("actor skeleton has no reference scale for bone {bone_index}"))
}

fn prepare_with_scale(clip: &mut AnimationClip, root_name: &str, scale: [f64; 3]) {
    let normalized = normalize_fnv_bone_name(root_name);
    let track_index = clip
        .channels
        .iter()
        .position(|channel| normalize_fnv_bone_name(&channel.bone_name) == normalized)
        .unwrap_or_else(|| {
            clip.channels.push(BoneChannel {
                bone_name: root_name.to_string(),
                rotations: Vec::new(),
                translations: Vec::new(),
                scales: Vec::new(),
            });
            clip.channels.len() - 1
        });
    let root = &mut clip.channels[track_index];
    if root.scales.is_empty() {
        root.scales.push(AnimationKeyframe {
            time: 0.0,
            value: scale.to_vec(),
            interpolation: Interpolation::Constant,
        });
    }
    // FNV AnimData initializes accumulator rotation to identity (VA 0x4901E8)
    // and resets its translation after extracting movement (VA 0x49BD0F).
    // Keep source translation keys until Havok reference-frame extraction.
    clip.actor_accumulator_track = Some(track_index);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::animations::{
        FnvClipMotion, FnvExtractedMotionPolicy, bind_fnv_kf_clip, clip_to_havok_xml,
    };

    fn key(time: f64, value: &[f64]) -> AnimationKeyframe {
        AnimationKeyframe {
            time,
            value: value.to_vec(),
            interpolation: Interpolation::Linear,
        }
    }

    fn clip() -> AnimationClip {
        AnimationClip {
            name: "Idle".to_string(),
            duration: 1.0,
            accum_root: "Bip01".to_string(),
            channels: vec![BoneChannel {
                bone_name: "Bip01 NonAccum".to_string(),
                translations: vec![key(0.0, &[0.0, 0.0, 88.0])],
                rotations: vec![key(0.0, &[0.0, 0.0, 0.70710678, 0.70710678])],
                scales: Vec::new(),
            }],
            ..Default::default()
        }
    }

    fn emit(clip: &AnimationClip) -> (String, FnvClipMotion) {
        let bones = vec!["Bip01".to_string(), "Bip01 NonAccum".to_string()];
        let declaration = bind_fnv_kf_clip(
            &bones,
            clip,
            "Actors/Test/Skeleton.hkx",
            "Actors/Test/Idle.hkx",
            "Test",
            &[],
        )
        .unwrap();
        clip_to_havok_xml(
            clip,
            &declaration,
            "Test",
            2.0,
            FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame,
        )
        .unwrap()
    }

    fn transforms(xml: &str) -> Vec<Vec<f64>> {
        let clip = havok_native::animation::clip::extract_clip(xml, None).unwrap();
        let mut poses = Vec::new();
        for frame in 0..clip.channels[0].translations.len() {
            for channel in &clip.channels {
                let scale = channel
                    .scales
                    .iter()
                    .find(|key| key.time == channel.translations[frame].time)
                    .map_or([1.0; 3], |key| key.value);
                poses.push(
                    channel.translations[frame]
                        .value
                        .into_iter()
                        .chain(channel.rotations[frame].value)
                        .chain(scale)
                        .map(f64::from)
                        .collect(),
                );
            }
        }
        poses
    }

    #[test]
    fn reference_scale_accepts_packed_and_compact_pose_rows_without_stride_assumptions() {
        for pose in [
            "(0 0 0 0 0 0 1 1 1 1)(3 4 5 0 0 0 1 1.75 2 2.25)",
            "(0 0 0 0)(0 0 0 1)(1 1 1 0)(3 4 5 0)(0 0 0 1)(1.75 2 2.25 0)",
            "(0 0 0)(0 0 0 1)(1 1 1)(3 4 5)(0 0 0 1)(1.75 2 2.25)",
        ] {
            let xml = format!(
                r#"<hkpackfile><hksection><hkobject class="hkaSkeleton">
                <hkparam name="name">Test</hkparam>
                <hkparam name="bones" numelements="2"><hkobject><hkparam name="name">Root</hkparam></hkobject><hkobject><hkparam name="name">Bip01</hkparam></hkobject></hkparam>
                <hkparam name="parentIndices">-1 0</hkparam>
                <hkparam name="referencePose" numelements="2">{pose}</hkparam>
                </hkobject></hksection></hkpackfile>"#
            );
            assert_eq!(reference_scale(&xml, 1).unwrap(), [1.75, 2.0, 2.25]);
        }
    }

    #[test]
    fn sparse_idle_explicitly_resets_accumulator_and_preserves_reference_scale() {
        let mut idle = clip();
        prepare_with_scale(&mut idle, "Bip01", [1.75; 3]);
        let (xml, motion) = emit(&idle);
        assert_eq!(motion, FnvClipMotion::InPlace);
        let packed = havok_native::api::havok_xml_to_hkx(&xml).unwrap();
        let roundtrip = havok_native::api::havok_hkx_to_xml(&packed).unwrap();
        assert_eq!(transforms(&roundtrip), transforms(&xml));
        for frame in transforms(&xml).chunks_exact(2) {
            assert_eq!(&frame[0][..3], &[0.0, 0.0, 88.0]);
            assert_eq!(
                &frame[1],
                &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.75, 1.75, 1.75]
            );
        }
    }

    #[test]
    fn moving_and_turning_clip_retains_extracted_deltas_and_authored_scale() {
        let mut moving = clip();
        moving.channels.push(BoneChannel {
            bone_name: "Bip01".to_string(),
            translations: vec![
                key(0.0, &[30.0, 40.0, 105.0]),
                key(1.0, &[130.0, 65.0, 105.0]),
            ],
            rotations: vec![
                key(0.0, &[0.0, 0.0, 0.0, 1.0]),
                key(1.0, &[0.0, 0.0, 0.7071067811865475, 0.7071067811865476]),
            ],
            scales: vec![key(0.0, &[2.0])],
        });
        prepare_with_scale(&mut moving, "Bip01", [1.75; 3]);
        let (xml, motion) = emit(&moving);
        let FnvClipMotion::ExtractedPlanarYaw {
            end_displacement,
            end_yaw_radians,
            ..
        } = motion
        else {
            panic!("moving clip lost extracted motion");
        };
        assert_eq!(end_displacement, [100.0, 25.0]);
        assert!((end_yaw_radians - std::f64::consts::FRAC_PI_2).abs() < 1.0e-6);
        for frame in transforms(&xml).chunks_exact(2) {
            assert_eq!(
                &frame[1],
                &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0]
            );
        }
    }

    #[test]
    #[ignore = "requires MODKIT_FNV_CREATURE_ANIMATION_INPUTS source corpus manifest"]
    fn source_fnv_actor_accumulator_corpus() {
        use crate::phase::animations::*;
        use crate::phase::skeleton::convert_source_owned_skeleton_artifact;
        use std::collections::HashMap;
        use std::path::Path;

        let manifest = std::env::var("MODKIT_FNV_CREATURE_ANIMATION_INPUTS").unwrap();
        let rows: serde_json::Value =
            serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
        let mut clips_checked = 0;
        let mut sparse_roots = 0;
        let mut extracted_motion = 0;
        for family in rows.as_array().unwrap() {
            let float_slots = family["float_slots"]
                .as_array()
                .unwrap()
                .iter()
                .map(|slot| slot.as_str().unwrap().to_string())
                .collect::<Vec<_>>();
            let skeleton = convert_source_owned_skeleton_artifact(
                Path::new(family["source_skeleton"].as_str().unwrap()),
                "Actors/Test/Skeleton.hkx",
                family["skeleton_name"].as_str().unwrap(),
                &float_slots,
            )
            .unwrap();
            for source in family["clips"].as_array().unwrap() {
                let path = source["source_kf"].as_str().unwrap();
                let index = source["sequence_index"].as_u64().unwrap() as usize;
                let bytes = std::fs::read(path).unwrap();
                let mut original = parse_kf_sequence_bytes(&bytes, path, index).unwrap();
                omit_missing_weapon_attachment_track(&mut original, &skeleton.ordered_bone_names);
                omit_unbound_stationary_scene_tracks(
                    &mut original,
                    &skeleton.ordered_bone_names,
                    path,
                );
                let root = original.channels.iter().position(|channel| {
                    normalize_fnv_bone_name(&channel.bone_name)
                        == normalize_fnv_bone_name(&original.accum_root)
                });
                sparse_roots += usize::from(root.is_none());
                let original_motion = extract_planar_root_motion_from_mapping(
                    &original,
                    &(0..original.channels.len()).collect::<Vec<_>>(),
                    target_sample_count(original.duration, 30.0),
                    FnvExtractedMotionPolicy::RejectNonzero,
                )
                .unwrap_or_else(|error| panic!("{path}: {error}"));
                extracted_motion += usize::from(original_motion.is_some());
                let event_map = HashMap::new();
                let staged = stage_fnv_creature_kf(
                    &bytes,
                    FnvKfStageRequest {
                        source_kf: path,
                        output_clip_path: "Actors/Test/Clip.hkx",
                        sequence_index: Some(index),
                        skeleton: FnvKfSkeletonContract {
                            skeleton_path: &skeleton.runtime_path,
                            ordered_bone_names: &skeleton.ordered_bone_names,
                            ordered_float_slot_names: &skeleton.float_slot_names,
                        },
                        reference_skeleton_hkx: &skeleton.hkx_bytes,
                        original_skeleton_name: &skeleton.skeleton_name,
                        event_map: &event_map,
                        target_sample_rate_hz: Some(30.0),
                        attack_kind: None,
                        automatic_attack: None,
                        attack_playback_speed: None,
                        equipment_index: None,
                        extracted_motion_policy: if original_motion.is_some() {
                            FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame
                        } else {
                            FnvExtractedMotionPolicy::RejectNonzero
                        },
                    },
                )
                .unwrap_or_else(|error| panic!("{path}: {error}"));
                let xml = havok_native::api::havok_hkx_to_xml(&staged.hkx_bytes).unwrap();
                if let Some(root_bone) = skeleton.ordered_bone_names.iter().position(|name| {
                    normalize_fnv_bone_name(name) == normalize_fnv_bone_name(&original.accum_root)
                }) {
                    let track = staged
                        .receipt
                        .binding
                        .transform_track_to_bone_indices
                        .iter()
                        .position(|bone| *bone == root_bone)
                        .expect("bound accumulator missing");
                    for frame in transforms(&xml).chunks_exact(staged.receipt.transform_track_count)
                    {
                        assert_eq!(&frame[track][..3], &[0.0; 3], "{path}");
                        if root.is_none() {
                            assert_eq!(&frame[track][3..7], &[0.0, 0.0, 0.0, 1.0], "{path}");
                        }
                    }
                }
                match (&original_motion, &staged.receipt.motion) {
                    (None, FnvClipMotion::InPlace) => {}
                    (
                        Some(source),
                        FnvClipMotion::ExtractedPlanar {
                            end_displacement, ..
                        },
                    )
                    | (
                        Some(source),
                        FnvClipMotion::ExtractedPlanarYaw {
                            end_displacement, ..
                        },
                    ) => {
                        let end = source.samples.last().unwrap();
                        assert_eq!(*end_displacement, [end[0], end[1]], "{path}");
                    }
                    _ => panic!("{path}: extracted motion changed"),
                }
                clips_checked += 1;
            }
        }
        assert!(clips_checked > 0 && sparse_roots > 0 && extracted_motion > 0);
        eprintln!(
            "FNV accumulator corpus: {clips_checked} clips, {sparse_roots} sparse roots, {extracted_motion} moving clips"
        );
    }
}
