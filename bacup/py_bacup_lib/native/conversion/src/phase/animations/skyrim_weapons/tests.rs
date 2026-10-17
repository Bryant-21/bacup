use super::*;

fn event(time: f64, text: &str) -> AnimationEvent {
    AnimationEvent {
        time,
        source_time: time,
        text: text.into(),
    }
}

#[test]
fn skyrim_weapon_contract_excludes_local_rigs_and_horse_variants() {
    let third =
        actor_clip_contract("Meshes\\Actors\\Character\\Animations\\Bow_Release.hkx").unwrap();
    let first =
        actor_clip_contract("actors/character/_1stperson/animations/bow_release.hkx").unwrap();
    assert!(!third.first_person);
    assert!(first.first_person);
    assert_ne!(first.target_skeleton_path, third.target_skeleton_path);
    assert_ne!(first.source_skeleton_path, third.source_skeleton_path);
    assert_ne!(first.output_path, third.output_path);
    assert_ne!(third.output_path.to_ascii_lowercase(), third.source_path);
    for path in [
        "weapons/bow/animations/bow_release.hkx",
        "actors/character/animations/horse_rider/patch/mc_bow_release.hkx",
        "actors/character/animations/unknown/bow_release.hkx",
        "actors/draugr/animations/bow_release.hkx",
    ] {
        assert!(actor_clip_contract(path).is_none(), "{path}");
    }
}

#[test]
fn skyrim_reload_arrow_release_never_becomes_weapon_fire() {
    let original = vec![
        event(0.133333, "arrowRelease"),
        event(0.2, "SoundPlay.WPNCrossbowReload"),
        event(0.666667, "arrowAttach"),
    ];
    let mut reload = AnimationClip {
        duration: 2.666667,
        events: original.clone(),
        ..Default::default()
    };
    assert_eq!(
        adapt_action_events(
            &mut reload,
            SkyrimWeaponAction::CrossbowReload,
            SkyrimActorConsumer::Fo4CrossbowGun
        )
        .unwrap()
        .0,
        None
    );
    assert!(
        reload
            .events
            .iter()
            .any(|event| event.text == "reloadComplete" && event.time == 0.666667)
    );
    assert!(reload.events.iter().all(|event| event.text != "weaponFire"));
    let mut release = AnimationClip {
        events: original,
        ..reload
    };
    assert_eq!(
        adapt_action_events(
            &mut release,
            SkyrimWeaponAction::CrossbowRelease,
            SkyrimActorConsumer::Fo4CrossbowGun
        )
        .unwrap()
        .0,
        Some(0.133333)
    );
    assert_eq!(
        release
            .events
            .iter()
            .filter(|event| event.text == "weaponFire")
            .count(),
        1
    );
    assert!(
        release
            .events
            .iter()
            .all(|event| event.text != "arrowAttach")
    );
}

#[test]
fn skyrim_release_rejects_missing_duplicate_and_invalid_shot_annotations() {
    for events in [
        vec![],
        vec![event(0.1, "arrowRelease"), event(0.2, "arrowRelease")],
        vec![event(2.0, "arrowRelease")],
    ] {
        let mut clip = AnimationClip {
            duration: 1.0,
            events,
            ..Default::default()
        };
        assert!(
            adapt_action_events(
                &mut clip,
                SkyrimWeaponAction::CrossbowRelease,
                SkyrimActorConsumer::Fo4CrossbowGun
            )
            .is_err()
        );
        assert!(clip.events.iter().all(|event| event.text != "weaponFire"));
    }
}

#[test]
fn skyrim_weapon_events_require_a_matching_actor_consumer() {
    let mut clip = AnimationClip {
        duration: 1.0,
        events: vec![
            event(0.1, "arrowRelease"),
            event(0.15, "SoundPlay.WPNCrossbowReload"),
            event(0.2, "arrowAttach"),
            event(0.3, "BowDrawn"),
        ],
        ..Default::default()
    };
    assert_eq!(
        adapt_action_events(
            &mut clip,
            SkyrimWeaponAction::BowRelease,
            SkyrimActorConsumer::SourceAnnotations
        )
        .unwrap()
        .0,
        None
    );
    assert_eq!(clip.events.len(), 4);
    assert!(
        adapt_action_events(
            &mut clip,
            SkyrimWeaponAction::BowRelease,
            SkyrimActorConsumer::Fo4CrossbowGun
        )
        .is_err()
    );
    assert_eq!(clip.events.len(), 4);
    let (fire, suppressed) = adapt_action_events(
        &mut clip,
        SkyrimWeaponAction::CrossbowRelease,
        SkyrimActorConsumer::Fo4CrossbowGun,
    )
    .unwrap();
    assert_eq!(fire, Some(0.1));
    assert_eq!(suppressed.len(), 4);
    assert_eq!(clip.events.len(), 1);
    assert_eq!(clip.events[0].text, "weaponFire");
}

