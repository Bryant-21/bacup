use super::*;
use crate::phase::animations::{
    AnimationEvent, AnimationKeyframe, BoneChannel, FloatChannel, FnvKfBindingCompatibility,
    FnvKfBindingEvidence, FnvKfCycle, FnvKfRootMotionEvidence, FnvKfTextKeyEvidence, Interpolation,
    parse_fnv_creature_kf, parse_kf_bytes,
};

fn weapon(rate: f64) -> WeaponAnimationFields {
    WeaponAnimationFields {
        animation_type: 5,
        grip_animation: 255,
        attack_animation: 80,
        reload_animation: 0,
        ammo_use: 1,
        automatic: true,
        per_round_reload: false,
        animation_multiplier: 1.0,
        attack_multiplier: 1.0,
        fire_rate: rate,
        attack_shots_sec: rate,
        reload_time: 0.0,
    }
}

fn sequence() -> ParsedFnvCreatureKf {
    ParsedFnvCreatureKf {
        source_kf: "creatures/sentrybot/2hrattackspin.kf".into(),
        sequence_index: 0,
        sequence_name: "AttackSpin".into(),
        cycle: FnvKfCycle::Clamp,
        start_time: 0.0,
        stop_time: 4.0,
        frequency: 1.0,
        text_keys: [(0.0, "start"), (1.0, "Fire"), (3.0, "Loop"), (4.0, "end")]
            .into_iter()
            .map(|(time, text)| FnvKfTextKeyEvidence {
                time,
                text: text.into(),
            })
            .collect(),
        binding: FnvKfBindingEvidence {
            skeleton_path: None,
            transform_track_count: 1,
            float_track_count: 1,
            controller_types: vec![],
            interpolator_types: vec![],
            target_names: vec!["Barrel".into()],
            transform_priorities: Default::default(),
            transform_components: Default::default(),
            required_float_slots: vec!["heat".into()],
            compatibility: FnvKfBindingCompatibility::Verified,
        },
        root_motion: FnvKfRootMotionEvidence::None,
    }
}

