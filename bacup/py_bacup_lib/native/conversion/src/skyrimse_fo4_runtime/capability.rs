use crate::record::Record;
use crate::sym::StringInterner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyrimRuntimeFamily {
    Quest,
    Dialogue,
    Scene,
    Actor,
    WorldObject,
    Appearance,
    Ai,
    Equipment,
    Magic,
    Music,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimRecordCapability {
    Unmanaged,
    SupportedByExistingTranslator {
        family: SkyrimRuntimeFamily,
    },
    SupportedByComponentProjection {
        family: SkyrimRuntimeFamily,
    },
    RequiresComponentPlan {
        family: SkyrimRuntimeFamily,
        reason: &'static str,
    },
    Unsupported {
        family: SkyrimRuntimeFamily,
        reason: &'static str,
    },
}

impl SkyrimRecordCapability {
    pub fn family(&self) -> Option<SkyrimRuntimeFamily> {
        match self {
            Self::Unmanaged => None,
            Self::SupportedByExistingTranslator { family }
            | Self::SupportedByComponentProjection { family }
            | Self::RequiresComponentPlan { family, .. }
            | Self::Unsupported { family, .. } => Some(*family),
        }
    }

    pub fn drop_reason(&self) -> Option<&'static str> {
        match self {
            Self::RequiresComponentPlan { reason, .. } | Self::Unsupported { reason, .. } => {
                Some(reason)
            }
            Self::Unmanaged
            | Self::SupportedByExistingTranslator { .. }
            | Self::SupportedByComponentProjection { .. } => None,
        }
    }
}

pub fn classify_record_capability(
    record: &Record,
    _interner: &StringInterner,
) -> SkyrimRecordCapability {
    use SkyrimRecordCapability::{
        RequiresComponentPlan, SupportedByComponentProjection, SupportedByExistingTranslator,
        Unmanaged,
    };
    use SkyrimRuntimeFamily::{
        Actor, Ai, Appearance, Dialogue, Equipment, Magic, Music, Quest, Scene, WorldObject,
    };

    match record.sig.as_str() {
        "MUSC" | "MUST" => SupportedByExistingTranslator { family: Music },
        "QUST" => RequiresComponentPlan {
            family: Quest,
            reason: "quest requires a supported start route, aliases, conditions, topology, and scripts",
        },
        "SMBN" | "SMEN" | "SMQN" => RequiresComponentPlan {
            family: Quest,
            reason: "Story Manager record requires a supported quest start-route component",
        },
        "DLVW" | "DLBR" | "DIAL" | "INFO" => RequiresComponentPlan {
            family: Dialogue,
            reason: "dialogue requires a supported quest component and source parent topology",
        },
        "SCEN" => RequiresComponentPlan {
            family: Scene,
            reason: "scene requires a supported quest, actor, phase, action, and fragment component",
        },
        "VTYP" | "NPC_" | "ACHR" | "RACE" => RequiresComponentPlan {
            family: Actor,
            reason: "actor requires a supported target-native humanoid and placement plan",
        },
        "BPTD" | "CLFM" | "EYES" | "HDPT" => SupportedByExistingTranslator { family: Appearance },
        "PACK" => RequiresComponentPlan {
            family: Ai,
            reason: "package requires an evidence-backed target-native component projection",
        },
        "CSTY" | "MOVT" | "PERK" => SupportedByExistingTranslator { family: Ai },
        "FURN" => SupportedByExistingTranslator {
            family: WorldObject,
        },
        "ARMA" | "ARMO" => SupportedByExistingTranslator { family: Equipment },
        "WEAP" if super::weapon::simple_melee_weapon_source(record, _interner).is_some() => {
            SupportedByComponentProjection { family: Equipment }
        }
        "AMMO" | "ENCH" | "LVLN" | "WEAP" => SupportedByExistingTranslator { family: Equipment },
        "SPEL" | "MGEF" | "PROJ" => SupportedByExistingTranslator { family: Magic },
        "SCRL" | "SHOU" => SupportedByComponentProjection { family: Magic },
        "LSCR" => SupportedByExistingTranslator { family: Scene },
        _ => Unmanaged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode};
    use crate::sym::StringInterner;

    fn record(signature: &str, interner: &StringInterner) -> Record {
        Record::new(
            SigCode::from_str(signature).unwrap(),
            FormKey {
                local: 1,
                plugin: interner.intern("Fixture.esm"),
            },
        )
    }

    #[test]
    fn quest_families_remain_excluded_while_non_quest_records_are_routed() {
        let interner = StringInterner::new();
        for signature in ["QUST", "DIAL", "INFO", "NPC_", "ACHR", "SCEN", "SMQN"] {
            assert!(matches!(
                classify_record_capability(&record(signature, &interner), &interner),
                SkyrimRecordCapability::RequiresComponentPlan { .. }
            ));
        }
        for signature in ["SPEL", "MGEF", "WEAP", "SCRL", "SHOU", "LSCR"] {
            assert!(
                classify_record_capability(&record(signature, &interner), &interner)
                    .drop_reason()
                    .is_none()
            );
        }
        assert!(matches!(
            classify_record_capability(&record("PACK", &interner), &interner),
            SkyrimRecordCapability::RequiresComponentPlan { .. }
        ));
    }

    #[test]
    fn translated_non_weapon_world_objects_are_supported() {
        let interner = StringInterner::new();
        for signature in ["FURN", "ARMA", "ARMO"] {
            assert!(matches!(
                classify_record_capability(&record(signature, &interner), &interner),
                SkyrimRecordCapability::SupportedByExistingTranslator { .. }
            ));
        }
    }

    #[test]
    fn only_unmanaged_and_translator_supported_records_bypass_component_planning() {
        let interner = StringInterner::new();
        assert!(
            classify_record_capability(&record("MUSC", &interner), &interner)
                .drop_reason()
                .is_none()
        );
        assert!(matches!(
            classify_record_capability(&record("VTYP", &interner), &interner),
            SkyrimRecordCapability::RequiresComponentPlan { .. }
        ));
        assert_eq!(
            classify_record_capability(&record("STAT", &interner), &interner),
            SkyrimRecordCapability::Unmanaged
        );
    }
}
