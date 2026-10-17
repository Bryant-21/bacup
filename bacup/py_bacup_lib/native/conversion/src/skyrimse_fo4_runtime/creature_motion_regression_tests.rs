use super::*;

#[test]
fn skyrim_cast_duration_comes_from_source_animation_not_last_annotation() {
    for class_name in [
        "hkaSplineCompressedAnimation",
        "hkaInterleavedUncompressedAnimation",
        "hkaDeltaCompressedAnimation",
        "hkaQuantizedAnimation",
        "hkaPredictiveCompressedAnimation",
    ] {
        for duration in [1.0, 3.75, 0.0, -1.0, f32::NAN, f32::INFINITY] {
            let file = HkxFile::from_tagxml(
                8,
                SKYRIM_CONTENTS_VERSION,
                vec![HkxObject {
                    name: None,
                    offset: 0,
                    signature: 0,
                    class_name: class_name.into(),
                    members: vec![HkxMember {
                        name: "duration".into(),
                        value: HkxValue::F32(duration),
                    }],
                }],
            );
            let evidence = extract_clip_evidence(&file, "animations/cast.hkx");
            let expected = (duration.is_finite() && duration > 0.0).then_some(duration);
            assert_eq!(evidence.source_duration, expected, "{class_name}");
            assert!(evidence.annotations.is_empty());
        }
    }
}

#[test]
#[ignore = "requires the extracted Skyrim creature animation corpus"]
fn source_skyrim_dragon_inhale_duration_survives_reemit() {
    let root =
        PathBuf::from(std::env::var("MODKIT_SKYRIM_MESH_ROOT").expect("MODKIT_SKYRIM_MESH_ROOT"));
    let path = "actors/dragon/animations/ground_inhale.hkx";
    let source = std::fs::read(root.join(path)).unwrap();
    let source_file = HkxFile::read(&source).unwrap();
    let evidence = extract_clip_evidence(&source_file, path);
    assert_eq!(evidence.source_duration, Some(1.0));
    assert!(evidence.annotations.is_empty());
    let target =
        havok_native::api::havok_reemit_skyrim_2010_animation_asset_to_fo4(&source).unwrap();
    let target_file = HkxFile::read(&target).unwrap();
    assert_eq!(
        extract_clip_evidence(&target_file, path).source_duration,
        evidence.source_duration
    );
}

#[test]
fn skyrim_contact_attack_uses_melee_role_without_changing_aimed_spells() {
    for (delivery, expected) in [
        (1, SkyrimCreatureMotionRole::MeleeAttack),
        (2, SkyrimCreatureMotionRole::SpellAttack),
    ] {
        let attack = SkyrimRaceAttackEvidence {
            event: "attackStartSpell".to_string(),
            has_attack_spell: true,
            attack_spell_delivery: Some(delivery),
        };
        let located = vec![LocatedRaceAttack {
            project_path: "test.hkx".to_string(),
            source_race: None,
            attack: attack.clone(),
        }];
        let matched = matching_race_attacks(&attack.event, &located);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].1, expected);
        assert_eq!(
            candidate_attack_role(&attack, &[]),
            motion_attack_role(expected).unwrap()
        );
    }
}

#[test]
fn skyrim_root_motion_keeps_closed_translation_and_rotation_paths() {
    let zero = [0.0; 4];
    let forward = [0.0, 20.0, 0.0, 0.0];
    let identity = [0.0, 0.0, 0.0, 1.0];
    let turn = [0.0, 0.0, 0.5_f32.sqrt(), 0.5_f32.sqrt()];
    for (translations, rotations) in [
        (vec![zero, forward, zero], Vec::new()),
        (vec![zero], vec![identity, turn, identity]),
    ] {
        let motion = root_motion_from_samples(
            SkyrimRootMotionSource::BoundAnims,
            &translations,
            &rotations,
        );
        assert!(matches!(
            motion,
            SkyrimRootMotion::Sampled {
                translation_delta: [0.0, 0.0, 0.0],
                ..
            }
        ));
        assert!(motion.policy().animation_driven);
    }
    assert!(matches!(
        root_motion_from_samples(
            SkyrimRootMotionSource::BoundAnims,
            &[zero],
            &[identity, identity.map(|v| -v)]
        ),
        SkyrimRootMotion::Stationary { .. }
    ));
}

