use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use havok_native::animation::clip as hk;
use havok_native::animation::parsers::SkeletonRecord;
use havok_native::animation::pose::{PoseSkeleton, QsTransform};
use serde::{Deserialize, Serialize};

use super::weapon_retarget::{
    retarget_weapon_clip, sample_channel, sample_pose, target_reference_pose,
};
use super::{AnimationClip, AnimationEvent, AnimationKeyframe, BoneChannel, Interpolation};

#[path = "skyrim_rider_combat.rs"]
mod combat;
#[path = "skyrim_rider_graph.rs"]
mod graph;
#[path = "skyrim_rider_records.rs"]
mod records;
#[path = "skyrim_rider_tree.rs"]
mod tree;

pub(crate) use records::{RiderRegistration, prepare_registration};
pub(crate) use tree::{RiderTreeContext, stage_graph_with_trees};

pub(crate) const RIDER_GRAPH: &str = "Actors/Character/Behaviors/B21SkyrimRider.hkx";
pub(crate) const RIDER_ANIMATIONS: &str = "Actors/Character/Animations/B21SkyrimRider";
const SOURCE_SKELETON: &str = "actors/character/character assets/skeleton.hkx";
const TARGET_SKELETON: &str = "actors/character/characterassets/skeleton.hkx";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RiderClipRequest {
    pub name: String,
    pub source_path: String,
    pub looping: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RiderPairedOrigin {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub heading_radians: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RiderClipReceipt {
    pub name: String,
    pub source_path: String,
    pub runtime_path: String,
    pub duration: f64,
    pub looping: bool,
    pub source_tracks: usize,
    pub target_tracks: usize,
    pub paired: bool,
    pub paired_origin: Option<RiderPairedOrigin>,
    pub source_events: Vec<String>,
    pub source_float_tracks: Vec<super::skyrim_weapons::SkyrimFloatTrack>,
}

pub(crate) struct StagedRiderClips {
    pub assets: Vec<(String, Vec<u8>)>,
    pub receipts: Vec<RiderClipReceipt>,
    pub declarations: Vec<crate::source_rig::ClipDecl>,
}

pub(crate) fn stage_horse_rider(
    source_meshes: &Path,
    target_meshes: &Path,
    catalog: &crate::skyrimse_fo4_runtime::creature_motion::SkyrimCreatureMotionCatalog,
    horse_manifest: &crate::source_rig::CreatureManifest,
    horse_graph: &crate::source_rig::CapabilityGraphManifest,
) -> Result<StagedRiderClips, String> {
    let combat = combat::prepare(source_meshes, target_meshes)?;
    let selected = catalog
        .horse_rider_generators(horse_graph)
        .map_err(|error| error.to_string())?;
    let mut indexed = BTreeMap::new();
    for (index, node) in selected {
        if index < 0 || index > 1024 || indexed.insert(index, node).is_some() {
            return Err("invalid horse rider state index".into());
        }
    }
    let max = *indexed
        .keys()
        .next_back()
        .ok_or("horse graph has no rider states")?;
    // Native selectors need dense slots; unsupported horse roles never publish these indices.
    let unused_slot = indexed.values().next().unwrap().clone();
    let mut states = Vec::new();
    for index in 0..=max {
        let node = indexed
            .get(&index)
            .or_else(|| {
                (index % 2 == 1)
                    .then(|| indexed.get(&(index - 1)))
                    .flatten()
            })
            .unwrap_or(&unused_slot);
        states.push(node.clone());
    }
    let mut requests = Vec::new();
    let mut seen = HashSet::new();
    for name in states.iter().flat_map(|node| node.clip_names()) {
        if !seen.insert(name.to_string()) {
            continue;
        }
        let clip = catalog
            .clip(name)
            .ok_or_else(|| format!("horse rider clip {name} absent from source catalog"))?;
        let leaf = clip
            .clip_path
            .replace('\\', "/")
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        let declaration = horse_manifest
            .clips
            .iter()
            .find(|clip| clip.name == name)
            .ok_or_else(|| format!("horse rider clip {name} has no binding"))?;
        requests.push(RiderClipRequest {
            name: name.into(),
            source_path: format!("actors/character/animations/horse_rider/{leaf}"),
            looping: declaration.looping,
        });
    }
    requests.extend([
        RiderClipRequest {
            name: "Mount".into(),
            source_path: "actors/sharedkillmoves/human&horse/paired_mount.hkx".into(),
            looping: false,
        },
        RiderClipRequest {
            name: "Dismount".into(),
            source_path: "actors/sharedkillmoves/human&horse/paired_dismount.hkx".into(),
            looping: false,
        },
    ]);
    requests.extend(combat.requests.iter().cloned());
    let mut staged = stage_rider_clips(source_meshes, target_meshes, &requests)?;
    let mut rider_manifest = horse_manifest.clone();
    rider_manifest.clips = staged.declarations.clone();
    rider_manifest.paths.project.clear();
    for clip in &mut rider_manifest.clips {
        clip.path = rider_animation_name(&clip.path)?;
    }
    let mut declarations = horse_manifest.core.clone();
    for event in &horse_graph.explicit_events {
        if !declarations
            .events
            .iter()
            .any(|existing| existing.name == event.name)
        {
            declarations.events.push(event.clone());
        }
    }
    let donor =
        std::fs::read(target_meshes.join("actors/character/behaviors/furniturebehavior.hkx"))
            .map_err(|e| e.to_string())?;
    let graph = stage_graph_with_trees(
        &donor,
        &staged.receipts,
        &RiderTreeContext {
            manifest: &rider_manifest,
            declarations: &declarations,
            states: &states,
        },
    )?;
    let graph = combat::graft(&graph, &combat, &staged.receipts)?;
    staged.assets.push((RIDER_GRAPH.into(), graph));
    Ok(staged)
}

pub(super) fn rider_animation_name(runtime_path: &str) -> Result<String, String> {
    let normalized = runtime_path.replace('\\', "/");
    normalized
        .strip_prefix("Actors/Character/")
        .map(|relative| relative.replace('/', "\\"))
        .ok_or_else(|| {
            format!("rider animation is outside the native character project: {runtime_path}")
        })
}

pub(crate) fn stage_rider_clips(
    source_meshes: &Path,
    target_meshes: &Path,
    requests: &[RiderClipRequest],
) -> Result<StagedRiderClips, String> {
    let source_record = read_skeleton(&source_meshes.join(SOURCE_SKELETON))?;
    let target_record = read_skeleton(&target_meshes.join(TARGET_SKELETON))?;
    let source = target_reference_pose(&source_record)?;
    let target = target_reference_pose(&target_record)?;
    let map = rider_bone_map(&source, &target)?;
    let mut staged = StagedRiderClips {
        assets: Vec::new(),
        receipts: Vec::new(),
        declarations: Vec::new(),
    };
    let mut names = HashSet::new();
    for request in requests {
        if request.name.is_empty()
            || request
                .name
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && b != b'_')
            || !names.insert(request.name.to_ascii_lowercase())
        {
            return Err(format!(
                "invalid or duplicate rider clip name {:?}",
                request.name
            ));
        }
        let relative = request.source_path.replace('\\', "/");
        if relative.starts_with('/')
            || relative.contains(':')
            || relative.split('/').any(|p| p == "..")
        {
            return Err(format!("invalid source rider path {relative}"));
        }
        let bytes =
            std::fs::read(source_meshes.join(&relative)).map_err(|e| format!("{relative}: {e}"))?;
        let xml = havok_native::api::havok_hkx_to_xml(&bytes).map_err(|e| e.to_string())?;
        let (clip, paired_origin) = decode_rider_clip(&xml, &source_record, request)?;
        let source_float_tracks = super::skyrim_weapons::decode_float_tracks(&xml, &source_record)?;
        for track in &source_float_tracks {
            let supported = match track.slot_name.as_str() {
                "hkVis:Shield" | "hkVis:Weapon" => track.values.iter().all(|value| *value == 1.0),
                "hkVis:NPC L MagicNode [LMag]" | "hkVis:NPC R MagicNode [RMag]" => {
                    let first = track.values[0];
                    matches!(first, 0.0 | 1.0) && track.values.iter().all(|value| *value == first)
                }
                _ => false,
            };
            if !supported {
                return Err(format!(
                    "rider clip {} requires an unbound float consumer for {}",
                    request.name, track.slot_name
                ));
            }
        }
        let source_tracks = clip.channels.len();
        let source_events = clip.events.iter().map(|event| event.text.clone()).collect();
        let sample_rate = native_sample_rate(&clip)?;
        let (mut converted, omitted) =
            retarget_weapon_clip(&clip, &source, &target, &map, sample_rate, 1.0)?;
        if !omitted.is_empty() {
            return Err(format!(
                "rider clip {} contains unknown skeleton tracks: {omitted:?}",
                request.name
            ));
        }
        align_pelvis(&clip, &mut converted, &source, &target, sample_rate)?;
        preserve_weapon_attachments(
            &clip,
            &mut converted,
            &source,
            &target,
            request,
            sample_rate,
        )?;
        combat::adapt_clip_events(&mut converted, request)?;
        let path = format!("{RIDER_ANIMATIONS}/{}.hkx", request.name);
        let declaration = super::bind_fnv_kf_clip(
            &target.bone_names,
            &converted,
            TARGET_SKELETON,
            &path,
            &target_record.name,
            &[],
        )
        .map_err(|e| e.to_string())?;
        let (xml, _) = super::clip_to_havok_xml_with_float_tracks(
            &converted,
            &declaration,
            &[],
            &target_record.name,
            sample_rate,
            super::FnvExtractedMotionPolicy::RejectNonzero,
        )
        .map_err(|e| e.to_string())?;
        let packed = havok_native::api::havok_xml_to_hkx(&xml).map_err(|e| e.to_string())?;
        super::validate_staged_fo4_animation(&packed).map_err(|e| e.to_string())?;
        validate_rider_roundtrip(&packed, &converted, &target_record)?;
        staged.receipts.push(RiderClipReceipt {
            name: request.name.clone(),
            source_path: relative,
            runtime_path: path.clone(),
            duration: converted.duration,
            looping: request.looping,
            source_tracks,
            target_tracks: converted.channels.len(),
            paired: paired_origin.is_some(),
            paired_origin,
            source_events,
            source_float_tracks,
        });
        staged.assets.push((path, packed));
        staged.declarations.push(declaration);
    }
    Ok(staged)
}

pub(crate) fn stage_paired_horse_clip(
    source_bytes: &[u8],
    source_skeleton_bytes: &[u8],
    runtime_skeleton_path: &str,
    runtime_clip_path: &str,
    name: &str,
) -> Result<(Vec<u8>, crate::source_rig::ClipDecl), String> {
    let skeleton_xml = havok_native::api::havok_hkx_to_xml(source_skeleton_bytes)
        .map_err(|error| error.to_string())?;
    let skeleton = havok_native::animation::parsers::parse_skeleton_xml(&skeleton_xml)
        .map_err(|error| error.to_string())?;
    let source = target_reference_pose(&skeleton)?;
    let xml =
        havok_native::api::havok_hkx_to_xml(source_bytes).map_err(|error| error.to_string())?;
    let decoded = hk::extract_clip(&xml, None).map_err(|error| error.to_string())?;
    if decoded.original_skeleton_name.as_deref() != Some("PairedRoot")
        || decoded.is_additive
        || !decoded.warnings.is_empty()
    {
        return Err("horse interaction requires an exact non-additive paired source clip".into());
    }
    let doc = roxmltree::Document::parse(&xml).map_err(|error| error.to_string())?;
    let names = doc
        .descendants()
        .find(|node| node.attribute("name") == Some("annotationTracks"))
        .ok_or("paired horse clip has no named tracks")?
        .children()
        .filter(roxmltree::Node::is_element)
        .map(|node| {
            node.children()
                .find(|child| child.attribute("name") == Some("trackName"))
                .and_then(|child| child.text())
                .unwrap_or("")
        })
        .collect::<Vec<_>>();
    if names.len() != decoded.channels.len() {
        return Err("paired horse track names do not match decoded channel count".into());
    }
    let wrapper_index = names
        .iter()
        .position(|name| *name == "2_Horse")
        .ok_or("paired clip has no horse wrapper")?;
    let wrapper = &decoded.channels[wrapper_index];
    for time in wrapper
        .translations
        .iter()
        .map(|key| key.time)
        .chain(wrapper.rotations.iter().map(|key| key.time))
        .chain(wrapper.scales.iter().map(|key| key.time))
    {
        let transform = sample_hk_channel(wrapper, time as f64);
        if transform.translation.iter().any(|v| v.abs() > 1e-4)
            || transform
                .rotation
                .iter()
                .zip([0.0, 0.0, 0.0, 1.0])
                .any(|(v, expected)| (*v - expected).abs() > 1e-4)
            || transform.scale.iter().any(|v| (*v - 1.0).abs() > 1e-4)
        {
            return Err("paired horse wrapper changes native mount placement".into());
        }
    }
    let horse_tracks = names
        .iter()
        .zip(&decoded.channels)
        .filter_map(|(name, channel)| {
            let bone = if *name == "2_" {
                "NPC Root [Root]"
            } else if *name == "SaddleBone" {
                "SaddleBone"
            } else {
                name.strip_prefix("2_")?
            };
            source
                .bone_names
                .iter()
                .any(|name| name == bone)
                .then_some((bone, channel))
        })
        .collect::<HashMap<_, _>>();
    for required in ["NPC Root [Root]", "Horse_COM", "HorsePelvis", "SaddleBone"] {
        if !horse_tracks.contains_key(required) {
            return Err(format!("paired horse has no {required} track"));
        }
    }
    let channels = source
        .bone_names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            if let Some(channel) = horse_tracks.get(name.as_str()) {
                BoneChannel {
                    bone_name: name.clone(),
                    translations: channel
                        .translations
                        .iter()
                        .map(|key| convert_key(key.time, &key.value))
                        .collect(),
                    rotations: channel
                        .rotations
                        .iter()
                        .map(|key| convert_key(key.time, &key.value))
                        .collect(),
                    scales: channel
                        .scales
                        .iter()
                        .map(|key| convert_key(key.time, &key.value))
                        .collect(),
                }
            } else {
                let reference = &source.reference_local[index];
                BoneChannel {
                    bone_name: name.clone(),
                    translations: vec![convert_key(0.0, &reference.translation)],
                    rotations: vec![convert_key(0.0, &reference.rotation)],
                    scales: vec![convert_key(0.0, &reference.scale)],
                }
            }
        })
        .collect();
    let clip = AnimationClip {
        name: name.into(),
        source_stop_time: decoded.duration as f64,
        duration: decoded.duration as f64,
        cycle_type: "clamp".into(),
        frequency: 1.0,
        channels,
        events: decoded
            .events
            .into_iter()
            .filter_map(|event| {
                event.text.strip_prefix("2_").map(|text| AnimationEvent {
                    time: event.time as f64,
                    source_time: event.time as f64,
                    text: text.into(),
                })
            })
            .collect(),
        ..Default::default()
    };
    let sample_rate = native_sample_rate(&clip)?;
    let declaration = super::bind_fnv_kf_clip(
        &source.bone_names,
        &clip,
        runtime_skeleton_path,
        runtime_clip_path,
        &skeleton.name,
        &[],
    )
    .map_err(|error| error.to_string())?;
    let (xml, _) = super::clip_to_havok_xml_with_float_tracks(
        &clip,
        &declaration,
        &[],
        &skeleton.name,
        sample_rate,
        super::FnvExtractedMotionPolicy::RejectNonzero,
    )
    .map_err(|error| error.to_string())?;
    let bytes = havok_native::api::havok_xml_to_hkx(&xml).map_err(|error| error.to_string())?;
    super::validate_staged_fo4_animation(&bytes).map_err(|error| error.to_string())?;
    validate_rider_roundtrip(&bytes, &clip, &skeleton)?;
    Ok((bytes, declaration))
}

