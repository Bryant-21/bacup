use std::collections::{BTreeMap, HashMap, HashSet};

use havok_native::animation::parsers::SkeletonRecord;
use havok_native::animation::pose::{
    PoseSkeleton, QsTransform, quat_conjugate, quat_mul, quat_rotate,
};
use serde::Serialize;

use super::starfield_afx::{AfxContract, AnnotationPlan, annotation_plan};
use super::starfield_decoder::{DecodedStarfieldAnimation, StarfieldBone, StarfieldBoneTrack};
use super::{AnimationClip, AnimationEvent, AnimationKeyframe, BoneChannel, Interpolation};

const METERS_TO_FO4: f64 = 69.99125;

fn rest_relative_channel(bone: &StarfieldBone, track: &StarfieldBoneTrack) -> BoneChannel {
    // The source importer applies AF keys to rest-relative PoseBone channels, even without AFX Additive.
    let rest_rotation = bone.rotation.map(|value| value as f32);
    let key = |frame: u16, value: Vec<f64>| AnimationKeyframe {
        time: f64::from(frame) / 30.,
        value,
        interpolation: Interpolation::Linear,
    };
    BoneChannel {
        bone_name: track.bone_name.clone(),
        rotations: track
            .rotations
            .iter()
            .map(|value| {
                key(
                    value.frame,
                    quat_mul(&rest_rotation, &value.value.map(|value| value as f32))
                        .map(f64::from)
                        .to_vec(),
                )
            })
            .collect(),
        translations: track
            .translations
            .iter()
            .map(|value| {
                let offset = quat_rotate(
                    &rest_rotation,
                    &value.value.map(|value| (value * METERS_TO_FO4) as f32),
                );
                key(
                    value.frame,
                    (0..3)
                        .map(|axis| {
                            bone.translation[axis] * METERS_TO_FO4 + f64::from(offset[axis])
                        })
                        .collect(),
                )
            })
            .collect(),
        scales: vec![],
    }
}

#[derive(Debug, Serialize)]
pub(super) struct StarfieldRetargetReceipt {
    pub source_tag: String,
    pub source_tracks: usize,
    pub target_tracks: usize,
    pub canonical_sources: BTreeMap<String, String>,
    pub folded_source_bones: Vec<String>,
    pub omitted_source_channels: Vec<String>,
    pub source_weapon_suffix_node_hashes: Vec<u32>,
    pub annotation_plan: AnnotationPlan,
    pub binding_blend_hint: StarfieldBlendHint,
    pub functionality_certified: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) enum StarfieldBlendHint {
    Normal,
    Additive,
}

fn canonical_map(
    source: &PoseSkeleton,
    target: &PoseSkeleton,
) -> Result<(HashMap<String, String>, Vec<String>), String> {
    #[derive(serde::Deserialize)]
    struct Table {
        bones: HashMap<String, String>,
    }
    let table: Table = serde_saphyr::from_str(crate::embedded::SKELETON_STARFIELD_TO_FO4)
        .map_err(|error| error.to_string())?;
    let source_names: HashSet<_> = source
        .bone_names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect();
    let target_names: HashSet<_> = target
        .bone_names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect();
    let mut groups: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (source, target) in table.bones {
        if source_names.contains(&source.to_ascii_lowercase())
            && target_names.contains(&target.to_ascii_lowercase())
        {
            groups
                .entry(target.to_ascii_lowercase())
                .or_default()
                .push((source, target));
        }
    }
    let preferred = [
        "Root",
        "C_Chest",
        "L_Biceps",
        "R_Biceps",
        "L_Forearm",
        "R_Forearm",
        "L_Wrist",
        "R_Wrist",
        "L_Calf",
        "R_Calf",
    ];
    let mut map = HashMap::new();
    let mut folded = Vec::new();
    for (_, mut aliases) in groups {
        aliases.sort();
        let selected = if aliases.len() == 1 {
            0
        } else {
            preferred
                .iter()
                .find_map(|preferred| {
                    aliases
                        .iter()
                        .position(|(source, _)| source.eq_ignore_ascii_case(preferred))
                })
                .ok_or_else(|| format!("ambiguous Starfield animation aliases {aliases:?}"))?
        };
        let canonical = aliases.swap_remove(selected);
        folded.extend(aliases.into_iter().map(|(name, _)| name));
        map.insert(canonical.0, canonical.1);
    }
    folded.sort();
    Ok((map, folded))
}

