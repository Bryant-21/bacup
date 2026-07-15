//! QUST translation.
//!
//! Mirrors `fnv_legacy_scripting/quest.py`.
//!
//! Reads QUST source records (as `serde_json::Value` dicts), extracts stage
//! script fragments, translates them through the Papyrus pipeline, and emits:
//!  - a Papyrus `.psc` source text,
//!  - alias synthesis for discovered reference variables,
//!  - a VMAD payload dict suitable for an authoring-dir record.

use std::collections::HashSet;

use regex::Regex;
use serde_json::{Map, Value, json};

use super::form_keys::object_id_from_form_key;
use super::naming::quest_fragment_name;
use super::vmad::{QuestStageFragment, synthesize_quest_vmad};
use super::{FnvScriptContext, TranslateError, translate_to_papyrus};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single translated quest stage fragment.
#[derive(Debug, Clone)]
pub struct StageFragment {
    pub stage_index: i32,
    pub psc_function_name: String,
    pub body: String,
}

/// Result of translating one QUST record.
#[derive(Debug, Clone)]
pub struct TranslatedQuest {
    pub source_editor_id: String,
    pub source_form_key: String,
    pub fragment_class_name: String,
    /// Papyrus `.psc` source text (caller writes this to disk if needed).
    pub fragment_psc_text: String,
    pub aliases: Vec<AliasEntry>,
    pub stage_fragments: Vec<StageFragment>,
    pub unresolved_reference_names: Vec<String>,
    /// Authoring-dir record payload with VMAD synthesized in.
    pub authoring_record_payload: Option<Value>,
    pub warnings: Vec<String>,
}

/// Alias synthesized for a reference variable discovered in stage scripts.
#[derive(Debug, Clone)]
pub struct AliasEntry {
    pub name: String,
    pub fill_type: String,
    pub flags: Vec<String>,
}

// ---------------------------------------------------------------------------
// synthesize_aliases_for_refs
// ---------------------------------------------------------------------------

/// Build alias entries for every unique reference variable name found in stage
/// scripts (matches Python's `synthesize_aliases_for_refs`).
pub fn synthesize_aliases_for_refs(ref_var_names: &[String]) -> Vec<AliasEntry> {
    let mut seen: HashSet<&str> = HashSet::new();
    ref_var_names
        .iter()
        .filter(|name| seen.insert(name.as_str()))
        .map(|name| AliasEntry {
            name: name.clone(),
            fill_type: "specific_reference".into(),
            flags: vec![],
        })
        .collect()
}

// ---------------------------------------------------------------------------
// translate_qust_record
// ---------------------------------------------------------------------------

/// Translate a single QUST record value.
///
/// `ctx` must already be loaded (`FnvScriptContext::load()`).
pub fn translate_qust_record(
    record: &Value,
    mod_prefix: &str,
    strict: bool,
    source_form_key: &str,
) -> Result<TranslatedQuest, TranslateError> {
    let eid = record_eid(record);
    let short_form_id = object_id_from_form_key(source_form_key);
    let class_name = quest_fragment_name(mod_prefix, &eid, &short_form_id);

    let ctx =
        FnvScriptContext::load().map_err(|e| TranslateError::Semantic(format!("load ctx: {e}")))?;

    let mut fragments: Vec<StageFragment> = Vec::new();
    let mut raw_fragment_sources: Vec<String> = Vec::new();
    let mut current_stage: Option<i32> = None;

    if let Some(fields) = record.get("fields").and_then(|f| f.as_array()) {
        for field in fields {
            if let Some(indx) = field_value(field, "INDX") {
                current_stage = indx.as_i64().map(|n| n as i32);
                continue;
            }
            if let Some(sctx) = field_value(field, "SCTX") {
                if let Some(source) = sctx.as_str() {
                    let source = source.trim();
                    if !source.is_empty() {
                        if let Some(stage) = current_stage {
                            let wrapped = format!("begin GameMode\n{source}\nend\n");
                            let papyrus =
                                translate_to_papyrus(&wrapped, &ctx, &class_name, "Quest")?;
                            let body = extract_first_event_body(&papyrus);
                            fragments.push(StageFragment {
                                stage_index: stage,
                                psc_function_name: format!("Fragment_{stage}"),
                                body,
                            });
                            raw_fragment_sources.push(source.to_string());
                            current_stage = None;
                        }
                    }
                }
            }
        }
    }

    let alias_names = collect_reference_names(&raw_fragment_sources);
    let aliases = synthesize_aliases_for_refs(&alias_names);
    let psc_text = build_quest_psc(&class_name, &fragments);

    let vmad_frags: Vec<QuestStageFragment> = fragments
        .iter()
        .map(|f| QuestStageFragment {
            stage_index: f.stage_index,
            psc_function_name: f.psc_function_name.clone(),
        })
        .collect();

    let authoring_payload =
        build_quest_payload(record, &class_name, &vmad_frags, aliases.len(), strict);

    Ok(TranslatedQuest {
        source_editor_id: eid,
        source_form_key: source_form_key.to_string(),
        fragment_class_name: class_name,
        fragment_psc_text: psc_text,
        aliases,
        stage_fragments: fragments,
        unresolved_reference_names: alias_names,
        authoring_record_payload: Some(authoring_payload),
        warnings: vec![],
    })
}

