//! SCPT record translation.
//!
//! # What this does
//! Take a parsed SCPT record (serde_json::Value matching the canonical
//! authoring-dir shape) and produce a `TranslatedScript` that captures:
//!   - The Papyrus class name derived from EditorID via
//!     `naming::standalone_script_name`.
//!   - The PapyrusType (ObjectReference / Quest / MagicEffect) derived
//!     from the SCHR `type` field.
//!   - The translated Papyrus `.psc` source text from `translate_to_papyrus`.
//!
//! `.psc` file emission is deliberately NOT in this module — the translated
//! text lives on `TranslatedScript.psc_text` so callers can hand it to a
//! file-writer later, matching `TranslatedInfo::fragment_psc_text` for INFO.
//!
//! # Deviations from Python
//! - **SCDA decompile stub.** The Rust path calls
//!   `fnv_script_native::decompile::decompile_bytecode` directly (no Python
//!   round-trip) when SCTX is absent. The underlying function is currently a
//!   stub that returns `Err(FnvScriptError::Decompile(...))` for all inputs,
//!   so SCDA-only records still surface a `TranslateError::Semantic` —
//!   identical to the Python path when `fnv_script_native` returns an error.
//!   When the decompile stub is filled in, both paths will produce translated
//!   output without any further wiring changes.
//! - **`fragment_psc_text` carries the translated source for later file
//!   emission**; Python writes the file in-line. Decoupling lets the
//!   payload-construction and file-write phases be tested independently
//!   and keeps the script-translation step free of I/O for the unit tests.

use fnv_script_native::decompile::decompile_bytecode;
use serde_json::Value;

use super::function_map::FnvScriptContext;
use super::naming::standalone_script_name;
use super::{TranslateError, translate_to_papyrus};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Mirrors Python `_SCHR_TYPE_TO_PAPYRUS`. ObjectReference is the safe fallback
/// for unknown types (Python returns it for any non-matching int).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PapyrusType {
    ObjectReference,
    Quest,
    MagicEffect,
}

impl PapyrusType {
    pub fn extends_class(self) -> &'static str {
        match self {
            PapyrusType::ObjectReference => "ObjectReference",
            PapyrusType::Quest => "Quest",
            PapyrusType::MagicEffect => "MagicEffect",
        }
    }

    /// Python `_papyrus_type_from_schr` mapping: SCHR.type ∈ {0, 1, 0x100}.
    /// Any other value (including missing) falls back to `ObjectReference`.
    pub fn from_schr_type(t: i64) -> Self {
        match t {
            1 => PapyrusType::Quest,
            0x100 => PapyrusType::MagicEffect,
            _ => PapyrusType::ObjectReference,
        }
    }
}

