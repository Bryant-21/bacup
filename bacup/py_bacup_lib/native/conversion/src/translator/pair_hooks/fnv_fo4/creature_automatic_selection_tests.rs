use super::*;
use crate::phase::animations::weapon_events::WeaponAnimationFields;
use crate::translator::pair_hooks::fnv_fo4::{
    creature_motion::{
        BindingEvidence, BindingSummary, CandidateConfidence, CreatureKfEvidence,
        CreatureMotionFamilyEvidence, NiControllerSequenceEvidence, SequenceCycle, TextKeyEvidence,
    },
    creature_recipe::FamilyRecipeDisposition,
    creature_weapon_attacks::CreatureWeaponAttackEvidence,
};

#[test]
fn exact_weapon_profiles_split_mixed_shots_rates_and_preserve_phase_proof() {
    let rig = RigFamilyKey {
        game: LegacyCreatureGame::Fnv,
        skeleton_path: "creatures/sentrybot/skeleton.nif".into(),
    };
    let candidate = MotionCandidate {
        source_game: LegacyCreatureGame::Fnv,
        source_kf: "creatures/sentrybot/2hrattackspin.kf".into(),
        sequence_index: 0,
        sequence_name: "AttackSpin".into(),
        cycle: SequenceCycle::Clamp,
        looping: false,
        events: vec![],
        idle_claims: vec![],
        root_motion: RootMotionEvidence::None,
        confidence: CandidateConfidence::Low,
        evidence: vec![],
        ambiguous_with: vec![],
        binding: BindingSummary {
            source_skeleton_path: rig.skeleton_path.clone(),
            transform_track_count: 1,
            ordered_transform_targets: vec!["Root".into()],
            float_track_count: 0,
            controller_types: vec![],
            interpolator_types: vec![],
            overlay_targets: vec![],
            required_float_slots: vec![],
            compatibility: super::super::super::creature_motion::BindingCompatibility::Verified,
            compatibility_detail: None,
        },
    };
    let mut motion = CreatureMotionFamilyEvidence {
        rig: rig.clone(),
        referenced_kfs: vec![candidate.source_kf.clone()],
        creature_traits: vec![],
        weapon_attacks: vec![],
        kf_evidence: vec![CreatureKfEvidence {
            source_game: LegacyCreatureGame::Fnv,
            source_kf: candidate.source_kf.clone(),
            idle_claims: vec![],
            sequence: KfParseEvidence::Parsed(NiControllerSequenceEvidence {
                sequence_index: 0,
                name: candidate.sequence_name.clone(),
                cycle: SequenceCycle::Clamp,
                start_time: 0.0,
                stop_time: 4.0,
                frequency: 1.0,
                text_keys: [(0.0, "start"), (1.0, "Fire"), (3.0, "Loop"), (4.0, "end")]
                    .into_iter()
                    .map(|(time, text)| TextKeyEvidence {
                        time,
                        text: text.into(),
                    })
                    .collect(),
                binding: BindingEvidence {
                    source_skeleton_path: rig.skeleton_path.clone(),
                    transform_track_count: 1,
                    float_track_count: 0,
                    controller_types: vec![],
                    interpolator_types: vec![],
                    target_names: vec!["Root".into()],
                    transform_priorities: Default::default(),
                    transform_components: Default::default(),
                    required_float_slots: vec![],
                    compatibility:
                        super::super::super::creature_motion::BindingCompatibility::Verified,
                    compatibility_detail: None,
                },
                root_motion: RootMotionEvidence::None,
            }),
        }],
    };
    for (local, automatic, rate) in [
        (0x100, true, 12.0),
        (0x101, true, 15.0),
        (0x102, false, 1.0),
    ] {
        motion.weapon_attacks.push(CreatureWeaponAttackEvidence {
            source: StableFormKey {
                plugin: "FalloutNV.esm".into(),
                local,
            },
            animation: WeaponAnimationFields {
                animation_type: 5,
                grip_animation: 255,
                attack_animation: 80,
                reload_animation: 0,
                ammo_use: 1,
                automatic,
                per_round_reload: false,
                animation_multiplier: 1.0,
                attack_multiplier: 1.0,
                fire_rate: rate,
                attack_shots_sec: rate,
                reload_time: 0.0,
            },
            embedded_weapon_node: None,
            held_weapon_node: None,
            equipment_group: None,
            equipment_index: None,
            attack_animation_groups: vec![],
        });
    }
    let mut family = CreatureFamilyRecipeJob {
        rig,
        source_plugins: vec![],
        members: vec![],
        actual_root_node: None,
        body_variants: vec![],
        graph_contract: None,
        indexed_motion_evidence: Some(motion),
        motion_evidence: None,
        required_motion: vec![],
        ragdoll: None,
        race_data: None,
        recursive_asset_claims: vec![],
        assets: vec![],
        disposition: FamilyRecipeDisposition::Ready,
    };
    let mut clips = vec![SelectedClip {
        thrown_attack: false,
        candidate,
        role: CreatureClipRole::ProjectileAttack,
        name: "attack".into(),
        state_name: "Attack".into(),
        trigger_event: Some("attackStart".into()),
        target_looping: false,
        locomotion_sample: None,
        source_idle: None,
        source_aim: None,
        source_attack: None,
        attack_blend_only: false,
        source_movement: None,
        automatic_attack: None,
        source_weapon: None,
        equipment_index: None,
    }];
    let mut unarmed_clip = clips[0].clone();
    expand(&family, &mut clips).unwrap();
    assert_eq!(clips.len(), 7);
    let entries = clips
        .iter()
        .filter(|clip| {
            clip.automatic_attack
                .as_ref()
                .is_none_or(|attack| attack.generator.is_attack_entry())
        })
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 3);
    assert_eq!(
        entries
            .iter()
            .map(|clip| clip.trigger_event.as_ref().unwrap())
            .collect::<BTreeSet<_>>()
            .len(),
        3
    );
    assert_eq!(
        entries
            .iter()
            .map(|clip| clip.source_weapon.as_ref().unwrap().local)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([0x100, 0x101, 0x102])
    );
    for selected in &clips {
        let Some(automatic) = &selected.automatic_attack else {
            assert!(!selected.target_looping);
            continue;
        };
        let expected_rate = if automatic.source_weapon.local == 0x100 {
            12.0
        } else {
            15.0
        };
        assert_eq!(automatic.segment.shot_interval_seconds, 1.0 / expected_rate);
        let mut role = CapabilityClipRole {
            equipment_index: None,
            role: selected.role,
            state_name: selected.state_name.clone(),
            clip_name: selected.name.clone(),
            generator: automatic.generator.clone(),
            trigger_event: selected.trigger_event.clone(),
            trigger_aliases: vec![],
            motion: Default::default(),
        };
        let clip = ClipDecl {
            name: selected.name.clone(),
            path: "Animations/Attack.hkx".into(),
            looping: selected.target_looping,
            binding: crate::source_rig::ClipBinding {
                skeleton_path: "Skeleton.hkx".into(),
                original_skeleton_name: "Source".into(),
                declared_transform_tracks: 1,
                transform_track_to_bone_indices: vec![0],
                declared_float_tracks: 0,
                float_track_to_float_slot_indices: vec![],
            },
        };
        assert!(
            validate_automatic_phase_projection(&family, &selected.candidate, &role, &clip)
                .unwrap()
        );
        role.state_name.push_str("_wrong_weapon");
        assert!(
            validate_automatic_phase_projection(&family, &selected.candidate, &role, &clip)
                .is_err()
        );
    }
    unarmed_clip.role = CreatureClipRole::MeleeAttack;
    unarmed_clip.candidate.source_kf = "creatures/source/h2hattackright_a.kf".into();
    unarmed_clip.candidate.sequence_name = "AttackRight_A".into();
    let motion = family.indexed_motion_evidence.as_mut().unwrap();
    motion.kf_evidence[0].source_kf = unarmed_clip.candidate.source_kf.clone();
    if let KfParseEvidence::Parsed(sequence) = &mut motion.kf_evidence[0].sequence {
        sequence.name = unarmed_clip.candidate.sequence_name.clone();
    }
    motion.weapon_attacks.truncate(2);
    for weapon in &mut motion.weapon_attacks {
        weapon.animation.animation_type = 0;
        weapon.animation.attack_animation = 32;
        weapon.animation.automatic = false;
    }
    let mut melee = vec![unarmed_clip];
    expand(&family, &mut melee).unwrap();
    assert_eq!(melee.len(), 3);
    assert_eq!(
        melee
            .iter()
            .filter(|clip| clip.source_weapon.is_none())
            .count(),
        1
    );
    assert_eq!(
        melee
            .iter()
            .filter_map(|clip| clip.source_weapon.as_ref().map(|source| source.local))
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([0x100, 0x101])
    );
    assert!(melee.iter().all(|clip| clip.automatic_attack.is_none()));
}