pub(super) fn retarget_absolute(
    animation: &DecodedStarfieldAnimation,
    afx: &AfxContract,
    target_record: &SkeletonRecord,
    playback_speed: f64,
) -> Result<(AnimationClip, StarfieldRetargetReceipt), String> {
    if afx
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case("Additive"))
    {
        return Err("Starfield additive AF requires verified source composition and target additive graph binding".into());
    }
    if animation
        .tracks
        .iter()
        .any(|track| !track.scalars.is_empty() || !track.priorities.is_empty())
    {
        return Err(
            "Starfield scalar/priority tracks require an explicit target lowering contract".into(),
        );
    }
    if animation
        .header
        .translation
        .iter()
        .any(|value| value.abs() > 1e-6)
        || animation.header.rotation[..3]
            .iter()
            .any(|value| value.abs() > 1e-6)
        || (animation.header.rotation[3].abs() - 1.).abs() > 1e-6
    {
        return Err("Starfield AF root header transform is not identity".into());
    }
    let source = PoseSkeleton {
        bone_names: animation
            .bones
            .iter()
            .map(|bone| bone.name.clone())
            .collect(),
        parent_indices: animation.bones.iter().map(|bone| bone.parent).collect(),
        reference_local: animation
            .bones
            .iter()
            .map(|bone| QsTransform {
                translation: bone.translation.map(|value| (value * METERS_TO_FO4) as f32),
                rotation: bone.rotation.map(|value| value as f32),
                scale: [1.; 3],
            })
            .collect(),
    };
    let target = super::weapon_retarget::target_reference_pose(target_record)?;
    let (map, folded_source_bones) = canonical_map(&source, &target)?;
    let duration = f64::from(animation.header.frame_count) / 30.;
    let plan = annotation_plan(afx, 30., playback_speed)?;
    let clip = AnimationClip {
        name: afx.tag.clone(),
        source_stop_time: duration,
        duration,
        frequency: 1.,
        cycle_type: "CYCLE_CLAMP".into(),
        channels: animation
            .tracks
            .iter()
            .map(|track| rest_relative_channel(&animation.bones[track.bone_index], track))
            .collect(),
        events: plan
            .annotations
            .iter()
            .map(|(time, text)| AnimationEvent {
                time: time * playback_speed,
                source_time: time * playback_speed,
                text: text.clone(),
            })
            .collect(),
        ..AnimationClip::default()
    };
    let (output, mut omitted_source_channels) = super::weapon_retarget::retarget_weapon_clip(
        &clip,
        &source,
        &target,
        &map,
        30.,
        playback_speed,
    )?;
    let source_indices: HashMap<_, _> = source
        .bone_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.to_ascii_lowercase(), index))
        .collect();
    let mut needed = HashSet::new();
    for name in map.keys() {
        let mut index = source_indices[&name.to_ascii_lowercase()] as i32;
        while index >= 0 && needed.insert(index as usize) {
            index = source.parent_indices[index as usize];
        }
    }
    omitted_source_channels.extend(
        animation
            .tracks
            .iter()
            .filter(|track| !needed.contains(&track.bone_index))
            .map(|track| track.bone_name.clone()),
    );
    omitted_source_channels.sort();
    omitted_source_channels.dedup();
    let receipt = StarfieldRetargetReceipt {
        source_tag: afx.tag.clone(),
        source_tracks: animation.tracks.len(),
        target_tracks: output.channels.len(),
        canonical_sources: map
            .into_iter()
            .map(|(source, target)| (target, source))
            .collect(),
        folded_source_bones,
        omitted_source_channels,
        source_weapon_suffix_node_hashes: animation.weapon_suffix_node_hashes.clone(),
        annotation_plan: plan,
        binding_blend_hint: StarfieldBlendHint::Normal,
        functionality_certified: false,
    };
    Ok((output, receipt))
}

