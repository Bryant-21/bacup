//! VMAD payload synthesis for FNV-translated records.
//!
//! Mirrors `fnv_legacy_scripting/vmad.py`.
//!
//! All functions return `serde_json::Value` (JSON-compatible map) that can be
//! serialised directly into an authoring-dir YAML field.  The Python layer
//! already round-trips through dicts, so we match that shape.
//!
//! This module emits dict payloads (not binary VMAD subrecords); the binary
//! encoding happens downstream in the ESP writer.

use serde_json::{Map, Value, json};

/// Kind of fragment embedding used for a given VMAD attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentKind {
    Object,
    QuestStage,
    TopicInfo,
    SceneAction,
}

/// Error type for VMAD synthesis failures.
#[derive(Debug)]
pub struct VmadSynthError(pub String);

impl std::fmt::Display for VmadSynthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vmad synthesis error: {}", self.0)
    }
}

impl std::error::Error for VmadSynthError {}

// ---------------------------------------------------------------------------
// Script property
// ---------------------------------------------------------------------------

/// A Papyrus script property.
#[derive(Debug, Clone)]
pub struct ScriptProperty {
    pub name: String,
    pub prop_type: String,
    pub value: Option<Value>,
}

/// Build the JSON representation of a single script property.
pub fn property_payload(prop: &ScriptProperty) -> Result<Value, VmadSynthError> {
    let prop_type = prop.prop_type.trim().to_lowercase();
    match prop_type.as_str() {
        "objectreference" | "object" | "form" => Ok(json!({
            "propertyName": prop.name,
            "Type": 1,
            "Flags": 0,
            "Value": {
                "Object Union Object Union": {
                    "Object v2 Unused": 0,
                    "Object v2 Alias": -1,
                    "Object v2 FormID": prop.value,
                }
            }
        })),
        "string" => Ok(json!({
            "propertyName": prop.name,
            "Type": 2,
            "Flags": 0,
            "Value": {
                "String String": prop.value.as_ref().and_then(|v| v.as_str()).unwrap_or(""),
            }
        })),
        "int" | "integer" => Ok(json!({
            "propertyName": prop.name,
            "Type": 3,
            "Flags": 0,
            "Value": {
                "Int32 Int32": prop.value.as_ref().and_then(|v| v.as_i64()).unwrap_or(0),
            }
        })),
        "float" => Ok(json!({
            "propertyName": prop.name,
            "Type": 4,
            "Flags": 0,
            "Value": {
                "Float Float": prop.value.as_ref().and_then(|v| v.as_f64()).unwrap_or(0.0),
            }
        })),
        "bool" | "boolean" => {
            let truthy = prop
                .value
                .as_ref()
                .map(|v| match v {
                    Value::Bool(b) => *b,
                    Value::String(s) => {
                        matches!(s.trim().to_lowercase().as_str(), "1" | "true" | "yes")
                    }
                    Value::Number(n) => n.as_i64().unwrap_or(0) != 0,
                    _ => false,
                })
                .unwrap_or(false);
            Ok(json!({
                "propertyName": prop.name,
                "Type": 5,
                "Flags": 0,
                "Value": { "Bool Bool": truthy }
            }))
        }
        other => Err(VmadSynthError(format!(
            "unsupported property type '{other}' for '{}'",
            prop.name
        ))),
    }
}

// ---------------------------------------------------------------------------
// VMAD base builder
// ---------------------------------------------------------------------------

fn vmad_base(scripts: Vec<Value>, extra: Option<Map<String, Value>>) -> Value {
    let mut payload = Map::new();
    payload.insert("Version".into(), json!(5));
    payload.insert("Object Format".into(), json!(2));
    payload.insert("Scripts".into(), Value::Array(scripts));
    if let Some(extra_fields) = extra {
        for (k, v) in extra_fields {
            payload.insert(k, v);
        }
    }
    Value::Object(payload)
}

// ---------------------------------------------------------------------------
// ScriptBindingIntent
// ---------------------------------------------------------------------------

/// Intent to attach a script's VMAD to a record.
#[derive(Debug, Clone)]
pub struct ScriptBindingIntent {
    pub target_form_key: String,
    pub script_class_name: String,
    pub properties: Vec<ScriptProperty>,
    pub fragment_kind: FragmentKind,
}

// ---------------------------------------------------------------------------
// build_scpt_vmad_intents — pair SCPT targets with translated scripts
// ---------------------------------------------------------------------------

