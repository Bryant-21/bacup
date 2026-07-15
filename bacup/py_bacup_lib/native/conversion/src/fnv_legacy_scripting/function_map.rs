//! FNV → FO4 function/AV map loaders.
//!
//! Loads `fnv_to_fo4_script_functions.yaml` and `fnv_to_fo4_actor_values.yaml`
//! embedded in the native library. All look-ups are case-folded (lower-case
//! keys) to match the Python fallback.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// FunctionEntry
// ---------------------------------------------------------------------------

/// One entry from `fnv_to_fo4_script_functions.yaml`.
///
/// Only the fields the semantic pass actually needs are captured.
#[derive(Debug, Clone)]
pub struct FunctionEntry {
    /// Papyrus call template (e.g. `"{self}.GetValue({arg0})"`).
    pub papyrus: Option<String>,
    /// Multi-line expansion template (alternative to `papyrus`).
    pub expansion: Option<String>,
    /// `"drop_with_warning"` — function has no FO4 equivalent.
    pub rewrite: Option<String>,
    /// Per-argument semantic kinds (`"actor_value"`, `"quest"`, …).
    pub arg_kinds: Vec<String>,
}

// ---------------------------------------------------------------------------
// FnvScriptContext
// ---------------------------------------------------------------------------

/// Translation context built from the two YAML map files.
///
/// Keys are case-folded (ASCII lower-case).
#[derive(Debug)]
pub struct FnvScriptContext {
    /// FNV function name (lower) → Papyrus mapping.
    pub function_map: HashMap<String, FunctionEntry>,
    /// FNV actor-value name (lower) → FO4 actor-value name.
    pub actor_value_map: HashMap<String, String>,
}

impl FnvScriptContext {
    /// Load both embedded YAML files.
    pub fn load() -> Result<Self, LoadError> {
        let function_map = load_function_map()?;
        let actor_value_map = load_actor_value_map()?;

        Ok(Self {
            function_map,
            actor_value_map,
        })
    }
}

// ---------------------------------------------------------------------------
// Loaders
// ---------------------------------------------------------------------------

fn load_function_map() -> Result<HashMap<String, FunctionEntry>, LoadError> {
    let path = "fnv_to_fo4_script_functions.yaml";
    let text = include_str!("data/fnv_to_fo4_script_functions.yaml");

    let raw: serde_json::Value = serde_saphyr::from_str(text)
        .map_err(|e| LoadError::Malformed(path.to_string(), e.to_string()))?;

    let map = raw.as_object().ok_or_else(|| {
        LoadError::Malformed(
            path.to_string(),
            "expected a mapping at the top level".into(),
        )
    })?;

    let mut out = HashMap::with_capacity(map.len());
    for (name, value) in map {
        let entry = parse_function_entry(name, value)?;
        out.insert(name.to_lowercase(), entry);
    }
    Ok(out)
}

fn parse_function_entry(name: &str, value: &serde_json::Value) -> Result<FunctionEntry, LoadError> {
    let obj = value.as_object().ok_or_else(|| {
        LoadError::Malformed(
            name.to_string(),
            "function-map entry must be a mapping".into(),
        )
    })?;

    let papyrus = obj
        .get("papyrus")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let expansion = obj
        .get("expansion")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let rewrite = obj
        .get("rewrite")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let arg_kinds = obj
        .get("arg_kinds")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    Ok(FunctionEntry {
        papyrus,
        expansion,
        rewrite,
        arg_kinds,
    })
}

fn load_actor_value_map() -> Result<HashMap<String, String>, LoadError> {
    let path = "fnv_to_fo4_actor_values.yaml";
    let text = include_str!("data/fnv_to_fo4_actor_values.yaml");

    let raw: serde_json::Value = serde_saphyr::from_str(text)
        .map_err(|e| LoadError::Malformed(path.to_string(), e.to_string()))?;

    let map = raw.as_object().ok_or_else(|| {
        LoadError::Malformed(
            path.to_string(),
            "expected a mapping at the top level".into(),
        )
    })?;

    let mut out = HashMap::with_capacity(map.len());
    for (name, value) in map {
        // null entries (e.g. Karma: null) are FNV AVs with no FO4 equivalent —
        // skip them rather than inserting.
        if let Some(mapped) = value.as_str() {
            out.insert(name.to_lowercase(), mapped.to_string());
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// LoadError
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum LoadError {
    Malformed(String, String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Malformed(path, msg) => {
                write!(f, "malformed map file {path}: {msg}")
            }
        }
    }
}

impl std::error::Error for LoadError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_function_map_has_get_actor_value() {
        let ctx = FnvScriptContext::load().expect("load should succeed");
        let entry = ctx.function_map.get("getactorvalue");
        assert!(entry.is_some(), "GetActorValue should be in function map");
        let entry = entry.unwrap();
        assert!(
            entry.papyrus.as_deref().unwrap_or("").contains("GetValue"),
            "papyrus template should contain GetValue"
        );
        assert_eq!(
            entry.arg_kinds.first().map(String::as_str),
            Some("actor_value")
        );
    }

    #[test]
    fn load_function_map_has_get_player() {
        let ctx = FnvScriptContext::load().expect("load should succeed");
        let entry = ctx.function_map.get("getplayer");
        assert!(entry.is_some(), "GetPlayer should be in function map");
        let papyrus = entry.unwrap().papyrus.as_deref().unwrap_or("");
        assert!(papyrus.contains("Game.GetPlayer()"));
    }

    #[test]
    fn load_function_map_drop_with_warning() {
        let ctx = FnvScriptContext::load().expect("load should succeed");
        let entry = ctx.function_map.get("rewardkarma");
        assert!(entry.is_some(), "RewardKarma should be in function map");
        assert_eq!(entry.unwrap().rewrite.as_deref(), Some("drop_with_warning"));
    }

    #[test]
    fn load_actor_value_map_strength() {
        let ctx = FnvScriptContext::load().expect("load should succeed");
        let mapped = ctx.actor_value_map.get("strength");
        assert_eq!(mapped.map(String::as_str), Some("Strength"));
    }

    #[test]
    fn load_actor_value_map_null_entries_skipped() {
        let ctx = FnvScriptContext::load().expect("load should succeed");
        // Karma is null in the YAML — must not appear in the map.
        assert!(!ctx.actor_value_map.contains_key("karma"));
    }

    #[test]
    fn function_map_lookup_is_case_insensitive() {
        let ctx = FnvScriptContext::load().expect("load should succeed");
        // All keys are stored lower-case; callers must lower before lookup.
        assert!(ctx.function_map.contains_key("getisid"));
        assert!(ctx.function_map.contains_key("activate"));
    }
}
