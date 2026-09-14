//! VMAD payload synthesis for FNV-translated records.
//!
//! Functions return JSON maps that serialize directly into an authoring-dir
//! YAML field; the binary VMAD encoding happens downstream in the ESP writer.

use serde_json::{Map, Value, json};

use super::dialogue::InfoFragmentPhase;

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
fn canonical_vmad_form_id(
    value: Option<&Value>,
    property_name: &str,
) -> Result<Value, VmadSynthError> {
    let Some(value) = value else {
        return Ok(Value::Null);
    };
    if value.is_null() || value.get("reference").is_some() || value.get("raw").is_some() {
        return Ok(value.clone());
    }
    let text = value.as_str().ok_or_else(|| {
        VmadSynthError(format!(
            "object property '{property_name}' has an invalid FormKey payload"
        ))
    })?;
    let (left, right) = text.split_once([':', '@']).ok_or_else(|| {
        VmadSynthError(format!(
            "object property '{property_name}' has invalid FormKey {text:?}"
        ))
    })?;
    let parse_object_id = |candidate: &str| {
        u32::from_str_radix(candidate.trim_start_matches("0x"), 16)
            .ok()
            .filter(|value| *value <= 0x00FF_FFFF)
    };
    let (plugin, object_id) = if text.contains('@') {
        (right, parse_object_id(left))
    } else {
        match (parse_object_id(left), parse_object_id(right)) {
            (Some(object_id), None) => (right, Some(object_id)),
            (None, Some(object_id)) => (left, Some(object_id)),
            _ => ("", None),
        }
    };
    let object_id = object_id
        .filter(|_| !plugin.trim().is_empty())
        .ok_or_else(|| {
            VmadSynthError(format!(
                "object property '{property_name}' has ambiguous FormKey {text:?}"
            ))
        })?;
    Ok(json!({
        "reference": {
            "plugin": plugin,
            "object_id": format!("{object_id:06X}")
        }
    }))
}

pub fn property_payload(prop: &ScriptProperty) -> Result<Value, VmadSynthError> {
    let prop_type = prop.prop_type.trim().to_lowercase();
    match prop_type.as_str() {
        "referencealias" => {
            let value = prop.value.as_ref().ok_or_else(|| {
                VmadSynthError(format!(
                    "ReferenceAlias property '{}' has no alias binding",
                    prop.name
                ))
            })?;
            let target_quest_form_key = value
                .get("target_quest_form_key")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    VmadSynthError(format!(
                        "ReferenceAlias property '{}' has no target QUST FormKey",
                        prop.name
                    ))
                })?;
            let alias_id = value
                .get("alias_id")
                .and_then(Value::as_i64)
                .filter(|value| *value >= 0)
                .ok_or_else(|| {
                    VmadSynthError(format!(
                        "ReferenceAlias property '{}' has an invalid alias id",
                        prop.name
                    ))
                })?;
            Ok(json!({
                "propertyName": prop.name,
                "Type": "Object",
                "Flags": 0,
                "Value": {
                    "Alias": alias_id,
                    "FormID": canonical_vmad_form_id(Some(&Value::String(target_quest_form_key.to_string())), &prop.name)?,
                }
            }))
        }
        "objectreference" | "object" | "form" | "actor" | "quest" | "message" | "faction"
        | "package" | "topic" | "keyword" | "spell" | "perk" | "actorvalue" | "globalvariable"
        | "musictype" | "explosion" => Ok(json!({
            "propertyName": prop.name,
            "Type": "Object",
            "Flags": 0,
            "Value": {
                "Alias": -1,
                "FormID": canonical_vmad_form_id(prop.value.as_ref(), &prop.name)?,
            }
        })),
        "string" => Ok(json!({
            "propertyName": prop.name,
            "Type": "String",
            "Flags": 0,
            "Value": prop.value.as_ref().and_then(|v| v.as_str()).unwrap_or(""),
        })),
        "int" | "integer" => Ok(json!({
            "propertyName": prop.name,
            "Type": "Int32",
            "Flags": 0,
            "Value": prop.value.as_ref().and_then(|v| v.as_i64()).unwrap_or(0),
        })),
        "float" => Ok(json!({
            "propertyName": prop.name,
            "Type": "Float",
            "Flags": 0,
            "Value": prop.value.as_ref().and_then(|v| v.as_f64()).unwrap_or(0.0),
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
                "Type": "Bool",
                "Flags": 0,
                "Value": truthy
            }))
        }
        other if is_compact_standalone_script_type(other) || other.ends_with("_fnvslicecompat") => {
            Ok(json!({
                "propertyName": prop.name,
                "Type": "Object",
                "Flags": 0,
                "Value": {
                    "Alias": -1,
                    "FormID": canonical_vmad_form_id(prop.value.as_ref(), &prop.name)?,
                }
            }))
        }
        other => Err(VmadSynthError(format!(
            "unsupported property type '{other}' for '{}'",
            prop.name
        ))),
    }
}