// ---------------------------------------------------------------------------
// PSC builder
// ---------------------------------------------------------------------------

fn build_quest_psc(class_name: &str, fragments: &[StageFragment]) -> String {
    let mut out = format!("ScriptName {class_name} extends Quest\n\n");
    for fragment in fragments {
        out.push_str(&format!("Function {}()\n", fragment.psc_function_name));
        for line in indent_fragment_lines(&fragment.body) {
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str("EndFunction\n\n");
    }
    out.trim_end().to_string() + "\n"
}

fn indent_fragment_lines(body: &str) -> Vec<String> {
    body.lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else if line.starts_with("    ") {
                line.to_string()
            } else {
                format!("    {}", line.trim_start())
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Payload builder
// ---------------------------------------------------------------------------

fn build_quest_payload(
    record: &Value,
    fragment_class_name: &str,
    fragments: &[QuestStageFragment],
    alias_count: usize,
    _strict: bool,
) -> Value {
    let drop_fields: HashSet<&str> = ["INDX", "SCTX", "VMAD", "VirtualMachineAdapter"]
        .into_iter()
        .collect();
    let mut payload = filtered_payload(record, &drop_fields);
    let vmad = synthesize_quest_vmad(fragment_class_name, fragments, alias_count);
    upsert_field(&mut payload, "VirtualMachineAdapter", vmad);
    payload
}

// ---------------------------------------------------------------------------
// Helpers shared across modules
// ---------------------------------------------------------------------------

/// Extract the text value of a named subrecord key from a field object.
pub(super) fn field_value<'a>(field: &'a Value, sig: &str) -> Option<&'a Value> {
    field.as_object()?.get(sig)
}

/// Read the EditorID (EDID) from a record value.
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

/// Filter out drop_fields from a record value, removing `__source_form_key`.
pub(super) fn filtered_payload(record: &Value, drop_fields: &HashSet<&str>) -> Value {
    let mut payload: Map<String, Value> = Map::new();
    if let Some(obj) = record.as_object() {
        for (k, v) in obj {
            if k == "__source_form_key" {
                continue;
            }
            if k == "fields" {
                continue; // rebuilt below
            }
            payload.insert(k.clone(), v.clone());
        }
    }
    let mut fields: Vec<Value> = Vec::new();
    if let Some(field_arr) = record.get("fields").and_then(|f| f.as_array()) {
        for field in field_arr {
            if let Some(obj) = field.as_object() {
                if obj.len() != 1 {
                    continue;
                }
                let key = obj.keys().next().unwrap().as_str();
                if drop_fields.contains(key) {
                    continue;
                }
                fields.push(field.clone());
            }
        }
    }
    payload.insert("fields".into(), Value::Array(fields));
    Value::Object(payload)
}

/// Insert or replace a named field in the record payload's `fields` array.
pub(super) fn upsert_field(payload: &mut Value, key: &str, value: Value) {
    let fields = payload
        .as_object_mut()
        .and_then(|o| o.get_mut("fields"))
        .and_then(|f| f.as_array_mut());
    if let Some(fields) = fields {
        for entry in fields.iter_mut() {
            if entry.as_object().and_then(|o| o.get(key)).is_some() {
                *entry = json!({ key: value });
                return;
            }
        }
        fields.insert(0, json!({ key: value }));
    } else {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("fields".into(), json!([{ key: value }]));
        }
    }
}

/// Collect unique reference variable names (pattern `r[A-Za-z0-9_]+`) from
/// FNV script source strings.
fn collect_reference_names(sources: &[String]) -> Vec<String> {
    let re = Regex::new(r"\b(r[A-Za-z0-9_]+)\b").unwrap();
    let mut seen: HashSet<String> = HashSet::new();
    let mut names: Vec<String> = Vec::new();
    for source in sources {
        for cap in re.captures_iter(source) {
            let name = cap[1].to_string();
            if seen.insert(name.clone()) {
                names.push(name);
            }
        }
    }
    names
}

