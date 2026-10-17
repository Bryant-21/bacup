use super::{AnimationClip, AnimationEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FnvCreatureAttackKind {
    Melee,
    Ranged,
    Thrown,
}

pub(super) fn lower(clip: &mut AnimationClip, kind: FnvCreatureAttackKind) {
    let mut lowered = Vec::new();
    for event in &clip.events {
        for text in event
            .text
            .lines()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            let mut emit = |text: &str| {
                lowered.push(AnimationEvent {
                    time: event.time,
                    source_time: event.source_time,
                    text: text.to_string(),
                });
            };
            match (kind, text.to_ascii_lowercase().as_str()) {
                (FnvCreatureAttackKind::Melee, "start") => emit("preHitFrame"),
                (FnvCreatureAttackKind::Melee, "hit" | "fire") => {
                    // FO4 requires attack state 2 before HitFrame; each source contact is a swing.
                    emit("weaponSwing");
                    emit("HitFrame");
                }
                (FnvCreatureAttackKind::Melee, "end") => emit("attackStop"),
                (FnvCreatureAttackKind::Melee, key) if key.starts_with("a:") => {
                    emit("attackWinStart");
                }
                (FnvCreatureAttackKind::Ranged, "hit" | "fire") => emit("weaponFire"),
                (FnvCreatureAttackKind::Thrown, "release") => emit("weaponFire"),
                (FnvCreatureAttackKind::Thrown, "attach") => emit("weaponAttach"),
                (FnvCreatureAttackKind::Thrown, "detach") => emit("weaponDetach"),
                _ => emit(text),
            }
        }
    }
    lowered.sort_by(|left, right| left.time.total_cmp(&right.time));
    clip.events = lowered;
}