fn make_target_additive(clip: &mut AnimationClip, target: &PoseSkeleton) -> Result<(), String> {
    for channel in &mut clip.channels {
        let index = target
            .bone_names
            .iter()
            .position(|name| name.eq_ignore_ascii_case(&channel.bone_name))
            .ok_or_else(|| format!("additive target bone {} is absent", channel.bone_name))?;
        let reference = &target.reference_local[index];
        if reference
            .scale
            .iter()
            .any(|value| !value.is_finite() || value.abs() < 1e-8)
        {
            return Err("additive target reference scale cannot be inverted".into());
        }
        let inverse = quat_conjugate(&reference.rotation);
        for key in &mut channel.rotations {
            let rotation: [f64; 4] = key
                .value
                .as_slice()
                .try_into()
                .map_err(|_| "additive target rotation width is invalid")?;
            key.value = quat_mul(&inverse, &rotation.map(|value| value as f32))
                .map(f64::from)
                .to_vec();
        }
        // FO4's ADDITIVE (2) composes baseQ*deltaQ and baseT+rotate(baseQ,deltaT).
        for key in &mut channel.translations {
            let value: [f64; 3] = key
                .value
                .as_slice()
                .try_into()
                .map_err(|_| "additive target translation width is invalid")?;
            let offset =
                std::array::from_fn(|axis| value[axis] as f32 - reference.translation[axis]);
            key.value = quat_rotate(&inverse, &offset).map(f64::from).to_vec();
        }
        for key in &mut channel.scales {
            for (axis, value) in key.value.iter_mut().enumerate() {
                *value /= f64::from(reference.scale[axis]);
            }
        }
    }
    Ok(())
}

pub(super) fn retarget_additive(
    animation: &DecodedStarfieldAnimation,
    afx: &AfxContract,
    target_record: &SkeletonRecord,
    playback_speed: f64,
) -> Result<(AnimationClip, StarfieldRetargetReceipt), String> {
    if !afx
        .flags
        .iter()
        .any(|flag| flag.eq_ignore_ascii_case("Additive"))
    {
        return Err("Starfield additive retarget requires an authored Additive contract".into());
    }
    let mut rest_pose_contract = afx.clone();
    rest_pose_contract
        .flags
        .retain(|flag| !flag.eq_ignore_ascii_case("Additive"));
    let (mut clip, mut receipt) = retarget_absolute(
        animation,
        &rest_pose_contract,
        target_record,
        playback_speed,
    )?;
    make_target_additive(
        &mut clip,
        &super::weapon_retarget::target_reference_pose(target_record)?,
    )?;
    receipt.binding_blend_hint = StarfieldBlendHint::Additive;
    Ok((clip, receipt))
}