/// Extract the body of the first event block from emitted Papyrus text.
///
/// Returns everything inside the first `Event OnInit()` … `EndEvent` block
/// (or the generic event block), or an empty string if none is found.
pub(super) fn extract_first_event_body(papyrus: &str) -> String {
    let mut in_event = false;
    let mut body_lines: Vec<&str> = Vec::new();
    for line in papyrus.lines() {
        let trimmed = line.trim();
        if !in_event {
            if trimmed.starts_with("Event ") || trimmed.starts_with("Function ") {
                in_event = true;
            }
            continue;
        }
        if trimmed == "EndEvent" || trimmed == "EndFunction" {
            break;
        }
        body_lines.push(line);
    }
    body_lines.join("\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesize_aliases_deduplicates() {
        let names = vec!["rNpc".to_string(), "rNpc".to_string(), "rItem".to_string()];
        let aliases = synthesize_aliases_for_refs(&names);
        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases[0].name, "rNpc");
        assert_eq!(aliases[0].fill_type, "specific_reference");
    }

    #[test]
    fn build_quest_psc_format() {
        let frags = vec![StageFragment {
            stage_index: 10,
            psc_function_name: "Fragment_10".into(),
            body: "x = 1".into(),
        }];
        let psc = build_quest_psc("QF_B21_nv_Q_001234", &frags);
        assert!(psc.contains("ScriptName QF_B21_nv_Q_001234 extends Quest"));
        assert!(psc.contains("Function Fragment_10()"));
        assert!(psc.contains("EndFunction"));
        assert!(psc.contains("    x = 1"), "psc:\n{psc}");
    }

    #[test]
    fn collect_reference_names_regex() {
        let sources = vec!["set rNpc to GetRef rItem".to_string()];
        let names = collect_reference_names(&sources);
        assert!(names.contains(&"rNpc".to_string()));
        assert!(names.contains(&"rItem".to_string()));
    }

    #[test]
    fn filtered_payload_drops_sctx_and_indx() {
        let record = json!({
            "eid": "Q1",
            "fields": [
                { "INDX": 10 },
                { "SCTX": "set x to 1" },
                { "FULL": "My Quest" },
            ]
        });
        let drop: HashSet<&str> = ["INDX", "SCTX", "VMAD", "VirtualMachineAdapter"]
            .into_iter()
            .collect();
        let payload = filtered_payload(&record, &drop);
        let fields = payload["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 1);
        assert!(fields[0].get("FULL").is_some());
    }

    #[test]
    fn upsert_field_inserts_new() {
        let mut payload = json!({ "fields": [] });
        upsert_field(&mut payload, "VMAD", json!({ "Version": 5 }));
        let fields = payload["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 1);
        assert!(fields[0]["VMAD"]["Version"] == 5);
    }

    #[test]
    fn upsert_field_replaces_existing() {
        let mut payload = json!({ "fields": [{ "VMAD": { "Version": 1 } }] });
        upsert_field(&mut payload, "VMAD", json!({ "Version": 5 }));
        let fields = payload["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0]["VMAD"]["Version"], 5);
    }

    #[test]
    fn translate_qust_record_smoke() {
        // Empty QUST record with no SCTX fields should produce a valid TranslatedQuest
        // with no fragments and the correct class name.
        let record = json!({
            "eid": "TestQuest",
            "fields": []
        });
        let result = translate_qust_record(&record, "B21", false, "001234:FNV.esm");
        let tq = result.expect("translate ok");
        assert_eq!(tq.source_editor_id, "TestQuest");
        assert!(tq.fragment_class_name.contains("QF_B21_nv_TestQuest"));
        assert!(tq.stage_fragments.is_empty());
    }

    #[test]
    fn translate_qust_record_with_stage_fragment() {
        let record = json!({
            "eid": "TestQuest",
            "fields": [
                { "INDX": 10 },
                { "SCTX": "set x to 1" },
            ]
        });
        let result = translate_qust_record(&record, "B21", false, "001234:FNV.esm");
        let tq = result.expect("translate ok");
        assert_eq!(tq.stage_fragments.len(), 1);
        assert_eq!(tq.stage_fragments[0].stage_index, 10);
        assert!(tq.fragment_psc_text.contains("Fragment_10"));
    }
}