pub(super) fn set_equipment_index(clip: &mut AnimationClip, index: u8) {
    for event in &mut clip.events {
        let name = event.text.split('.').next().unwrap_or(&event.text);
        if matches!(name, "weaponFire" | "weaponAttach" | "weaponDetach" | "weaponSwing" | "HitFrame") {
            event.text = format!("{name}.{index}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(keys: &[(f64, &str)]) -> AnimationClip {
        AnimationClip {
            events: keys
                .iter()
                .map(|(time, text)| AnimationEvent {
                    time: *time,
                    source_time: *time + 3.0,
                    text: (*text).to_string(),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn melee_contacts_retain_timestamps_and_enter_swing_before_each_hit() {
        let mut clip = clip(&[
            (0.0, "start"),
            (0.3, "Hit"),
            (0.7, "Hit"),
            (0.9, "a:R"),
            (1.0, "end"),
        ]);
        lower(&mut clip, FnvCreatureAttackKind::Melee);
        assert_eq!(
            clip.events
                .iter()
                .map(|event| (event.time, event.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (0.0, "preHitFrame"),
                (0.3, "weaponSwing"),
                (0.3, "HitFrame"),
                (0.7, "weaponSwing"),
                (0.7, "HitFrame"),
                (0.9, "attackWinStart"),
                (1.0, "attackStop"),
            ]
        );
        assert_eq!(clip.events[2].source_time, 3.3);
    }

    #[test]
    fn ranged_contacts_do_not_enter_melee_attack_states() {
        let mut clip = clip(&[
            (0.2, "Hit\r\nSound: Spit"),
            (0.8, "Fire"),
            (0.9, "a:R"),
            (1.0, "end"),
        ]);
        lower(&mut clip, FnvCreatureAttackKind::Ranged);
        assert_eq!(
            clip.events
                .iter()
                .filter(|event| event.text == "weaponFire")
                .map(|event| event.time)
                .collect::<Vec<_>>(),
            vec![0.2, 0.8]
        );
        assert!(clip.events.iter().any(|event| event.text == "Sound: Spit"));
        assert!(!clip.events.iter().any(|event| matches!(
            event.text.as_str(),
            "weaponSwing" | "HitFrame" | "attackWinStart" | "attackStop"
        )));
    }

    #[test]
    fn thrown_release_and_reattach_use_source_times_and_explicit_slot() {
        let mut clip = clip(&[
            (0.0, "start"),
            (1.333333, "Hold"),
            (1.6, "Release\r\nSound: Throw"),
            (2.333333, "Attach"),
            (2.5, "end"),
        ]);
        lower(&mut clip, FnvCreatureAttackKind::Thrown);
        set_equipment_index(&mut clip, 3);
        assert_eq!(
            clip.events.iter().map(|event| (event.time, event.text.as_str())).collect::<Vec<_>>(),
            vec![(0.0, "start"), (1.333333, "Hold"), (1.6, "weaponFire.3"),
                (1.6, "Sound: Throw"), (2.333333, "weaponAttach.3"), (2.5, "end")]
        );
        assert_eq!(clip.events[2].source_time, 4.6);
        assert_eq!(clip.events[4].source_time, 2.333333 + 3.0);
    }

    #[test]
    fn thrown_actions_do_not_fire_from_gun_or_melee_contact_keys() {
        let mut clip = clip(&[(0.1, "Hit\nFire"), (0.2, "Detach"), (0.3, "Release")]);
        lower(&mut clip, FnvCreatureAttackKind::Thrown);
        set_equipment_index(&mut clip, 2);
        assert_eq!(clip.events.iter().map(|event| event.text.as_str()).collect::<Vec<_>>(),
            vec!["Hit", "Fire", "weaponDetach.2", "weaponFire.2"]);
    }

    #[test]
    fn equipment_index_retargets_only_native_weapon_events() {
        let mut clip = clip(&[(0.1, "weaponFire.2"), (0.2, "weaponAttach.2"),
            (0.3, "weaponDetach.2"), (0.4, "SoundPlay.weaponFire"), (0.5, "weaponSwing"), (0.5, "HitFrame")]);
        set_equipment_index(&mut clip, 4);
        assert_eq!(clip.events.iter().map(|event| event.text.as_str()).collect::<Vec<_>>(),
            vec!["weaponFire.4", "weaponAttach.4", "weaponDetach.4", "SoundPlay.weaponFire", "weaponSwing.4", "HitFrame.4"]);
    }

    #[test]
    #[ignore = "requires MODKIT_FNV_CREATURE_THROW_INPUTS source KF corpus manifest"]
    fn source_fnv_thrown_annotations_corpus() {
        let path = std::env::var("MODKIT_FNV_CREATURE_THROW_INPUTS").unwrap();
        let rows: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let rows = rows.as_array().unwrap();
        assert!(!rows.is_empty());
        for row in rows {
            let path = row["source_path"].as_str().unwrap();
            let source = super::super::parse_kf_sequence_bytes(
                &std::fs::read(path).unwrap(), path,
                row["sequence_index"].as_u64().unwrap() as usize,
            ).unwrap_or_else(|error| panic!("{path}: {error}"));
            let mut converted = source.clone();
            lower(&mut converted, FnvCreatureAttackKind::Thrown);
            set_equipment_index(&mut converted, 2);
            for (source_key, target_key) in [
                ("release", "weaponFire.2"), ("attach", "weaponAttach.2"), ("hold", "Hold"),
            ] {
                let mut expected = source.events.iter().flat_map(|event| {
                    event.text.lines().filter(move |text| text.trim().eq_ignore_ascii_case(source_key))
                        .map(move |_| (event.time, event.source_time))
                }).collect::<Vec<_>>();
                expected.sort_by(|left, right| left.0.total_cmp(&right.0));
                assert!(!expected.is_empty(), "{path}: missing {source_key}");
                let actual = converted.events.iter().filter(|event| event.text.eq_ignore_ascii_case(target_key))
                    .map(|event| (event.time, event.source_time)).collect::<Vec<_>>();
                assert_eq!(actual, expected, "{path}: {source_key}");
            }
            assert!(!converted.events.iter().any(|event| matches!(
                event.text.as_str(), "HitFrame" | "weaponSwing" | "attackStop"
            )), "{path}");
        }
        eprintln!("verified {} source thrown attack sequences", rows.len());
    }

    #[test]
    #[ignore = "requires MODKIT_FNV_CREATURE_ATTACK_INPUTS source KF corpus manifest"]
    fn source_fnv_attack_annotations_corpus() {
        let path = std::env::var("MODKIT_FNV_CREATURE_ATTACK_INPUTS").unwrap();
        let rows: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let rows = rows.as_array().unwrap();
        assert!(!rows.is_empty());
        for row in rows {
            let path = row["source_path"].as_str().unwrap();
            let source = super::super::parse_kf_sequence_bytes(
                &std::fs::read(path).unwrap(),
                path,
                row["sequence_index"].as_u64().unwrap() as usize,
            )
            .unwrap_or_else(|error| panic!("{path}: {error}"));
            let mut contacts = source
                .events
                .iter()
                .flat_map(|event| {
                    event
                        .text
                        .lines()
                        .filter(|text| {
                            matches!(text.trim().to_ascii_lowercase().as_str(), "hit" | "fire")
                        })
                        .map(move |_| event.time)
                })
                .collect::<Vec<_>>();
            contacts.sort_by(f64::total_cmp);
            assert!(!contacts.is_empty(), "{path}");
            for (kind, target) in [
                (FnvCreatureAttackKind::Melee, "HitFrame"),
                (FnvCreatureAttackKind::Ranged, "weaponFire"),
            ] {
                let mut clip = source.clone();
                lower(&mut clip, kind);
                assert_eq!(
                    clip.events
                        .iter()
                        .filter(|event| event.text == target)
                        .map(|event| event.time)
                        .collect::<Vec<_>>(),
                    contacts,
                    "{path}"
                );
                if kind == FnvCreatureAttackKind::Melee {
                    for (index, event) in clip
                        .events
                        .iter()
                        .enumerate()
                        .filter(|(_, event)| event.text == "HitFrame")
                    {
                        assert_eq!(clip.events[index - 1].text, "weaponSwing", "{path}");
                        assert_eq!(clip.events[index - 1].time, event.time, "{path}");
                    }
                }
            }
        }
        eprintln!("verified {} source attack sequences", rows.len());
    }
}
