//! Loaders for source-game condition-function tables.
//!
//! The YAML file maps integer FO76 function codes to FO4 named enum strings.
//! Unknown codes are NOT in the table; conditions that reference them are dropped.
//!
//! Data is embedded in the binary via `include_str!` in
//! `crate::phase::record_translation::embedded`.
//! Cached lazily via `OnceLock` — safe for concurrent access after first load.

use rustc_hash::FxHashMap;
use std::sync::OnceLock;

use crate::embedded;

/// Lazily-loaded condition function table for FO76 → FO4.
static FO76_CONDITION_FUNCTIONS: OnceLock<FxHashMap<u16, String>> = OnceLock::new();
static SKYRIMSE_CONDITION_FUNCTIONS: OnceLock<FxHashMap<u16, String>> = OnceLock::new();

/// Return the condition function map for FO76.
///
/// The map is built once on first call and reused. An empty map is returned
/// if the embedded YAML is unparseable (non-fatal — conditions with numeric
/// codes will simply be dropped).
pub fn fo76_condition_functions() -> &'static FxHashMap<u16, String> {
    FO76_CONDITION_FUNCTIONS
        .get_or_init(|| load_condition_functions(embedded::FO76_CONDITION_FUNCTIONS))
}

pub fn condition_functions_for(source_game: &str) -> &'static FxHashMap<u16, String> {
    if source_game.eq_ignore_ascii_case("skyrimse") {
        SKYRIMSE_CONDITION_FUNCTIONS
            .get_or_init(|| load_condition_functions(embedded::SKYRIMSE_CONDITION_FUNCTIONS))
    } else {
        fo76_condition_functions()
    }
}

fn load_condition_functions(text: &str) -> FxHashMap<u16, String> {
    let raw: serde_json::Value = match serde_saphyr::from_str(text) {
        Ok(v) => v,
        Err(_) => return FxHashMap::default(),
    };

    let functions_obj = match raw.get("functions").and_then(|v| v.as_object()) {
        Some(obj) => obj,
        None => return FxHashMap::default(),
    };

    let mut map = FxHashMap::default();
    for (k, v) in functions_obj {
        let code: u16 = match k.parse::<u64>() {
            Ok(n) if n <= u16::MAX as u64 => n as u16,
            _ => continue,
        };
        let name = match v.as_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        map.insert(code, name);
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fo76_condition_functions_loads_without_panic() {
        // The YAML file currently has `functions: {}` so the map is empty.
        // Just verifying load succeeds and doesn't panic.
        let map = fo76_condition_functions();
        // Should be empty (the current file has no entries).
        // If entries are added later this assertion can be relaxed.
        let _ = map; // just check it loads
    }

    #[test]
    fn build_map_from_yaml_text() {
        // Parse a synthetic YAML blob to verify the numeric key parsing logic.
        let yaml = "functions:\n  100: GetIsID\n  200: GetInCell\n";
        let raw: serde_json::Value = serde_saphyr::from_str(yaml).unwrap();
        let obj = raw.get("functions").unwrap().as_object().unwrap();
        let mut map: FxHashMap<u16, String> = FxHashMap::default();
        for (k, v) in obj {
            if let (Ok(code), Some(name)) = (k.parse::<u16>(), v.as_str()) {
                map.insert(code, name.to_string());
            }
        }
        assert_eq!(map.get(&100), Some(&"GetIsID".to_string()));
        assert_eq!(map.get(&200), Some(&"GetInCell".to_string()));
        assert_eq!(map.get(&999), None);
    }

    #[test]
    fn source_game_selects_skyrim_table() {
        let map = condition_functions_for("skyrimse");
        assert_eq!(map.get(&1).map(String::as_str), Some("GetDistance"));
        assert_eq!(map.get(&72).map(String::as_str), Some("GetIsID"));
        assert_eq!(map.get(&875), None);
        assert_eq!(condition_functions_for("SKYRIMSE"), map);
    }
}