pub(super) fn read_skeleton(path: &Path) -> Result<SkeletonRecord, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let xml = havok_native::api::havok_hkx_to_xml(&bytes).map_err(|e| e.to_string())?;
    havok_native::animation::parsers::parse_skeleton_xml(&xml).map_err(|e| e.to_string())
}

pub(super) fn rider_bone_map(
    source: &PoseSkeleton,
    target: &PoseSkeleton,
) -> Result<HashMap<String, String>, String> {
    let mut result = super::weapon_retarget::skyrim_actor_bone_map(source, target)?;
    let mut targets: HashSet<_> = result
        .values()
        .map(|name| name.to_ascii_lowercase())
        .collect();
    if source.bone_names.iter().any(|name| name == "Shield")
        && target.bone_names.iter().any(|name| name == "WeaponLeft")
    {
        result.insert("Shield".into(), "WeaponLeft".into());
        targets.insert("weaponleft".into());
    }
    for required in [
        "Root",
        "COM",
        "Pelvis",
        "LLeg_Thigh",
        "RLeg_Thigh",
        "Spine1",
        "Spine2",
        "Chest",
        "LArm_Hand",
        "RArm_Hand",
    ] {
        if !targets.contains(&required.to_ascii_lowercase()) {
            return Err(format!("rider skeleton cannot bind {required}"));
        }
    }
    Ok(result)
}

