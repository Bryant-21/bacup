use serde_json::{Map, Value};

use crate::errors::RecordReadError;
use crate::fnv_legacy_scripting::{FnvLegacyScriptingResult, FnvScriptingError};
use crate::record::{FieldValue, Record};
use crate::run::ConversionRun;
use crate::source_read::{form_key_to_read_str, read_record};
use crate::sym::StringInterner;
use crate::translator::DeferredKind;

#[derive(Default)]
struct DeferredFnvInputs {
    script_records: Vec<(Value, String)>,
    quest_records: Vec<(Value, String)>,
    scene_records: Vec<(Value, String)>,
    info_records: Vec<(Value, String)>,
    dial_records: Vec<(Value, String)>,
}

pub fn run_from_deferred(
    run: &mut ConversionRun,
    mod_prefix: &str,
    source_plugin: &str,
    mod_path: &str,
) -> Result<FnvLegacyScriptingResult, FnvScriptingError> {
    let inputs = collect_deferred_inputs(run)?;
    run.run_fnv_legacy_scripting(
        mod_prefix,
        source_plugin,
        mod_path,
        &inputs.script_records,
        &inputs.quest_records,
        &inputs.scene_records,
        &inputs.info_records,
        &inputs.dial_records,
        &run.fnv_scri_links
            .iter()
            .map(|link| {
                (
                    link.target_form_key.clone(),
                    link.source_scpt_form_key.clone(),
                )
            })
            .collect::<Vec<_>>(),
    )
}

fn collect_deferred_inputs(
    run: &mut ConversionRun,
) -> Result<DeferredFnvInputs, FnvScriptingError> {
    let mut inputs = DeferredFnvInputs::default();
    let deferred = run
        .deferred
        .iter()
        .filter(|(_, kind)| matches!(kind, DeferredKind::FnvLegacyScripting))
        .map(|(fk, _)| *fk)
        .collect::<Vec<_>>();

    for fk in deferred {
        let source_key = form_key_to_read_str(&fk, &run.interner);
        if source_key.is_empty() {
            continue;
        }
        let record = read_record(
            run.source_handle_id,
            &source_key,
            &run.schema_source,
            &mut run.interner,
        )
        .map_err(fnv_read_error)?;
        let legacy_key = form_key_to_legacy_key(&source_key);
        let payload = record_to_legacy_payload(&record, &legacy_key, &run.interner);
        match record.sig.as_str() {
            "SCPT" => inputs.script_records.push((payload, legacy_key)),
            "QUST" => inputs.quest_records.push((payload, legacy_key)),
            "SCEN" => inputs.scene_records.push((payload, legacy_key)),
            "INFO" => inputs.info_records.push((payload, legacy_key)),
            "DIAL" => inputs.dial_records.push((payload, legacy_key)),
            _ => {}
        }
    }

    Ok(inputs)
}

fn fnv_read_error(err: RecordReadError) -> FnvScriptingError {
    FnvScriptingError::Setup(format!("native deferred FNV read: {err}"))
}

fn form_key_to_legacy_key(source_key: &str) -> String {
    let Some((plugin, local)) = source_key.rsplit_once(':') else {
        return source_key.to_string();
    };
    format!("{}:{}", local.trim(), plugin.trim())
}

fn record_to_legacy_payload(
    record: &Record,
    source_form_key: &str,
    interner: &StringInterner,
) -> Value {
    let mut map = Map::new();
    if let Some(eid) = record.eid.and_then(|sym| interner.resolve(sym)) {
        map.insert("eid".to_string(), Value::String(eid.to_string()));
    }
    if record.sig.as_str() == "DIAL" {
        map.insert(
            "__source_form_key".to_string(),
            Value::String(source_form_key.to_string()),
        );
    }
    let fields = record
        .fields
        .iter()
        .map(|entry| {
            let mut field = Map::new();
            field.insert(
                entry.sig.as_str().to_string(),
                field_value_to_json(&entry.value, interner),
            );
            Value::Object(field)
        })
        .collect::<Vec<_>>();
    map.insert("fields".to_string(), Value::Array(fields));
    Value::Object(map)
}

fn field_value_to_json(value: &FieldValue, interner: &StringInterner) -> Value {
    match value {
        FieldValue::None => Value::Null,
        FieldValue::Bool(v) => Value::Bool(*v),
        FieldValue::Int(v) => Value::Number((*v).into()),
        FieldValue::Uint(v) => serde_json::Number::from(*v).into(),
        FieldValue::Float(v) => serde_json::Number::from_f64(*v as f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        FieldValue::String(sym) => interner
            .resolve(*sym)
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null),
        FieldValue::Bytes(bytes) => Value::String(hex::encode_upper(bytes)),
        FieldValue::FormKey(fk) => {
            let plugin = interner.resolve(fk.plugin).unwrap_or("");
            Value::String(format!("{:06X}:{}", fk.local, plugin))
        }
        FieldValue::List(items) => Value::Array(
            items
                .iter()
                .map(|item| field_value_to_json(item, interner))
                .collect(),
        ),
        FieldValue::Struct(fields) => {
            let mut map = Map::new();
            for (key, value) in fields {
                if let Some(name) = interner.resolve(*key) {
                    map.insert(name.to_string(), field_value_to_json(value, interner));
                }
            }
            Value::Object(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record};

    #[test]
    fn form_key_to_legacy_key_flips_plugin_first_shape() {
        assert_eq!(
            form_key_to_legacy_key("FalloutNV.esm:001234"),
            "001234:FalloutNV.esm"
        );
    }

    #[test]
    fn record_to_legacy_payload_adds_source_key_for_dial() {
        let mut interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let eid = interner.intern("TopicOne");
        let qnam = interner.intern("speaker");
        let mut record = Record::new(
            SigCode::from_str("DIAL").unwrap(),
            FormKey {
                local: 0x1234,
                plugin,
            },
        );
        record.eid = Some(eid);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("QNAM").unwrap(),
            value: FieldValue::String(qnam),
        });

        let payload = record_to_legacy_payload(&record, "001234:FalloutNV.esm", &interner);
        assert_eq!(payload["eid"], "TopicOne");
        assert_eq!(payload["__source_form_key"], "001234:FalloutNV.esm");
        assert_eq!(payload["fields"][0]["QNAM"], "speaker");
    }

    #[test]
    fn field_value_to_json_renders_form_key_in_legacy_shape() {
        let mut interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let value = FieldValue::FormKey(FormKey {
            local: 0x5678,
            plugin,
        });
        assert_eq!(
            field_value_to_json(&value, &interner),
            Value::String("005678:FalloutNV.esm".to_string())
        );
    }
}