#[test]
fn skyrim_crossbow_rejects_missing_reload_boundary_without_mutating_clip() {
    for action in [
        SkyrimWeaponAction::CrossbowRelease,
        SkyrimWeaponAction::CrossbowReload,
    ] {
        let mut clip = AnimationClip {
            duration: 1.0,
            events: vec![event(0.1, "arrowRelease")],
            ..Default::default()
        };
        assert!(
            adapt_action_events(&mut clip, action, SkyrimActorConsumer::Fo4CrossbowGun).is_err()
        );
        assert_eq!(clip.duration, 1.0);
        assert_eq!(clip.events.len(), 1);
        assert_eq!(clip.events[0].text, "arrowRelease");
    }
}

#[test]
fn skyrim_crossbow_profile_rejects_a_source_only_reload_consumer() {
    let staged = |leaf: &str, consumer, weapon_fire_time| StagedSkyrimActorClip {
        contract: actor_clip_contract(&format!("Actors/Character/Animations/DLC01/{leaf}.hkx"))
            .unwrap(),
        consumer,
        bytes: Vec::new(),
        source_float_tracks: Vec::new(),
        unmapped_source_bones: Vec::new(),
        weapon_fire_time,
        suppressed_source_events: Vec::new(),
        duration: 1.0,
        source_attachment_channels: Vec::new(),
    };
    let result = crossbow_graph::stage_crossbow_actor_profile(
        &[],
        staged(
            "Crossbow_Release",
            SkyrimActorConsumer::Fo4CrossbowGun,
            Some(0.133333),
        ),
        staged(
            "Crossbow_Reload",
            SkyrimActorConsumer::SourceAnnotations,
            None,
        ),
        Vec::new(),
    );
    assert!(matches!(result, Err(error) if error.contains("matching FO4 gun")));
}

#[test]
fn skyrim_animation_map_does_not_collapse_skin_aliases() {
    let source_names = [
        "NPC Pelvis [Pelv]",
        "NPC Spine [Spn0]",
        "NPC Spine1 [Spn1]",
        "NPC Spine2 [Spn2]",
        "NPC L UpperArm [LUar]",
        "NPC L Forearm [LLar]",
        "SkirtBBone01",
        "WeaponSword",
        "Weapon",
    ];
    let target_names = [
        "Pelvis",
        "Spine1",
        "Spine2",
        "Chest",
        "LArm_UpperArm",
        "LArm_ForeArm1",
        "Weapon",
    ];
    let skeleton = |names: &[&str]| havok_native::animation::pose::PoseSkeleton {
        bone_names: names.iter().map(|name| (*name).to_string()).collect(),
        parent_indices: vec![-1; names.len()],
        reference_local: vec![havok_native::animation::pose::QsTransform::IDENTITY; names.len()],
    };
    let source = skeleton(&source_names);
    let mut target = skeleton(&target_names);
    let map = actor_bone_map(&source, &target, false).unwrap();
    assert_eq!(
        map.get("NPC Pelvis [Pelv]").map(String::as_str),
        Some("Pelvis")
    );
    assert_eq!(map["NPC Spine [Spn0]"], "Spine1");
    assert_eq!(map["NPC Spine1 [Spn1]"], "Spine2");
    assert_eq!(map["NPC Spine2 [Spn2]"], "Chest");
    assert_eq!(map["NPC L UpperArm [LUar]"], "LArm_UpperArm");
    assert_eq!(map["NPC L Forearm [LLar]"], "LArm_ForeArm1");
    assert!(!map.contains_key("SkirtBBone01"));
    assert!(!map.contains_key("WeaponSword"));
    assert!(!map.contains_key("Weapon"));
    assert_eq!(map.get("WeaponBow").map(String::as_str), Some(""));
    target.bone_names[4] = "LArm_UpperArm_skin".into();
    assert!(
        actor_bone_map(&source, &target, false)
            .unwrap_err()
            .contains("NPC L UpperArm")
    );
}

