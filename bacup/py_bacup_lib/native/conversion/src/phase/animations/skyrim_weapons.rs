use std::collections::{HashMap, HashSet};

use havok_native::animation::parsers::{SkeletonRecord, parse_skeleton_xml};
use havok_native::animation::spline::decompress_spline_full;
use serde::{Deserialize, Serialize};

use super::weapon_retarget::{retarget_weapon_clip, target_reference_pose};
use super::{AnimationClip, AnimationEvent, AnimationKeyframe, BoneChannel, Interpolation};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum SkyrimWeaponAction {
    BowDrawLight,
    BowDrawHeavy,
    BowIdleDrawn,
    BowRelease,
    BowIdleHeld,
    BowEquip,
    BowUnequip,
    CrossbowRelease,
    CrossbowReload,
    CrossbowIdleHeld,
    CrossbowIdleDrawn,
    CrossbowEquip,
    CrossbowUnequip,
}

impl SkyrimWeaponAction {
    fn is_crossbow(self) -> bool {
        matches!(
            self,
            Self::CrossbowRelease
                | Self::CrossbowReload
                | Self::CrossbowIdleHeld
                | Self::CrossbowIdleDrawn
                | Self::CrossbowEquip
                | Self::CrossbowUnequip
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SkyrimActorClipContract {
    pub source_path: String,
    pub action: SkyrimWeaponAction,
    pub first_person: bool,
    pub source_skeleton_path: String,
    pub target_skeleton_path: String,
    pub output_path: String,
}

pub(crate) fn actor_clip_contract(path: &str) -> Option<SkyrimActorClipContract> {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let normalized = normalized.strip_prefix("meshes/").unwrap_or(&normalized);
    let (first_person, leaf) =
        if let Some(leaf) = normalized.strip_prefix("actors/character/_1stperson/animations/") {
            (true, leaf)
        } else {
            (
                false,
                normalized.strip_prefix("actors/character/animations/")?,
            )
        };
    let action = match leaf {
        "bow_drawlight.hkx" | "sneakbow_drawlight.hkx" => SkyrimWeaponAction::BowDrawLight,
        "bow_drawheavy.hkx" => SkyrimWeaponAction::BowDrawHeavy,
        "bow_idledrawn.hkx" | "sneakbow_idledrawn.hkx" => SkyrimWeaponAction::BowIdleDrawn,
        "bow_release.hkx" | "sneakbow_release.hkx" => SkyrimWeaponAction::BowRelease,
        "bow_idleheld.hkx" => SkyrimWeaponAction::BowIdleHeld,
        "bow_equip.hkx" => SkyrimWeaponAction::BowEquip,
        "bow_unequip.hkx" => SkyrimWeaponAction::BowUnequip,
        "dlc01/crossbow_release.hkx" => SkyrimWeaponAction::CrossbowRelease,
        "dlc01/crossbow_reload.hkx" => SkyrimWeaponAction::CrossbowReload,
        "dlc01/crossbow_idleheld.hkx" => SkyrimWeaponAction::CrossbowIdleHeld,
        "dlc01/crossbow_idledrawn.hkx" | "dlc01/sneakcrossbow_idledrawn.hkx" => {
            SkyrimWeaponAction::CrossbowIdleDrawn
        }
        "dlc01/crossbow_equip.hkx" => SkyrimWeaponAction::CrossbowEquip,
        _ => return None,
    };
    let actor_root = if first_person {
        "Actors/Character/_1stPerson"
    } else {
        "Actors/Character"
    };
    Some(SkyrimActorClipContract {
        source_path: normalized.into(),
        action,
        first_person,
        source_skeleton_path: if first_person {
            "Actors/Character/_1stPerson/CharacterAssets/SkeletonFirst.hkx".into()
        } else {
            "Actors/Character/Character Assets/Skeleton.hkx".into()
        },
        target_skeleton_path: format!("{actor_root}/CharacterAssets/Skeleton.hkx"),
        output_path: format!("{actor_root}/Animations/B21_SkyrimWeapons/{leaf}"),
    })
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct SkyrimFloatTrack {
    pub slot_name: String,
    pub frame_duration: f32,
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkyrimActorConsumer {
    SourceAnnotations,
    Fo4CrossbowGun,
}

#[derive(Debug)]
pub(crate) struct StagedSkyrimActorClip {
    pub contract: SkyrimActorClipContract,
    pub consumer: SkyrimActorConsumer,
    pub bytes: Vec<u8>,
    pub source_float_tracks: Vec<SkyrimFloatTrack>,
    pub unmapped_source_bones: Vec<String>,
    pub weapon_fire_time: Option<f64>,
    pub suppressed_source_events: Vec<AnimationEvent>,
    pub duration: f64,
    pub source_attachment_channels: Vec<BoneChannel>,
}

// Staging is deliberately separate from source-path emission: these indices only
// belong in a generated FO4 actor profile, never in a copied Skyrim character rig.
pub(crate) fn stage_actor_clip(
    contract: SkyrimActorClipContract,
    consumer: SkyrimActorConsumer,
    source_bytes: &[u8],
    source_skeleton_bytes: &[u8],
    target_skeleton_bytes: &[u8],
) -> Result<StagedSkyrimActorClip, String> {
    let source_xml = unpack(source_bytes)?;
    let source_skeleton =
        parse_skeleton_xml(&unpack(source_skeleton_bytes)?).map_err(|error| error.to_string())?;
    let target_skeleton =
        parse_skeleton_xml(&unpack(target_skeleton_bytes)?).map_err(|error| error.to_string())?;
    let source = decode_actor_clip(&source_xml, &source_skeleton)?;
    let source_float_tracks = decode_float_tracks(&source_xml, &source_skeleton)?;
    stage_decoded_actor_clip(
        contract,
        consumer,
        source,
        &source_skeleton,
        &target_skeleton,
        source_float_tracks,
    )
}

fn stage_decoded_actor_clip(
    contract: SkyrimActorClipContract,
    consumer: SkyrimActorConsumer,
    mut source: AnimationClip,
    source_skeleton: &SkeletonRecord,
    target_skeleton: &SkeletonRecord,
    source_float_tracks: Vec<SkyrimFloatTrack>,
) -> Result<StagedSkyrimActorClip, String> {
    let source_attachment_channels = source
        .channels
        .iter()
        .filter(|channel| {
            matches!(
                channel.bone_name.as_str(),
                "Shield" | "Weapon" | "WeaponBow"
            )
        })
        .cloned()
        .collect();
    let (weapon_fire_time, suppressed_source_events) =
        adapt_action_events(&mut source, contract.action, consumer)?;
    let source_pose = target_reference_pose(&source_skeleton)?;
    let target_pose = target_reference_pose(&target_skeleton)?;
    let bow = !contract.action.is_crossbow();
    let bone_map = actor_bone_map(&source_pose, &target_pose, bow)?;
    let target_names: HashSet<_> = target_pose
        .bone_names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect();
    let mapping: HashMap<_, _> = bone_map
        .iter()
        .map(|(source, target)| (source.to_ascii_lowercase(), target.to_ascii_lowercase()))
        .collect();
    let unmapped_source_bones = source_pose
        .bone_names
        .iter()
        .filter(|name| {
            let normalized = name.to_ascii_lowercase();
            !target_names.contains(mapping.get(&normalized).unwrap_or(&normalized))
        })
        .cloned()
        .collect();
    let (mut clip, omitted) =
        retarget_weapon_clip(&source, &source_pose, &target_pose, &bone_map, 30.0, 1.0)?;
    if !omitted.is_empty() {
        return Err(format!("unbound Skyrim actor channels: {omitted:?}"));
    }
    let bindings = [
        ("Shield", "WeaponLeft", "NPC L Hand [LHnd]", "LArm_Hand"),
        ("Weapon", "Weapon", "NPC R Hand [RHnd]", "RArm_Hand"),
    ];
    super::weapon_retarget::preserve_weapon_attachments(
        &source,
        &mut clip,
        &source_pose,
        &target_pose,
        if bow { &bindings[..] } else { &bindings[1..] },
        30.0,
    )?;
    let declaration = super::bind_fnv_kf_clip(
        &target_pose.bone_names,
        &clip,
        &contract.target_skeleton_path,
        &contract.output_path,
        &target_skeleton.name,
        &[],
    )
    .map_err(|error| error.to_string())?;
    let (xml, _) = super::clip_to_havok_xml_with_float_tracks(
        &clip,
        &declaration,
        &[],
        &target_skeleton.name,
        30.0,
        super::FnvExtractedMotionPolicy::RejectNonzero,
    )
    .map_err(|error| error.to_string())?;
    let bytes = havok_native::api::havok_xml_to_hkx(&xml).map_err(|error| error.to_string())?;
    super::validate_staged_fo4_animation(&bytes).map_err(|error| error.to_string())?;
    Ok(StagedSkyrimActorClip {
        contract,
        consumer,
        bytes,
        source_float_tracks,
        unmapped_source_bones,
        weapon_fire_time,
        suppressed_source_events,
        duration: clip.duration,
        source_attachment_channels,
    })
}

fn unpack(bytes: &[u8]) -> Result<String, String> {
    havok_native::api::havok_hkx_to_xml(bytes).map_err(|error| error.to_string())
}

fn actor_bone_map(
    source: &havok_native::animation::pose::PoseSkeleton,
    target: &havok_native::animation::pose::PoseSkeleton,
    bow: bool,
) -> Result<HashMap<String, String>, String> {
    let mut result = super::weapon_retarget::skyrim_actor_bone_map(source, target)?;
    for required in [
        "NPC Root [Root]",
        "NPC COM [COM ]",
        "NPC Pelvis [Pelv]",
        "NPC Spine [Spn0]",
        "NPC Spine1 [Spn1]",
        "NPC Spine2 [Spn2]",
        "NPC L Clavicle [LClv]",
        "NPC R Clavicle [RClv]",
        "NPC L UpperArm [LUar]",
        "NPC R UpperArm [RUar]",
        "NPC L Forearm [LLar]",
        "NPC R Forearm [RLar]",
        "NPC L Hand [LHnd]",
        "NPC R Hand [RHnd]",
        "NPC L Thigh [LThg]",
        "NPC R Thigh [RThg]",
        "NPC L Calf [LClf]",
        "NPC R Calf [RClf]",
        "NPC L Foot [Lft ]",
        "NPC R Foot [Rft ]",
    ] {
        if source.bone_names.iter().any(|name| name == required) && !result.contains_key(required) {
            return Err(format!(
                "Skyrim weapon actor skeleton cannot bind {required}"
            ));
        }
    }
    result.insert(
        "Shield".into(),
        if bow {
            "WeaponLeft".into()
        } else {
            String::new()
        },
    );
    result.insert("WeaponBow".into(), String::new());
    Ok(result)
}

fn decode_actor_clip(xml: &str, skeleton: &SkeletonRecord) -> Result<AnimationClip, String> {
    let decoded = havok_native::animation::clip::extract_clip(xml, None)
        .map_err(|error| error.to_string())?;
    if !decoded.warnings.is_empty() || decoded.channels.is_empty() {
        return Err(format!(
            "Skyrim actor clip decoding failed: {:?}",
            decoded.warnings
        ));
    }
    if decoded.is_additive || !decoded.extracted_motion_ref.is_empty() {
        return Err(
            "Skyrim weapon actor clip requires an explicit additive/root-motion policy".into(),
        );
    }
    if decoded.original_skeleton_name.as_deref() != Some(skeleton.name.as_str()) {
        return Err("Skyrim actor clip is bound to a different source skeleton".into());
    }
    let indices = if decoded.track_to_bone_indices.is_empty() {
        if decoded.channels.len() != skeleton.bone_names.len() {
            return Err("implicit Skyrim actor binding does not cover the source skeleton".into());
        }
        (0..decoded.channels.len()).collect::<Vec<_>>()
    } else {
        decoded
            .track_to_bone_indices
            .iter()
            .map(|index| *index as usize)
            .collect()
    };
    if indices.len() != decoded.channels.len()
        || indices
            .iter()
            .any(|index| *index >= skeleton.bone_names.len())
        || indices.iter().copied().collect::<HashSet<_>>().len() != indices.len()
    {
        return Err("invalid Skyrim actor transform-track binding".into());
    }
    let channels = decoded
        .channels
        .into_iter()
        .zip(indices)
        .map(|(channel, index)| BoneChannel {
            bone_name: skeleton.bone_names[index].clone(),
            translations: keys(channel.translations),
            rotations: keys(channel.rotations),
            scales: keys(channel.scales),
        })
        .collect();
    Ok(AnimationClip {
        duration: decoded.duration.into(),
        source_stop_time: decoded.duration.into(),
        channels,
        events: decoded
            .events
            .into_iter()
            .map(|event| AnimationEvent {
                time: event.time.into(),
                source_time: event.time.into(),
                text: event.text,
            })
            .collect(),
        ..Default::default()
    })
}

fn keys<const N: usize>(
    values: Vec<havok_native::animation::clip::AnimationKeyframe<[f32; N]>>,
) -> Vec<AnimationKeyframe> {
    values
        .into_iter()
        .map(|key| AnimationKeyframe {
            time: key.time.into(),
            value: key.value.into_iter().map(f64::from).collect(),
            interpolation: Interpolation::Linear,
        })
        .collect()
}

fn adapt_action_events(
    clip: &mut AnimationClip,
    action: SkyrimWeaponAction,
    consumer: SkyrimActorConsumer,
) -> Result<(Option<f64>, Vec<AnimationEvent>), String> {
    if clip.events.iter().any(|event| {
        !event.time.is_finite() || event.time < 0.0 || event.time > clip.duration + 1.0e-5
    }) {
        return Err("Skyrim weapon annotation falls outside its clip".into());
    }
    if clip
        .events
        .iter()
        .any(|event| event.text.eq_ignore_ascii_case("weaponFire"))
    {
        return Err("source Skyrim weapon clip already contains a target firing event".into());
    }
    if consumer == SkyrimActorConsumer::SourceAnnotations {
        return Ok((None, Vec::new()));
    }
    if !matches!(
        action,
        SkyrimWeaponAction::CrossbowRelease | SkyrimWeaponAction::CrossbowReload
    ) {
        return Err("Skyrim bow actions require a draw/hold consumer; a FO4 crossbow gun profile cannot consume them".into());
    }
    let releases = clip
        .events
        .iter()
        .filter(|event| event.text == "arrowRelease")
        .collect::<Vec<_>>();
    if action == SkyrimWeaponAction::CrossbowRelease && releases.len() != 1 {
        return Err(format!(
            "Skyrim release action requires exactly one arrowRelease, found {}",
            releases.len()
        ));
    }
    let time = (action == SkyrimWeaponAction::CrossbowRelease).then(|| releases[0].time);
    let reload_marker = if action == SkyrimWeaponAction::CrossbowRelease {
        "SoundPlay.WPNCrossbowReload"
    } else {
        "arrowAttach"
    };
    let reload_times: Vec<_> = clip
        .events
        .iter()
        .filter(|event| event.text == reload_marker)
        .map(|event| event.time)
        .collect();
    if reload_times.len() != 1 || time.is_some_and(|fire| reload_times[0] <= fire) {
        return Err(format!(
            "crossbow action requires one ordered {reload_marker} marker"
        ));
    }
    let reload_time = reload_times[0];
    // FO4's bow handlers set attack states 9/10/11, but WeaponFireHandler only
    // fires in states 15..17. A gun profile must own those state transitions.
    let mut suppressed = Vec::new();
    clip.events.retain(|event| {
        if (time.is_some() && event.time >= reload_time)
            || matches!(
                event.text.to_ascii_lowercase().as_str(),
                "arrowattach"
                    | "arrowrelease"
                    | "arrowdetach"
                    | "bowdrawn"
                    | "bowzoomstart"
                    | "bowzoomstop"
            )
        {
            suppressed.push(event.clone());
            false
        } else {
            true
        }
    });
    if let Some(time) = time {
        clip.duration = reload_time;
        clip.source_stop_time = reload_time;
        clip.events.push(AnimationEvent {
            time,
            source_time: time,
            text: "weaponFire".into(),
        });
    } else {
        clip.events.push(AnimationEvent {
            time: reload_time,
            source_time: reload_time,
            text: "reloadComplete".into(),
        });
    }
    clip.events
        .sort_by(|left, right| left.time.total_cmp(&right.time));
    Ok((time, suppressed))
}

pub(super) fn decode_float_tracks(
    xml: &str,
    skeleton: &SkeletonRecord,
) -> Result<Vec<SkyrimFloatTrack>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|error| error.to_string())?;
    let anim = doc
        .descendants()
        .find(|node| node.attribute("class") == Some("hkaSplineCompressedAnimation"))
        .ok_or("Skyrim actor clip is not a spline animation")?;
    let param = |name: &str| -> Result<&str, String> {
        anim.children()
            .find(|node| node.attribute("name") == Some(name))
            .map(|node| node.text().unwrap_or(""))
            .ok_or_else(|| format!("missing spline {name}"))
    };
    let uint = |name: &str| -> Result<u32, String> {
        param(name)?
            .trim()
            .parse()
            .map_err(|error| format!("invalid spline {name}: {error}"))
    };
    let float = |name: &str| -> Result<f32, String> {
        param(name)?
            .trim()
            .parse()
            .map_err(|error| format!("invalid spline {name}: {error}"))
    };
    let array = |name: &str| -> Result<Vec<u32>, String> {
        param(name)?
            .split_whitespace()
            .map(|value| {
                value
                    .parse()
                    .map_err(|error| format!("invalid spline {name}: {error}"))
            })
            .collect()
    };
    let count = uint("numberOfFloatTracks")?;
    if count == 0 {
        return Ok(Vec::new());
    }
    let binding = doc
        .descendants()
        .find(|node| node.attribute("class") == Some("hkaAnimationBinding"))
        .ok_or("missing Skyrim actor binding")?;
    let mapping = binding
        .children()
        .find(|node| node.attribute("name") == Some("floatTrackToFloatSlotIndices"))
        .ok_or("missing Skyrim float binding")?
        .text()
        .unwrap_or("")
        .split_whitespace()
        .map(|value| value.parse::<usize>().map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    if mapping.len() != count as usize
        || mapping
            .iter()
            .any(|index| *index >= skeleton.float_slots.len())
    {
        return Err("invalid Skyrim float-track binding".into());
    }
    let data = param("data")?
        .split_whitespace()
        .map(|value| value.parse::<u8>().map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let frame_duration = float("frameDuration")?;
    let decoded = decompress_spline_full(
        &data,
        uint("numberOfTransformTracks")?,
        count,
        uint("numFrames")?,
        uint("maxFramesPerBlock")?,
        uint("numBlocks")?,
        &array("blockOffsets")?,
        &array("floatBlockOffsets")?,
        uint("maskAndQuantizationSize")?,
        float("blockDuration")?,
        float("blockInverseDuration")?,
        frame_duration,
    )
    .map_err(|error| error.to_string())?;
    decoded
        .float_tracks
        .into_iter()
        .zip(mapping)
        .map(|(values, index)| {
            if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
                return Err("invalid Skyrim source float samples".into());
            }
            Ok(SkyrimFloatTrack {
                slot_name: skeleton.float_slots[index].clone(),
                frame_duration,
                values,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;

pub(crate) mod bow_graph;
pub(crate) mod crossbow_graph;
mod equipment_graph;
pub(crate) mod phase;
