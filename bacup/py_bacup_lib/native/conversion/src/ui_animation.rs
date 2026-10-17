use havok_native::animation::clip::extract_clip;
use havok_native::animation::writer::write_interleaved_animation_xml;
use havok_native::api::{havok_convert_bytes, havok_hkx_to_xml};
use havok_native::hkx::descriptors::DescriptorRegistry;
use havok_native::hkx::tagxml::read_tagxml_string;
use havok_native::hkx::types::HkxValue;
use havok_native::hkx::{HkxFile, HkxMember, write_hkx};
use serde_json::{Value, json};

fn member<'a>(members: &'a [HkxMember], name: &str) -> Option<&'a HkxValue> {
    members
        .iter()
        .find(|member| member.name == name)
        .map(|member| &member.value)
}

fn integer(value: &HkxValue) -> Option<i64> {
    match value {
        HkxValue::I16(value) => Some(*value as i64),
        HkxValue::U16(value) => Some(*value as i64),
        HkxValue::I32(value) => Some(*value as i64),
        HkxValue::U32(value) => Some(*value as i64),
        _ => None,
    }
}

fn decimal(value: &HkxValue) -> Option<f64> {
    match value {
        // Catalogs historically use TagXML's six-decimal float representation.
        HkxValue::F32(value) => format!("{value:.6}").parse().ok(),
        _ => None,
    }
}

fn is_animation(class: &str) -> bool {
    matches!(
        class,
        "hkaSplineCompressedAnimation" | "hkaInterleavedUncompressedAnimation"
    )
}

fn metadata(file: &HkxFile, expected_tracks: usize) -> Result<Value, String> {
    let animations: Vec<_> = file
        .objects()
        .iter()
        .filter(|object| is_animation(&object.class_name))
        .collect();
    if animations.len() != 1 {
        return Err("Expected one emote animation".into());
    }
    let animation = animations[0];
    let duration = member(&animation.members, "duration")
        .and_then(decimal)
        .ok_or("Missing animation duration")?;
    let tracks = member(&animation.members, "numberOfTransformTracks")
        .and_then(integer)
        .ok_or("Missing animation tracks")?;
    let binding = file
        .objects()
        .iter()
        .find(|object| object.class_name == "hkaAnimationBinding")
        .ok_or("Missing animation binding")?;
    let indices = match member(&binding.members, "transformTrackToBoneIndices") {
        Some(HkxValue::Array(values)) => values.iter().filter_map(integer).collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    if !(duration > 0.0
        && duration <= 120.0
        && tracks > 0
        && tracks <= expected_tracks as i64
        && indices == (0..tracks).collect::<Vec<_>>())
    {
        return Err(format!(
            "Emote did not produce an FO4 {expected_tracks}-bone binding ({tracks} tracks, {} bound, {duration} s)",
            indices.len()
        ));
    }
    let mut annotations = Vec::new();
    let mut sound_cues = Vec::new();
    if let Some(HkxValue::Array(tracks)) = member(&animation.members, "annotationTracks") {
        for track in tracks {
            let Some(members) = track.as_object_members() else {
                continue;
            };
            let Some(HkxValue::Array(events)) = member(members, "annotations") else {
                continue;
            };
            for event in events {
                let Some(members) = event.as_object_members() else {
                    continue;
                };
                let Some(HkxValue::String { value, is_null }) = member(members, "text") else {
                    continue;
                };
                let text = if *is_null { "null" } else { value };
                annotations.push(if text.is_empty() {
                    Value::Null
                } else {
                    json!(text)
                });
                if let Some(sound) = text.trim().strip_prefix("SoundPlay.") {
                    let time = member(members, "time").and_then(decimal).unwrap_or(0.0);
                    sound_cues.push((time, sound.to_owned()));
                }
            }
        }
    }
    sound_cues.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
    });
    sound_cues.dedup();
    Ok(
        json!({"duration": duration, "tracks": tracks, "annotations": annotations, "sound_cues": sound_cues}),
    )
}