#[test]
fn skyrim_havok_yaw_only_motion_is_animation_driven() {
    let file = HkxFile::from_tagxml(
        8,
        "hk_2010.2.0-r1",
        vec![havok_native::hkx::model::HkxObject {
            name: None,
            offset: 0,
            signature: 0x6d85_e445,
            class_name: "hkaDefaultAnimatedReferenceFrame".to_string(),
            members: vec![havok_native::hkx::model::HkxMember {
                name: "referenceFrameSamples".to_string(),
                value: HkxValue::Array(vec![
                    HkxValue::F32List(vec![0.0; 4]),
                    HkxValue::F32List(vec![0.0, 0.0, 0.0, std::f32::consts::FRAC_PI_2]),
                    HkxValue::F32List(vec![0.0; 4]),
                ]),
            }],
        }],
    );
    assert!(matches!(
        extract_havok_root_motion(&file),
        SkyrimRootMotion::Sampled {
            sample_count: 3,
            ..
        }
    ));
}

#[test]
fn skyrim_reduced_bound_tracks_restore_identity_origin_and_clip_frame_count() {
    let source = parse_bound_motion_samples_lines(
        Path::new("bound.txt"),
        &[
            "0",
            "1",
            "1",
            "1 0 12 0",
            "1",
            "1 0 0 0.70710678 0.70710678",
        ],
    )
    .unwrap();
    assert_eq!(
        source[&0].translations[0],
        SkyrimTimedMotionSample {
            time: 0.0,
            value: [0.0; 4]
        }
    );
    assert_eq!(
        source[&0].rotations[0],
        SkyrimTimedMotionSample {
            time: 0.0,
            value: [0.0, 0.0, 0.0, 1.0]
        }
    );
    let root_motion = parse_bound_anims_lines(
        Path::new("bound.txt"),
        &[
            "0",
            "1",
            "1",
            "1 0 12 0",
            "1",
            "1 0 0 0.70710678 0.70710678",
        ],
    )
    .unwrap()
    .remove(&0)
    .unwrap();
    let mut clip = SkyrimCreatureClipEvidence {
        clip_path: "test.hkx".to_string(),
        original_skeleton_name: None,
        source_frame_count: Some(31),
        source_duration: None,
        blend_hint: 0,
        behavior: Vec::new(),
        animation_data: Vec::new(),
        annotations: Vec::new(),
        root_motion,
    };
    set_bound_motion_frame_count(&mut clip);
    assert!(matches!(
        clip.root_motion,
        SkyrimRootMotion::Sampled {
            sample_count: 31,
            translation_delta: [0.0, 12.0, 0.0],
            ..
        }
    ));
}

#[test]
fn skyrim_inline_annotation_tracks_expose_contact_and_cast_cues() {
    let member = |name: &str, value| HkxMember {
        name: name.to_string(),
        value,
    };
    let events = [
        (0.3, "HitFrame"),
        (0.4, "BeginCastRight"),
        (0.8, "StopCastRight"),
    ];
    let file = HkxFile::from_tagxml(
        8,
        SKYRIM_CONTENTS_VERSION,
        vec![HkxObject {
            name: None,
            offset: 0,
            signature: 0,
            class_name: "hkaSplineCompressedAnimation".to_string(),
            members: vec![member(
                "annotationTracks",
                HkxValue::Array(vec![HkxValue::Object(vec![member(
                    "annotations",
                    HkxValue::Array(
                        events
                            .iter()
                            .map(|(time, text)| {
                                HkxValue::Object(vec![
                                    member("time", HkxValue::F32(*time)),
                                    member(
                                        "text",
                                        HkxValue::String {
                                            value: (*text).to_string(),
                                            is_null: false,
                                        },
                                    ),
                                ])
                            })
                            .collect(),
                    ),
                )])]),
            )],
        }],
    );
    assert_eq!(
        extract_annotations(&file),
        events.map(|(time, name)| SkyrimTimedEvent {
            time,
            name: name.to_string()
        })
    );
}

