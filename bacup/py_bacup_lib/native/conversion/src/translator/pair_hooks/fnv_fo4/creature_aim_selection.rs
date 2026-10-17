use super::super::creature_motion::{
    CandidateConfidence, CreatureKfEvidence, NiControllerSequenceEvidence, RoleSignalProvenance,
    RoleSignalSource, binding_summary, event_provenance,
};
use super::*;
use crate::source_rig::CapabilityBlenderChild;

const BANKS: [&str; 11] = [
    "h2h", "1hm", "2hm", "1hp", "2hr", "2ha", "2hh", "2hl", "1gt", "1md", "1lm",
];

fn weapon_bank(animation_type: u32) -> Option<i32> {
    Some(match animation_type {
        0 => 1,
        1 => 2,
        2 => 3,
        3 | 4 => 4,
        5 | 7 => 5,
        6 => 6,
        8 => 7,
        9 => 8,
        10 | 13 => 9,
        12 => 10,
        11 => 11,
        _ => return None,
    })
}

fn aim_identity(path: &str, sequence: &str) -> Option<(i32, u8)> {
    let leaf = path.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
    let stem = leaf.strip_suffix(".kf")?;
    let (bank, name) = BANKS
        .iter()
        .enumerate()
        .find_map(|(index, prefix)| {
            stem.strip_prefix(prefix)
                .map(|name| (index as i32 + 1, name))
        })
        .unwrap_or((1, stem));
    let channel = match name {
        "aim" => 4,
        "aimup" => 5,
        "aimdown" => 6,
        _ => return None,
    };
    sequence
        .eq_ignore_ascii_case(name)
        .then_some((bank, channel))
}

fn candidate(
    entry: &CreatureKfEvidence,
    sequence: &NiControllerSequenceEvidence,
) -> MotionCandidate {
    MotionCandidate {
        source_game: entry.source_game,
        source_kf: entry.source_kf.clone(),
        sequence_index: sequence.sequence_index,
        sequence_name: sequence.name.clone(),
        cycle: sequence.cycle,
        looping: sequence.cycle.loops(),
        events: event_provenance(&sequence.text_keys),
        idle_claims: entry.idle_claims.clone(),
        binding: binding_summary(&sequence.binding),
        root_motion: sequence.root_motion.clone(),
        confidence: CandidateConfidence::High,
        evidence: vec![RoleSignalProvenance {
            source: RoleSignalSource::SequenceName,
            detail: "exact source Aim group and weapon hand bank".into(),
            event_time: None,
        }],
        ambiguous_with: Vec::new(),
    }
}

pub(crate) fn candidates(family: &CreatureFamilyRecipeJob, bank: i32) -> Vec<MotionCandidate> {
    family
        .indexed_motion_evidence
        .iter()
        .flat_map(|motion| &motion.kf_evidence)
        .filter_map(|entry| {
            let KfParseEvidence::Parsed(sequence) = &entry.sequence else {
                return None;
            };
            aim_identity(&entry.source_kf, &sequence.name)
                .filter(|(source_bank, _)| *source_bank == bank)
                .map(|_| candidate(entry, sequence))
        })
        .collect()
}