/// One target record awaiting a SCPT VMAD attachment.
///
/// Mirrors the per-tuple entries Python's `iter_vmad_targets` yields:
/// `(record, target_form_key, source_scpt_form_key)`. The caller supplies
/// the source SCPT FormKey (matched against `TranslatedScript.source_form_key`)
/// and the target record's FormKey (used as the intent's binding target).
pub struct VmadTarget {
    /// FormKey of the target record getting the VMAD binding. Format
    /// matches `TranslatedScript.source_form_key` shape (e.g. `001234:FNV.esm`).
    pub target_form_key: String,
    /// FormKey of the source SCPT to bind. Matched case-insensitively
    /// against `TranslatedScript.source_form_key`.
    pub source_scpt_form_key: String,
}

/// Pair every target with its translated script and build the resulting
/// `ScriptBindingIntent` list. Mirrors Python `build_scpt_vmad_intents`.
///
/// Targets whose source SCPT FormKey doesn't match any translated script
/// are silently skipped (matches Python's `if translated is None: continue`).
pub fn build_scpt_vmad_intents(
    vmad_targets: &[VmadTarget],
    translated_scripts: &[super::script_synthesizer::TranslatedScript],
) -> Vec<ScriptBindingIntent> {
    // Index scripts by normalized source FormKey (uppercase, trimmed).
    let mut by_key: std::collections::HashMap<
        String,
        &super::script_synthesizer::TranslatedScript,
    > = std::collections::HashMap::new();
    for script in translated_scripts {
        if script.source_form_key.is_empty() {
            continue;
        }
        by_key.insert(normalize_form_key(&script.source_form_key), script);
    }

    let mut out: Vec<ScriptBindingIntent> = Vec::with_capacity(vmad_targets.len());
    for target in vmad_targets {
        let key = normalize_form_key(&target.source_scpt_form_key);
        let translated = match by_key.get(&key) {
            Some(s) => *s,
            None => continue,
        };
        out.push(ScriptBindingIntent {
            target_form_key: target.target_form_key.clone(),
            script_class_name: translated.script_class_name.clone(),
            // Python: `properties=list(translated.properties)`. The Python
            // SCPT translator never populates that list; we mirror by
            // emitting an empty Vec. If a future port surfaces properties
            // on TranslatedScript, the conversion lights up here.
            properties: Vec::new(),
            fragment_kind: FragmentKind::Object,
        });
    }
    out
}

/// Attach (or replace) a `VirtualMachineAdapter` field on a target record
/// using the supplied intent. Mirrors Python `attach_vmad_to_record`.
///
/// If the record already carries a `VMAD` or `VirtualMachineAdapter` field,
/// it is replaced in place. Otherwise the new VMAD payload is inserted at
/// index 0 of the `fields` array (matches Python's insert-first behavior).
///
/// Returns `Err(VmadSynthError)` when the intent has an empty script class
/// name; the record is not mutated in that case.
pub fn attach_vmad_to_record(
    record: &mut Value,
    intent: &ScriptBindingIntent,
) -> Result<(), VmadSynthError> {
    let vmad_payload = synthesize_vmad(intent)?;
    let new_field = json!({ "VirtualMachineAdapter": vmad_payload });

    let obj = record
        .as_object_mut()
        .ok_or_else(|| VmadSynthError("attach_vmad_to_record: record is not an object".into()))?;
    let fields_entry = obj.entry("fields").or_insert(Value::Array(Vec::new()));
    let fields = fields_entry
        .as_array_mut()
        .ok_or_else(|| VmadSynthError("attach_vmad_to_record: fields is not an array".into()))?;

    // Replace any existing VMAD / VirtualMachineAdapter entry.
    for slot in fields.iter_mut() {
        if let Some(slot_obj) = slot.as_object() {
            if slot_obj.contains_key("VMAD") || slot_obj.contains_key("VirtualMachineAdapter") {
                *slot = new_field;
                return Ok(());
            }
        }
    }
    // Otherwise insert at index 0.
    fields.insert(0, new_field);
    Ok(())
}

/// Normalize a FormKey for case-insensitive lookup (Python
/// `_normalize_form_key`: `str(value).strip().upper()`).
fn normalize_form_key(value: &str) -> String {
    value.trim().to_uppercase()
}

/// Build a VMAD payload for a generic object script binding.
pub fn synthesize_vmad(intent: &ScriptBindingIntent) -> Result<Value, VmadSynthError> {
    if intent.script_class_name.is_empty() {
        return Err(VmadSynthError(format!(
            "empty script class name for {}",
            intent.target_form_key
        )));
    }
    let mut props: Vec<Value> = Vec::new();
    for prop in &intent.properties {
        props.push(property_payload(prop)?);
    }
    let script = json!({
        "ScriptName": intent.script_class_name,
        "Flags": 0,
        "Properties": props,
    });
    Ok(vmad_base(vec![script], None))
}