#[test]
#[ignore = "requires official Skyrim third/first person source behavior graphs"]
fn skyrim_crossbow_source_equipment_graphs_preserve_reverse_playback() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../extracted/skyrimse/meshes");
    for actor in ["Actors/Character", "Actors/Character/_1stPerson"] {
        let xml = unpack(&std::fs::read(root.join(actor).join("Behaviors/0_Master.hkx")).unwrap())
            .unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let parameter = |node: roxmltree::Node<'_, '_>, name: &str| {
            node.children()
                .find(|child| child.attribute("name") == Some(name))
                .and_then(|child| child.text())
                .unwrap_or("")
                .to_string()
        };
        let unequip = doc
            .descendants()
            .find(|node| {
                node.attribute("class") == Some("hkbClipGenerator")
                    && parameter(*node, "name") == "CrossBow_Unequip"
            })
            .unwrap();
        assert!(
            parameter(unequip, "animationName")
                .to_ascii_lowercase()
                .ends_with("crossbow_equip.hkx")
        );
        assert_eq!(
            parameter(unequip, "playbackSpeed").parse::<f32>().unwrap(),
            -1.0,
            "{actor}: source reverse playback"
        );
        eprintln!(
            "{actor}: crossbow unequip uses equip clip; speed={} start={} flags={} mode={}",
            parameter(unequip, "playbackSpeed"),
            parameter(unequip, "startTime"),
            parameter(unequip, "flags"),
            parameter(unequip, "mode")
        );
    }
}