fn decode_rider_clip(
    xml: &str,
    skeleton: &SkeletonRecord,
    request: &RiderClipRequest,
) -> Result<(AnimationClip, Option<RiderPairedOrigin>), String> {
    let decoded = hk::extract_clip(xml, Some(skeleton)).map_err(|e| e.to_string())?;
    if decoded.is_additive || !decoded.warnings.is_empty() || decoded.channels.is_empty() {
        return Err(format!(
            "unsupported rider animation {}: {:?}",
            request.source_path, decoded.warnings
        ));
    }
    let paired = decoded.original_skeleton_name.as_deref() == Some("PairedRoot");
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let names = if paired {
        doc.descendants()
            .find(|node| node.attribute("name") == Some("annotationTracks"))
            .ok_or("paired rider animation has no named tracks")?
            .children()
            .filter(roxmltree::Node::is_element)
            .map(|track| {
                track
                    .children()
                    .find(|node| node.attribute("name") == Some("trackName"))
                    .and_then(|node| node.text())
                    .unwrap_or("")
                    .to_string()
            })
            .collect::<Vec<_>>()
    } else if decoded.track_to_bone_indices.is_empty() {
        skeleton
            .bone_names
            .iter()
            .take(decoded.channels.len())
            .cloned()
            .collect()
    } else {
        decoded
            .track_to_bone_indices
            .iter()
            .map(|index| {
                skeleton
                    .bone_names
                    .get(*index as usize)
                    .cloned()
                    .ok_or("rider track index is outside source skeleton")
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    if names.len() != decoded.channels.len() {
        return Err("rider track-name/binding count mismatch".into());
    }
    let channels = decoded
        .channels
        .iter()
        .zip(&names)
        .filter(|(_, name)| skeleton.bone_names.contains(name))
        .map(|(channel, name)| BoneChannel {
            bone_name: name.clone(),
            translations: channel
                .translations
                .iter()
                .map(|key| convert_key(key.time, &key.value))
                .collect(),
            rotations: channel
                .rotations
                .iter()
                .map(|key| convert_key(key.time, &key.value))
                .collect(),
            scales: channel
                .scales
                .iter()
                .map(|key| convert_key(key.time, &key.value))
                .collect(),
        })
        .collect::<Vec<_>>();
    let paired_origin = if paired {
        let wrapper = names
            .iter()
            .position(|name| name == "NPC")
            .ok_or("paired rider animation has no NPC wrapper")?;
        channels
            .iter()
            .find(|channel| channel.bone_name == "NPC Root [Root]")
            .ok_or("paired rider animation has no humanoid root")?;
        let wrapper = &decoded.channels[wrapper];
        let origin = sample_hk_channel(wrapper, 0.0);
        for time in wrapper
            .translations
            .iter()
            .map(|key| key.time)
            .chain(wrapper.rotations.iter().map(|key| key.time))
            .chain(wrapper.scales.iter().map(|key| key.time))
        {
            let sample = sample_hk_channel(wrapper, time as f64);
            if sample
                .translation
                .iter()
                .zip(origin.translation)
                .any(|(a, b)| (*a - b).abs() > 1e-4)
                || sample
                    .rotation
                    .iter()
                    .zip(origin.rotation)
                    .any(|(a, b)| (*a - b).abs() > 1e-4)
                || sample.scale.iter().any(|value| (*value - 1.0).abs() > 1e-4)
            {
                return Err(
                    "paired rider wrapper must be a static unscaled placement transform".into(),
                );
            }
        }
        let [x, y, z, w] = origin.rotation;
        Some(RiderPairedOrigin {
            translation: origin.translation,
            rotation: origin.rotation,
            heading_radians: -(2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z)),
        })
    } else {
        None
    };
    let events = decoded
        .events
        .into_iter()
        .filter(|event| !paired || !event.text.starts_with("2_"))
        .map(|event| AnimationEvent {
            time: event.time as f64,
            source_time: event.time as f64,
            text: event.text,
        })
        .collect();
    Ok((
        AnimationClip {
            name: request.name.clone(),
            source_stop_time: decoded.duration as f64,
            duration: decoded.duration as f64,
            cycle_type: if request.looping { "loop" } else { "clamp" }.into(),
            frequency: 1.0,
            channels,
            events,
            ..Default::default()
        },
        paired_origin,
    ))
}

fn convert_key(time: f32, values: &[f32]) -> AnimationKeyframe {
    AnimationKeyframe {
        time: time as f64,
        value: values.iter().map(|v| *v as f64).collect(),
        interpolation: Interpolation::Linear,
    }
}

fn native_sample_rate(clip: &AnimationClip) -> Result<f64, String> {
    let step = clip
        .channels
        .iter()
        .flat_map(|channel| [&channel.translations, &channel.rotations, &channel.scales])
        .flat_map(|keys| keys.windows(2))
        .map(|pair| pair[1].time - pair[0].time)
        .filter(|step| *step > 1e-7)
        .min_by(f64::total_cmp);
    let rate = step.map_or(30.0, |step| 1.0 / step);
    if !rate.is_finite() || rate > 1000.0 {
        return Err("invalid rider sample spacing".into());
    }
    Ok(rate)
}

fn sample_hk_channel(channel: &hk::BoneChannel, time: f64) -> QsTransform {
    sample_channel(
        &BoneChannel {
            bone_name: channel.bone_name.clone(),
            translations: channel
                .translations
                .iter()
                .map(|key| convert_key(key.time, &key.value))
                .collect(),
            rotations: channel
                .rotations
                .iter()
                .map(|key| convert_key(key.time, &key.value))
                .collect(),
            scales: channel
                .scales
                .iter()
                .map(|key| convert_key(key.time, &key.value))
                .collect(),
        },
        &QsTransform::IDENTITY,
        time,
    )
}

fn align_pelvis(
    source_clip: &AnimationClip,
    target_clip: &mut AnimationClip,
    source: &PoseSkeleton,
    target: &PoseSkeleton,
    sample_rate: f64,
) -> Result<(), String> {
    let source_pelvis = source
        .bone_names
        .iter()
        .position(|bone| bone == "NPC Pelvis [Pelv]")
        .ok_or("source rider has no pelvis")?;
    let target_pelvis = target
        .bone_names
        .iter()
        .position(|bone| bone == "Pelvis")
        .ok_or("target rider has no pelvis")?;
    let root = target_clip
        .channels
        .iter()
        .position(|channel| channel.bone_name == "Root")
        .ok_or("target rider has no root track")?;
    let samples = super::target_sample_count(target_clip.duration, sample_rate);
    for frame in 0..samples {
        let time = if samples > 1 {
            target_clip.duration * frame as f64 / (samples - 1) as f64
        } else {
            0.0
        };
        let source_position = sample_pose(source_clip, source, time)
            .model_at(source_pelvis)
            .translation;
        let target_position = sample_pose(target_clip, target, time)
            .model_at(target_pelvis)
            .translation;
        let key = &mut target_clip.channels[root].translations[frame];
        for axis in 0..3 {
            key.value[axis] += (source_position[axis] - target_position[axis]) as f64;
        }
    }
    Ok(())
}

fn preserve_weapon_attachments(
    source_clip: &AnimationClip,
    target_clip: &mut AnimationClip,
    source: &PoseSkeleton,
    target: &PoseSkeleton,
    request: &RiderClipRequest,
    sample_rate: f64,
) -> Result<(), String> {
    let path = request.source_path.replace('\\', "/").to_ascii_lowercase();
    let bow = path.contains("/mc_bow_");
    if !bow && !path.contains("/mc_crossbow_") {
        return Ok(());
    }
    let bindings = [
        ("Shield", "WeaponLeft", "NPC L Hand [LHnd]", "LArm_Hand"),
        ("Weapon", "Weapon", "NPC R Hand [RHnd]", "RArm_Hand"),
    ];
    super::weapon_retarget::preserve_weapon_attachments(
        source_clip,
        target_clip,
        source,
        target,
        if bow { &bindings[..] } else { &bindings[1..] },
        sample_rate,
    )
}

fn validate_rider_roundtrip(
    bytes: &[u8],
    clip: &AnimationClip,
    skeleton: &SkeletonRecord,
) -> Result<(), String> {
    let xml = havok_native::api::havok_hkx_to_xml(bytes).map_err(|e| e.to_string())?;
    let loaded = hk::extract_clip(&xml, Some(skeleton)).map_err(|e| e.to_string())?;
    if (loaded.duration as f64 - clip.duration).abs() > 1e-4
        || loaded.channels.len() != clip.channels.len()
    {
        return Err("rider duration or track count changed during packing".into());
    }
    for (index, channel) in loaded.channels.iter().enumerate() {
        let bone = *loaded
            .track_to_bone_indices
            .get(index)
            .ok_or("rider binding missing after packing")? as usize;
        if skeleton.bone_names.get(bone) != Some(&clip.channels[index].bone_name) {
            return Err("rider binding changed during packing".into());
        }
        if channel
            .translations
            .iter()
            .flat_map(|key| key.value)
            .chain(channel.rotations.iter().flat_map(|key| key.value))
            .any(|v| !v.is_finite())
        {
            return Err("nonfinite rider frame after packing".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use havok_native::animation::pose::quat_rotate;

    #[test]
    #[ignore = "requires extracted Skyrim and Fallout 4 animation corpus"]
    fn mounted_projectile_weapons_keep_source_grip_axes_and_arrow_attachment() {
        let root = std::env::var("MODKIT_REPO_ROOT").expect("MODKIT_REPO_ROOT");
        let source_root = Path::new(&root).join("extracted/skyrimse/meshes");
        let target_root = Path::new(&root).join("extracted/fo4/meshes");
        for (name, path, bow) in [
            (
                "MountedCombatBowIdle",
                "actors/character/animations/horse_rider/patch/mc_bow_idledrawn.hkx",
                true,
            ),
            (
                "MountedCombatCrossbowIdle",
                "actors/character/animations/horse_rider/dlc01/mc_crossbow_idledrawn.hkx",
                false,
            ),
        ] {
            let request = RiderClipRequest {
                name: name.into(),
                source_path: path.into(),
                looping: true,
            };
            let source_skeleton = read_skeleton(&source_root.join(SOURCE_SKELETON)).unwrap();
            let target_skeleton = read_skeleton(&target_root.join(TARGET_SKELETON)).unwrap();
            let source_xml = havok_native::api::havok_hkx_to_xml(
                &std::fs::read(source_root.join(&request.source_path)).unwrap(),
            )
            .unwrap();
            let (source_clip, _) =
                decode_rider_clip(&source_xml, &source_skeleton, &request).unwrap();
            let staged =
                stage_rider_clips(&source_root, &target_root, std::slice::from_ref(&request))
                    .unwrap();
            let target_xml = havok_native::api::havok_hkx_to_xml(&staged.assets[0].1).unwrap();
            let (target_clip, _) =
                decode_rider_clip(&target_xml, &target_skeleton, &request).unwrap();
            let source = target_reference_pose(&source_skeleton).unwrap();
            let target = target_reference_pose(&target_skeleton).unwrap();
            for time in [0.0, source_clip.duration] {
                let source_pose = sample_pose(&source_clip, &source, time).all_model();
                let target_pose = sample_pose(&target_clip, &target, time).all_model();
                for (source_bone, target_bone, source_hand, target_hand, vertical) in [
                    (
                        "Shield",
                        "WeaponLeft",
                        "NPC L Hand [LHnd]",
                        "LArm_Hand",
                        true,
                    ),
                    ("Weapon", "Weapon", "NPC R Hand [RHnd]", "RArm_Hand", false),
                ] {
                    if vertical && !bow {
                        continue;
                    }
                    let source_bone = &source_pose[source
                        .bone_names
                        .iter()
                        .position(|name| name == source_bone)
                        .unwrap()];
                    let target_bone = &target_pose[target
                        .bone_names
                        .iter()
                        .position(|name| name == target_bone)
                        .unwrap()];
                    let source_hand = &source_pose[source
                        .bone_names
                        .iter()
                        .position(|name| name == source_hand)
                        .unwrap()];
                    let target_hand = &target_pose[target
                        .bone_names
                        .iter()
                        .position(|name| name == target_hand)
                        .unwrap()];
                    let axis = quat_rotate(&target_bone.rotation, &[0.0, 1.0, 0.0]);
                    assert!(if vertical {
                        axis[2] > 0.9
                    } else {
                        axis[1] > 0.9
                    });
                    if !bow {
                        assert!(quat_rotate(&target_bone.rotation, &[0.0, 0.0, 1.0])[2] < -0.9);
                    }
                    let alignment: f32 = source_bone
                        .rotation
                        .iter()
                        .zip(target_bone.rotation)
                        .map(|(a, b)| a * b)
                        .sum();
                    assert!(alignment.abs() > 0.9999);
                    for axis in 0..3 {
                        assert!(
                            ((source_bone.translation[axis] - source_hand.translation[axis])
                                - (target_bone.translation[axis] - target_hand.translation[axis]))
                                .abs()
                                < 0.01
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "requires extracted Skyrim and Fallout 4 animation corpus"]
    fn source_mounted_combat_float_track_census() {
        let root = std::env::var("MODKIT_REPO_ROOT").expect("MODKIT_REPO_ROOT");
        let root = Path::new(&root);
        let source = root.join("extracted/skyrimse/meshes");
        let target = root.join("extracted/fo4/meshes");
        let prepared = combat::prepare(&source, &target).unwrap();
        let skeleton = read_skeleton(&source.join(SOURCE_SKELETON)).unwrap();
        let mut census = Vec::new();
        for request in prepared.requests {
            let bytes = std::fs::read(source.join(&request.source_path)).unwrap();
            let xml = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
            let floats =
                super::super::skyrim_weapons::decode_float_tracks(&xml, &skeleton).unwrap();
            let (clip, _) = decode_rider_clip(&xml, &skeleton, &request).unwrap();
            census.push(serde_json::json!({
                "request": request,
                "float_tracks": floats,
                "events": clip.events.iter().map(|event| serde_json::json!({
                    "time": event.time, "text": event.text,
                })).collect::<Vec<_>>(),
            }));
        }
        std::fs::write(
            root.join("tmp/rider_source_float_census.json"),
            serde_json::to_vec_pretty(&census).unwrap(),
        )
        .unwrap();
        assert!(!census.is_empty());
    }

    #[test]
    #[ignore = "requires extracted Skyrim and Fallout 4 animation corpus"]
    fn source_skyrim_rider_clips_pack_and_keep_binding_and_timing() {
        let root = std::env::var("MODKIT_REPO_ROOT").expect("MODKIT_REPO_ROOT");
        let root = Path::new(&root);
        let requests = [
            (
                "Mount",
                "actors/sharedkillmoves/human&horse/paired_mount.hkx",
                false,
            ),
            (
                "Dismount",
                "actors/sharedkillmoves/human&horse/paired_dismount.hkx",
                false,
            ),
            (
                "Idle",
                "actors/character/animations/horse_rider/idle.hkx",
                true,
            ),
            (
                "Walk",
                "actors/character/animations/horse_rider/walkforward.hkx",
                true,
            ),
            (
                "Run",
                "actors/character/animations/horse_rider/runforward.hkx",
                true,
            ),
            (
                "Sprint",
                "actors/character/animations/horse_rider/sprintforward.hkx",
                true,
            ),
            (
                "Jump",
                "actors/character/animations/horse_rider/jumpstart.hkx",
                false,
            ),
            (
                "Swim",
                "actors/character/animations/horse_rider/swimf.hkx",
                true,
            ),
        ]
        .into_iter()
        .map(|(name, path, looping)| RiderClipRequest {
            name: name.into(),
            source_path: path.into(),
            looping,
        })
        .collect::<Vec<_>>();
        let staged = stage_rider_clips(
            &root.join("extracted/skyrimse/meshes"),
            &root.join("extracted/fo4/meshes"),
            &requests,
        )
        .unwrap();
        assert_eq!(staged.assets.len(), requests.len());
        assert!(staged.receipts[..2].iter().all(|receipt| receipt.paired));
        let mount_origin = staged.receipts[0].paired_origin.as_ref().unwrap();
        assert!((mount_origin.translation[0] + 63.479).abs() < 0.001);
        assert!((mount_origin.heading_radians - 1.571773).abs() < 0.001);
        let source_skeleton =
            read_skeleton(&root.join("extracted/skyrimse/meshes").join(SOURCE_SKELETON)).unwrap();
        let bytes = std::fs::read(
            root.join("extracted/skyrimse/meshes")
                .join(&requests[0].source_path),
        )
        .unwrap();
        let xml = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
        let (mount_clip, _) = decode_rider_clip(&xml, &source_skeleton, &requests[0]).unwrap();
        let source_root = mount_clip
            .channels
            .iter()
            .find(|channel| channel.bone_name == "NPC Root [Root]")
            .unwrap();
        assert!(
            source_root.translations[0]
                .value
                .iter()
                .all(|value| value.abs() < 0.001)
        );
        assert!(staged.receipts[2..].iter().all(|receipt| !receipt.paired));
        assert!(
            staged
                .receipts
                .iter()
                .all(|receipt| receipt.target_tracks > 60)
        );
        let horse_skeleton = std::fs::read(
            root.join("extracted/skyrimse/meshes/actors/horse/character assets/skeleton.hkx"),
        )
        .unwrap();
        for request in &requests[..2] {
            let paired = std::fs::read(
                root.join("extracted/skyrimse/meshes")
                    .join(&request.source_path),
            )
            .unwrap();
            let (bytes, declaration) = stage_paired_horse_clip(
                &paired,
                &horse_skeleton,
                "Actors/B21_SkyrimHorse/CharacterAssets/Skeleton.hkx",
                &format!("Actors/B21_SkyrimHorse/Animations/{}.hkx", request.name),
                &request.name,
            )
            .unwrap();
            assert_eq!(declaration.binding.declared_transform_tracks, 53);
            let xml = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
            assert!(!xml.contains("PairedRoot"));
        }
    }
}
