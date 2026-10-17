pub(crate) fn native_actor_value_binding(name: &str) -> Option<(&'static str, &'static str)> {
    match name.to_ascii_lowercase().as_str() {
        "aggression" => Some(("Aggression", "0002BC:Fallout4.esm")),
        "health" => Some(("Health", "0002D4:Fallout4.esm")),
        "variable01" => Some(("Variable01", "000301:Fallout4.esm")),
        "perceptioncondition" => Some(("PerceptionCondition", "00036C:Fallout4.esm")),
        "endurancecondition" => Some(("EnduranceCondition", "00036D:Fallout4.esm")),
        "leftattackcondition" => Some(("LeftAttackCondition", "00036E:Fallout4.esm")),
        "rightattackcondition" => Some(("RightAttackCondition", "00036F:Fallout4.esm")),
        "leftmobilitycondition" => Some(("LeftMobilityCondition", "000370:Fallout4.esm")),
        "rightmobilitycondition" => Some(("RightMobilityCondition", "000371:Fallout4.esm")),
        "braincondition" => Some(("BrainCondition", "000372:Fallout4.esm")),
        "sneak" => Some(("Sneak", "00037F:Fallout4.esm")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::native_actor_value_binding;

    #[test]
    fn legacy_creature_av_bindings_use_registered_native_identities() {
        let registrations = [
            ("Aggression", 0),
            ("Health", 24),
            ("Variable01", 69),
            ("PerceptionCondition", 176),
            ("EnduranceCondition", 177),
            ("LeftAttackCondition", 178),
            ("RightAttackCondition", 179),
            ("LeftMobilityCondition", 180),
            ("RightMobilityCondition", 181),
            ("BrainCondition", 182),
            ("Sneak", 195),
        ];
        for (name, legacy_index) in registrations {
            let expected = format!("{:06X}:Fallout4.esm", legacy_index + 700);
            assert_eq!(
                native_actor_value_binding(name),
                Some((name, expected.as_str()))
            );
            assert_eq!(
                native_actor_value_binding(&name.to_ascii_lowercase()),
                Some((name, expected.as_str()))
            );
        }
        for unsupported in [
            "Repair",
            "Science",
            "CraftingAbilityScience",
            "PowerArmorHeadCondition",
        ] {
            assert_eq!(native_actor_value_binding(unsupported), None);
        }
    }
}
