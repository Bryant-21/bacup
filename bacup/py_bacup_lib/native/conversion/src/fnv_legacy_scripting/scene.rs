//! SCEN translation.
//!
//! Mirrors `fnv_legacy_scripting/scene.py`.

use std::collections::HashSet;

use serde_json::{Value, json};

use super::form_keys::object_id_from_form_key;
use super::naming::scene_action_fragment_name;
use super::quest::{extract_first_event_body, field_value, filtered_payload, upsert_field};
use super::vmad::synthesize_scene_vmad;
use super::{FnvScriptContext, TranslateError, translate_to_papyrus};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Result of translating one SCEN record.
#[derive(Debug, Clone)]
pub struct TranslatedScene {
    pub source_editor_id: String,
    pub source_form_key: String,
    pub parent_quest_form_key: String,
    pub fragment_class_name: String,
    /// Papyrus `.psc` source text.
    pub fragment_psc_text: String,
    pub actions: Vec<SceneAction>,
    pub authoring_record_payload: Option<Value>,
    pub warnings: Vec<String>,
}

/// A translated scene action.
#[derive(Debug, Clone)]
pub struct SceneAction {
    pub index: usize,
    pub source: String,
}

// ---------------------------------------------------------------------------
// translate_scen_record
// ---------------------------------------------------------------------------

/// Translate a single SCEN record value.
pub fn translate_scen_record(
    record: &Value,
    mod_prefix: &str,
    strict: bool,
    source_form_key: &str,
) -> Result<TranslatedScene, TranslateError> {
    let eid = record_eid(record);
    let short_form_id = object_id_from_form_key(source_form_key);
    let parent_quest_form_key = record_field_str(record, "PNAM");
    let class_name = scene_action_fragment_name(&eid, &short_form_id);

    let ctx =
        FnvScriptContext::load().map_err(|e| TranslateError::Semantic(format!("load ctx: {e}")))?;

    let mut translated_functions: Vec<(String, String)> = Vec::new();
    let mut actions: Vec<SceneAction> = Vec::new();
    let mut action_index: usize = 0;

    if let Some(fields) = record.get("fields").and_then(|f| f.as_array()) {
        for field in fields {
            let sctx = match field_value(field, "SCTX").and_then(|v| v.as_str()) {
                Some(s) if !s.trim().is_empty() => s,
                _ => continue,
            };
            action_index += 1;
            let wrapped = format!("begin GameMode\n{sctx}\nend\n");
            let papyrus = translate_to_papyrus(&wrapped, &ctx, &class_name, "Scene")?;
            let body = extract_first_event_body(&papyrus);
            translated_functions.push((format!("Fragment_{action_index}"), body));
            actions.push(SceneAction {
                index: action_index,
                source: sctx.to_string(),
            });
        }
    }

    let psc_text = build_scene_psc(&class_name, &translated_functions);
    let payload = build_scene_payload(record, &class_name, actions.len(), strict);

    Ok(TranslatedScene {
        source_editor_id: eid,
        source_form_key: source_form_key.to_string(),
        parent_quest_form_key,
        fragment_class_name: class_name,
        fragment_psc_text: psc_text,
        actions,
        authoring_record_payload: Some(payload),
        warnings: vec![],
    })
}

// ---------------------------------------------------------------------------
// PSC builder
// ---------------------------------------------------------------------------

fn build_scene_psc(class_name: &str, functions: &[(String, String)]) -> String {
    let mut out = format!("ScriptName {class_name} extends Scene\n\n");
    for (function_name, body) in functions {
        out.push_str(&format!("Function {function_name}()\n"));
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
        out.push_str("EndFunction\n\n");
    }
    out.trim_end().to_string() + "\n"
}

// ---------------------------------------------------------------------------
// Payload builder
// ---------------------------------------------------------------------------

fn build_scene_payload(
    record: &Value,
    fragment_class_name: &str,
    action_count: usize,
    _strict: bool,
) -> Value {
    let drop_fields: HashSet<&str> = ["SCTX", "VMAD", "VirtualMachineAdapter"]
        .into_iter()
        .collect();
    let mut payload = filtered_payload(record, &drop_fields);
    let vmad = synthesize_scene_vmad(fragment_class_name, action_count);
    upsert_field(&mut payload, "VirtualMachineAdapter", vmad);
    payload
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn record_eid(record: &Value) -> String {
    if let Some(eid) = record.get("eid").and_then(|v| v.as_str()) {
        return eid.to_string();
    }
    if let Some(fields) = record.get("fields").and_then(|f| f.as_array()) {
        for field in fields {
            if let Some(edid) = field_value(field, "EDID") {
                if let Some(s) = edid.as_str() {
                    return s.to_string();
                }
            }
        }
    }
    "Unnamed".to_string()
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_scen_record_empty_no_fragments() {
        let record = json!({
            "eid": "TestScene",
            "fields": []
        });
        let result = translate_scen_record(&record, "B21", false, "001234:FNV.esm");
        let ts = result.expect("translate ok");
        assert_eq!(ts.source_editor_id, "TestScene");
        assert!(ts.actions.is_empty());
        assert!(
            ts.fragment_psc_text
                .contains("ScriptName SF_TestScene_001234 extends Scene")
        );
    }

    #[test]
    fn translate_scen_record_with_sctx() {
        let record = json!({
            "eid": "TestScene",
            "fields": [
                { "SCTX": "set x to 1" },
            ]
        });
        let result = translate_scen_record(&record, "B21", false, "001234:FNV.esm");
        let ts = result.expect("translate ok");
        assert_eq!(ts.actions.len(), 1);
        assert!(ts.fragment_psc_text.contains("Fragment_1"));
        assert!(ts.fragment_psc_text.contains("EndFunction"));
    }

    #[test]
    fn translate_scen_record_parent_quest_extracted() {
        let record = json!({
            "eid": "TestScene",
            "fields": [
                { "PNAM": "001000:FNV.esm" },
            ]
        });
        let result = translate_scen_record(&record, "B21", false, "001234:FNV.esm");
        let ts = result.expect("translate ok");
        assert_eq!(ts.parent_quest_form_key, "001000:FNV.esm");
    }

    #[test]
    fn build_scene_psc_format() {
        let fns = vec![
            ("Fragment_1".to_string(), "x = 1".to_string()),
            ("Fragment_2".to_string(), "y = 2".to_string()),
        ];
        let psc = build_scene_psc("SF_MyScene_001234", &fns);
        assert!(psc.contains("ScriptName SF_MyScene_001234 extends Scene"));
        assert!(psc.contains("Function Fragment_1()"));
        assert!(psc.contains("Function Fragment_2()"));
        assert!(psc.contains("    x = 1"));
    }
}
