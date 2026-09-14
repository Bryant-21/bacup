use serde::Serialize;

use super::super::quest::TranslatedQuest;
use super::super::script_synthesizer::TranslatedScript;

const HOSTAGE_ACTOR_SCPT: u32 = 0x123191;
const VTECHATTICUP_QUST: u32 = 0x11F935;
const POWER_ARMOR_QUST: u32 = 0x06136D;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SliceIntegrationRequirement {
    pub source_record: &'static str,
    pub target_record_type: &'static str,
    pub operation: &'static str,
    pub helper_class_suffix: &'static str,
    pub binding: &'static str,
}

pub fn compatibility_sources(
    scripts: &[TranslatedScript],
    quests: &[TranslatedQuest],
) -> Vec<(String, String)> {
    let prefix = discover_prefix(scripts, quests).unwrap_or("B21");
    let mut sources = Vec::new();
    if contains_local(
        quests.iter().map(|quest| quest.source_form_key.as_str()),
        VTECHATTICUP_QUST,
    ) || contains_local(
        quests.iter().map(|quest| quest.source_form_key.as_str()),
        POWER_ARMOR_QUST,
    ) || contains_local(
        scripts.iter().map(|script| script.source_form_key.as_str()),
        HOSTAGE_ACTOR_SCPT,
    ) {
        let class = format!("{prefix}_FnvSliceCompat");
        sources.push((class.clone(), slice_compat_source(&class)));
    }
    sources
}

pub fn integration_requirements() -> Vec<SliceIntegrationRequirement> {
    vec![
        SliceIntegrationRequirement {
            source_record: "PACK 1231B6 TecMineHostageEscape",
            target_record_type: "QUST unfilled data alias + SCPT VMAD",
            operation: "set ALPC to the mapped PACK; bind ReferenceAlias property; ApplyToRef(actor) then EvaluatePackage(true)",
            helper_class_suffix: "",
            binding: "create the alias on mapped QUST 11F935, bind TecMineHostageEscapeData, ApplyToRef(Self); TecMineHostage actor PSC owns OnPackageEnd and disables only after mapped PACK 1231B6 ends",
        },
        SliceIntegrationRequirement {
            source_record: "PACK 13289E TechaticupNCRRenoldsDialoguePackage",
            target_record_type: "QUST unfilled data alias + SCPT VMAD",
            operation: "set ALPC to the mapped PACK; bind ReferenceAlias property; ApplyToRef(actor) then EvaluatePackage(true)",
            helper_class_suffix: "",
            binding: "create the alias on mapped QUST 11F935, bind TechaticupNCRRenoldsDialoguePackageData, ApplyToRef(NVTecNCRRenoldsREF); preserve lowered GetStage(QUST 11F935) < 10 CTDA and owner quest",
        },
        SliceIntegrationRequirement {
            source_record: "REPU 0F43DE RepNVNCR",
            target_record_type: "QUST compatibility state (REPU projection, never FACT)",
            operation: "map reputation tier 1/2/3/4/5 to point delta 1/2/4/7/12 and update fame or infamy",
            helper_class_suffix: "_FnvSliceCompat",
            binding: "bind FNVSliceCompat to mapped QUST 11F935; future RepNVNCR conditions query GetRepNVNCRFame/GetRepNVNCRInfamy",
        },
        SliceIntegrationRequirement {
            source_record: "DIAL 0000C8 GREETING referenced by SCPT 123191 SayTo",
            target_record_type: "generated KYWD + projected INFO responses",
            operation: "when the audited SayTo GREETING reference is consumed, generate the dedicated Keyword and bind it to SayCustom",
            helper_class_suffix: "",
            binding: "TecMineHostageFreedGreeting is a generated Keyword property; project only the referenced three hostage INFO responses and call Self.SayCustom",
        },
        SliceIntegrationRequirement {
            source_record: "QUST 11F935 VTechatticup",
            target_record_type: "QUST VMAD",
            operation: "attach compatibility quest script",
            helper_class_suffix: "_FnvSliceCompat",
            binding: "actor and stage fragments bind the mapped QUST as FNVSliceCompat",
        },
        SliceIntegrationRequirement {
            source_record: "QUST 06136D FreeformPowerArmor",
            target_record_type: "QUST VMAD",
            operation: "attach compatibility quest script",
            helper_class_suffix: "_FnvSliceCompat",
            binding: "stage 100 sets PCCanUsePowerArmor after mapped AddPerk",
        },
    ]
}