fn clip(sequence: &ParsedFnvCreatureKf) -> AnimationClip {
    let keys = |values: &[(f64, f64)]| {
        values
            .iter()
            .map(|&(time, value)| AnimationKeyframe {
                time,
                value: vec![value],
                interpolation: Interpolation::Linear,
            })
            .collect()
    };
    AnimationClip {
        duration: sequence.stop_time,
        cycle_type: "clamp".into(),
        channels: vec![BoneChannel {
            bone_name: "Barrel".into(),
            rotations: vec![],
            scales: vec![],
            translations: [0.0, 1.0, 2.0, 3.0, 4.0]
                .into_iter()
                .map(|time| AnimationKeyframe {
                    time,
                    value: vec![time, 0.0, 0.0],
                    interpolation: Interpolation::Linear,
                })
                .collect(),
        }],
        float_channels: vec![FloatChannel {
            slot_name: "heat".into(),
            target_name: "Barrel".into(),
            keyframes: keys(&[(0.0, 0.0), (2.0, 8.0), (4.0, 0.0)]),
        }],
        events: sequence
            .text_keys
            .iter()
            .map(|key| AnimationEvent {
                time: key.time,
                source_time: key.time,
                text: key.text.clone(),
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn automatic_phases_preserve_source_boundaries_floats_and_record_cadence() {
    let source = sequence();
    let mut fields = weapon(15.0);
    fields.animation_multiplier = 2.0;
    let segments = plan(&fields, &source).unwrap();
    assert_eq!(
        segments.iter().map(|part| part.phase).collect::<Vec<_>>(),
        [
            ContinuousAttackPhase::Start,
            ContinuousAttackPhase::Loop,
            ContinuousAttackPhase::Stop
        ]
    );
    let mut results = Vec::new();
    for segment in segments {
        let mut result = clip(&source);
        apply(&mut result, segment).unwrap();
        results.push(result);
    }
    assert_eq!(
        results.iter().map(|clip| clip.duration).collect::<Vec<_>>(),
        [0.5, 1.0, 0.5]
    );
    for index in [0, 2] {
        assert_eq!(results[index].cycle_type, "clamp");
        assert!(
            !results[index]
                .events
                .iter()
                .any(|event| event.text.eq_ignore_ascii_case("Fire") || event.text == "weaponFire")
        );
    }
    let repeating = &results[1];
    assert_eq!(repeating.cycle_type, "loop");
    let shots = repeating
        .events
        .iter()
        .filter(|event| event.text == "weaponFire")
        .collect::<Vec<_>>();
    assert_eq!(shots.len(), 15);
    for (index, event) in shots.iter().enumerate() {
        assert!((event.time - (index as f64 / 15.0).max(0.00001)).abs() < 1e-8);
    }
    assert_eq!(
        repeating.channels[0].translations.first().unwrap().value,
        [1.0, 0.0, 0.0]
    );
    assert_eq!(
        repeating.channels[0].translations.last().unwrap().value,
        [3.0, 0.0, 0.0]
    );
    assert_eq!(
        repeating.float_channels[0]
            .keyframes
            .iter()
            .map(|key| (key.time, key.value[0]))
            .collect::<Vec<_>>(),
        [(0.0, 4.0), (0.5, 8.0), (1.0, 4.0)]
    );
}

#[test]
fn automatic_loop_repeats_pose_and_float_cycles_until_shot_cadence_wraps() {
    let mut source = sequence();
    source.cycle = FnvKfCycle::Loop;
    source.stop_time = 0.2;
    source.text_keys = vec![];
    for (rate, repeats, shots) in [(12.0, 5, 12), (15.0, 1, 3), (20.0, 1, 4)] {
        let segments = plan(&weapon(rate), &source).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].repeat_count, repeats);
        let mut animation = AnimationClip {
            duration: 0.2,
            float_channels: vec![FloatChannel {
                slot_name: "heat".into(),
                target_name: "Barrel".into(),
                keyframes: [(0.0, 0.0), (0.1, 1.0), (0.2, 0.0)]
                    .into_iter()
                    .map(|(time, value)| AnimationKeyframe {
                        time,
                        value: vec![value],
                        interpolation: Interpolation::Linear,
                    })
                    .collect(),
            }],
            ..Default::default()
        };
        apply(&mut animation, segments[0]).unwrap();
        assert_eq!(
            animation
                .events
                .iter()
                .filter(|event| event.text == "weaponFire")
                .count(),
            shots
        );
        let keys = &animation.float_channels[0].keyframes;
        assert_eq!(keys.len(), repeats * 2 + 1);
        assert!(keys.windows(2).all(|pair| pair[0].time < pair[1].time));
        assert!((keys.last().unwrap().time - animation.duration).abs() < 1e-8);
    }
}

#[test]
#[ignore = "requires extracted FNV meshes and CLI WEAP corpus (MODKIT_FNV_WEAPON_CORPUS)"]
fn real_sentrybot_automatic_weapon_and_kf_create_fire_phases() {
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record};
    use crate::sym::StringInterner;
    let root = std::env::var_os("MODKIT_FNV_CREATURE_MESH_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../../extracted/fnv/meshes")
        });
    let corpus = std::fs::read(
        std::env::var_os("MODKIT_FNV_WEAPON_CORPUS").expect("MODKIT_FNV_WEAPON_CORPUS"),
    )
    .unwrap();
    let corpus: serde_json::Value = serde_json::from_slice(&corpus).unwrap();
    let source = corpus
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["editor_id"] == "SentryBotMinigun2HR")
        .unwrap();
    let hex = source["dnam_hex"].as_str().unwrap();
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let interner = StringInterner::new();
    let mut record = Record::new(
        SigCode(*b"WEAP"),
        FormKey::parse("057E8E@FalloutNV.esm", &interner).unwrap(),
    );
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"DNAM"),
        value: FieldValue::Bytes(bytes.into()),
    });
    let fields =
        super::super::weapon_events::decode_weapon_animation_fields(&record, &interner).unwrap();
    assert!(fields.automatic);
    assert_eq!(fields.fire_rate, 15.0);
    let path = root.join("creatures/sentrybot/2hrattackspin.kf");
    let bytes = std::fs::read(&path).unwrap();
    let source = parse_fnv_creature_kf(&bytes, &path.to_string_lossy(), None, None).unwrap();
    let segments = plan(&fields, &source).unwrap();
    assert_eq!(segments.len(), 3);
    let raw = parse_kf_bytes(&bytes, &path.to_string_lossy()).unwrap();
    for segment in segments {
        let mut result = raw.clone();
        apply(&mut result, segment).unwrap();
        let shots = result
            .events
            .iter()
            .filter(|event| event.text == "weaponFire")
            .collect::<Vec<_>>();
        if segment.phase == ContinuousAttackPhase::Loop {
            assert_eq!(shots.len(), 14);
            assert!((result.duration - 14.0 / 15.0).abs() < 1e-5);
        } else {
            assert!(shots.is_empty());
        }
        assert!(result.channels.iter().all(|channel| {
            channel
                .translations
                .iter()
                .chain(&channel.rotations)
                .chain(&channel.scales)
                .all(|key| key.time >= 0.0 && key.time <= result.duration + 1e-6)
        }));
    }
}

#[test]
fn automatic_equipment_payload_targets_only_the_selected_slot() {
    let source = sequence();
    let segments = plan(&weapon(15.0), &source).unwrap();
    for mut segment in segments {
        segment.equip_index = Some(4);
        let mut animation = clip(&source);
        for event in &mut animation.events {
            if event.text == "Fire" {
                event.text = "weaponFire.4".into();
            }
        }
        apply(&mut animation, segment).unwrap();
        let events = animation
            .events
            .iter()
            .filter(|event| event.text.starts_with("weaponFire"))
            .collect::<Vec<_>>();
        if segment.phase == ContinuousAttackPhase::Loop {
            assert_eq!(events.len(), 30);
            assert!(events.iter().all(|event| event.text == "weaponFire.4"));
        } else {
            assert!(events.is_empty());
        }
        segment.equip_index = Some(5);
        assert!(apply(&mut clip(&source), segment).is_err());
    }
}

#[test]
fn single_shot_source_speed_scales_motion_and_contacts_once() {
    let source = sequence();
    let mut animation = clip(&source);
    scale_playback(&mut animation, 2.0).unwrap();
    assert_eq!(animation.duration, 2.0);
    let fire = animation
        .events
        .iter()
        .find(|event| event.text == "Fire")
        .unwrap();
    assert_eq!((fire.time, fire.source_time), (0.5, 1.0));
    assert_eq!(animation.events.len(), source.text_keys.len());
    assert_eq!(animation.channels[0].translations.last().unwrap().time, 2.0);
    assert_eq!(
        animation.float_channels[0].keyframes.last().unwrap().time,
        2.0
    );
}