pub fn convert(
    data: &[u8],
    bones: Option<&[String]>,
    expected_tracks: usize,
) -> Result<(Vec<u8>, Value), String> {
    let mut converted =
        havok_convert_bytes(data, "hk_2014.1.0-r1").map_err(|error| error.to_string())?;
    if let Some(bones) = bones {
        let xml = havok_hkx_to_xml(&converted).map_err(|error| error.to_string())?;
        let mut clip = extract_clip(&xml, None).map_err(|error| error.to_string())?;
        if !clip.warnings.is_empty() {
            return Err(format!("PA clip decode warnings: {:?}", clip.warnings));
        }
        let count = bones.len();
        if clip.track_to_bone_indices.get(..count)
            != Some((0..count as u32).collect::<Vec<_>>().as_slice())
        {
            return Err("PA clip tracks do not start with the FO4 PA skeleton order".into());
        }
        clip.channels.truncate(count);
        for (channel, bone) in clip.channels.iter_mut().zip(bones) {
            channel.bone_name.clone_from(bone);
        }
        clip.track_to_bone_indices = (0..count as u32).collect();
        clip.extracted_motion_ref.clear();
        let replacement_xml =
            write_interleaved_animation_xml(&clip, bones).map_err(|error| error.to_string())?;
        let replacement_file =
            read_tagxml_string(&replacement_xml).map_err(|error| error.to_string())?;
        let mut replacement = replacement_file
            .objects()
            .iter()
            .find(|object| is_animation(&object.class_name))
            .ok_or("Missing replacement animation")?
            .clone();
        let mut file = read_tagxml_string(&xml).map_err(|error| error.to_string())?;
        let original = file
            .objects_mut()
            .iter_mut()
            .find(|object| is_animation(&object.class_name))
            .ok_or("Missing source animation")?;
        replacement.name = original.name.clone();
        if let Some(motion) = member(&original.members, "extractedMotion") {
            if let Some(replacement_motion) = replacement
                .members
                .iter_mut()
                .find(|member| member.name == "extractedMotion")
            {
                replacement_motion.value = motion.clone();
            }
        }
        *original = replacement;
        let binding = file
            .objects_mut()
            .iter_mut()
            .find(|object| object.class_name == "hkaAnimationBinding")
            .ok_or("Missing animation binding")?;
        let indices = binding
            .members
            .iter_mut()
            .find(|member| member.name == "transformTrackToBoneIndices")
            .ok_or("Missing animation track indices")?;
        indices.value = HkxValue::Array(
            (0..count)
                .map(|index| HkxValue::I16(index as i16))
                .collect(),
        );
        converted = write_hkx(&file, &mut DescriptorRegistry::new());
    }
    let file = HkxFile::read(&converted).map_err(|error| error.to_string())?;
    let metadata = metadata(&file, expected_tracks)?;
    Ok((converted, metadata))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armor_trim_preserves_motion_and_events_in_retained_tracks() {
        use havok_native::animation::clip::{
            AnimationClip, AnimationEvent, AnimationKeyframe, BoneChannel,
        };
        let channels = (0..3)
            .map(|index| BoneChannel {
                bone_name: format!("track_{index}"),
                translations: vec![
                    AnimationKeyframe {
                        time: 0.0,
                        value: [index as f32 + 1.0, 2.0, 3.0],
                    },
                    AnimationKeyframe {
                        time: 1.0,
                        value: [index as f32 + 4.0, 5.0, 6.0],
                    },
                ],
                rotations: vec![AnimationKeyframe {
                    time: 0.0,
                    value: [0.0, 0.0, 0.0, 1.0],
                }],
                scales: vec![AnimationKeyframe {
                    time: 0.0,
                    value: [1.0, 1.0, 1.0],
                }],
            })
            .collect();
        let clip = AnimationClip {
            source_format: "hkx".into(),
            duration: 1.0,
            native_fps: 1.0,
            channels,
            events: vec![AnimationEvent {
                time: 0.5,
                text: "SoundPlay.Test".into(),
            }],
            original_skeleton_name: Some("Root".into()),
            warnings: vec![],
            is_additive: false,
            track_to_bone_indices: vec![0, 1, 2],
            extracted_motion_ref: String::new(),
        };
        let xml = write_interleaved_animation_xml(
            &clip,
            &["track_0".into(), "track_1".into(), "track_2".into()],
        )
        .unwrap();
        let original = havok_native::api::havok_xml_to_hkx(&xml).unwrap();
        let (bytes, result) =
            convert(&original, Some(&["Root".into(), "Spine".into()]), 2).unwrap();
        let emitted = extract_clip(&havok_hkx_to_xml(&bytes).unwrap(), None).unwrap();
        assert_eq!(emitted.channels.len(), 2);
        assert_eq!(emitted.track_to_bone_indices, vec![0, 1]);
        assert_eq!(emitted.duration, 1.0);
        for (index, channel) in emitted.channels.iter().enumerate() {
            assert_eq!(
                channel.translations.first().unwrap().value,
                clip.channels[index].translations.first().unwrap().value
            );
            assert_eq!(
                channel.translations.last().unwrap().value,
                clip.channels[index].translations.last().unwrap().value
            );
        }
        assert_eq!(result["annotations"], json!(["SoundPlay.Test"]));
        assert_eq!(result["sound_cues"], json!([[0.5, "Test"]]));
    }

    fn animation_xml(indices: &str) -> String {
        format!(
            r##"<hkpackfile classversion="11" contentsversion="hk_2014.1.0-r1"><hksection name="__data__">
            <hkobject name="#animation" class="hkaInterleavedUncompressedAnimation">
                <hkparam name="duration">1.2345678</hkparam>
                <hkparam name="numberOfTransformTracks">2</hkparam>
                <hkparam name="annotationTracks" numelements="1"><hkobject><hkparam name="annotations" numelements="3">
                    <hkobject><hkparam name="time">0.033</hkparam><hkparam name="text">SoundPlay.RockOut</hkparam></hkobject>
                    <hkobject><hkparam name="time">0.033</hkparam><hkparam name="text">SoundPlay.RockOut</hkparam></hkobject>
                    <hkobject><hkparam name="time">1</hkparam><hkparam name="text">AnimObjLoad</hkparam></hkobject>
                </hkparam></hkobject></hkparam>
            </hkobject>
            <hkobject class="hkaAnimationBinding"><hkparam name="animation">#animation</hkparam>
                <hkparam name="transformTrackToBoneIndices" numelements="2">{indices}</hkparam>
            </hkobject>
        </hksection></hkpackfile>"##
        )
    }

    #[test]
    fn metadata_preserves_catalog_precision_order_and_deduplicates_sound_cues() {
        let file = read_tagxml_string(&animation_xml("0 1")).unwrap();
        let result = metadata(&file, 3).unwrap();
        assert_eq!(result["duration"], json!(1.234568));
        assert_eq!(result["tracks"], 2);
        assert_eq!(
            result["annotations"],
            json!(["SoundPlay.RockOut", "SoundPlay.RockOut", "AnimObjLoad"])
        );
        assert_eq!(result["sound_cues"], json!([[0.033, "RockOut"]]));
        assert!(metadata(&file, 1).is_err());
        assert!(metadata(&read_tagxml_string(&animation_xml("1 0")).unwrap(), 2).is_err());
    }
}
