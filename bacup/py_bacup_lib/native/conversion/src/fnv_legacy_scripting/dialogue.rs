//! DIAL grouping and INFO fragment translation.
//!
//! Mirrors `fnv_legacy_scripting/dialogue.py`.

use std::collections::HashSet;

use serde_json::{Value, json};

use super::form_keys::object_id_from_form_key;
use super::naming::topic_info_fragment_name;
use super::quest::{extract_first_event_body, field_value, filtered_payload, upsert_field};
use super::vmad::synthesize_topic_info_vmad;
use super::voice::{fnv_to_fo4_voice_path, fnv_voice_source_path};
use super::{FnvScriptContext, TranslateError, translate_to_papyrus};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A group of DIAL topics sharing the same speaker.
#[derive(Debug, Clone)]
pub struct DialogueGroup {
    pub speaker_form_key: String,
    pub topics: Vec<Value>,
}

/// Result of translating one INFO record.
#[derive(Debug, Clone)]
pub struct TranslatedInfo {
    pub source_form_key: String,
    pub fragment_class_name: Option<String>,
    /// Papyrus `.psc` source text (None when the INFO has no SCTX).
    pub fragment_psc_text: Option<String>,
    pub voice_target_path: String,
    pub voice_source_path: String,
    /// LIP sync data is always dropped (not carried across).
    pub lip_dropped: bool,
    pub lip_regeneration_target: Option<String>,
    pub authoring_record_payload: Option<Value>,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// group_dial_records
// ---------------------------------------------------------------------------

/// Group DIAL records by speaker FormKey (QNAM field).
pub fn group_dial_records(dial_records: &[Value]) -> Vec<DialogueGroup> {
    let mut map: indexmap::IndexMap<String, Vec<Value>> = indexmap::IndexMap::new();
    for record in dial_records {
        let speaker = record_field_str(record, "QNAM");
        map.entry(speaker).or_default().push(record.clone());
    }
    map.into_iter()
        .map(|(speaker_form_key, topics)| DialogueGroup {
            speaker_form_key,
            topics,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// DIAL payload construction (mirrors Python `_dial_payload`)
// ---------------------------------------------------------------------------

/// Strip fields that are owned by the legacy scripting layer from a DIAL
/// record before re-emission. Mirrors `phases.py::_dial_payload` — drops
/// `__source_form_key` top-level and `SCTX`/`VTCK`/`VMAD`/`VirtualMachineAdapter`
/// field entries.
pub fn dial_payload(record: &Value) -> Value {
    let mut payload = match record {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                if key == "__source_form_key" {
                    continue;
                }
                out.insert(key.clone(), value.clone());
            }
            Value::Object(out)
        }
        _ => return record.clone(),
    };

    // Filter `fields` to drop legacy-scripting subrecords.
    if let Some(fields) = payload
        .as_object_mut()
        .and_then(|m| m.get_mut("fields"))
        .and_then(|v| v.as_array_mut())
    {
        let dropped: HashSet<&'static str> = ["SCTX", "VTCK", "VMAD", "VirtualMachineAdapter"]
            .into_iter()
            .collect();
        fields.retain(|entry| match entry.as_object() {
            Some(obj) if obj.len() == 1 => {
                let key = obj.keys().next().map(|s| s.as_str()).unwrap_or("");
                !dropped.contains(key)
            }
            _ => false, // Non-object or multi-key entries — drop (matches Python's filter).
        });
    }
    payload
}

/// Accumulate stripped DIAL payloads onto a `FnvLegacyScriptingContext`.
///
/// Each DIAL record is re-emitted as a payload that drops the legacy
/// scripting subrecords (`SCTX`/`VTCK`/`VMAD`/`VirtualMachineAdapter`) so
/// the converted plugin no longer carries FNV-specific dialogue script
/// hooks.
pub fn accumulate_dial_records(
    ctx: &mut super::FnvLegacyScriptingContext,
    dial_records: &[(Value, String)],
) {
    for (record, form_key) in dial_records {
        let payload = dial_payload(record);
        ctx.translated_record_payloads
            .push(super::TranslatedRecordPayload {
                source_form_key: form_key.clone(),
                signature: "DIAL".into(),
                translated_record: payload,
                warnings: Vec::new(),
            });
    }
}

// ---------------------------------------------------------------------------
// translate_info_record
// ---------------------------------------------------------------------------

/// Translate a single INFO record value.
pub fn translate_info_record(
    record: &Value,
    mod_prefix: &str,
    source_plugin: &str,
    strict: bool,
    source_form_key: &str,
) -> Result<TranslatedInfo, TranslateError> {
    let short_form_id = object_id_from_form_key(source_form_key);

    // Voice type — VTCK field may be a string or a dict with "voice_type" key.
    let voice_type = extract_voice_type(record);

    let voice_target =
        fnv_to_fo4_voice_path(mod_prefix, source_plugin, &voice_type, &short_form_id);
    let voice_source = fnv_voice_source_path(source_plugin, &voice_type, &short_form_id);

    let source = record_field_str_optional(record, "SCTX");
    let (fragment_class_name, fragment_psc_text) = if let Some(source) = source {
        let class_name = topic_info_fragment_name(&short_form_id);
        let ctx = FnvScriptContext::load()
            .map_err(|e| TranslateError::Semantic(format!("load ctx: {e}")))?;
        let wrapped = format!("begin GameMode\n{source}\nend\n");
        let papyrus = translate_to_papyrus(&wrapped, &ctx, &class_name, "TopicInfo")?;
        let bodies = extract_first_event_body(&papyrus);
        let psc_text = build_info_psc(&class_name, &bodies);
        (Some(class_name), Some(psc_text))
    } else {
        (None, None)
    };

    let payload = build_info_payload(record, fragment_class_name.as_deref(), strict);

    Ok(TranslatedInfo {
        source_form_key: source_form_key.to_string(),
        fragment_class_name: fragment_class_name.clone(),
        fragment_psc_text,
        voice_target_path: voice_target.clone(),
        voice_source_path: voice_source,
        lip_dropped: true,
        lip_regeneration_target: Some(voice_target),
        authoring_record_payload: Some(payload),
        warnings: vec![],
    })
}

// ---------------------------------------------------------------------------
// PSC builder
// ---------------------------------------------------------------------------

fn build_info_psc(class_name: &str, body: &str) -> String {
    let mut out = format!("ScriptName {class_name} extends TopicInfo\n\nFunction Fragment_0()\n");
    for line in body.lines() {
        if line.trim().is_empty() {
            out.push('\n');
        } else if line.starts_with("    ") {
            out.push_str(line);
            out.push('\n');
        } else {
            out.push_str(&format!("    {}\n", line.trim_start()));
        }
    }
    out.push_str("EndFunction\n");
    out.trim_end().to_string() + "\n"
}

// ---------------------------------------------------------------------------
// Payload builder
// ---------------------------------------------------------------------------

fn build_info_payload(record: &Value, fragment_class_name: Option<&str>, _strict: bool) -> Value {
    let drop_fields: HashSet<&str> = ["SCTX", "VTCK", "VMAD", "VirtualMachineAdapter"]
        .into_iter()
        .collect();
    let mut payload = filtered_payload(record, &drop_fields);
    if let Some(class_name) = fragment_class_name {
        let vmad = synthesize_topic_info_vmad(class_name);
        upsert_field(&mut payload, "VirtualMachineAdapter", vmad);
    }
    payload
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn extract_voice_type(record: &Value) -> String {
    if let Some(fields) = record.get("fields").and_then(|f| f.as_array()) {
        for field in fields {
            if let Some(vtck) = field_value(field, "VTCK") {
                // May be a plain string or a dict with "voice_type" key.
                if let Some(s) = vtck.as_str() {
                    return s.to_string();
                }
                if let Some(obj) = vtck.as_object() {
                    if let Some(vt) = obj.get("voice_type").and_then(|v| v.as_str()) {
                        return vt.to_string();
                    }
                }
            }
        }
    }
    "MaleEvenToned".to_string()
}

fn record_field_str(record: &Value, sig: &str) -> String {
    if let Some(fields) = record.get("fields").and_then(|f| f.as_array()) {
        for field in fields {
            if let Some(val) = field_value(field, sig) {
                if let Some(s) = val.as_str() {
                    return s.to_string();
                }
            }
        }
    }
    String::new()
}

fn record_field_str_optional(record: &Value, sig: &str) -> Option<String> {
    if let Some(fields) = record.get("fields").and_then(|f| f.as_array()) {
        for field in fields {
            if let Some(val) = field_value(field, sig) {
                if let Some(s) = val.as_str() {
                    let s = s.trim();
                    if !s.is_empty() {
                        return Some(s.to_string());
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_info_record_no_sctx() {
        let record = json!({
            "eid": "Info1",
            "fields": []
        });
        let result = translate_info_record(&record, "B21", "FNV.esm", false, "001234:FNV.esm");
        let ti = result.expect("translate ok");
        assert!(ti.fragment_class_name.is_none());
        assert!(ti.fragment_psc_text.is_none());
        assert!(ti.voice_target_path.contains("FNV.esm"));
        assert!(ti.lip_dropped);
    }

    #[test]
    fn translate_info_record_with_sctx() {
        let record = json!({
            "eid": "Info2",
            "fields": [
                { "SCTX": "set x to 1" },
            ]
        });
        let result = translate_info_record(&record, "B21", "FNV.esm", false, "001234:FNV.esm");
        let ti = result.expect("translate ok");
        assert!(ti.fragment_class_name.is_some());
        let class = ti.fragment_class_name.as_ref().unwrap();
        assert!(class.starts_with("TIF__"));
        let psc = ti.fragment_psc_text.as_ref().unwrap();
        assert!(psc.contains("extends TopicInfo"));
        assert!(psc.contains("Fragment_0"));
    }

    #[test]
    fn translate_info_record_voice_type_default() {
        let record = json!({
            "fields": []
        });
        let ti =
            translate_info_record(&record, "B21", "FNV.esm", false, "001234:FNV.esm").expect("ok");
        assert!(ti.voice_target_path.contains("MaleEvenToned"));
    }

    #[test]
    fn translate_info_record_voice_type_from_vtck() {
        let record = json!({
            "fields": [
                { "VTCK": "FemaleSultry" },
            ]
        });
        let ti =
            translate_info_record(&record, "B21", "FNV.esm", false, "001234:FNV.esm").expect("ok");
        assert!(ti.voice_target_path.contains("FemaleSultry"));
    }

    #[test]
    fn group_dial_records_groups_by_speaker() {
        let records = vec![
            json!({ "fields": [{ "QNAM": "001:FNV.esm" }] }),
            json!({ "fields": [{ "QNAM": "002:FNV.esm" }] }),
            json!({ "fields": [{ "QNAM": "001:FNV.esm" }] }),
        ];
        let groups = group_dial_records(&records);
        assert_eq!(groups.len(), 2);
        let grp1 = groups
            .iter()
            .find(|g| g.speaker_form_key == "001:FNV.esm")
            .unwrap();
        assert_eq!(grp1.topics.len(), 2);
    }

    #[test]
    fn build_info_psc_format() {
        let psc = build_info_psc("TIF__001234", "x = 1");
        assert!(psc.contains("ScriptName TIF__001234 extends TopicInfo"));
        assert!(psc.contains("Function Fragment_0()"));
        assert!(psc.contains("    x = 1"));
        assert!(psc.contains("EndFunction"));
    }

    // -----------------------------------------------------------------------
    // dial_payload / accumulate_dial_records
    // -----------------------------------------------------------------------

    #[test]
    fn dial_payload_strips_legacy_script_subrecords() {
        let record = json!({
            "__source_form_key": "001234:FNV.esm",
            "form_id": "001234",
            "fields": [
                { "EDID": "GreetingTopic" },
                { "QNAM": "001:FNV.esm" },
                { "SCTX": "begin GameMode\nset x to 1\nend\n" },
                { "VTCK": "FemaleSultry" },
                { "VMAD": { "version": 5 } },
                { "VirtualMachineAdapter": {} },
                { "FULL": "Hello there" },
            ]
        });
        let payload = dial_payload(&record);
        let obj = payload.as_object().expect("object");
        assert!(!obj.contains_key("__source_form_key"));
        assert_eq!(obj.get("form_id").and_then(|v| v.as_str()), Some("001234"));

        let fields = obj.get("fields").and_then(|v| v.as_array()).unwrap();
        let keys: Vec<&str> = fields
            .iter()
            .filter_map(|f| f.as_object()?.keys().next().map(|s| s.as_str()))
            .collect();
        // Stripped:
        assert!(!keys.contains(&"SCTX"));
        assert!(!keys.contains(&"VTCK"));
        assert!(!keys.contains(&"VMAD"));
        assert!(!keys.contains(&"VirtualMachineAdapter"));
        // Kept:
        assert!(keys.contains(&"EDID"));
        assert!(keys.contains(&"QNAM"));
        assert!(keys.contains(&"FULL"));
    }

    #[test]
    fn dial_payload_keeps_record_when_no_fields_array() {
        let record = json!({ "form_id": "001234" });
        let payload = dial_payload(&record);
        // No `fields` array → payload still has `form_id`, no error.
        assert_eq!(
            payload
                .as_object()
                .unwrap()
                .get("form_id")
                .and_then(|v| v.as_str()),
            Some("001234")
        );
    }

    #[test]
    fn dial_payload_drops_multikey_field_entries() {
        // Python's filter only keeps single-key field dicts; mirror that.
        let record = json!({
            "fields": [
                { "EDID": "Topic", "QNAM": "stray" },
                { "FULL": "Hello" },
            ]
        });
        let payload = dial_payload(&record);
        let fields = payload
            .as_object()
            .unwrap()
            .get("fields")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(
            fields[0].as_object().unwrap().keys().next().unwrap(),
            "FULL"
        );
    }

    #[test]
    fn accumulate_dial_records_pushes_one_payload_per_record() {
        use crate::fnv_legacy_scripting::FnvLegacyScriptingContext;
        let mut ctx = FnvLegacyScriptingContext::new("B21", "FNV.esm", false);
        let records = vec![
            (
                json!({ "fields": [{ "EDID": "T1" }, { "SCTX": "dead" }] }),
                "001:FNV.esm".to_string(),
            ),
            (
                json!({ "fields": [{ "EDID": "T2" }] }),
                "002:FNV.esm".to_string(),
            ),
        ];
        accumulate_dial_records(&mut ctx, &records);
        assert_eq!(ctx.translated_record_payloads.len(), 2);
        assert_eq!(ctx.translated_record_payloads[0].signature, "DIAL");
        assert_eq!(
            ctx.translated_record_payloads[0].source_form_key,
            "001:FNV.esm"
        );
        // SCTX should be stripped from the first record's payload.
        let fields = ctx.translated_record_payloads[0]
            .translated_record
            .get("fields")
            .and_then(|v| v.as_array())
            .unwrap();
        let keys: Vec<&str> = fields
            .iter()
            .filter_map(|f| f.as_object()?.keys().next().map(|s| s.as_str()))
            .collect();
        assert!(!keys.contains(&"SCTX"));
        assert!(keys.contains(&"EDID"));
    }
}