#[test]
#[ignore = "requires extracted Skyrim and Fallout 4 actor skeletons and bow/crossbow clips"]
fn skyrim_weapon_actor_source_corpus_retargets_both_perspectives_and_roundtrips() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let source_root = root.join("extracted/skyrimse/meshes");
    let target_root = root.join("extracted/fo4/meshes");
    let directory = tempfile::tempdir().unwrap();
    for first in [false, true] {
        let prefix = if first {
            "actors/character/_1stperson/animations"
        } else {
            "actors/character/animations"
        };
        for leaf in [
            "bow_drawlight.hkx",
            "bow_drawheavy.hkx",
            "bow_idledrawn.hkx",
            "bow_release.hkx",
            "sneakbow_drawlight.hkx",
            "sneakbow_idledrawn.hkx",
            "sneakbow_release.hkx",
            "bow_idleheld.hkx",
            "bow_equip.hkx",
            "bow_unequip.hkx",
            "dlc01/crossbow_release.hkx",
            "dlc01/crossbow_reload.hkx",
            "dlc01/crossbow_idleheld.hkx",
            "dlc01/crossbow_idledrawn.hkx",
            "dlc01/sneakcrossbow_idledrawn.hkx",
            "dlc01/crossbow_equip.hkx",
        ] {
            let contract = actor_clip_contract(&format!("{prefix}/{leaf}")).unwrap();
            let source = std::fs::read(source_root.join(&contract.source_path)).unwrap();
            let source_skeleton =
                std::fs::read(source_root.join(&contract.source_skeleton_path)).unwrap();
            let target_skeleton =
                std::fs::read(target_root.join(&contract.target_skeleton_path)).unwrap();
            let target_record = parse_skeleton_xml(&unpack(&target_skeleton).unwrap()).unwrap();
            let source_xml = unpack(&source).unwrap();
            let original = havok_native::animation::clip::extract_clip(&source_xml, None).unwrap();
            let consumer = if matches!(
                contract.action,
                SkyrimWeaponAction::CrossbowRelease | SkyrimWeaponAction::CrossbowReload
            ) {
                SkyrimActorConsumer::Fo4CrossbowGun
            } else {
                SkyrimActorConsumer::SourceAnnotations
            };
            let staged = stage_actor_clip(
                contract,
                consumer,
                &source,
                &source_skeleton,
                &target_skeleton,
            )
            .unwrap_or_else(|error| panic!("{prefix}/{leaf}: {error}"));
            let output = directory.path().join(&staged.contract.output_path);
            std::fs::create_dir_all(output.parent().unwrap()).unwrap();
            std::fs::write(&output, &staged.bytes).unwrap();
            let reread = std::fs::read(&output).unwrap();
            let decoded =
                havok_native::animation::clip::extract_clip(&unpack(&reread).unwrap(), None)
                    .unwrap();
            assert!(decoded.warnings.is_empty());
            assert_eq!(
                decoded.original_skeleton_name.as_deref(),
                Some(target_record.name.as_str())
            );
            assert_eq!(decoded.channels.len(), decoded.track_to_bone_indices.len());
            assert!(
                decoded
                    .track_to_bone_indices
                    .iter()
                    .all(|index| (*index as usize) < target_record.bone_names.len())
            );
            assert!(
                decoded
                    .track_to_bone_indices
                    .iter()
                    .any(|index| { target_record.bone_names[*index as usize] == "Weapon" })
            );
            let bow = !leaf.starts_with("dlc01/");
            assert_eq!(
                decoded
                    .track_to_bone_indices
                    .iter()
                    .any(|index| { target_record.bone_names[*index as usize] == "WeaponLeft" }),
                bow
            );
            let source_record = parse_skeleton_xml(&unpack(&source_skeleton).unwrap()).unwrap();
            let source_pose = target_reference_pose(&source_record).unwrap();
            let target_pose = target_reference_pose(&target_record).unwrap();
            let source_clip = decode_actor_clip(&source_xml, &source_record).unwrap();
            let target_clip = decode_actor_clip(&unpack(&reread).unwrap(), &target_record).unwrap();
            for time in [0.0, staged.duration] {
                let source =
                    super::super::weapon_retarget::sample_pose(&source_clip, &source_pose, time)
                        .all_model();
                let target =
                    super::super::weapon_retarget::sample_pose(&target_clip, &target_pose, time)
                        .all_model();
                for (source_name, target_name, source_hand, target_hand) in [
                    ("Shield", "WeaponLeft", "NPC L Hand [LHnd]", "LArm_Hand"),
                    ("Weapon", "Weapon", "NPC R Hand [RHnd]", "RArm_Hand"),
                ] {
                    if source_name == "Shield" && !bow {
                        continue;
                    }
                    let source_index = |name| {
                        source_pose
                            .bone_names
                            .iter()
                            .position(|bone| bone == name)
                            .unwrap()
                    };
                    let target_index = |name| {
                        target_pose
                            .bone_names
                            .iter()
                            .position(|bone| bone == name)
                            .unwrap()
                    };
                    let source_attachment = &source[source_index(source_name)];
                    let target_attachment = &target[target_index(target_name)];
                    let dot: f32 = source_attachment
                        .rotation
                        .iter()
                        .zip(target_attachment.rotation)
                        .map(|(a, b)| a * b)
                        .sum();
                    assert!(
                        (dot.abs() - 1.0).abs() < 1e-4,
                        "{prefix}/{leaf} {target_name} basis"
                    );
                    for axis in 0..3 {
                        let source_offset = source_attachment.translation[axis]
                            - source[source_index(source_hand)].translation[axis];
                        let target_offset = target_attachment.translation[axis]
                            - target[target_index(target_hand)].translation[axis];
                        assert!(
                            (source_offset - target_offset).abs() < 1e-3,
                            "{prefix}/{leaf} {target_name} grip"
                        );
                    }
                }
            }
            for name in ["Shield", "Weapon", "WeaponBow"] {
                let attachment = staged
                    .source_attachment_channels
                    .iter()
                    .find(|channel| channel.bone_name == name)
                    .unwrap();
                assert!(!attachment.translations.is_empty());
                assert!(!attachment.rotations.is_empty());
                assert_eq!(
                    staged.unmapped_source_bones.iter().any(|bone| bone == name),
                    name == "WeaponBow" || (name == "Shield" && !bow)
                );
            }
            assert!((f64::from(decoded.duration) - staged.duration).abs() < 1e-5);
            if staged.contract.action == SkyrimWeaponAction::CrossbowRelease {
                assert!((staged.duration - 0.2).abs() < 1e-5);
            } else {
                assert!((decoded.duration - original.duration).abs() < 1e-5);
            }
            for source_event in &original.events {
                assert!(
                    decoded
                        .events
                        .iter()
                        .any(|event| event.text == source_event.text
                            && (event.time - source_event.time).abs() < 1e-5)
                        || staged
                            .suppressed_source_events
                            .iter()
                            .any(|event| event.text == source_event.text
                                && (event.time - f64::from(source_event.time)).abs() < 1e-5)
                );
            }
            let fire = decoded
                .events
                .iter()
                .filter(|event| event.text == "weaponFire")
                .collect::<Vec<_>>();
            if matches!(staged.contract.action, SkyrimWeaponAction::CrossbowRelease) {
                assert_eq!(fire.len(), 1);
                assert!((f64::from(fire[0].time) - staged.weapon_fire_time.unwrap()).abs() < 1e-5);
            } else {
                assert!(fire.is_empty());
                assert!(staged.weapon_fire_time.is_none());
            }
            for channel in &decoded.channels {
                assert!(
                    channel
                        .translations
                        .iter()
                        .chain(&channel.scales)
                        .all(|key| key.value.iter().all(|value| value.is_finite()))
                );
                assert!(channel.rotations.iter().all(|key| {
                    key.value.iter().all(|value| value.is_finite())
                        && (key.value.iter().map(|value| value * value).sum::<f32>() - 1.0).abs()
                            < 1e-3
                }));
            }
            assert!(
                staged
                    .source_float_tracks
                    .iter()
                    .all(|track| !track.slot_name.is_empty() && !track.values.is_empty())
            );
            eprintln!(
                "{}: {} target tracks; {} source visibility tracks retained in staging; {} unmapped source bones",
                staged.contract.source_path,
                decoded.channels.len(),
                staged.source_float_tracks.len(),
                staged.unmapped_source_bones.len()
            );
        }
    }
}

