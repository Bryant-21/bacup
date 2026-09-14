use serde_json::Value;

use crate::ids::FormKey;

use super::{FnvScriptingError, TranslatedRecordPayload};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyRecordTopology {
    TopLevel,
    QuestChild {
        source_parent: FormKey,
        target_parent: FormKey,
    },
    TopicChild {
        source_parent: FormKey,
        target_parent: FormKey,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyRecordIdentity {
    pub signature: String,
    pub source_form_key: FormKey,
    pub target_form_key: FormKey,
    pub source_form_key_text: String,
    pub target_form_key_text: String,
    pub topology: LegacyRecordTopology,
}

#[derive(Debug, Clone)]
pub struct PreparedLegacyRecordPayload {
    pub identity: LegacyRecordIdentity,
    pub translated_record: Value,
    pub warnings: Vec<String>,
}

impl PreparedLegacyRecordPayload {
    pub fn from_translated(
        payload: TranslatedRecordPayload,
        identity: LegacyRecordIdentity,
    ) -> Result<Self, FnvScriptingError> {
        if payload.signature != identity.signature {
            return Err(FnvScriptingError::Setup(format!(
                "legacy payload identity mismatch for {}: payload={} identity={}",
                payload.source_form_key, payload.signature, identity.signature
            )));
        }
        if payload.source_form_key != identity.source_form_key_text {
            return Err(FnvScriptingError::Setup(format!(
                "legacy payload source mismatch: payload={} identity={}",
                payload.source_form_key, identity.source_form_key_text
            )));
        }
        let mut translated_record = payload.translated_record;
        let object = translated_record.as_object_mut().ok_or_else(|| {
            FnvScriptingError::Setup(format!(
                "legacy {} {} payload is not an object",
                identity.signature, identity.source_form_key_text
            ))
        })?;
        object.insert(
            "signature".to_string(),
            Value::String(identity.signature.clone()),
        );
        object.insert(
            "form_id".to_string(),
            Value::String(identity.target_form_key_text.clone()),
        );
        if let Some(fields) = object.get_mut("fields").and_then(Value::as_array_mut) {
            fields.retain(|field| {
                !field.as_object().is_some_and(|entry| {
                    entry.len() == 1
                        && entry
                            .keys()
                            .next()
                            .is_some_and(|signature| matches!(signature.as_str(), "SCTX" | "SCDA"))
                })
            });
        }
        Ok(Self {
            identity,
            translated_record,
            warnings: payload.warnings,
        })
    }

    pub fn write_rank(&self) -> u8 {
        match self.identity.signature.as_str() {
            "QUST" => 0,
            "DIAL" => 1,
            "INFO" => 2,
            "SCEN" => 3,
            _ => 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;

    fn fk(local: u32, plugin: crate::sym::Sym) -> FormKey {
        FormKey { local, plugin }
    }

    fn identity(
        signature: &str,
        source_local: u32,
        target_local: u32,
        plugin: crate::sym::Sym,
        topology: LegacyRecordTopology,
    ) -> LegacyRecordIdentity {
        LegacyRecordIdentity {
            signature: signature.to_string(),
            source_form_key: fk(source_local, plugin),
            target_form_key: fk(target_local, plugin),
            source_form_key_text: format!("{source_local:06X}:FalloutNV.esm"),
            target_form_key_text: format!("{target_local:06X}:FalloutNV.esm"),
            topology,
        }
    }

    #[test]
    fn production_shaped_quest_dialogue_info_order_is_stable() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let quest_source = fk(0x130100, plugin);
        let quest_target = fk(0x230100, plugin);
        let dial_source = fk(0x13015B, plugin);
        let dial_target = fk(0x23015B, plugin);
        let mut payloads = vec![
            PreparedLegacyRecordPayload::from_translated(
                TranslatedRecordPayload {
                    source_form_key: "130161:FalloutNV.esm".to_string(),
                    signature: "INFO".to_string(),
                    translated_record: serde_json::json!({ "fields": [] }),
                    warnings: Vec::new(),
                },
                identity(
                    "INFO",
                    0x130161,
                    0x230161,
                    plugin,
                    LegacyRecordTopology::TopicChild {
                        source_parent: dial_source,
                        target_parent: dial_target,
                    },
                ),
            )
            .unwrap(),
            PreparedLegacyRecordPayload::from_translated(
                TranslatedRecordPayload {
                    source_form_key: "13015B:FalloutNV.esm".to_string(),
                    signature: "DIAL".to_string(),
                    translated_record: serde_json::json!({ "fields": [] }),
                    warnings: Vec::new(),
                },
                identity(
                    "DIAL",
                    0x13015B,
                    0x23015B,
                    plugin,
                    LegacyRecordTopology::QuestChild {
                        source_parent: quest_source,
                        target_parent: quest_target,
                    },
                ),
            )
            .unwrap(),
            PreparedLegacyRecordPayload::from_translated(
                TranslatedRecordPayload {
                    source_form_key: "130100:FalloutNV.esm".to_string(),
                    signature: "QUST".to_string(),
                    translated_record: serde_json::json!({ "fields": [] }),
                    warnings: Vec::new(),
                },
                identity(
                    "QUST",
                    0x130100,
                    0x230100,
                    plugin,
                    LegacyRecordTopology::TopLevel,
                ),
            )
            .unwrap(),
        ];

        payloads.sort_by_key(PreparedLegacyRecordPayload::write_rank);
        assert_eq!(
            payloads
                .iter()
                .map(|payload| payload.identity.signature.as_str())
                .collect::<Vec<_>>(),
            ["QUST", "DIAL", "INFO"]
        );
        assert_eq!(
            payloads[1].translated_record["form_id"],
            "23015B:FalloutNV.esm"
        );
    }

    #[test]
    fn zero_info_dialogue_keeps_quest_child_topology() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let prepared = PreparedLegacyRecordPayload::from_translated(
            TranslatedRecordPayload {
                source_form_key: "138A74:FalloutNV.esm".to_string(),
                signature: "DIAL".to_string(),
                translated_record: serde_json::json!({ "fields": [] }),
                warnings: Vec::new(),
            },
            identity(
                "DIAL",
                0x138A74,
                0x238A74,
                plugin,
                LegacyRecordTopology::QuestChild {
                    source_parent: fk(0x138A00, plugin),
                    target_parent: fk(0x238A00, plugin),
                },
            ),
        )
        .unwrap();

        assert_eq!(prepared.write_rank(), 1);
        assert!(matches!(
            prepared.identity.topology,
            LegacyRecordTopology::QuestChild { .. }
        ));
    }
}
