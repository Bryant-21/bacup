use super::creature_motion::TextKeyEvidence;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SourceNextAttack {
    pub group: u8,
    pub time: f64,
}

pub(crate) fn source_default_attack_group(configured_group: u8) -> u8 {
    if configured_group == 255 {
        32
    } else {
        configured_group
    }
}

pub(crate) fn first_source_next_attack(
    text_keys: &[TextKeyEvidence],
    configured_group: u8,
) -> Option<SourceNextAttack> {
    // AnimData::GetNextWeaponSequenceKey (FNV 0x495E40) returns the first
    // prefix match in source array order, even when its suffix is unrecognized.
    text_keys.iter().find_map(|key| {
        let text = key.text.as_bytes();
        if !text.get(..2)?.eq_ignore_ascii_case(b"a:") {
            return None;
        }
        let group = match text.get(2).map(u8::to_ascii_lowercase) {
            Some(b'l') => 26,
            Some(b'3') => 38,
            Some(b'4') => 44,
            Some(b'5') => 50,
            Some(b'6') => 56,
            Some(b'7') => 62,
            Some(b'8') => 68,
            _ => source_default_attack_group(configured_group),
        };
        Some(SourceNextAttack {
            group,
            time: key.time,
        })
    })
}

pub(crate) fn next_source_movement_attack(
    text_keys: &[TextKeyEvidence],
    current_time: f64,
    configured_group: u8,
) -> u8 {
    if configured_group != 255 {
        return configured_group;
    }
    // FNV 0x495F50 returns the first matching movement key at or after the
    // current sequence time; the opening selector only recognizes its l suffix.
    let next_key = text_keys.iter().find(|key| {
        key.time >= current_time
            && key
                .text
                .as_bytes()
                .get(..2)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"m:"))
    });
    if next_key.is_some_and(|key| {
        key.text
            .as_bytes()
            .get(2)
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case(&b'l'))
    }) {
        26
    } else {
        32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(time: f64, text: &str) -> TextKeyEvidence {
        TextKeyEvidence {
            time,
            text: text.into(),
        }
    }

    #[test]
    fn first_source_key_wins_even_if_a_later_key_has_an_earlier_time() {
        let keys = [key(0.2, "Hit"), key(1.2, "a:3"), key(0.8, "a:l")];
        assert_eq!(
            first_source_next_attack(&keys, 255),
            Some(SourceNextAttack {
                group: 38,
                time: 1.2,
            })
        );
    }

    #[test]
    fn native_prefix_matching_does_not_trim_or_split_keys() {
        let keys = [
            key(0.1, " a:3"),
            key(0.2, "Hit\na:4"),
            key(0.9, "A:L suffix"),
        ];
        assert_eq!(
            first_source_next_attack(&keys, 255),
            Some(SourceNextAttack {
                group: 26,
                time: 0.9,
            })
        );
    }

    #[test]
    fn right_missing_and_unknown_suffixes_use_the_weapon_default() {
        for text in ["a:r", "a:", "a:x"] {
            let keys = [key(0.5, text), key(0.6, "a:3")];
            assert_eq!(first_source_next_attack(&keys, 44).unwrap().group, 44);
            assert_eq!(first_source_next_attack(&keys, 255).unwrap().group, 32);
        }
        assert_eq!(first_source_next_attack(&[key(0.5, "Hit")], 255), None);
    }

    #[test]
    fn source_next_attack_groups_follow_the_native_switch() {
        for (suffix, group) in [
            ('l', 26),
            ('3', 38),
            ('4', 44),
            ('5', 50),
            ('6', 56),
            ('7', 62),
            ('8', 68),
        ] {
            assert_eq!(
                first_source_next_attack(&[key(0.4, &format!("a:{suffix}"))], 114)
                    .unwrap()
                    .group,
                group
            );
        }
    }

    #[test]
    fn default_opening_follows_the_next_movement_key_through_the_cycle() {
        let keys = [key(0.4, "m:R"), key(0.8, "m:L")];
        for (time, group) in [(0.0, 32), (0.4, 32), (0.5, 26), (0.8, 26), (0.9, 32)] {
            assert_eq!(next_source_movement_attack(&keys, time, 255), group);
        }
    }

    #[test]
    fn configured_weapons_bypass_movement_phase_and_suffix_spaces_are_literal() {
        let keys = [key(0.4, "M: L"), key(0.8, "m:l")];
        assert_eq!(next_source_movement_attack(&keys, 0.0, 255), 32);
        assert_eq!(next_source_movement_attack(&keys, 0.5, 255), 26);
        assert_eq!(next_source_movement_attack(&keys, 0.5, 44), 44);
    }

    #[test]
    fn movement_key_selection_preserves_source_array_order() {
        let keys = [key(0.8, "m:L"), key(0.4, "m:R")];
        assert_eq!(next_source_movement_attack(&keys, 0.0, 255), 26);
        assert_eq!(next_source_movement_attack(&[], 0.0, 255), 32);
    }
}