#[test]
#[ignore = "requires extracted Skyrim crossbow actors and FO4 human weapon behavior graphs"]
fn skyrim_crossbow_actor_profiles_connect_shot_completion_to_real_reload_and_roundtrip() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let source_root = root.join("extracted/skyrimse/meshes");
    let target_root = root.join("extracted/fo4/meshes");
    for first in [false, true] {
        let actor_root = if first {
            "Actors/Character/_1stPerson"
        } else {
            "Actors/Character"
        };
        let stage = |leaf: &str| {
            let contract =
                actor_clip_contract(&format!("{actor_root}/Animations/DLC01/{leaf}.hkx")).unwrap();
            stage_actor_clip(
                contract.clone(),
                if matches!(
                    contract.action,
                    SkyrimWeaponAction::CrossbowRelease | SkyrimWeaponAction::CrossbowReload
                ) {
                    SkyrimActorConsumer::Fo4CrossbowGun
                } else {
                    SkyrimActorConsumer::SourceAnnotations
                },
                &std::fs::read(source_root.join(&contract.source_path)).unwrap(),
                &std::fs::read(source_root.join(&contract.source_skeleton_path)).unwrap(),
                &std::fs::read(target_root.join(&contract.target_skeleton_path)).unwrap(),
            )
            .unwrap()
        };
        let fire = stage("Crossbow_Release");
        let reload = stage("Crossbow_Reload");
        let donor_path = format!(
            "{actor_root}/Behaviors/{}.hkx",
            if first {
                "GunBehavior"
            } else {
                "WeaponBehavior"
            }
        );
        let donor = std::fs::read(target_root.join(donor_path)).unwrap();
        let mut support: Vec<_> = [
            "Crossbow_IdleHeld",
            "Crossbow_IdleDrawn",
            "SneakCrossbow_IdleDrawn",
            "Crossbow_Equip",
        ]
        .into_iter()
        .map(stage)
        .collect();
        support
            .push(crossbow_graph::stage_source_unequip(&source_root, &target_root, first).unwrap());
        let profile =
            crossbow_graph::stage_crossbow_actor_profile(&donor, fire, reload, support).unwrap();
        assert!(profile.replaced_fire_clips > 0);
        assert!(profile.replaced_reload_clips > 0);
        assert!((profile.clips[0].duration - 0.2).abs() < 1e-5);
        assert_eq!(profile.clips[0].contract.first_person, first);
        let graph = unpack(&profile.graph_bytes).unwrap();
        let doc = roxmltree::Document::parse(&graph).unwrap();
        assert!(
            doc.descendants()
                .any(|node| node.attribute("class") == Some("BSIsActiveModifier"))
        );
        let variables: Vec<_> = doc
            .descendants()
            .find(|node| node.attribute("name") == Some("variableNames"))
            .unwrap()
            .children()
            .filter_map(|node| node.is_element().then(|| node.text()).flatten())
            .collect();
        assert!(variables.contains(&"B21SourceCrossbowActive"));
        for array in ["variableInfos", "wordVariableValues"] {
            assert_eq!(
                doc.descendants()
                    .find(|node| node.attribute("name") == Some(array))
                    .unwrap()
                    .children()
                    .filter(roxmltree::Node::is_element)
                    .count(),
                variables.len()
            );
        }
        for leaf in ["crossbow_idleheld.hkx", "crossbow_idledrawn.hkx"] {
            assert!(doc.descendants().any(|node| {
                node.attribute("name") == Some("animationName")
                    && node
                        .text()
                        .is_some_and(|value| value.to_ascii_lowercase().ends_with(leaf))
            }));
        }
        let parameter = |node: roxmltree::Node<'_, '_>, name: &str| {
            node.children()
                .find(|child| child.attribute("name") == Some(name))
                .and_then(|node| node.text())
                .unwrap_or("")
                .to_string()
        };
        let names: Vec<_> = doc
            .descendants()
            .find(|node| node.attribute("name") == Some("eventNames"))
            .unwrap()
            .children()
            .filter(roxmltree::Node::is_element)
            .filter_map(|node| node.text())
            .collect();
        let mut fire_count = 0;
        let mut reload_count = 0;
        for clip in doc
            .descendants()
            .filter(|node| node.attribute("class") == Some("hkbClipGenerator"))
        {
            let animation = parameter(clip, "animationName").to_ascii_lowercase();
            let action = if animation.ends_with("crossbow_release.hkx") {
                fire_count += 1;
                "reloadStart"
            } else if animation.ends_with("crossbow_reload.hkx") {
                reload_count += 1;
                "reloadEnd"
            } else {
                continue;
            };
            assert!(
                !parameter(clip, "name")
                    .to_ascii_lowercase()
                    .contains("slave")
            );
            assert_eq!(parameter(clip, "mode"), "MODE_SINGLE_PLAY");
            let trigger_id = parameter(clip, "triggers");
            let triggers = doc
                .descendants()
                .find(|node| node.attribute("name") == Some(&trigger_id))
                .unwrap();
            let ids: Vec<_> = triggers
                .descendants()
                .filter(|node| node.attribute("name") == Some("id"))
                .map(|node| names[node.text().unwrap().parse::<usize>().unwrap()])
                .collect();
            assert_eq!(
                ids,
                [
                    if action == "reloadStart" {
                        "BowRelease"
                    } else {
                        "Reload"
                    },
                    "initiateStart",
                    action
                ]
            );
        }
        assert_eq!(fire_count, profile.replaced_fire_clips);
        assert_eq!(reload_count, profile.replaced_reload_clips);
        for (index, staged) in profile.clips.iter().enumerate() {
            let clip =
                havok_native::animation::clip::extract_clip(&unpack(&staged.bytes).unwrap(), None)
                    .unwrap();
            assert_eq!(
                clip.events
                    .iter()
                    .filter(|event| event.text == "weaponFire")
                    .count(),
                usize::from(index == 0)
            );
            assert_eq!(
                clip.events
                    .iter()
                    .filter(|event| event.text == "reloadComplete")
                    .count(),
                usize::from(index == 1)
            );
            if staged.consumer == SkyrimActorConsumer::Fo4CrossbowGun {
                assert!(clip.events.iter().all(|event| !matches!(
                    event.text.as_str(),
                    "arrowAttach" | "arrowRelease" | "bowZoomStop"
                )));
            }
        }
        eprintln!(
            "{}: {} fire slots, {} reload slots; source action split at0.2s",
            profile.graph_path, fire_count, reload_count
        );
    }
}