/// One translated SCPT record. Mirrors Python `TranslatedScript`.
///
/// `psc_text` carries the Papyrus source — the caller (the .psc-emission
/// slice) will write it to `mod_path / "Source" / "User" / f"{script_class_name}.psc"`.
#[derive(Debug, Clone)]
pub struct TranslatedScript {
    pub source_editor_id: String,
    pub source_form_key: String,
    pub script_class_name: String,
    pub papyrus_type: PapyrusType,
    pub psc_text: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Translate one SCPT record into a `TranslatedScript`.
///
/// Returns `Err(TranslateError)` when the script source is missing
/// (no SCTX, and SCDA decompile is not yet ported), or when the Papyrus
/// translation pipeline rejects the source. Mirrors Python
/// `translate_scpt_record` minus the file-write step (deferred to the
/// .psc-emission slice).
pub fn translate_scpt_record(
    record: &Value,
    mod_prefix: &str,
    source_form_key: &str,
) -> Result<TranslatedScript, TranslateError> {
    let eid = extract_eid(record);
    let papyrus_type = papyrus_type_from_schr(record);
    let class_name = standalone_script_name(mod_prefix, &eid);

    let source = match extract_script_source(record) {
        Some(Ok(s)) if !s.trim().is_empty() => s,
        Some(Ok(_)) | None => {
            return Err(TranslateError::Semantic(format!(
                "SCPT '{eid}' has no SCTX or SCDA source"
            )));
        }
        Some(Err(decompile_err)) => {
            return Err(TranslateError::Semantic(format!(
                "SCPT '{eid}' SCDA decompile failed: {decompile_err}"
            )));
        }
    };

    let ctx =
        FnvScriptContext::load().map_err(|e| TranslateError::Semantic(format!("load ctx: {e}")))?;
    let psc_text = translate_to_papyrus(&source, &ctx, &class_name, papyrus_type.extends_class())?;

    Ok(TranslatedScript {
        source_editor_id: eid,
        source_form_key: source_form_key.to_string(),
        script_class_name: class_name,
        papyrus_type,
        psc_text,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Look up the SCPT's EditorID. Python `translate_scpt_record:71` uses
/// `record.get("eid") or _field(record, "EDID") or "Unnamed"`.
fn extract_eid(record: &Value) -> String {
    if let Some(eid) = record.get("eid").and_then(|v| v.as_str()) {
        if !eid.is_empty() {
            return eid.to_string();
        }
    }
    if let Some(edid) = field_value(record, "EDID").and_then(|v| v.as_str()) {
        if !edid.is_empty() {
            return edid.to_string();
        }
    }
    "Unnamed".to_string()
}

/// Determine PapyrusType from SCHR.type. Mirrors Python `_papyrus_type_from_schr`.
fn papyrus_type_from_schr(record: &Value) -> PapyrusType {
    let schr = match field_value(record, "SCHR") {
        Some(v) => v,
        None => return PapyrusType::ObjectReference,
    };
    let type_val = match schr.get("type") {
        Some(v) => v,
        None => return PapyrusType::ObjectReference,
    };
    let int_val = match type_val {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => s.parse::<i64>().unwrap_or(0),
        _ => 0,
    };
    PapyrusType::from_schr_type(int_val)
}

/// Extract the script source text. Prefer SCTX (zstring); fall back to SCDA
/// (compiled bytes) via `decompile_bytecode`.
///
/// Returns `Some(Ok(source))` when a usable source is found or decompiled.
/// Returns `Some(Err(msg))` when SCDA bytes are present but decompile fails —
/// the caller can emit a warning and skip rather than silently dropping.
/// Returns `None` when neither SCTX nor SCDA is present.
fn extract_script_source(record: &Value) -> Option<Result<String, String>> {
    if let Some(sctx) = field_value(record, "SCTX").and_then(|v| v.as_str()) {
        if !sctx.trim().is_empty() {
            return Some(Ok(sctx.to_string()));
        }
    }
    // SCDA path: the field may be a dict with a "compiled_script" list, a
    // bare list of ints, or (rarely) a raw bytes value. Mirror Python's
    // `extract_script_source` coercion logic.
    let scda_bytes = extract_scda_bytes(record)?;
    match decompile_bytecode(&scda_bytes) {
        Ok(source) => Some(Ok(source)),
        Err(e) => Some(Err(e.to_string())),
    }
}

/// Coerce the SCDA field value to raw bytes. Returns `None` when the field is
/// absent or has an unrecognised shape.
fn extract_scda_bytes(record: &Value) -> Option<Vec<u8>> {
    let scda = field_value(record, "SCDA")?;
    // dict { "compiled_script": [...] }
    let list = if let Some(obj) = scda.as_object() {
        obj.get("compiled_script")?.as_array()?
    } else if let Some(arr) = scda.as_array() {
        arr
    } else {
        return None;
    };
    Some(
        list.iter()
            .filter_map(|v| v.as_u64().map(|n| n as u8))
            .collect(),
    )
}

/// Look up the first `field` entry in `record.fields` whose single key
/// matches `sig`. Mirrors Python `_field` in `script_translator.py`.
fn field_value<'a>(record: &'a Value, sig: &str) -> Option<&'a Value> {
    let fields = record.get("fields")?.as_array()?;
    for entry in fields {
        let obj = entry.as_object()?;
        if let Some(v) = obj.get(sig) {
            return Some(v);
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
    use serde_json::json;

    // -----------------------------------------------------------------------
    // PapyrusType / SCHR mapping
    // -----------------------------------------------------------------------

    #[test]
    fn schr_type_0_is_object_reference() {
        assert_eq!(PapyrusType::from_schr_type(0), PapyrusType::ObjectReference);
    }

    #[test]
    fn schr_type_1_is_quest() {
        assert_eq!(PapyrusType::from_schr_type(1), PapyrusType::Quest);
    }

    #[test]
    fn schr_type_256_is_magic_effect() {
        assert_eq!(PapyrusType::from_schr_type(0x100), PapyrusType::MagicEffect);
    }

    #[test]
    fn schr_unknown_type_falls_back_to_object_reference() {
        assert_eq!(
            PapyrusType::from_schr_type(42),
            PapyrusType::ObjectReference
        );
        assert_eq!(
            PapyrusType::from_schr_type(-1),
            PapyrusType::ObjectReference
        );
    }

    #[test]
    fn papyrus_type_extends_class_matches_python() {
        assert_eq!(
            PapyrusType::ObjectReference.extends_class(),
            "ObjectReference"
        );
        assert_eq!(PapyrusType::Quest.extends_class(), "Quest");
        assert_eq!(PapyrusType::MagicEffect.extends_class(), "MagicEffect");
    }

    // -----------------------------------------------------------------------
    // extract_eid
    // -----------------------------------------------------------------------

    #[test]
    fn extract_eid_prefers_top_level() {
        let record = json!({ "eid": "TopLevel", "fields": [{ "EDID": "FieldOnly" }] });
        assert_eq!(extract_eid(&record), "TopLevel");
    }

    #[test]
    fn extract_eid_falls_back_to_field_edid() {
        let record = json!({ "fields": [{ "EDID": "FieldOnly" }] });
        assert_eq!(extract_eid(&record), "FieldOnly");
    }

    #[test]
    fn extract_eid_unnamed_when_missing() {
        let record = json!({ "fields": [] });
        assert_eq!(extract_eid(&record), "Unnamed");
    }

    #[test]
    fn extract_eid_skips_empty_top_level() {
        // Python `record.get("eid") or _field(...)` falls through on empty.
        let record = json!({ "eid": "", "fields": [{ "EDID": "Fallback" }] });
        assert_eq!(extract_eid(&record), "Fallback");
    }

    // -----------------------------------------------------------------------
    // papyrus_type_from_schr (full record path)
    // -----------------------------------------------------------------------

    #[test]
    fn papyrus_type_from_schr_quest() {
        let record = json!({
            "fields": [
                { "SCHR": { "type": 1 } }
            ]
        });
        assert_eq!(papyrus_type_from_schr(&record), PapyrusType::Quest);
    }

    #[test]
    fn papyrus_type_from_schr_magic_effect() {
        let record = json!({
            "fields": [
                { "SCHR": { "type": 256 } }
            ]
        });
        assert_eq!(papyrus_type_from_schr(&record), PapyrusType::MagicEffect);
    }

    #[test]
    fn papyrus_type_from_schr_default_no_field() {
        let record = json!({ "fields": [] });
        assert_eq!(
            papyrus_type_from_schr(&record),
            PapyrusType::ObjectReference
        );
    }

    #[test]
    fn papyrus_type_from_schr_default_missing_type() {
        let record = json!({ "fields": [{ "SCHR": {} }] });
        assert_eq!(
            papyrus_type_from_schr(&record),
            PapyrusType::ObjectReference
        );
    }

    // -----------------------------------------------------------------------
    // extract_script_source
    // -----------------------------------------------------------------------

    #[test]
    fn extract_source_prefers_sctx() {
        let record = json!({
            "fields": [
                { "SCTX": "begin GameMode\nset x to 1\nend\n" }
            ]
        });
        let src = extract_script_source(&record).unwrap().unwrap();
        assert!(src.contains("begin GameMode"));
    }

    #[test]
    fn extract_source_skips_empty_sctx_no_scda() {
        // Empty SCTX with no SCDA field → None.
        let record = json!({ "fields": [{ "SCTX": "   " }] });
        assert!(extract_script_source(&record).is_none());
    }

    #[test]
    fn extract_source_scda_only_attempts_decompile() {
        // SCDA-only records now attempt decompile. The stub returns Err, so we
        // expect Some(Err(...)) rather than None — the decompile path is wired.
        let record = json!({
            "fields": [
                { "SCDA": { "compiled_script": [0, 1, 2, 3] } }
            ]
        });
        let result = extract_script_source(&record);
        // Must be Some (SCDA bytes found) and Err (stub not yet implemented).
        assert!(result.is_some(), "SCDA field should be detected");
        assert!(result.unwrap().is_err(), "stub decompile should return Err");
    }

    #[test]
    fn extract_scda_bytes_dict_shape() {
        let record = json!({ "fields": [{ "SCDA": { "compiled_script": [1, 2, 3] } }] });
        assert_eq!(extract_scda_bytes(&record), Some(vec![1, 2, 3]));
    }

    #[test]
    fn extract_scda_bytes_list_shape() {
        let record = json!({ "fields": [{ "SCDA": [10, 20, 30] }] });
        assert_eq!(extract_scda_bytes(&record), Some(vec![10, 20, 30]));
    }

    #[test]
    fn extract_scda_bytes_absent() {
        let record = json!({ "fields": [] });
        assert_eq!(extract_scda_bytes(&record), None);
    }

    // -----------------------------------------------------------------------
    // translate_scpt_record (end-to-end)
    // -----------------------------------------------------------------------

    #[test]
    fn translate_scpt_record_produces_papyrus_text() {
        let record = json!({
            "eid": "TestScript",
            "fields": [
                { "SCHR": { "type": 0 } },
                { "SCTX": "begin GameMode\nset x to 1\nend\n" }
            ]
        });
        let translated =
            translate_scpt_record(&record, "B21", "001234:FNV.esm").expect("translates");
        assert_eq!(translated.source_editor_id, "TestScript");
        assert_eq!(translated.source_form_key, "001234:FNV.esm");
        assert_eq!(translated.script_class_name, "B21_nv_TestScript");
        assert_eq!(translated.papyrus_type, PapyrusType::ObjectReference);
        // The translated text should carry the class declaration and the
        // statement we fed in.
        assert!(translated.psc_text.contains("ScriptName B21_nv_TestScript"));
        assert!(translated.psc_text.contains("x = 1"));
    }

    #[test]
    fn translate_scpt_record_quest_papyrus_type() {
        let record = json!({
            "eid": "TestQuest",
            "fields": [
                { "SCHR": { "type": 1 } },
                { "SCTX": "begin GameMode\nset x to 1\nend\n" }
            ]
        });
        let translated =
            translate_scpt_record(&record, "B21", "001234:FNV.esm").expect("translates");
        assert_eq!(translated.papyrus_type, PapyrusType::Quest);
        assert!(translated.psc_text.contains("extends Quest"));
    }

    #[test]
    fn translate_scpt_record_no_source_errors() {
        let record = json!({
            "eid": "Empty",
            "fields": [
                { "SCHR": { "type": 0 } }
            ]
        });
        let err = translate_scpt_record(&record, "B21", "001234:FNV.esm")
            .expect_err("expected ScriptSourceMissing");
        assert!(err.to_string().contains("no SCTX or SCDA source"));
    }

    #[test]
    fn translate_scpt_record_uses_unnamed_when_eid_missing() {
        let record = json!({
            "fields": [
                { "SCHR": { "type": 0 } },
                { "SCTX": "begin GameMode\nset x to 1\nend\n" }
            ]
        });
        let translated =
            translate_scpt_record(&record, "B21", "001234:FNV.esm").expect("translates");
        assert_eq!(translated.script_class_name, "B21_nv_Unnamed");
    }
}
