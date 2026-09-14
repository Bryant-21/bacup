use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeRecordIdentity {
    pub form_key: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenRecordMapping {
    pub source: RuntimeRecordIdentity,
    pub target: RuntimeRecordIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeTopologyKind {
    QuestChild,
    TopicChild,
    PersistentCellChild,
    TemporaryCellChild,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTopologyEdge {
    pub parent_form_key: String,
    pub child_form_key: String,
    pub kind: RuntimeTopologyKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureAdaptationReceipt {
    pub source_form_key: String,
    pub source_signature: String,
    pub target_signature: String,
    pub semantic_role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeScriptPropertyReceipt {
    pub name: String,
    pub papyrus_type: String,
    pub target_form_key: String,
    pub auto_const: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeScriptBindingReceipt {
    pub carrier_form_key: String,
    pub class_name: String,
    pub attachment_kind: String,
    pub properties: Vec<RuntimeScriptPropertyReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeAssetReceipt {
    pub semantic_role: String,
    pub source_path: String,
    pub target_path: String,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeComponentDecision {
    Supported,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeComponentReceipt {
    pub component_id: String,
    pub family: String,
    pub roots: Vec<RuntimeRecordIdentity>,
    pub members: Vec<RuntimeRecordIdentity>,
    pub decision: RuntimeComponentDecision,
    pub reason: Option<String>,
    pub adaptations: Vec<SignatureAdaptationReceipt>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimRuntimeAccounting {
    pub components_seen: u32,
    pub components_supported: u32,
    pub components_dropped: u32,
    pub records_written: u32,
    pub records_dropped: u32,
    pub script_bindings_expected: u32,
    pub script_bindings_attached: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkyrimRuntimeReceipt {
    pub version: u32,
    pub source_game: String,
    pub target_game: String,
    pub source_plugin: String,
    pub target_plugin: String,
    pub components: Vec<RuntimeComponentReceipt>,
    pub mappings: Vec<FrozenRecordMapping>,
    pub topology: Vec<RuntimeTopologyEdge>,
    pub signature_adaptations: Vec<SignatureAdaptationReceipt>,
    pub script_bindings: Vec<RuntimeScriptBindingReceipt>,
    pub assets: Vec<RuntimeAssetReceipt>,
    pub accounting: SkyrimRuntimeAccounting,
}

impl SkyrimRuntimeReceipt {
    pub const VERSION: u32 = 1;

    pub fn validate_shape(&self) -> Result<(), String> {
        if self.version != Self::VERSION {
            return Err(format!(
                "unsupported Skyrim runtime receipt version {}",
                self.version
            ));
        }
        if !self.source_game.eq_ignore_ascii_case("skyrimse")
            || !self.target_game.eq_ignore_ascii_case("fo4")
            || self.source_plugin.trim().is_empty()
            || self.target_plugin.trim().is_empty()
        {
            return Err("Skyrim runtime receipt has an invalid game/plugin identity".to_string());
        }
        let mut sources = std::collections::BTreeSet::new();
        let mut target_signatures = std::collections::BTreeMap::new();
        for mapping in &self.mappings {
            let target_form_key = mapping.target.form_key.to_ascii_lowercase();
            if mapping.source.signature.len() != 4
                || mapping.target.signature.len() != 4
                || !sources.insert(mapping.source.form_key.to_ascii_lowercase())
            {
                return Err("Skyrim runtime receipt mapping inventory is invalid".to_string());
            }
            if target_signatures
                .insert(target_form_key, mapping.target.signature.as_str())
                .is_some_and(|signature| signature != mapping.target.signature)
            {
                return Err("Skyrim runtime receipt target signature is inconsistent".to_string());
            }
        }
        let mut component_ids = std::collections::BTreeSet::new();
        for component in &self.components {
            let unsupported = matches!(component.decision, RuntimeComponentDecision::Unsupported);
            let has_reason = component
                .reason
                .as_deref()
                .is_some_and(|reason| !reason.trim().is_empty());
            if component.component_id.trim().is_empty()
                || component.family.trim().is_empty()
                || component.roots.is_empty()
                || component.members.is_empty()
                || !component_ids.insert(component.component_id.to_ascii_lowercase())
                || unsupported != has_reason
            {
                return Err("Skyrim runtime component receipt is incomplete".to_string());
            }
            for member in &component.members {
                if member.signature.len() != 4 {
                    return Err("Skyrim runtime component member is invalid".to_string());
                }
            }
            if component.roots.iter().any(|root| {
                !component
                    .members
                    .iter()
                    .any(|member| member.form_key.eq_ignore_ascii_case(&root.form_key))
            }) {
                return Err("Skyrim runtime component root is not a member".to_string());
            }
        }
        let supported = self
            .components
            .iter()
            .filter(|component| matches!(component.decision, RuntimeComponentDecision::Supported))
            .count() as u32;
        let dropped = self.components.len() as u32 - supported;
        let supported_members = self
            .components
            .iter()
            .filter(|component| matches!(component.decision, RuntimeComponentDecision::Supported))
            .flat_map(|component| component.members.iter())
            .map(|member| member.form_key.to_ascii_lowercase())
            .collect::<std::collections::BTreeSet<_>>();
        let unsupported_members = self
            .components
            .iter()
            .filter(|component| matches!(component.decision, RuntimeComponentDecision::Unsupported))
            .flat_map(|component| component.members.iter())
            .map(|member| member.form_key.to_ascii_lowercase())
            .collect::<std::collections::BTreeSet<_>>();
        let admitted_members = supported_members
            .difference(&unsupported_members)
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        if sources != admitted_members
            || sources
                .iter()
                .any(|source| unsupported_members.contains(source))
        {
            return Err(
                "Skyrim runtime receipt mappings do not match atomic component admission"
                    .to_string(),
            );
        }
        for adaptation in &self.signature_adaptations {
            let source = adaptation.source_form_key.to_ascii_lowercase();
            let Some(mapping) = self
                .mappings
                .iter()
                .find(|mapping| mapping.source.form_key.eq_ignore_ascii_case(&source))
            else {
                return Err(
                    "Skyrim runtime signature adaptation has no admitted mapping".to_string(),
                );
            };
            if mapping.source.signature != adaptation.source_signature
                || mapping.target.signature != adaptation.target_signature
            {
                return Err(
                    "Skyrim runtime signature adaptation differs from its mapping".to_string(),
                );
            }
        }
        let dropped_records = unsupported_members.difference(&admitted_members).count() as u32;
        if self.accounting.components_seen != self.components.len() as u32
            || self.accounting.components_supported != supported
            || self.accounting.components_dropped != dropped
            || self.accounting.script_bindings_attached > self.accounting.script_bindings_expected
            || self.accounting.script_bindings_expected != self.script_bindings.len() as u32
            || self.accounting.records_written != self.mappings.len() as u32
            || self.accounting.records_dropped != dropped_records
        {
            return Err("Skyrim runtime receipt accounting is inconsistent".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_rejects_duplicate_source_mapping_and_inconsistent_accounting() {
        let identity = RuntimeRecordIdentity {
            form_key: "000001@Source.esm".to_string(),
            signature: "QUST".to_string(),
        };
        let mut receipt = SkyrimRuntimeReceipt {
            version: SkyrimRuntimeReceipt::VERSION,
            source_game: "skyrimse".to_string(),
            target_game: "fo4".to_string(),
            source_plugin: "Source.esm".to_string(),
            target_plugin: "Output.esm".to_string(),
            components: vec![RuntimeComponentReceipt {
                component_id: "quest:1".to_string(),
                family: "quest".to_string(),
                roots: vec![identity.clone()],
                members: vec![identity.clone()],
                decision: RuntimeComponentDecision::Supported,
                reason: None,
                adaptations: Vec::new(),
            }],
            mappings: vec![
                FrozenRecordMapping {
                    source: identity.clone(),
                    target: RuntimeRecordIdentity {
                        form_key: "000800@Output.esm".to_string(),
                        signature: "QUST".to_string(),
                    },
                },
                FrozenRecordMapping {
                    source: identity,
                    target: RuntimeRecordIdentity {
                        form_key: "000801@Output.esm".to_string(),
                        signature: "QUST".to_string(),
                    },
                },
            ],
            topology: Vec::new(),
            signature_adaptations: Vec::new(),
            script_bindings: Vec::new(),
            assets: Vec::new(),
            accounting: SkyrimRuntimeAccounting {
                components_seen: 1,
                components_supported: 1,
                ..Default::default()
            },
        };
        assert!(receipt.validate_shape().is_err());
        receipt.mappings.truncate(1);
        receipt.accounting.components_dropped = 1;
        assert!(receipt.validate_shape().is_err());
    }

    #[test]
    fn receipt_accepts_multiple_source_races_mapped_to_one_target_race() {
        let source_races = ["013746@Skyrim.esm", "013747@Skyrim.esm"];
        let components = source_races
            .iter()
            .map(|form_key| {
                let identity = RuntimeRecordIdentity {
                    form_key: (*form_key).to_string(),
                    signature: "RACE".to_string(),
                };
                RuntimeComponentReceipt {
                    component_id: format!("race:{form_key}"),
                    family: "appearance".to_string(),
                    roots: vec![identity.clone()],
                    members: vec![identity],
                    decision: RuntimeComponentDecision::Supported,
                    reason: None,
                    adaptations: Vec::new(),
                }
            })
            .collect::<Vec<_>>();
        let mappings = source_races
            .iter()
            .map(|form_key| FrozenRecordMapping {
                source: RuntimeRecordIdentity {
                    form_key: (*form_key).to_string(),
                    signature: "RACE".to_string(),
                },
                target: RuntimeRecordIdentity {
                    form_key: "013746@Fallout4.esm".to_string(),
                    signature: "RACE".to_string(),
                },
            })
            .collect::<Vec<_>>();
        let mut receipt = SkyrimRuntimeReceipt {
            version: SkyrimRuntimeReceipt::VERSION,
            source_game: "skyrimse".to_string(),
            target_game: "fo4".to_string(),
            source_plugin: "Skyrim.esm".to_string(),
            target_plugin: "Skyrim.esm".to_string(),
            components,
            mappings,
            topology: Vec::new(),
            signature_adaptations: Vec::new(),
            script_bindings: Vec::new(),
            assets: Vec::new(),
            accounting: SkyrimRuntimeAccounting {
                components_seen: 2,
                components_supported: 2,
                records_written: 2,
                ..Default::default()
            },
        };

        assert_eq!(receipt.validate_shape(), Ok(()));
        receipt.mappings[1].target.signature = "NPC_".to_string();
        assert_eq!(
            receipt.validate_shape(),
            Err("Skyrim runtime receipt target signature is inconsistent".to_string())
        );
    }
}