// ---------------------------------------------------------------------------
// Per-record-type synthesizers
// ---------------------------------------------------------------------------

/// Synthesize a VMAD payload for a translated TopicInfo (INFO) record.
pub fn synthesize_topic_info_vmad(script_class_name: &str) -> Value {
    let mut extra = Map::new();
    extra.insert("Script Fragments Extra bind data version".into(), json!(1));
    extra.insert("Script Fragments Flags".into(), json!(0));
    extra.insert(
        "Script Fragments Script ScriptName".into(),
        json!(script_class_name),
    );
    extra.insert("Script Fragments Script Flags".into(), json!(0));
    extra.insert("Script Fragments Script Properties".into(), json!([]));
    extra.insert(
        "Script Fragments Fragments".into(),
        json!([{
            "Unknown": 0,
            "ScriptName": script_class_name,
            "FragmentName": "Fragment_0",
        }]),
    );
    vmad_base(vec![], Some(extra))
}

/// A stage fragment description used when synthesizing a quest VMAD.
#[derive(Debug, Clone)]
pub struct QuestStageFragment {
    pub stage_index: i32,
    pub psc_function_name: String,
}

/// Synthesize a VMAD payload for a translated QUST record.
pub fn synthesize_quest_vmad(
    script_class_name: &str,
    fragments: &[QuestStageFragment],
    alias_count: usize,
) -> Value {
    let fragment_list: Vec<Value> = fragments
        .iter()
        .map(|f| {
            json!({
                "Quest Stage": f.stage_index,
                "Unknown": 0,
                "Quest Stage Index": 0,
                "Unknown 1": 0,
                "ScriptName": script_class_name,
                "FragmentName": f.psc_function_name,
            })
        })
        .collect();

    let alias_list: Vec<Value> = (0..alias_count)
        .map(|_| {
            json!({
                "Object Union Object Union": {
                    "Object v2 Unused": 0,
                    "Object v2 Alias": -1,
                    "Object v2 FormID": null,
                },
                "Version": 0,
                "Object Format": 2,
                "Scripts": [],
            })
        })
        .collect();

    let mut extra = Map::new();
    extra.insert("Script Fragments Extra bind data version".into(), json!(1));
    extra.insert(
        "Script Fragments FragmentCount".into(),
        json!(fragments.len()),
    );
    extra.insert(
        "Script Fragments ScriptName".into(),
        json!(script_class_name),
    );
    extra.insert(
        "Script Fragments Script".into(),
        json!({ "Flags": 0, "Properties": [] }),
    );
    extra.insert(
        "Script Fragments Fragments".into(),
        Value::Array(fragment_list),
    );
    extra.insert("Aliases".into(), Value::Array(alias_list));
    vmad_base(vec![], Some(extra))
}