#[test]
#[ignore = "requires the extracted Skyrim creature animation corpus"]
fn source_skyrim_root_motion_corpus() {
    let root =
        PathBuf::from(std::env::var("MODKIT_SKYRIM_MESH_ROOT").expect("MODKIT_SKYRIM_MESH_ROOT"));
    let path = root.join("animationdatasinglefile.txt");
    let mut clips = 0;
    let mut closed_paths = 0;
    let mut irregular_tracks = 0;
    let mut tilted_rotations = 0;
    for (_, project) in RACE_PROJECTS {
        let stem = Path::new(project).file_stem().unwrap().to_str().unwrap();
        let samples = load_single_file_bound_motion_samples(&path, stem).unwrap();
        for motion in samples.values() {
            clips += 1;
            let translations = motion
                .translations
                .iter()
                .map(|s| s.value)
                .collect::<Vec<_>>();
            let rotations = motion.rotations.iter().map(|s| s.value).collect::<Vec<_>>();
            let classification = root_motion_from_samples(
                SkyrimRootMotionSource::BoundAnims,
                &translations,
                &rotations,
            );
            assert!(!matches!(
                classification,
                SkyrimRootMotion::Unknown | SkyrimRootMotion::Unsupported { .. }
            ));
            if let SkyrimRootMotion::Sampled {
                translation_delta,
                rotation_start,
                rotation_end,
                ..
            } = classification
            {
                if translation_delta.iter().all(|v| v.abs() <= 1.0e-5)
                    && rotation_start == rotation_end
                {
                    closed_paths += 1;
                }
            }
            for track in [&motion.translations, &motion.rotations] {
                if track.len() > 1
                    && track.iter().enumerate().any(|(i, s)| {
                        (s.time - motion.duration * i as f32 / (track.len() - 1) as f32).abs()
                            > 1.0e-4
                    })
                {
                    irregular_tracks += 1;
                }
            }
            tilted_rotations += motion
                .rotations
                .iter()
                .filter(|s| s.value[0].abs() > 1.0e-5 || s.value[1].abs() > 1.0e-5)
                .count();
        }
    }
    assert!(clips > 100);
    eprintln!(
        "Skyrim root motion: {clips} BoundAnims clips; {closed_paths} closed moving paths; {irregular_tracks} irregular tracks; {tilted_rotations} pitch/roll samples"
    );
}

#[test]
fn skyrim_lifecycle_roles_emit_action_bindings_and_death_capability() {
    let family = SkyrimFamilyMotionSet {
        family_id: "test".to_string(),
        project_path: "test.hkx".to_string(),
        race_attacks: Vec::new(),
        race_attack_sources: Vec::new(),
        clip_ids: Vec::new(),
    };
    let clips = [
        SkyrimCreatureMotionRole::Idle,
        SkyrimCreatureMotionRole::Hurt,
        SkyrimCreatureMotionRole::Death,
    ]
    .map(|role| SkyrimCatalogClip {
        clip_id: format!("{role:?}"),
        clip_path: format!("{role:?}.hkx"),
        original_skeleton_name: None,
        source_duration: None,
        family_ids: vec!["test".to_string()],
        disposition: SkyrimClipDisposition::Role {
            role,
            trigger_event: None,
            trigger_aliases: Vec::new(),
            evidence: Vec::new(),
        },
        root_motion: SkyrimRootMotion::Stationary {
            source: SkyrimRootMotionSource::HavokReferenceFrame,
            sample_count: 1,
        },
        events: Vec::new(),
        annotations: Vec::new(),
    });
    let (graph, variables) =
        build_capability_manifest(&family, None, &clips.iter().collect::<Vec<_>>()).unwrap();
    for role in [CreatureClipRole::Hurt, CreatureClipRole::Death] {
        assert_eq!(
            graph
                .roles
                .iter()
                .filter(|entry| entry.role == role)
                .count(),
            1
        );
    }
    assert!(
        variables.iter().any(
            |v| v.name == "bSupportedDeathAnim" && v.initial_value == VariableValue::Bool(true)
        )
    );
    assert!(
        graph
            .explicit_events
            .iter()
            .any(|v| v.name == "B21_DeathAnimationComplete")
    );
    let actions = crate::source_rig::required_actor_action_records(&graph, "Behaviors/Root.hkx");
    for (parent, role) in [
        ("ActionDeath", CreatureClipRole::Death),
        ("ActionStaggerStart", CreatureClipRole::Hurt),
    ] {
        let expected = graph
            .roles
            .iter()
            .find(|entry| entry.role == role)
            .unwrap()
            .trigger_event
            .as_ref()
            .unwrap();
        assert!(
            actions
                .iter()
                .any(|action| action.parent_editor_id == parent
                    && action.animation_event == *expected)
        );
    }
}