fn is_compact_standalone_script_type(prop_type: &str) -> bool {
    prop_type.rsplit_once("_s_").is_some_and(|(prefix, local)| {
        !prefix.is_empty() && local.len() == 6 && local.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

pub fn reference_alias_script_property(
    name: &str,
    target_quest_form_key: &str,
    alias_id: i32,
) -> Result<ScriptProperty, VmadSynthError> {
    if name.trim().is_empty() || target_quest_form_key.trim().is_empty() || alias_id < 0 {
        return Err(VmadSynthError(
            "ReferenceAlias binding requires a property name, mapped QUST FormKey, and nonnegative alias id"
                .into(),
        ));
    }
    Ok(ScriptProperty {
        name: name.into(),
        prop_type: "ReferenceAlias".into(),
        value: Some(json!({
            "target_quest_form_key": target_quest_form_key,
            "alias_id": alias_id,
        })),
    })
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
    pub compiled_evidence: CompiledScriptEvidence,
}

// ---------------------------------------------------------------------------
// build_scpt_vmad_intents — pair SCPT targets with translated scripts
// ---------------------------------------------------------------------------

/// One target record awaiting a SCPT VMAD attachment.
pub struct VmadTarget {
    /// FormKey of the target record getting the VMAD binding. Format
    /// matches `TranslatedScript.source_form_key` shape (e.g. `001234:FNV.esm`).
    pub target_form_key: String,
    /// FormKey of the source SCPT to bind. Matched case-insensitively
    /// against `TranslatedScript.source_form_key`.
    pub source_scpt_form_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledScriptEvidence {
    pub class_name: String,
    pub relative_pex_path: String,
    pub compiled_success: bool,
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
    build_scpt_vmad_intents_with_compiled_evidence(vmad_targets, translated_scripts, &[])
        .unwrap_or_default()
}

/// Build VMAD intents only for scripts with explicit successful compiler
/// evidence. Source generation alone is never sufficient to attach a VMAD.
pub fn build_scpt_vmad_intents_with_compiled_evidence(
    vmad_targets: &[VmadTarget],
    translated_scripts: &[super::script_synthesizer::TranslatedScript],
    compiled: &[CompiledScriptEvidence],
) -> Result<Vec<ScriptBindingIntent>, VmadSynthError> {
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

    let compiled_classes = compiled
        .iter()
        .filter(|evidence| evidence.compiled_success && !evidence.relative_pex_path.is_empty())
        .map(|evidence| evidence.class_name.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    let mut out: Vec<ScriptBindingIntent> = Vec::with_capacity(vmad_targets.len());
    for target in vmad_targets {
        let key = normalize_form_key(&target.source_scpt_form_key);
        let translated = match by_key.get(&key) {
            Some(s) => *s,
            None => continue,
        };
        if !compiled_classes.contains(&translated.script_class_name.to_ascii_lowercase()) {
            continue;
        }
        let properties = translated
            .properties
            .iter()
            .map(|property| {
                let target_form_key = property.target_form_key.clone().ok_or_else(|| {
                    VmadSynthError(format!(
                        "script {} property {} has no mapped target FormKey",
                        translated.script_class_name, property.papyrus_name
                    ))
                })?;
                Ok(ScriptProperty {
                    name: property.papyrus_name.clone(),
                    prop_type: property.papyrus_type.clone(),
                    value: Some(Value::String(target_form_key)),
                })
            })
            .collect::<Result<Vec<_>, VmadSynthError>>()?;
        out.push(ScriptBindingIntent {
            target_form_key: target.target_form_key.clone(),
            script_class_name: translated.script_class_name.clone(),
            properties,
            fragment_kind: FragmentKind::Object,
            compiled_evidence: compiled
                .iter()
                .find(|evidence| {
                    evidence
                        .class_name
                        .eq_ignore_ascii_case(&translated.script_class_name)
                        && evidence.compiled_success
                })
                .expect("compiled class set and evidence list must agree")
                .clone(),
        });
    }
    Ok(out)
}

/// Attach a `VirtualMachineAdapter` built from `intent` to a target record.
///
/// Merges into an existing `VirtualMachineAdapter` (or `VMAD`, which is
/// renamed) field; otherwise inserts the payload at index 0 of `fields`. Fails
/// without mutating the record when the intent's script class name is empty.
pub fn attach_vmad_to_record(
    record: &mut Value,
    intent: &ScriptBindingIntent,
) -> Result<(), VmadSynthError> {
    let vmad_payload = synthesize_vmad(intent)?;
    attach_vmad_payload_to_record(record, vmad_payload)
}

/// Merge one VMAD payload into a record without discarding another script or
/// fragment owner already attached during this reconciliation pass.
pub fn attach_vmad_payload_to_record(
    record: &mut Value,
    vmad_payload: Value,
) -> Result<(), VmadSynthError> {
    let new_field = json!({ "VirtualMachineAdapter": vmad_payload.clone() });

    let obj = record
        .as_object_mut()
        .ok_or_else(|| VmadSynthError("attach_vmad_to_record: record is not an object".into()))?;
    let fields_entry = obj.entry("fields").or_insert(Value::Array(Vec::new()));
    let fields = fields_entry
        .as_array_mut()
        .ok_or_else(|| VmadSynthError("attach_vmad_to_record: fields is not an array".into()))?;

    // Preserve existing standalone scripts and fragment/alias metadata.
    for slot in fields.iter_mut() {
        let Some(slot_obj) = slot.as_object_mut() else {
            continue;
        };
        if let Some(existing) = slot_obj.get_mut("VirtualMachineAdapter") {
            merge_vmad_payload(existing, vmad_payload)?;
            return Ok(());
        }
        if let Some(mut existing) = slot_obj.remove("VMAD") {
            merge_vmad_payload(&mut existing, vmad_payload)?;
            slot_obj.insert("VirtualMachineAdapter".to_string(), existing);
            return Ok(());
        }
    }
    // Otherwise insert at index 0.
    fields.insert(0, new_field);
    Ok(())
}

fn merge_vmad_payload(existing: &mut Value, incoming: Value) -> Result<(), VmadSynthError> {
    let existing = existing
        .as_object_mut()
        .ok_or_else(|| VmadSynthError("existing VMAD payload is not an object".into()))?;
    let incoming = incoming
        .as_object()
        .ok_or_else(|| VmadSynthError("incoming VMAD payload is not an object".into()))?;
    for (key, value) in incoming {
        if key == "Scripts" {
            let target = existing
                .entry(key.clone())
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .ok_or_else(|| VmadSynthError("existing VMAD Scripts is not an array".into()))?;
            let scripts = value
                .as_array()
                .ok_or_else(|| VmadSynthError("incoming VMAD Scripts is not an array".into()))?;
            for script in scripts {
                let name = script
                    .get("ScriptName")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !target.iter().any(|current| {
                    current
                        .get("ScriptName")
                        .and_then(Value::as_str)
                        .is_some_and(|current| current.eq_ignore_ascii_case(name))
                }) {
                    target.push(script.clone());
                }
            }
        } else {
            existing.insert(key.clone(), value.clone());
        }
    }
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
    validate_compiled_evidence(&intent.script_class_name, &intent.compiled_evidence)?;
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
pub fn synthesize_topic_info_vmad(
    script_class_name: &str,
    phases: &[InfoFragmentPhase],
    compiled_evidence: &CompiledScriptEvidence,
) -> Result<Value, VmadSynthError> {
    synthesize_topic_info_vmad_with_properties(script_class_name, phases, &[], compiled_evidence)
}

pub fn synthesize_topic_info_vmad_with_properties(
    script_class_name: &str,
    phases: &[InfoFragmentPhase],
    fragment_properties: &[ScriptProperty],
    compiled_evidence: &CompiledScriptEvidence,
) -> Result<Value, VmadSynthError> {
    validate_compiled_evidence(script_class_name, compiled_evidence)?;
    if phases.is_empty() {
        return Err(VmadSynthError(format!(
            "INFO fragment '{script_class_name}' has no source result-script phase"
        )));
    }
    let mut flags = 0_u8;
    let mut fragments = Vec::new();
    for phase in phases {
        let phase_flag = phase.vmad_flag();
        if flags & phase_flag != 0 {
            return Err(VmadSynthError(format!(
                "INFO fragment '{script_class_name}' repeats {:?} phase",
                phase
            )));
        }
        flags |= phase_flag;
        fragments.push(json!({
            "Unknown": 1,
            "ScriptName": script_class_name,
            "FragmentName": phase.psc_function_name(),
        }));
    }
    let mut extra = Map::new();
    extra.insert(
        "Script Fragments".into(),
        json!({
            "Version": 3,
            "Flags": flags,
            "Script": {
                "ScriptName": script_class_name,
                "Flags": 0,
                "Properties": Value::Array(
            fragment_properties
                .iter()
                .map(property_payload)
                .collect::<Result<Vec<_>, _>>()?,
                ),
            },
            "Fragments": fragments,
        }),
    );
    let mut payload = vmad_base(vec![], Some(extra));
    payload["Version"] = json!(6);
    payload["semantic_type"] = json!("INFO");
    Ok(payload)
}

/// A stage fragment description used when synthesizing a quest VMAD.
#[derive(Debug, Clone)]
pub struct QuestStageFragment {
    pub stage_index: i32,
    pub stage_item_index: u16,
    pub psc_function_name: String,
}

#[derive(Debug, Clone)]
pub struct QuestAliasScriptBinding {
    pub script_class_name: String,
    pub properties: Vec<ScriptProperty>,
}

#[derive(Debug, Clone)]
pub struct QuestAliasBinding {
    pub alias_id: i32,
    pub target_quest_form_key: String,
    pub target_form_key: String,
    pub scripts: Vec<QuestAliasScriptBinding>,
}

#[derive(Debug, Clone)]
pub struct PendingQuestFragmentBinding {
    pub target_form_key: String,
    pub script_class_name: String,
    pub fragments: Vec<QuestStageFragment>,
    pub fragment_properties: Vec<ScriptProperty>,
    pub aliases: Vec<QuestAliasBinding>,
}

#[derive(Debug, Clone)]
pub struct PendingObjectScriptBinding {
    pub target_form_key: String,
    pub script_class_name: String,
    pub properties: Vec<ScriptProperty>,
}

#[derive(Debug, Clone)]
pub struct PendingInfoFragmentBinding {
    pub target_form_key: String,
    pub script_class_name: String,
    pub phases: Vec<InfoFragmentPhase>,
    pub fragment_properties: Vec<ScriptProperty>,
}

#[derive(Debug, Clone)]
pub struct PendingSceneFragmentBinding {
    pub target_form_key: String,
    pub script_class_name: String,
    pub action_count: usize,
}

/// Synthesize a VMAD payload for a translated QUST record.
pub fn synthesize_quest_vmad(
    script_class_name: &str,
    fragments: &[QuestStageFragment],
    aliases: &[QuestAliasBinding],
    compiled_evidence: &CompiledScriptEvidence,
) -> Result<Value, VmadSynthError> {
    synthesize_quest_vmad_with_properties(
        script_class_name,
        fragments,
        &[],
        aliases,
        compiled_evidence,
    )
}

pub fn synthesize_quest_vmad_with_properties(
    script_class_name: &str,
    fragments: &[QuestStageFragment],
    fragment_properties: &[ScriptProperty],
    aliases: &[QuestAliasBinding],
    compiled_evidence: &CompiledScriptEvidence,
) -> Result<Value, VmadSynthError> {
    validate_compiled_evidence(script_class_name, compiled_evidence)?;
    let fragment_properties = fragment_properties
        .iter()
        .map(property_payload)
        .collect::<Result<Vec<_>, _>>()?;
    let fragment_list: Vec<Value> = fragments
        .iter()
        .map(|f| {
            json!({
                "Quest Stage": f.stage_index,
                "Unknown": 0,
                "Quest Stage Index": f.stage_item_index,
                "Unknown1": 0,
                "ScriptName": script_class_name,
                "FragmentName": f.psc_function_name,
            })
        })
        .collect();

    let alias_list: Vec<Value> = aliases
        .iter()
        .map(|alias| {
            if alias.alias_id < 0 {
                return Err(VmadSynthError(format!(
                    "quest alias {} has a negative alias id",
                    alias.target_form_key
                )));
            }
            if alias.target_quest_form_key.is_empty() || alias.target_form_key.is_empty() {
                return Err(VmadSynthError(format!(
                    "quest alias {} is missing mapped quest/target FormKeys",
                    alias.alias_id
                )));
            }
            let scripts = alias
                .scripts
                .iter()
                .map(|script| {
                    let properties = script
                        .properties
                        .iter()
                        .map(property_payload)
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(json!({
                        "ScriptName": script.script_class_name,
                        "Flags": 0,
                        "Properties": properties,
                    }))
                })
                .collect::<Result<Vec<_>, VmadSynthError>>()?;
            let form_id = canonical_vmad_form_id(
                Some(&Value::String(alias.target_quest_form_key.clone())),
                "quest alias",
            )?;
            Ok(json!({
                "Object": {
                    "Unused": 0,
                    "Alias": alias.alias_id,
                    "FormID": form_id,
                },
                "Version": 0,
                "Object Format": 2,
                "Alias Scripts": scripts,
            }))
        })
        .collect::<Result<Vec<_>, VmadSynthError>>()?;

    let mut extra = Map::new();
    extra.insert(
        "Script Fragments".into(),
        json!({
            "Version": 1,
            "Script": {
                "ScriptName": script_class_name,
                "Flags": 0,
                "Properties": fragment_properties,
            },
            "Fragments": fragment_list,
            "Aliases": alias_list,
        }),
    );
    let mut payload = vmad_base(vec![], Some(extra));
    payload["semantic_type"] = json!("QUST");
    Ok(payload)
}

/// Synthesize a VMAD payload for a translated SCEN record.
pub fn synthesize_scene_vmad(
    script_class_name: &str,
    action_count: usize,
    compiled_evidence: &CompiledScriptEvidence,
) -> Result<Value, VmadSynthError> {
    validate_compiled_evidence(script_class_name, compiled_evidence)?;
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
    extra.insert(
        "Script Fragments".into(),
        json!({
            "Version": 1,
            "Flags": 0,
            "Script": {
                "ScriptName": script_class_name,
                "Flags": 0,
                "Properties": [],
            },
            "Fragments": fragment_list,
            "Phase Fragments": [],
        }),
    );
    let mut payload = vmad_base(vec![], Some(extra));
    payload["semantic_type"] = json!("SCEN");
    Ok(payload)
}

fn validate_compiled_evidence(
    script_class_name: &str,
    evidence: &CompiledScriptEvidence,
) -> Result<(), VmadSynthError> {
    if !evidence.compiled_success
        || evidence.relative_pex_path.trim().is_empty()
        || !evidence.class_name.eq_ignore_ascii_case(script_class_name)
    {
        return Err(VmadSynthError(format!(
            "script '{script_class_name}' has no matching compiled-success evidence"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn compiled(class_name: &str) -> CompiledScriptEvidence {
        CompiledScriptEvidence {
            class_name: class_name.into(),
            relative_pex_path: format!("data/Scripts/{class_name}.pex"),
            compiled_success: true,
        }
    }

    #[test]
    fn topic_info_begin_vmad_matches_fo4_fragment_contract() {
        let vmad = synthesize_topic_info_vmad(
            "TIF__001234",
            &[InfoFragmentPhase::Begin],
            &compiled("TIF__001234"),
        )
        .unwrap();
        assert_eq!(vmad["Version"], 6);
        assert_eq!(vmad["Object Format"], 2);
        assert_eq!(vmad["Script Fragments"]["Version"], 3);
        assert_eq!(vmad["Script Fragments"]["Flags"], 1);
        assert_eq!(
            vmad["Script Fragments"]["Script"]["ScriptName"],
            "TIF__001234"
        );
        let frags = vmad["Script Fragments"]["Fragments"].as_array().unwrap();
        assert_eq!(frags.len(), 1);
        assert_eq!(frags[0]["Unknown"], 1);
        assert_eq!(frags[0]["FragmentName"], "Fragment_Begin");
    }

    #[test]
    fn topic_info_end_and_combined_vmad_preserve_phase_flags() {
        let end = synthesize_topic_info_vmad(
            "TIF__134B9B",
            &[InfoFragmentPhase::End],
            &compiled("TIF__134B9B"),
        )
        .unwrap();
        assert_eq!(end["Script Fragments"]["Flags"], 2);
        assert_eq!(
            end["Script Fragments"]["Fragments"][0]["FragmentName"],
            "Fragment_End"
        );

        let both = synthesize_topic_info_vmad(
            "TIF__001234",
            &[InfoFragmentPhase::Begin, InfoFragmentPhase::End],
            &compiled("TIF__001234"),
        )
        .unwrap();
        assert_eq!(both["Script Fragments"]["Flags"], 3);
        assert_eq!(
            both["Script Fragments"]["Fragments"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn topic_info_vmad_binds_typed_quest_script_property() {
        let vmad = synthesize_topic_info_vmad_with_properties(
            "TIF__130161",
            &[InfoFragmentPhase::Begin],
            &[ScriptProperty {
                name: "VTechatticup".into(),
                prop_type: "FNV_FO3_S_11FC64".into(),
                value: Some(json!("21F935:FalloutNV.esm")),
            }],
            &compiled("TIF__130161"),
        )
        .unwrap();
        let properties = vmad["Script Fragments"]["Script"]["Properties"]
            .as_array()
            .unwrap();
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0]["propertyName"], "VTechatticup");
        assert_eq!(properties[0]["Type"], "Object");
        assert_eq!(
            properties[0]["Value"]["FormID"]["reference"],
            json!({
                "plugin": "FalloutNV.esm",
                "object_id": "21F935"
            })
        );
    }

    #[test]
    fn quest_vmad_fragments_and_aliases() {
        let frags = vec![
            QuestStageFragment {
                stage_index: 10,
                stage_item_index: 0,
                psc_function_name: "Fragment_10".into(),
            },
            QuestStageFragment {
                stage_index: 20,
                stage_item_index: 2,
                psc_function_name: "Fragment_20".into(),
            },
        ];
        let aliases = vec![QuestAliasBinding {
            alias_id: 7,
            target_quest_form_key: "201234:Output.esm".into(),
            target_form_key: "202345:Output.esm".into(),
            scripts: Vec::new(),
        }];
        let vmad = synthesize_quest_vmad(
            "QF_B21_001234",
            &frags,
            &aliases,
            &compiled("QF_B21_001234"),
        )
        .unwrap();
        assert_eq!(vmad["Version"], 5);
        let frag_list = vmad["Script Fragments"]["Fragments"].as_array().unwrap();
        assert_eq!(frag_list.len(), 2);
        assert_eq!(frag_list[0]["Quest Stage"], 10);
        assert_eq!(frag_list[1]["FragmentName"], "Fragment_20");
        let aliases = vmad["Script Fragments"]["Aliases"].as_array().unwrap();
        assert_eq!(frag_list[1]["Quest Stage Index"], 2);
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0]["Object"]["Alias"], 7);
        assert_eq!(
            aliases[0]["Object"]["FormID"]["reference"],
            json!({ "plugin": "Output.esm", "object_id": "201234" })
        );
        let alias_json = serde_json::to_string(aliases).unwrap();
        assert!(!alias_json.contains(":-1"));
        assert!(!alias_json.contains("null"));
    }

    #[test]
    fn quest_vmad_preserves_fragment_perk_and_alias_package_properties() {
        let aliases = vec![QuestAliasBinding {
            alias_id: 2,
            target_quest_form_key: "201234:Output.esm".into(),
            target_form_key: "202345:Output.esm".into(),
            scripts: vec![QuestAliasScriptBinding {
                script_class_name: "B21_TecMineHostageEscapeAlias".into(),
                properties: vec![ScriptProperty {
                    name: "TecMineHostageEscape".into(),
                    prop_type: "Package".into(),
                    value: Some(json!("209ABC:Output.esm")),
                }],
            }],
        }];
        let vmad = synthesize_quest_vmad_with_properties(
            "QF_B21_06136D",
            &[],
            &[ScriptProperty {
                name: "PowerArmorTraining".into(),
                prop_type: "Perk".into(),
                value: Some(json!("208FDF:Output.esm")),
            }],
            &aliases,
            &compiled("QF_B21_06136D"),
        )
        .unwrap();
        assert_eq!(
            vmad["Script Fragments"]["Script"]["Properties"][0]["propertyName"],
            "PowerArmorTraining"
        );
        assert_eq!(
            vmad["Script Fragments"]["Aliases"][0]["Alias Scripts"][0]["ScriptName"],
            "B21_TecMineHostageEscapeAlias"
        );
        assert_eq!(
            vmad["Script Fragments"]["Aliases"][0]["Alias Scripts"][0]["Properties"][0]["propertyName"],
            "TecMineHostageEscape"
        );
    }

    #[test]
    fn scene_vmad_fragment_count() {
        let vmad =
            synthesize_scene_vmad("SF_MyScene_001234", 3, &compiled("SF_MyScene_001234")).unwrap();
        let frags = vmad["Script Fragments"]["Fragments"].as_array().unwrap();
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
            compiled_evidence: compiled(""),
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
            compiled_evidence: compiled("MyScript"),
        };
        let vmad = synthesize_vmad(&intent).unwrap();
        let scripts = vmad["Scripts"].as_array().unwrap();
        assert_eq!(scripts.len(), 1);
        let props = scripts[0]["Properties"].as_array().unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[0]["Type"], "Int32");
        assert_eq!(props[1]["Type"], "String");
    }

    #[test]
    fn vmad_rejects_missing_or_mismatched_compile_evidence() {
        let mut intent = intent("MyScript");
        intent.compiled_evidence.compiled_success = false;
        assert!(synthesize_vmad(&intent).is_err());
        intent.compiled_evidence.compiled_success = true;
        intent.compiled_evidence.class_name = "OtherScript".into();
        assert!(synthesize_vmad(&intent).is_err());
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

    #[test]
    fn reference_alias_property_encodes_quest_and_alias_identity() {
        let property =
            reference_alias_script_property("TecMineHostageEscapeData", "21F935:Target.esp", 7)
                .unwrap();
        let payload = property_payload(&property).unwrap();
        let object = &payload["Value"];
        assert_eq!(
            object["FormID"]["reference"],
            json!({ "plugin": "Target.esp", "object_id": "21F935" })
        );
        assert_eq!(object["Alias"], 7);
        assert!(reference_alias_script_property("", "21F935:Target.esp", 7).is_err());
        assert!(reference_alias_script_property("Alias", "21F935:Target.esp", -1).is_err());
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
            properties: Vec::new(),
            package_data_aliases: Vec::new(),
            dedicated_topics: Vec::new(),
            compile_status: super::super::script_synthesizer::ScriptCompileStatus::SourceGeneratedPendingCompile,
            terminal_status: super::super::script_synthesizer::ScriptTerminalStatus::SourceGeneratedPendingCompile,
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
        let evidence = scripts
            .iter()
            .map(|script| CompiledScriptEvidence {
                class_name: script.script_class_name.clone(),
                relative_pex_path: format!("data/Scripts/{}.pex", script.script_class_name),
                compiled_success: true,
            })
            .collect::<Vec<_>>();
        let intents =
            build_scpt_vmad_intents_with_compiled_evidence(&targets, &scripts, &evidence).unwrap();
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
        let evidence = vec![CompiledScriptEvidence {
            class_name: "Known".into(),
            relative_pex_path: "data/Scripts/Known.pex".into(),
            compiled_success: true,
        }];
        let intents =
            build_scpt_vmad_intents_with_compiled_evidence(&targets, &scripts, &evidence).unwrap();
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
        let evidence = vec![CompiledScriptEvidence {
            class_name: "Foo".into(),
            relative_pex_path: "data/Scripts/Foo.pex".into(),
            compiled_success: true,
        }];
        let intents =
            build_scpt_vmad_intents_with_compiled_evidence(&targets, &scripts, &evidence).unwrap();
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
        let evidence = vec![CompiledScriptEvidence {
            class_name: "Orphan".into(),
            relative_pex_path: "data/Scripts/Orphan.pex".into(),
            compiled_success: true,
        }];
        let intents =
            build_scpt_vmad_intents_with_compiled_evidence(&targets, &scripts, &evidence).unwrap();
        assert!(intents.is_empty());
    }

    #[test]
    fn source_generation_without_compile_evidence_never_binds_vmad() {
        let scripts = vec![make_translated("Foo", "001ABC:FNV.esm")];
        let targets = vec![VmadTarget {
            target_form_key: "T1:Output.esp".into(),
            source_scpt_form_key: "001ABC:FNV.esm".into(),
        }];
        assert!(build_scpt_vmad_intents(&targets, &scripts).is_empty());
    }

    #[test]
    fn compiled_binding_requires_mapped_property_formkey() {
        let mut script = make_translated("Foo", "001ABC:FNV.esm");
        script
            .properties
            .push(super::super::script_synthesizer::TranslatedProperty {
                source_name: "SomeActor".into(),
                papyrus_name: "SomeActor".into(),
                papyrus_type: "Actor".into(),
                source_form_key: "111111:FNV.esm".into(),
                target_form_key: None,
            });
        let targets = vec![VmadTarget {
            target_form_key: "T1:Output.esp".into(),
            source_scpt_form_key: "001ABC:FNV.esm".into(),
        }];
        let evidence = vec![CompiledScriptEvidence {
            class_name: "Foo".into(),
            relative_pex_path: "data/Scripts/Foo.pex".into(),
            compiled_success: true,
        }];
        assert!(
            build_scpt_vmad_intents_with_compiled_evidence(&targets, &[script], &evidence).is_err()
        );
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
            compiled_evidence: compiled(class_name),
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
    fn attach_vmad_merges_existing_virtual_machine_adapter() {
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
        assert_eq!(new_vmad["stale"], true);
        assert_eq!(new_vmad["Scripts"][0]["ScriptName"], "Foo");
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