pub(super) fn pack_retargeted_animation(
    clip: &AnimationClip,
    receipt: &StarfieldRetargetReceipt,
    target: &SkeletonRecord,
    target_path: &str,
    output_name: &str,
) -> Result<Vec<u8>, String> {
    let declaration = super::bind_fnv_kf_clip(
        &target.bone_names,
        clip,
        target_path,
        output_name,
        &target.name,
        &[],
    )
    .map_err(|error| error.to_string())?;
    let (mut xml, _) = super::clip_to_havok_xml_with_float_tracks(
        clip,
        &declaration,
        &[],
        &target.name,
        30.,
        super::FnvExtractedMotionPolicy::RejectNonzero,
    )
    .map_err(|error| error.to_string())?;
    if receipt.binding_blend_hint == StarfieldBlendHint::Additive {
        let range = {
            let doc = roxmltree::Document::parse(&xml).map_err(|error| error.to_string())?;
            let bindings: Vec<_> = doc
                .descendants()
                .filter(|node| node.attribute("class") == Some("hkaAnimationBinding"))
                .collect();
            let [binding] = bindings.as_slice() else {
                return Err("Starfield output requires exactly one animation binding".into());
            };
            binding
                .children()
                .find(|node| node.attribute("name") == Some("blendHint"))
                .ok_or("Starfield output binding has no blend hint")?
                .range()
        };
        xml.replace_range(range, "<hkparam name=\"blendHint\">ADDITIVE</hkparam>");
    }
    let bytes = havok_native::api::havok_xml_to_hkx(&xml).map_err(|error| error.to_string())?;
    super::validate_staged_fo4_animation(&bytes).map_err(|error| error.to_string())?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::super::starfield_afx::parse_afx;
    use super::super::starfield_decoder::{AfHeader, StarfieldKey};
    use super::*;

    #[test]
    fn target_additive_channels_reconstruct_nonidentity_reference_pose() {
        let reference = QsTransform {
            translation: [12., 34., 56.],
            rotation: [0., 0., 0.5_f32.sqrt(), 0.5_f32.sqrt()],
            scale: [1., 1.25, 0.75],
        };
        let target = PoseSkeleton {
            bone_names: vec!["Chest".into()],
            parent_indices: vec![-1],
            reference_local: vec![reference.clone()],
        };
        let key = |value| AnimationKeyframe {
            time: 0.,
            value,
            interpolation: Interpolation::Linear,
        };
        let delta_q = [0.5_f32.sqrt(), 0., 0., 0.5_f32.sqrt()];
        let delta_t = [1., 2., 3.];
        let rotation = quat_mul(&reference.rotation, &delta_q);
        let offset = quat_rotate(&reference.rotation, &delta_t);
        let translation: [f32; 3] =
            std::array::from_fn(|axis| reference.translation[axis] + offset[axis]);
        let mut clip = AnimationClip {
            channels: vec![BoneChannel {
                bone_name: "Chest".into(),
                rotations: vec![key(rotation.map(f64::from).to_vec())],
                translations: vec![key(translation.map(f64::from).to_vec())],
                scales: vec![key(reference.scale.map(f64::from).to_vec())],
            }],
            ..Default::default()
        };
        make_target_additive(&mut clip, &target).unwrap();
        for axis in 0..4 {
            assert!(
                (clip.channels[0].rotations[0].value[axis] - f64::from(delta_q[axis])).abs() < 1e-6
            );
        }
        for axis in 0..3 {
            assert!(
                (clip.channels[0].translations[0].value[axis] - f64::from(delta_t[axis])).abs()
                    < 1e-5
            );
            assert!((clip.channels[0].scales[0].value[axis] - 1.).abs() < 1e-6);
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires source CALUMI DLL, generated HumanRace table, source clips and target skeletons"]
    fn real_eon_selected_fire_and_reload_emit_separate_additive_and_normal_hkx() {
        use super::super::starfield_decoder::CalumiDecoder;
        use super::super::starfield_subgraphs::{
            parse_generated_subgraphs, resolve_source_clip, select_weapon_subgraph,
        };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let meshes = root.join("extracted/starfield/Meshes");
        let rows = parse_generated_subgraphs(
            &std::fs::read_to_string(
                meshes.join("AnimTextData/GeneratedSubgraphData/HumanRace.txt"),
            )
            .unwrap(),
        )
        .unwrap();
        let decoder =
            CalumiDecoder::open(&root.join("refs/sf_animation_io/CALUMI.Animation.dll")).unwrap();
        let output = tempfile::tempdir().unwrap();
        let mut receipts = Vec::new();
        for (perspective, folder, actor) in [
            ("1st", "_1stperson/", vec![]),
            ("3rd", "", vec!["AnimArchetypePlayer".into()]),
        ] {
            let row = select_weapon_subgraph(
                &rows,
                perspective,
                &actor,
                &["AnimStinger".into(), "AnimGripPistol".into()],
            )
            .unwrap();
            let rig = meshes.join(format!("actors/human/{folder}characterassets/skeleton.rig"));
            let skeleton_path = root.join(format!(
                "extracted/fo4/meshes/actors/character/{folder}characterassets/skeleton.hkx"
            ));
            let skeleton_xml =
                havok_native::api::havok_hkx_to_xml(&std::fs::read(&skeleton_path).unwrap())
                    .unwrap();
            let target =
                havok_native::animation::parsers::parse_skeleton_xml(&skeleton_xml).unwrap();
            for tag in ["FireSingle", "Reload"] {
                let selected = resolve_source_clip(row, tag, &meshes).unwrap();
                let animation = decoder
                    .decode(&rig, &meshes.join(&selected.source_af))
                    .unwrap();
                let afx = parse_afx(
                    &std::fs::read_to_string(meshes.join(&selected.source_afx)).unwrap(),
                    f64::from(animation.header.frame_count),
                )
                .unwrap();
                let (clip, receipt) = if tag == "FireSingle" {
                    retarget_additive(&animation, &afx, &target, 1.).unwrap()
                } else {
                    retarget_absolute(&animation, &afx, &target, 1.).unwrap()
                };
                let bytes = pack_retargeted_animation(
                    &clip,
                    &receipt,
                    &target,
                    &skeleton_path.to_string_lossy(),
                    &format!("{tag}.hkx"),
                )
                .unwrap();
                let path = output.path().join(format!("{perspective}_{tag}.hkx"));
                std::fs::write(&path, &bytes).unwrap();
                let xml =
                    havok_native::api::havok_hkx_to_xml(&std::fs::read(&path).unwrap()).unwrap();
                let loaded =
                    havok_native::animation::clip::extract_clip(&xml, Some(&target)).unwrap();
                assert_eq!(loaded.is_additive, tag == "FireSingle");
                assert!(
                    (f64::from(loaded.duration) - f64::from(animation.header.frame_count) / 30.)
                        .abs()
                        < 1e-5
                );
                assert_eq!(loaded.channels.len(), clip.channels.len());
                assert!(clip.channels.len() < target.bone_names.len());
                assert_eq!(
                    loaded
                        .track_to_bone_indices
                        .iter()
                        .map(|index| &target.bone_names[*index as usize])
                        .collect::<Vec<_>>(),
                    clip.channels
                        .iter()
                        .map(|channel| &channel.bone_name)
                        .collect::<Vec<_>>()
                );
                let event = if tag == "FireSingle" {
                    "weaponFire"
                } else {
                    "reloadComplete"
                };
                assert!(loaded.events.iter().any(|item| item.text == event));
                assert!(!receipt.functionality_certified);
                receipts.push(serde_json::json!({"perspective":perspective,"selected":selected,"authored_last_frame":animation.header.frame_count,"target_clip":receipt,"packed_bytes":bytes.len()}));
            }
        }
        std::fs::write(
            std::env::temp_dir().join("b21_sf_eon_actor_hkx_receipt.json"),
            serde_json::to_vec_pretty(&receipts).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn neutral_af_keys_preserve_nonidentity_rig_rest_pose() {
        let bone = StarfieldBone {
            name: "C_Chest".into(),
            parent: -1,
            rotation: [0., 0., 0.5_f64.sqrt(), 0.5_f64.sqrt()],
            translation: [0.05781761, 2., 3.],
        };
        let mut track = StarfieldBoneTrack {
            bone_index: 0,
            bone_name: bone.name.clone(),
            rotations: vec![StarfieldKey {
                frame: 0,
                value: [0., 0., 0., 1.],
            }],
            translations: vec![StarfieldKey {
                frame: 0,
                value: [0.; 3],
            }],
            scalars: vec![],
            priorities: vec![],
        };
        let channel = rest_relative_channel(&bone, &track);
        for axis in 0..4 {
            assert!((channel.rotations[0].value[axis] - bone.rotation[axis]).abs() < 1e-7);
        }
        for axis in 0..3 {
            assert!(
                (channel.translations[0].value[axis] - bone.translation[axis] * METERS_TO_FO4)
                    .abs()
                    < 1e-7
            );
        }
        track.translations[0].value = [1., 0., 0.];
        let translated = rest_relative_channel(&bone, &track);
        assert!(
            (translated.translations[0].value[0] - bone.translation[0] * METERS_TO_FO4).abs()
                < 1e-5
        );
        assert!(
            (translated.translations[0].value[1] - (bone.translation[1] + 1.) * METERS_TO_FO4)
                .abs()
                < 1e-5
        );
    }

    #[test]
    fn canonical_source_is_structural_and_intermediate_bones_remain_in_pose() {
        let source = PoseSkeleton {
            bone_names: vec![
                "Root".into(),
                "C_Spine2".into(),
                "C_Chest".into(),
                "L_Biceps".into(),
                "L_Deltoid".into(),
            ],
            parent_indices: vec![-1, 0, 1, 2, 3],
            reference_local: vec![QsTransform::IDENTITY; 5],
        };
        let target = PoseSkeleton {
            bone_names: vec!["Root".into(), "Chest".into(), "LArm_UpperArm".into()],
            parent_indices: vec![-1, 0, 1],
            reference_local: vec![QsTransform::IDENTITY; 3],
        };
        let (map, folded) = canonical_map(&source, &target).unwrap();
        assert_eq!(map["C_Chest"], "Chest");
        assert_eq!(map["L_Biceps"], "LArm_UpperArm");
        assert_eq!(folded, ["C_Spine2", "L_Deltoid"]);
        assert_eq!(source.bone_names.len(), 5);
    }

    #[test]
    fn additive_source_is_rejected_before_absolute_pose_conversion() {
        let animation = DecodedStarfieldAnimation {
            title: "Fire".into(),
            header: AfHeader {
                magic: 0,
                rotation: [0., 0., 0., 1.],
                translation: [0.; 3],
                flags: [0; 4],
                version: 5,
                bone_count: 1,
                frame_count: 1,
                index_atlas_count: 0,
                fill_count: 0,
                preamble_offset: 0,
            },
            last_key_frame: 1,
            bones: vec![StarfieldBone {
                name: "Root".into(),
                parent: -1,
                rotation: [0., 0., 0., 1.],
                translation: [0.; 3],
            }],
            tracks: vec![],
            weapon_suffix_node_hashes: vec![],
        };
        let source = parse_afx("<root><is_state>0</is_state><tag>Fire</tag><filename>Fire.af</filename><rampout>0</rampout><flags><flag>Additive</flag></flags></root>",1.).unwrap();
        let target = SkeletonRecord {
            name: "test".into(),
            bone_count: 0,
            bone_names: vec![],
            parent_indices: vec![],
            reference_pose: vec![],
            lock_translation: vec![],
            float_count: 0,
            float_slots: vec![],
            reference_floats: vec![],
            partition_names: vec![],
        };
        assert!(
            retarget_absolute(&animation, &source, &target, 1.)
                .unwrap_err()
                .contains("additive")
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires extracted target FO4 pistol clips and graph"]
    fn real_target_pistol_binding_modes_identify_separate_additive_layer() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let meshes = root.join("extracted/fo4/meshes/actors/character/_1stperson");
        for (name, additive) in [
            ("WPNFireSingleReady.hkx", false),
            ("WPNAfterJiggleFireSingleAdd.hkx", true),
        ] {
            let bytes = std::fs::read(meshes.join("animations/pistol").join(name)).unwrap();
            let xml = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
            let clip = havok_native::animation::clip::extract_clip(&xml, None).unwrap();
            println!(
                "Target {name}: additive={}, {} tracks",
                clip.is_additive,
                clip.channels.len()
            );
            assert_eq!(clip.is_additive, additive);
        }
        let xml = havok_native::api::havok_hkx_to_xml(
            &std::fs::read(meshes.join("behaviors/GunBehavior.hkx")).unwrap(),
        )
        .unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        std::fs::write(
            std::env::temp_dir().join("b21_sf_target_gunbehavior.xml"),
            &xml,
        )
        .unwrap();
        for clip in doc
            .descendants()
            .filter(|node| node.attribute("class") == Some("hkbClipGenerator"))
        {
            let animation = clip
                .children()
                .find(|node| node.attribute("name") == Some("animationName"))
                .and_then(|node| node.text())
                .unwrap_or("");
            if !animation.contains("WPNFireSingleReady.")
                && !animation.contains("WPNAfterJiggleFireSingleAdd.")
            {
                continue;
            }
            let id = clip.attribute("name").unwrap();
            let parents = doc
                .descendants()
                .filter(|node| {
                    node.attribute("name") == Some("generator") && node.text() == Some(id)
                })
                .filter_map(|node| node.parent())
                .map(|node| {
                    format!(
                        "{} {} weight={}",
                        node.attribute("class").unwrap_or(""),
                        node.attribute("name").unwrap_or(""),
                        node.children()
                            .find(|child| child.attribute("name") == Some("weight"))
                            .and_then(|child| child.text())
                            .unwrap_or("")
                    )
                })
                .collect::<Vec<_>>();
            println!("Target graph {animation}: {parents:?}");
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires source CALUMI DLL and extracted source/target skeletons"]
    fn real_reload_retargets_and_packs_with_source_transfer_event_in_both_perspectives() {
        use super::super::starfield_decoder::CalumiDecoder;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let decoder =
            CalumiDecoder::open(&root.join("refs/sf_animation_io/CALUMI.Animation.dll")).unwrap();
        for perspective in ["", "_1stperson/"] {
            let meshes = root.join("extracted/starfield/Meshes");
            let rig = meshes.join(format!(
                "actors/human/{perspective}characterassets/skeleton.rig"
            ));
            let af = meshes.join(format!(
                "actors/human/{perspective}animations/gun/grippistol/stinger/reload.af"
            ));
            let animation = decoder.decode(&rig, &af).unwrap();
            let chest = animation
                .tracks
                .iter()
                .find(|track| track.bone_name == "C_Chest")
                .unwrap();
            let bone = &animation.bones[chest.bone_index];
            assert!(bone.translation[0].abs() > 0.01);
            let channel = rest_relative_channel(bone, chest);
            if !perspective.is_empty() {
                assert!(
                    chest.translations[0]
                        .value
                        .iter()
                        .all(|value| value.abs() < 1e-7)
                );
                assert!(
                    (channel.translations[0].value[0] - bone.translation[0] * METERS_TO_FO4).abs()
                        < 1e-5
                );
            } else {
                assert!(channel.translations.is_empty());
            }
            let afx = parse_afx(
                &std::fs::read_to_string(af.with_extension("afx")).unwrap(),
                f64::from(animation.header.frame_count),
            )
            .unwrap();
            let skeleton_path = root.join(format!(
                "extracted/fo4/meshes/actors/character/{perspective}characterassets/skeleton.hkx"
            ));
            let xml = havok_native::api::havok_hkx_to_xml(&std::fs::read(&skeleton_path).unwrap())
                .unwrap();
            let target = havok_native::animation::parsers::parse_skeleton_xml(&xml).unwrap();
            let (clip, receipt) = retarget_absolute(&animation, &afx, &target, 1.).unwrap();
            assert!((clip.duration - 101. / 30.).abs() < 1e-6);
            assert!(receipt.target_tracks > 40 && receipt.target_tracks < target.bone_names.len());
            let declaration = super::super::bind_fnv_kf_clip(
                &target.bone_names,
                &clip,
                &skeleton_path.to_string_lossy(),
                "SourceReload.hkx",
                &target.name,
                &[],
            )
            .unwrap();
            let (xml, _) = super::super::clip_to_havok_xml_with_float_tracks(
                &clip,
                &declaration,
                &[],
                &target.name,
                30.,
                super::super::FnvExtractedMotionPolicy::RejectNonzero,
            )
            .unwrap();
            let bytes = havok_native::api::havok_xml_to_hkx(&xml).unwrap();
            super::super::validate_staged_fo4_animation(&bytes).unwrap();
            let reloaded = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
            let reloaded =
                havok_native::animation::clip::extract_clip(&reloaded, Some(&target)).unwrap();
            assert!(reloaded.events.iter().any(
                |event| event.text == "reloadComplete" && (event.time - 50. / 30.).abs() < 1e-5
            ));
            assert!(!receipt.functionality_certified);
            println!(
                "Starfield {perspective}reload: {} source tracks→{} target, folded{:?}, omitted{:?}, unresolved{}",
                receipt.source_tracks,
                receipt.target_tracks,
                receipt.folded_source_bones,
                receipt.omitted_source_channels,
                receipt.annotation_plan.unresolved.len()
            );
        }
    }
}