pub(super) fn attach(
    family: &CreatureFamilyRecipeJob,
    clips: &mut Vec<SelectedClip>,
) -> Result<(), String> {
    let Some(motion) = &family.indexed_motion_evidence else {
        return Ok(());
    };
    let mut events = BTreeMap::<i32, BTreeSet<String>>::new();
    for clip in clips
        .iter()
        .filter(|clip| !clip.attack_blend_only && clip.source_attack.is_some())
    {
        let bank = if let Some(source) = &clip.source_weapon {
            motion
                .weapon_attacks
                .iter()
                .find(|weapon| &weapon.source == source)
                .and_then(|weapon| weapon_bank(weapon.animation.animation_type))
        } else {
            Some(1)
        };
        if let (Some(bank), Some(event)) = (bank, &clip.trigger_event) {
            events.entry(bank).or_default().insert(event.clone());
        }
    }
    let mut sequences = BTreeMap::new();
    for entry in &motion.kf_evidence {
        let KfParseEvidence::Parsed(sequence) = &entry.sequence else {
            continue;
        };
        let Some(identity) = aim_identity(&entry.source_kf, &sequence.name) else {
            continue;
        };
        if !events.contains_key(&identity.0) {
            continue;
        }
        if let Some((previous, _)) = sequences.insert(identity, (entry, sequence)) {
            if canonical_mesh_path(&previous.source_kf) != canonical_mesh_path(&entry.source_kf) {
                return Err(format!(
                    "source aim bank {} channel {} has ambiguous owned sequences",
                    identity.0, identity.1
                ));
            }
        }
    }
    for (bank, events) in events {
        let Some(&(center, sequence)) = sequences.get(&(bank, 4)) else {
            continue;
        };
        let mut samples = Vec::new();
        let mut additions = Vec::new();
        for (channel, pitch) in [
            (5, -std::f32::consts::FRAC_PI_2),
            (4, 0.0),
            (6, std::f32::consts::FRAC_PI_2),
        ] {
            let Some(&(entry, sequence)) = sequences.get(&(bank, channel)) else {
                continue;
            };
            let name = format!("source_aim_{bank}_{channel}");
            let mut used_events = BTreeSet::new();
            add_selected_clip(
                &mut additions,
                &mut used_events,
                candidate(entry, sequence),
                CreatureClipRole::WeaponAim,
                &name,
                &format!("SourceAim{bank}"),
                false,
                false,
            )?;
            additions.last_mut().unwrap().attack_blend_only = channel != 4;
            samples.push(CapabilityBlenderChild {
                clip_name: name,
                weight_bits: pitch.to_bits(),
                world_from_model_weight_bits: 1.0_f32.to_bits(),
            });
        }
        if !sequence.cycle.loops() {
            return Err(format!(
                "source aim {} must retain its source looping cycle",
                center.source_kf
            ));
        }
        additions
            .iter_mut()
            .find(|clip| !clip.attack_blend_only)
            .unwrap()
            .source_aim = Some(CapabilityRoleGenerator::SourceAim {
            bank_index: bank,
            attack_events: events.into_iter().collect(),
            aim_children: samples,
        });
        clips.extend(additions);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_aim_banks_do_not_select_idle_attack_or_another_bank() {
        assert_eq!(
            aim_identity("creatures/robot/1hpAim.kf", "AIM"),
            Some((4, 4))
        );
        assert_eq!(aim_identity("2hlAimUp.kf", "AimUp"), Some((8, 5)));
        assert_eq!(aim_identity("2hrAimDown.kf", "AimDown"), Some((5, 6)));
        assert_eq!(aim_identity("Aim.kf", "Aim"), Some((1, 4)));
        for path in [
            "mtIdle.kf",
            "1hpAttackLeft.kf",
            "1hpAimIS.kf",
            "1hpAim_extra.kf",
            "sneak1hpAim.kf",
        ] {
            assert_eq!(aim_identity(path, "Aim"), None);
        }
        assert_eq!(aim_identity("1hpAim.kf", "Idle"), None);
    }

    #[test]
    fn source_weapon_types_share_only_their_native_hand_bank() {
        assert_eq!(weapon_bank(3), weapon_bank(4));
        assert_eq!(weapon_bank(5), weapon_bank(7));
        assert_eq!(weapon_bank(10), weapon_bank(13));
        assert_eq!(weapon_bank(9), Some(8));
        assert_eq!(weapon_bank(12), Some(10));
        assert_eq!(weapon_bank(11), Some(11));
        assert_eq!(weapon_bank(99), None);
    }

    #[test]
    fn owned_aim_selection_retains_source_channels_priorities_and_candidate_hashes() {
        let interner = StringInterner::new();
        let ledger = super::super::super::creature_recipe::tests::source_rig_bridge_test_ledger(
            &interner,
            LegacyCreatureGame::Fnv,
        );
        let mut family = ledger.families[0].clone();
        let attack = family
            .required_motion
            .iter()
            .find(|motion| motion.role == MotionRole::MeleeAttack)
            .unwrap()
            .candidates[0]
            .clone();
        let mut clips = Vec::new();
        add_selected_clip(
            &mut clips,
            &mut BTreeSet::new(),
            attack,
            CreatureClipRole::MeleeAttack,
            "attack",
            "Attack",
            true,
            true,
        )
        .unwrap();
        clips[0].source_attack = Some(crate::source_rig::SourceAttackContract {
            gate_variable: "B21_TestAttack".into(),
            group: 32,
            opening_group: 32,
            opening_variable: None,
            window: None,
            aim_children: Vec::new(),
        });
        let motion = family.indexed_motion_evidence.as_mut().unwrap();
        let source = motion.kf_evidence[0].clone();
        for (prefix, name) in [
            ("h2h", "Aim"),
            ("h2h", "AimUp"),
            ("h2h", "AimDown"),
            ("2hl", "Aim"),
        ] {
            let mut entry = source.clone();
            entry.source_kf = format!("creatures/test/{prefix}{name}.kf");
            let KfParseEvidence::Parsed(sequence) = &mut entry.sequence else {
                panic!()
            };
            sequence.name = name.into();
            sequence.cycle = super::super::super::creature_motion::SequenceCycle::Loop;
            sequence
                .binding
                .transform_priorities
                .insert("Weapon".into(), 50);
            motion.kf_evidence.push(entry);
        }
        attach(&family, &mut clips).unwrap();
        let selected = &clips[1..];
        assert_eq!(selected.len(), 3);
        let center = selected
            .iter()
            .find(|clip| !clip.attack_blend_only)
            .unwrap();
        let Some(CapabilityRoleGenerator::SourceAim {
            bank_index,
            attack_events,
            aim_children,
        }) = &center.source_aim
        else {
            panic!()
        };
        assert_eq!(*bank_index, 1);
        assert_eq!(
            attack_events,
            &vec![clips[0].trigger_event.clone().unwrap()]
        );
        assert_eq!(
            aim_children
                .iter()
                .map(|child| f32::from_bits(child.weight_bits))
                .collect::<Vec<_>>(),
            [
                -std::f32::consts::FRAC_PI_2,
                0.0,
                std::f32::consts::FRAC_PI_2
            ]
        );
        let channels = creature_channel_selection::collect(&family, selected, &[]).unwrap();
        assert_eq!(
            channels.iter().map(|clip| clip.channel).collect::<Vec<_>>(),
            [5, 4, 6]
        );
        assert!(
            channels
                .iter()
                .all(|clip| clip.transform_priorities.get("Weapon") == Some(&50))
        );
        let source_hashes = candidates(&family, 1)
            .iter()
            .map(motion_candidate_hash)
            .collect::<BTreeSet<_>>();
        assert!(
            selected
                .iter()
                .all(|clip| source_hashes.contains(&motion_candidate_hash(&clip.candidate)))
        );
    }
}
