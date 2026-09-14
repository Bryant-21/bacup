//! DIAL grouping and INFO fragment translation.
//!
//! Mirrors `fnv_legacy_scripting/dialogue.py`.

use std::collections::HashSet;

use encoding_rs::WINDOWS_1252;
use serde_json::{Value, json};

use super::form_keys::object_id_from_form_key;
use super::naming::{standalone_script_name, topic_info_fragment_name};
use super::quest::{extract_first_event_body, field_value, filtered_payload};
use super::voice::{fnv_to_fo4_voice_path, fnv_voice_source_path};
use super::{FnvScriptContext, TranslateError, translate_to_papyrus};

#[path = "dialogue_ir.rs"]
pub mod ir;
pub use ir::{
    DialogueLowerError, DialogueResponseIr, HostageGreetingProjection, LoweredInfo, LoweredTopic,
    VoiceManifestRequest, VoiceResolution, lower_dial_topic, lower_hostage_greeting_info_record,
    lower_hostage_greeting_projection, lower_hostage_greeting_topic, lower_info_record,
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A group of FNV DIAL topics sharing the same owning quest.
#[derive(Debug, Clone)]
pub struct DialogueGroup {
    pub quest_owner_form_key: String,
    pub topics: Vec<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InfoFragmentPhase {
    Begin,
    End,
}

impl InfoFragmentPhase {
    pub fn psc_function_name(self) -> &'static str {
        match self {
            Self::Begin => "Fragment_Begin",
            Self::End => "Fragment_End",
        }
    }

    pub fn vmad_flag(self) -> u8 {
        match self {
            Self::Begin => 1,
            Self::End => 2,
        }
    }
}

/// Result of translating one INFO record.
#[derive(Debug, Clone)]
pub struct TranslatedInfo {
    pub source_form_key: String,
    pub fragment_class_name: Option<String>,
    /// Papyrus `.psc` source text (None when the INFO has no SCTX).
    pub fragment_psc_text: Option<String>,
    pub fragment_phases: Vec<InfoFragmentPhase>,
    pub fragment_properties: Vec<InfoFragmentProperty>,
    pub voice_target_path: String,
    pub voice_source_path: String,
    /// LIP sync data is always dropped (not carried across).
    pub lip_dropped: bool,
    pub lip_regeneration_target: Option<String>,
    pub authoring_record_payload: Option<Value>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoFragmentProperty {
    pub name: String,
    pub papyrus_type: String,
    pub source_form_key: String,
}

// ---------------------------------------------------------------------------
// group_dial_records
// ---------------------------------------------------------------------------

/// Group FNV DIAL records by every repeated source quest owner (`QSTI`).
pub fn group_dial_records(dial_records: &[Value]) -> Vec<DialogueGroup> {
    let mut map: indexmap::IndexMap<String, Vec<Value>> = indexmap::IndexMap::new();
    for record in dial_records {
        for owner in record_field_strings(record, "QSTI") {
            map.entry(owner).or_default().push(record.clone());
        }
    }
    map.into_iter()
        .map(|(quest_owner_form_key, topics)| DialogueGroup {
            quest_owner_form_key,
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
/// Each DIAL is re-emitted without its FNV dialogue-script subrecords
/// (`SCTX`/`VTCK`/`VMAD`/`VirtualMachineAdapter`).
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
    let resolved_voice_type = extract_voice_type(record);
    let voice_type = resolved_voice_type
        .clone()
        .unwrap_or_else(|| format!("UNRESOLVED_{short_form_id}"));

    let voice_target =
        fnv_to_fo4_voice_path(mod_prefix, source_plugin, &voice_type, &short_form_id);
    let voice_source = fnv_voice_source_path(source_plugin, &voice_type, &short_form_id);

    let scripts = info_script_fragments(record)?;
    let fragment_properties = info_fragment_properties(record, mod_prefix, &scripts)?;
    let (fragment_class_name, fragment_psc_text, fragment_phases) = if scripts.is_empty() {
        (None, None, Vec::new())
    } else {
        let class_name = topic_info_fragment_name(&short_form_id);
        let ctx =
            FnvScriptContext::load_for_exact_slice_record("INFO", source_form_key, mod_prefix)
                .map_err(|e| TranslateError::Semantic(format!("load ctx: {e}")))?;
        let mut translated = Vec::with_capacity(scripts.len());
        for (phase, source) in scripts {
            let normalized_source = normalize_info_fragment_source(&source);
            let wrapped = format!("begin GameMode\n{normalized_source}\nend\n");
            let papyrus = translate_to_papyrus(&wrapped, &ctx, &class_name, "TopicInfo")?;
            translated.push((phase, extract_first_event_body(&papyrus)));
        }
        let phases = translated.iter().map(|(phase, _)| *phase).collect();
        let psc_text = build_info_psc(&class_name, &fragment_properties, &translated);
        (Some(class_name), Some(psc_text), phases)
    };

    let payload = build_info_payload(record, fragment_class_name.as_deref(), strict);

    Ok(TranslatedInfo {
        source_form_key: source_form_key.to_string(),
        fragment_class_name: fragment_class_name.clone(),
        fragment_psc_text,
        fragment_phases,
        fragment_properties,
        voice_target_path: voice_target.clone(),
        voice_source_path: voice_source,
        lip_dropped: true,
        lip_regeneration_target: Some(voice_target),
        authoring_record_payload: Some(payload),
        warnings: resolved_voice_type
            .is_none()
            .then(|| {
                format!(
                    "INFO {source_form_key} has no resolved target VTYP; voice manifest is unresolved"
                )
            })
            .into_iter()
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// PSC builder
// ---------------------------------------------------------------------------

fn build_info_psc(
    class_name: &str,
    properties: &[InfoFragmentProperty],
    fragments: &[(InfoFragmentPhase, String)],
) -> String {
    let mut out = format!("ScriptName {class_name} extends TopicInfo\n\n");
    for property in properties {
        out.push_str(&format!(
            "{} Property {} Auto Const\n",
            property.papyrus_type, property.name
        ));
    }
    if !properties.is_empty() {
        out.push('\n');
    }
    for (phase, body) in fragments {
        out.push_str(&format!("Function {}()\n", phase.psc_function_name()));
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

fn build_info_payload(record: &Value, _fragment_class_name: Option<&str>, _strict: bool) -> Value {
    let drop_fields: HashSet<&str> = [
        "SCHR",
        "SCDA",
        "SCTX",
        "SCRO",
        "NEXT",
        "ScriptBegins",
        "ScriptEnds",
        "VTCK",
        "VMAD",
        "VirtualMachineAdapter",
    ]
    .into_iter()
    .collect();
    filtered_payload(record, &drop_fields)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn extract_voice_type(record: &Value) -> Option<String> {
    if let Some(resolved) = record
        .get("__target_voice_folder")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        return Some(resolved.to_string());
    }
    if let Some(fields) = record.get("fields").and_then(|f| f.as_array()) {
        for field in fields {
            if let Some(vtck) = field_value(field, "VTCK") {
                // May be a plain string or a dict with "voice_type" key.
                if let Some(s) = vtck.as_str() {
                    return Some(s.to_string());
                }
                if let Some(obj) = vtck.as_object() {
                    if let Some(vt) = obj.get("voice_type").and_then(|v| v.as_str()) {
                        return Some(vt.to_string());
                    }
                }
            }
        }
    }
    None
}

fn record_field_strings(record: &Value, sig: &str) -> Vec<String> {
    record
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|field| field_value(field, sig).and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn info_script_fragments(
    record: &Value,
) -> Result<Vec<(InfoFragmentPhase, String)>, TranslateError> {
    let mut fragments = Vec::new();
    let Some(fields) = record.get("fields").and_then(Value::as_array) else {
        return Ok(fragments);
    };
    let mut flat_phase = InfoFragmentPhase::Begin;
    for field in fields {
        let Some(object) = field.as_object().filter(|object| object.len() == 1) else {
            continue;
        };
        let (signature, value) = object.iter().next().unwrap();
        let phase = match signature.as_str() {
            "ScriptBegins" => {
                flat_phase = InfoFragmentPhase::Begin;
                Some(flat_phase)
            }
            "ScriptEnds" | "NEXT" => {
                flat_phase = InfoFragmentPhase::End;
                Some(flat_phase)
            }
            "SCHR" => Some(flat_phase),
            "SCTX" | "EmbeddedScriptSource" => {
                let source = decode_script_source(value)?;
                if !source.trim().is_empty() {
                    fragments.push((flat_phase, source));
                }
                None
            }
            _ => None,
        };
        let Some(phase) = phase else { continue };
        let mut sources = Vec::new();
        collect_nested_script_sources(value, &mut sources)?;
        for source in sources {
            if !source.trim().is_empty() {
                fragments.push((phase, source));
            }
        }
    }
    for phase in [InfoFragmentPhase::Begin, InfoFragmentPhase::End] {
        if fragments
            .iter()
            .filter(|(found, _)| *found == phase)
            .count()
            > 1
        {
            return Err(TranslateError::Semantic(format!(
                "INFO contains multiple {:?} result scripts; FO4 has one fragment slot per phase",
                phase
            )));
        }
    }
    fragments.sort_by_key(|(phase, _)| phase.vmad_flag());
    Ok(fragments)
}

fn normalize_info_fragment_source(source: &str) -> String {
    let normalized = source
        .lines()
        .map(|line| {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() == 3
                && parts[0].eq_ignore_ascii_case("setstage")
                && parts[1].eq_ignore_ascii_case("VTechatticup")
            {
                let indentation = &line[..line.len() - line.trim_start().len()];
                format!("{indentation}SetStage(VTechatticup, {})", parts[2])
            } else if parts.len() == 4
                && parts[0].eq_ignore_ascii_case("set")
                && parts[1].eq_ignore_ascii_case("VTechatticup.HostageStorVar")
                && parts[2].eq_ignore_ascii_case("to")
            {
                let indentation = &line[..line.len() - line.trim_start().len()];
                format!("{indentation}SetVTechatticupHostageStorVar({})", parts[3])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    super::quest::normalize_fragment_command_syntax(&normalized)
}

fn info_fragment_properties(
    record: &Value,
    mod_prefix: &str,
    scripts: &[(InfoFragmentPhase, String)],
) -> Result<Vec<InfoFragmentProperty>, TranslateError> {
    if !scripts.iter().any(|(_, source)| {
        source
            .to_ascii_lowercase()
            .contains("vtechatticup.hostagestorvar")
    }) {
        return Ok(Vec::new());
    }
    let mut references = Vec::new();
    if let Some(fields) = record.get("fields") {
        collect_nested_script_references(fields, &mut references);
    }
    references.sort();
    references.dedup();
    let source_form_key = references
        .into_iter()
        .find(|reference| reference.eq_ignore_ascii_case("11F935:FalloutNV.esm"))
        .ok_or_else(|| {
            TranslateError::Semantic(
                "INFO HostageStorVar result script is missing exact SCRO 11F935:FalloutNV.esm"
                    .to_string(),
            )
        })?;
    Ok(vec![InfoFragmentProperty {
        name: "VTechatticup".to_string(),
        papyrus_type: standalone_script_name(mod_prefix, 0x11FC64),
        source_form_key,
    }])
}

fn collect_nested_script_references(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_nested_script_references(value, output);
            }
        }
        Value::Object(object) => {
            if let Some(reference) = object.get("SCRO").and_then(script_form_key) {
                output.push(reference);
            }
            for value in object.values() {
                collect_nested_script_references(value, output);
            }
        }
        _ => {}
    }
}

fn script_form_key(value: &Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        return Some(value.to_string());
    }
    let reference = value.get("reference").unwrap_or(value);
    let plugin = reference.get("plugin")?.as_str()?;
    let local = reference
        .get("object_id")
        .or_else(|| reference.get("local"))?
        .as_str()?;
    Some(format!("{local}:{plugin}"))
}

fn collect_nested_script_sources(
    value: &Value,
    output: &mut Vec<String>,
) -> Result<(), TranslateError> {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_nested_script_sources(value, output)?;
            }
        }
        Value::Object(object) => {
            for (key, value) in object {
                if matches!(key.as_str(), "SCTX" | "EmbeddedScriptSource") {
                    output.push(decode_script_source(value)?);
                } else {
                    collect_nested_script_sources(value, output)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn decode_script_source(value: &Value) -> Result<String, TranslateError> {
    if let Some(source) = value.as_str() {
        return Ok(source.trim_end_matches('\0').to_string());
    }
    let bytes = super::quest_ir::raw_bytes(value).ok_or_else(|| {
        TranslateError::Semantic("INFO SCTX is not a raw-bytes-hex payload".to_string())
    })?;
    let (decoded, _, _) = WINDOWS_1252.decode(&bytes);
    Ok(decoded.trim_end_matches('\0').to_string())
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
            "__target_voice_folder": "VTechatticupRenolds",
            "fields": []
        });
        let result = translate_info_record(&record, "B21", "FNV.esm", false, "001234:FNV.esm");
        let ti = result.expect("translate ok");
        assert!(ti.fragment_class_name.is_none());
        assert!(ti.fragment_psc_text.is_none());
        assert!(ti.fragment_phases.is_empty());
        assert!(ti.voice_target_path.contains("FNV.esm"));
        assert!(ti.lip_dropped);
    }

    #[test]
    fn translate_info_record_with_sctx() {
        let record = json!({
            "eid": "Info2",
            "__target_voice_folder": "VTechatticupRenolds",
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
        assert!(psc.contains("Fragment_Begin"));
        assert_eq!(ti.fragment_phases, [InfoFragmentPhase::Begin]);
        let fields = ti
            .authoring_record_payload
            .as_ref()
            .and_then(|payload| payload.get("fields"))
            .and_then(Value::as_array)
            .unwrap();
        assert!(fields.iter().all(|field| {
            !field
                .as_object()
                .is_some_and(|field| field.contains_key("VirtualMachineAdapter"))
        }));
    }

    #[test]
    fn translate_info_record_without_voice_resolution_never_defaults_voice() {
        let record = json!({
            "fields": []
        });
        let translated = translate_info_record(&record, "B21", "FNV.esm", false, "001234:FNV.esm")
            .expect("legacy wrapper records unresolved voice metadata");
        assert!(translated.voice_target_path.contains("UNRESOLVED_001234"));
        assert!(!translated.voice_target_path.contains("MaleEvenToned"));
        assert_eq!(translated.warnings.len(), 1);
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
    fn group_dial_records_groups_by_repeated_quest_owners() {
        let records = vec![
            json!({ "fields": [{ "QSTI": "001:FNV.esm" }] }),
            json!({ "fields": [{ "QSTI": "002:FNV.esm" }] }),
            json!({ "fields": [{ "QSTI": "001:FNV.esm" }, { "QSTI": "002:FNV.esm" }] }),
        ];
        let groups = group_dial_records(&records);
        assert_eq!(groups.len(), 2);
        let grp1 = groups
            .iter()
            .find(|g| g.quest_owner_form_key == "001:FNV.esm")
            .unwrap();
        assert_eq!(grp1.topics.len(), 2);
        let grp2 = groups
            .iter()
            .find(|g| g.quest_owner_form_key == "002:FNV.esm")
            .unwrap();
        assert_eq!(grp2.topics.len(), 2);
    }

    #[test]
    fn build_info_psc_format() {
        let psc = build_info_psc(
            "TIF__001234",
            &[],
            &[(InfoFragmentPhase::End, "x = 1".to_string())],
        );
        assert!(psc.contains("ScriptName TIF__001234 extends TopicInfo"));
        assert!(psc.contains("Function Fragment_End()"));
        assert!(psc.contains("    x = 1"));
        assert!(psc.contains("EndFunction"));
    }

    #[test]
    fn live_info_130161_preserves_begin_result_script() {
        let record = json!({
            "__target_voice_folder": "VTechatticupRenolds",
            "fields": [{
                "SCHR": [{
                    "SCHR": { "RefCount": 1, "CompiledSize": 28 },
                    "SCTX": { "encoding": "raw-bytes-hex", "hex": "7365747374616765207674656368617474696375702031300D0A736574207674656368617474696375702E486F737461676553746F7256617220746F2032" },
                    "SCRO": { "reference": { "plugin": "FalloutNV.esm", "object_id": "11F935" } }
                }]
            }]
        });
        let translated = translate_info_record(
            &record,
            "FNV_FO3",
            "FalloutNV.esm",
            true,
            "130161:FalloutNV.esm",
        )
        .unwrap();
        assert_eq!(translated.fragment_phases, [InfoFragmentPhase::Begin]);
        let psc = translated.fragment_psc_text.as_deref().unwrap();
        assert!(psc.contains("Function Fragment_Begin()"));
        assert!(psc.contains("VTechatticup.SetStage(10)"));
        assert!(psc.contains("VTechatticup.HostageStorVar = 2"));
        assert!(psc.contains("FNV_FO3_S_11FC64 Property VTechatticup Auto Const"));
        assert_eq!(
            translated.fragment_properties,
            [InfoFragmentProperty {
                name: "VTechatticup".into(),
                papyrus_type: "FNV_FO3_S_11FC64".into(),
                source_form_key: "11F935:FalloutNV.esm".into(),
            }]
        );
        assert!(translated.fragment_properties[0].papyrus_type.len() <= 38);
        assert!(!psc.contains("Fragment_End"));
    }

    #[test]
    fn live_info_134b9b_preserves_end_result_script() {
        let record = json!({
            "__target_voice_folder": "VTechatticupRenolds",
            "fields": [
                { "SCHR": [{ "SCHR": { "Flags": ["Enabled"] } }] },
                { "NEXT": [{
                    "Marker": true,
                    "SCHR": { "RefCount": 1, "CompiledSize": 28 },
                    "SCTX": { "encoding": "raw-bytes-hex", "hex": "736574537461676520565465636861747469637570203130300D0A736574207674656368617474696375702E486F737461676553746F7256617220746F2033" },
                    "SCRO": { "reference": { "plugin": "FalloutNV.esm", "object_id": "11F935" } }
                }] }
            ]
        });
        let translated = translate_info_record(
            &record,
            "FNV_FO3",
            "FalloutNV.esm",
            true,
            "134B9B:FalloutNV.esm",
        )
        .unwrap();
        assert_eq!(translated.fragment_phases, [InfoFragmentPhase::End]);
        let psc = translated.fragment_psc_text.as_deref().unwrap();
        assert!(psc.contains("Function Fragment_End()"));
        assert!(psc.contains("VTechatticup.SetStage(100)"));
        assert!(psc.contains("VTechatticup.HostageStorVar = 3"));
        assert!(!psc.contains("Fragment_Begin"));
    }

    #[test]
    fn production_flat_info_134b9b_marks_trailing_sctx_as_end() {
        let record = json!({
            "__target_voice_folder": "VTechatticupRenolds",
            "fields": [
                { "SCHR": { "Flags": ["Enabled"], "RefCount": 0 } },
                { "NEXT": null },
                { "SCHR": { "Flags": ["Enabled"], "RefCount": 1, "CompiledSize": 28 } },
                { "SCDA": { "encoding": "raw-bytes-hex", "hex": "01020304" } },
                { "SCTX": { "encoding": "raw-bytes-hex", "hex": "736574537461676520565465636861747469637570203130300D0A736574207674656368617474696375702E486F737461676553746F7256617220746F2033" } },
                { "SCRO": { "reference": { "plugin": "FalloutNV.esm", "object_id": "11F935" } } }
            ]
        });
        let translated = translate_info_record(
            &record,
            "FNV_FO3",
            "FalloutNV.esm",
            true,
            "134B9B:FalloutNV.esm",
        )
        .unwrap();
        assert_eq!(translated.fragment_phases, [InfoFragmentPhase::End]);
        let psc = translated.fragment_psc_text.unwrap();
        assert!(psc.contains("Function Fragment_End()"));
        assert!(!psc.contains("Fragment_Begin"));
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