fn slice_compat_source(class_name: &str) -> String {
    format!(
        "ScriptName {class_name} extends Quest\n\nInt Property RepNVNCRFame Auto\nInt Property RepNVNCRInfamy Auto\nBool Property PCCanUsePowerArmor Auto\n\nInt Function ReputationBumpForTier(Int aiTier)\n    If aiTier == 1\n        Return 1\n    ElseIf aiTier == 2\n        Return 2\n    ElseIf aiTier == 3\n        Return 4\n    ElseIf aiTier == 4\n        Return 7\n    ElseIf aiTier == 5\n        Return 12\n    EndIf\n    Return 0\nEndFunction\n\nFunction ModRepNVNCR(Int aiFameMode, Int aiTier)\n    Int amount = ReputationBumpForTier(aiTier)\n    If amount <= 0\n        Return\n    EndIf\n    If aiFameMode != 0\n        RepNVNCRFame += amount\n    Else\n        RepNVNCRInfamy += amount\n    EndIf\nEndFunction\n\nInt Function GetRepNVNCRFame()\n    Return RepNVNCRFame\nEndFunction\n\nInt Function GetRepNVNCRInfamy()\n    Return RepNVNCRInfamy\nEndFunction\n\nFunction SetPCCanUsePowerArmor(Bool abAllowed)\n    PCCanUsePowerArmor = abAllowed\nEndFunction\n\nBool Function GetPCCanUsePowerArmor()\n    Return PCCanUsePowerArmor\nEndFunction\n"
    )
}

fn discover_prefix<'a>(
    scripts: &'a [TranslatedScript],
    quests: &'a [TranslatedQuest],
) -> Option<&'a str> {
    scripts
        .iter()
        .find_map(|script| compact_name_prefix(&script.script_class_name, "_S_"))
        .or_else(|| {
            quests.iter().find_map(|quest| {
                compact_name_prefix(quest.fragment_class_name.strip_prefix("QF_")?, "_")
            })
        })
}

fn compact_name_prefix<'a>(class_name: &'a str, separator: &str) -> Option<&'a str> {
    let (prefix, local) = class_name.rsplit_once(separator)?;
    (!prefix.is_empty() && local.len() == 6 && local.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then_some(prefix)
}

fn contains_local<'a>(mut values: impl Iterator<Item = &'a str>, expected: u32) -> bool {
    values.any(|value| {
        value
            .split([':', '@'])
            .next()
            .and_then(|local| u32::from_str_radix(local.trim_start_matches("0x"), 16).ok())
            == Some(expected)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_prefix_from_compact_scpt_and_quest_classes() {
        assert_eq!(
            compact_name_prefix("FNV_FO3_S_123191", "_S_"),
            Some("FNV_FO3")
        );
        assert_eq!(compact_name_prefix("FNV_FO3_06136D", "_"), Some("FNV_FO3"));
        assert_eq!(compact_name_prefix("FNV_S_bad", "_S_"), None);
    }

    #[test]
    fn integration_contract_uses_dynamic_package_aliases_and_actor_owned_on_end() {
        let requirements = integration_requirements();
        let package_rows = requirements
            .iter()
            .filter(|row| row.source_record.starts_with("PACK "))
            .collect::<Vec<_>>();
        assert_eq!(package_rows.len(), 2);
        assert!(package_rows.iter().all(|row| {
            row.target_record_type == "QUST unfilled data alias + SCPT VMAD"
                && row.operation.contains("set ALPC to the mapped PACK")
                && row.operation.contains("ApplyToRef(actor)")
                && row.operation.contains("EvaluatePackage(true)")
                && row.helper_class_suffix.is_empty()
        }));
        let hostage = package_rows
            .iter()
            .find(|row| row.source_record.contains("1231B6"))
            .unwrap();
        assert!(
            hostage
                .binding
                .contains("TecMineHostage actor PSC owns OnPackageEnd")
        );
        let renolds = package_rows
            .iter()
            .find(|row| row.source_record.contains("13289E"))
            .unwrap();
        assert!(
            renolds
                .binding
                .contains("TechaticupNCRRenoldsDialoguePackageData")
        );
        assert!(
            !requirements
                .iter()
                .any(|row| row.helper_class_suffix == "_TecMineHostageEscapeAlias")
        );
    }

    #[test]
    fn compatibility_source_uses_reputation_tiers_and_queryable_state() {
        let source = slice_compat_source("B21_FnvSliceCompat");
        for (tier, amount) in [(1, 1), (2, 2), (3, 4), (4, 7), (5, 12)] {
            assert!(source.contains(&format!("aiTier == {tier}\n        Return {amount}")));
        }
        assert!(source.contains("Int amount = ReputationBumpForTier(aiTier)"));
        assert!(source.contains("RepNVNCRFame += amount"));
        assert!(source.contains("RepNVNCRInfamy += amount"));
        assert!(!source.contains("+= aiTier"));

        let reputation = integration_requirements()
            .into_iter()
            .find(|row| row.source_record == "REPU 0F43DE RepNVNCR")
            .unwrap();
        assert!(reputation.target_record_type.contains("REPU projection"));
        assert!(reputation.target_record_type.contains("never FACT"));
        assert!(reputation.binding.contains("GetRepNVNCRFame"));
    }

    #[test]
    fn saycustom_contract_is_keyword_projection_only_when_referenced() {
        let requirement = integration_requirements()
            .into_iter()
            .find(|row| row.source_record.contains("DIAL 0000C8 GREETING"))
            .unwrap();
        assert_eq!(
            requirement.target_record_type,
            "generated KYWD + projected INFO responses"
        );
        assert!(
            requirement
                .operation
                .starts_with("when the audited SayTo GREETING reference")
        );
        assert!(requirement.binding.contains("generated Keyword property"));
        assert!(requirement.binding.contains("Self.SayCustom"));
    }
}