#[test]
#[ignore = "requires the extracted Skyrim creature animation corpus"]
fn source_skyrim_contact_attack_and_lifecycle_roundtrip() {
    let root =
        PathBuf::from(std::env::var("MODKIT_SKYRIM_MESH_ROOT").expect("MODKIT_SKYRIM_MESH_ROOT"));
    let interner = crate::sym::StringInterner::new();
    let attack = SkyrimRaceAttackEvidence {
        event: "attackPowerStart_Stomp".to_string(),
        has_attack_spell: true,
        attack_spell_delivery: Some(1),
    };
    let animation_data = load_animation_data_catalog(&root.join("animationdata")).unwrap();
    let family = load_family_evidence(
        &root.join("actors"),
        &animation_data["giantproject"],
        "giant",
        "giant/giantproject.hkx",
        vec![SkyrimRaceAttackSourceEvidence {
            source_race: FormKey {
                plugin: interner.intern("Skyrim.esm"),
                local: 0x0131F9,
            },
            source_plugin: "Skyrim.esm".to_string(),
            attack: attack.clone(),
        }],
    )
    .unwrap();
    let catalog = build_skyrim_creature_motion_catalog(&[family]).unwrap();
    let (graph, _) = catalog.capability_graph_bundle("giant").unwrap();
    let binding = graph
        .candidate_attack_bindings
        .iter()
        .find(|entry| entry.event.eq_ignore_ascii_case(&attack.event))
        .expect("contact event is executable");
    assert_eq!(binding.role, CreatureClipRole::MeleeAttack);
    assert!(
        graph
            .roles
            .iter()
            .any(|entry| entry.role == CreatureClipRole::Hurt)
    );
    assert!(
        graph
            .roles
            .iter()
            .any(|entry| entry.role == CreatureClipRole::Death)
    );
    let source = std::fs::read(root.join("actors/giant/animations/attack_stomp.hkx")).unwrap();
    let source_hkx = HkxFile::read(&source).unwrap();
    let source_annotations = extract_annotations(&source_hkx);
    assert!(
        source_annotations
            .iter()
            .any(|event| event.name == "HitFrame")
    );
    let target =
        havok_native::api::havok_reemit_skyrim_2010_animation_asset_to_fo4(&source).unwrap();
    let target_hkx = HkxFile::read(&target).unwrap();
    assert_eq!(source_annotations, extract_annotations(&target_hkx));
    assert_eq!(target_hkx.save(), target);
    use super::super::creature_recipe::*;
    let mut checked_motion_clips = 0;
    for clip in &catalog.clips {
        let SkyrimRootMotion::Sampled {
            source: SkyrimRootMotionSource::BoundAnims,
            sample_count,
            translation_delta,
            rotation_start,
            rotation_end,
        } = clip.root_motion
        else {
            continue;
        };
        let SkyrimClipDisposition::Role { evidence, .. } = &clip.disposition else {
            continue;
        };
        let locators = evidence
            .iter()
            .filter_map(|entry| match &entry.root_motion_locator {
                Some(SkyrimRootMotionSourceLocator::BoundAnims {
                    source_path,
                    project_stem,
                    animation_index,
                }) => Some(SkyrimRootMotionLocatorReceipt::BoundAnims {
                    source_path: source_path.clone(),
                    project_stem: project_stem.clone(),
                    animation_index: *animation_index,
                }),
                _ => None,
            })
            .collect::<Vec<_>>();
        if locators.is_empty() {
            continue;
        }
        let receipt = SkyrimCreatureClipReceipt {
            clip_id: clip.clip_id.clone(),
            clip_path: clip.clip_path.clone(),
            source_duration: clip.source_duration,
            family_ids: clip.family_ids.clone(),
            role: SkyrimCreatureClipRoleReceipt::Role(
                SkyrimCreatureMotionRoleReceipt::GroundLocomotion,
            ),
            trigger_event: None,
            trigger_aliases: Vec::new(),
            role_evidence: Vec::new(),
            root_motion: SkyrimRootMotionReceipt::Sampled {
                source: SkyrimRootMotionSourceReceipt::BoundAnims,
                sample_count,
                translation_delta,
                rotation_start,
                rotation_end,
            },
            root_motion_locators: locators,
            events: Vec::new(),
            annotations: Vec::new(),
            original_skeleton_name: clip.original_skeleton_name.clone(),
        };
        let source = std::fs::read(root.join(clip.clip_path.replace('\\', "/"))).unwrap();
        let converted =
            havok_native::api::havok_reemit_skyrim_2010_animation_asset_to_fo4(&source).unwrap();
        let staged =
            super::super::creature_mvp_live::preserve_bound_root_motion(&converted, &receipt)
                .unwrap();
        let packed = HkxFile::read(&staged).unwrap();
        assert_eq!(packed.save(), staged);
        let frame = packed
            .objects()
            .iter()
            .find(|object| object.class_name == "hkaDefaultAnimatedReferenceFrame")
            .unwrap();
        let samples = array_member(frame, "referenceFrameSamples").unwrap();
        assert_eq!(samples.len(), sample_count);
        let start = vector4(&samples[0]).unwrap();
        let end = vector4(samples.last().unwrap()).unwrap();
        for axis in 0..3 {
            assert!((end[axis] - start[axis] - translation_delta[axis]).abs() < 1.0e-4);
        }
        checked_motion_clips += 1;
    }
    assert!(checked_motion_clips > 0);
    eprintln!(
        "Giant: {checked_motion_clips} moving clips packed and reread with source-frame counts and displacement"
    );
    eprintln!(
        "Giant contact Stomp remains melee with {} exact annotations; Hurt and Death emitted",
        source_annotations.len()
    );
}