/// Synthesize a VMAD payload for a translated SCEN record.
pub fn synthesize_scene_vmad(script_class_name: &str, action_count: usize) -> Value {
    let fragment_list: Vec<Value> = (1..=action_count)
        .map(|index| {
            json!({
                "Unknown": 0,
                "ScriptName": script_class_name,
                "FragmentName": format!("Fragment_{index}"),
            })
        })
        .collect();

    let mut extra = Map::new();
    extra.insert("Script Fragments Extra bind data version".into(), json!(1));
    extra.insert("Script Fragments Flags".into(), json!(0));
    extra.insert(
        "Script Fragments Script ScriptName".into(),
        json!(script_class_name),
    );
    extra.insert("Script Fragments Script Flags".into(), json!(0));
    extra.insert("Script Fragments Script Properties".into(), json!([]));
    extra.insert(
        "Script Fragments Fragments".into(),
        Value::Array(fragment_list),
    );
    extra.insert("Script Fragments Phase Fragments".into(), json!([]));
    vmad_base(vec![], Some(extra))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_info_vmad_version() {
        let vmad = synthesize_topic_info_vmad("TIF__001234");
        assert_eq!(vmad["Version"], 5);
        assert_eq!(vmad["Object Format"], 2);
        assert_eq!(vmad["Script Fragments Script ScriptName"], "TIF__001234");
        let frags = vmad["Script Fragments Fragments"].as_array().unwrap();
        assert_eq!(frags.len(), 1);
        assert_eq!(frags[0]["FragmentName"], "Fragment_0");
    }

    #[test]
    fn quest_vmad_fragments_and_aliases() {
        let frags = vec![
            QuestStageFragment {
                stage_index: 10,
                psc_function_name: "Fragment_10".into(),
            },
            QuestStageFragment {
                stage_index: 20,
                psc_function_name: "Fragment_20".into(),
            },
        ];
        let vmad = synthesize_quest_vmad("QF_B21_nv_Q_001234", &frags, 2);
        assert_eq!(vmad["Version"], 5);
        assert_eq!(vmad["Script Fragments FragmentCount"], 2);
        let frag_list = vmad["Script Fragments Fragments"].as_array().unwrap();
        assert_eq!(frag_list[0]["Quest Stage"], 10);
        assert_eq!(frag_list[1]["FragmentName"], "Fragment_20");
        let aliases = vmad["Aliases"].as_array().unwrap();
        assert_eq!(aliases.len(), 2);
    }

    #[test]
    fn scene_vmad_fragment_count() {
        let vmad = synthesize_scene_vmad("SF_MyScene_001234", 3);
        let frags = vmad["Script Fragments Fragments"].as_array().unwrap();
        assert_eq!(frags.len(), 3);
        assert_eq!(frags[0]["FragmentName"], "Fragment_1");
        assert_eq!(frags[2]["FragmentName"], "Fragment_3");
    }

    #[test]
    fn synthesize_vmad_empty_class_name_errors() {
        let intent = ScriptBindingIntent {
            target_form_key: "001234:Test.esm".into(),
            script_class_name: String::new(),
            properties: vec![],
            fragment_kind: FragmentKind::Object,
        };
        assert!(synthesize_vmad(&intent).is_err());
    }

    #[test]
    fn synthesize_vmad_with_properties() {
        let intent = ScriptBindingIntent {
            target_form_key: "001234:Test.esm".into(),
            script_class_name: "MyScript".into(),
            properties: vec![
                ScriptProperty {
                    name: "myInt".into(),
                    prop_type: "int".into(),
                    value: Some(json!(42)),
                },
                ScriptProperty {
                    name: "myStr".into(),
                    prop_type: "string".into(),
                    value: Some(json!("hello")),
                },
            ],
            fragment_kind: FragmentKind::Object,
        };
        let vmad = synthesize_vmad(&intent).unwrap();
        let scripts = vmad["Scripts"].as_array().unwrap();
        assert_eq!(scripts.len(), 1);
        let props = scripts[0]["Properties"].as_array().unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[0]["Type"], 3); // int
        assert_eq!(props[1]["Type"], 2); // string
    }

    #[test]
    fn property_payload_unsupported_type_errors() {
        let prop = ScriptProperty {
            name: "bad".into(),
            prop_type: "unknown_type".into(),
            value: None,
        };
        assert!(property_payload(&prop).is_err());
    }

    // -----------------------------------------------------------------------
    // build_scpt_vmad_intents
    // -----------------------------------------------------------------------

    use super::super::script_synthesizer::{PapyrusType, TranslatedScript};

    fn make_translated(class_name: &str, source_form_key: &str) -> TranslatedScript {
        TranslatedScript {
            source_editor_id: "TestScript".into(),
            source_form_key: source_form_key.into(),
            script_class_name: class_name.into(),
            papyrus_type: PapyrusType::ObjectReference,
            psc_text: String::new(),
        }
    }

    #[test]
    fn build_intents_pairs_targets_with_scripts() {
        let scripts = vec![
            make_translated("B21_nv_FooScript", "001234:FNV.esm"),
            make_translated("B21_nv_BarScript", "00ABCD:FNV.esm"),
        ];
        let targets = vec![
            VmadTarget {
                target_form_key: "BB0001:Output.esp".into(),
                source_scpt_form_key: "001234:FNV.esm".into(),
            },
            VmadTarget {
                target_form_key: "BB0002:Output.esp".into(),
                source_scpt_form_key: "00ABCD:FNV.esm".into(),
            },
        ];
        let intents = build_scpt_vmad_intents(&targets, &scripts);
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[0].target_form_key, "BB0001:Output.esp");
        assert_eq!(intents[0].script_class_name, "B21_nv_FooScript");
        assert_eq!(intents[1].script_class_name, "B21_nv_BarScript");
        assert!(intents[0].properties.is_empty());
        assert_eq!(intents[0].fragment_kind, FragmentKind::Object);
    }

    #[test]
    fn build_intents_skips_targets_without_matching_script() {
        let scripts = vec![make_translated("Known", "001234:FNV.esm")];
        let targets = vec![
            VmadTarget {
                target_form_key: "T1:Output.esp".into(),
                source_scpt_form_key: "001234:FNV.esm".into(),
            },
            VmadTarget {
                target_form_key: "T2:Output.esp".into(),
                source_scpt_form_key: "deadbeef:FNV.esm".into(),
            },
        ];
        let intents = build_scpt_vmad_intents(&targets, &scripts);
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].target_form_key, "T1:Output.esp");
    }

    #[test]
    fn build_intents_matches_case_insensitively() {
        let scripts = vec![make_translated("Foo", "001ABC:FNV.esm")];
        let targets = vec![VmadTarget {
            target_form_key: "T1:Output.esp".into(),
            source_scpt_form_key: "001abc:fnv.esm".into(),
        }];
        let intents = build_scpt_vmad_intents(&targets, &scripts);
        assert_eq!(intents.len(), 1);
    }

    #[test]
    fn build_intents_ignores_scripts_without_source_form_key() {
        let mut script = make_translated("Orphan", "");
        // Empty source_form_key — should not index into the matching map.
        script.source_form_key = String::new();
        let scripts = vec![script];
        let targets = vec![VmadTarget {
            target_form_key: "T1:Output.esp".into(),
            source_scpt_form_key: "001234:FNV.esm".into(),
        }];
        let intents = build_scpt_vmad_intents(&targets, &scripts);
        assert!(intents.is_empty());
    }

    // -----------------------------------------------------------------------
    // attach_vmad_to_record
    // -----------------------------------------------------------------------

    fn intent(class_name: &str) -> ScriptBindingIntent {
        ScriptBindingIntent {
            target_form_key: "T1:Output.esp".into(),
            script_class_name: class_name.into(),
            properties: vec![],
            fragment_kind: FragmentKind::Object,
        }
    }

    #[test]
    fn attach_vmad_inserts_at_index_zero_when_absent() {
        let mut record = json!({
            "fields": [
                { "EDID": "MyRecord" },
                { "FULL": "Display Name" },
            ]
        });
        attach_vmad_to_record(&mut record, &intent("Foo")).unwrap();
        let fields = record["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 3);
        // First field is now the VMAD.
        assert!(
            fields[0]
                .as_object()
                .unwrap()
                .contains_key("VirtualMachineAdapter")
        );
        assert_eq!(
            fields[1].as_object().unwrap().keys().next().unwrap(),
            "EDID"
        );
    }

    #[test]
    fn attach_vmad_replaces_existing_virtual_machine_adapter() {
        let mut record = json!({
            "fields": [
                { "EDID": "MyRecord" },
                { "VirtualMachineAdapter": { "stale": true } },
                { "FULL": "Display Name" },
            ]
        });
        attach_vmad_to_record(&mut record, &intent("Foo")).unwrap();
        let fields = record["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 3);
        // Same position as the stale entry.
        let new_vmad = &fields[1]["VirtualMachineAdapter"];
        assert_eq!(new_vmad["Version"], 5);
        assert!(new_vmad["stale"].is_null());
    }

    #[test]
    fn attach_vmad_replaces_existing_short_form_vmad() {
        let mut record = json!({
            "fields": [
                { "VMAD": { "stale": true } },
            ]
        });
        attach_vmad_to_record(&mut record, &intent("Foo")).unwrap();
        let fields = record["fields"].as_array().unwrap();
        // VMAD entry replaced with the canonical VirtualMachineAdapter key.
        assert!(
            fields[0]
                .as_object()
                .unwrap()
                .contains_key("VirtualMachineAdapter")
        );
        assert!(!fields[0].as_object().unwrap().contains_key("VMAD"));
    }

    #[test]
    fn attach_vmad_empty_class_name_errors_and_leaves_record_alone() {
        let mut record = json!({ "fields": [{ "EDID": "X" }] });
        let original = record.clone();
        let bad_intent = intent("");
        assert!(attach_vmad_to_record(&mut record, &bad_intent).is_err());
        assert_eq!(record, original);
    }

    #[test]
    fn attach_vmad_creates_fields_when_missing() {
        let mut record = json!({ "form_id": "001234" });
        attach_vmad_to_record(&mut record, &intent("Foo")).unwrap();
        let fields = record["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 1);
        assert!(
            fields[0]
                .as_object()
                .unwrap()
                .contains_key("VirtualMachineAdapter")
        );
    }
}
