//! Path and FormKey formatting for Tales per-plugin catalogs; mirrors
//! `bacup_lib/plugin_catalog.py` (docs/2026-10-12-plugin-catalogs-plan.md in the Tales mod).

use std::path::PathBuf;

pub const ROOT: &str = "F4SE/Plugins/B21_TalesFromAppalachia";

/// `F4SE/Plugins/B21_TalesFromAppalachia/<feature>/<plugin>.json`, relative to the output root.
pub fn catalog_path(feature: &str, plugin: &str) -> PathBuf {
    PathBuf::from(ROOT).join(feature).join(format!("{plugin}.json"))
}

/// `Plugin.esm:XXXXXX`; six uppercase hex digits up to 0xFFFFFF, else eight.
pub fn form_key(plugin: &str, object_id: u32) -> String {
    if object_id <= 0xFF_FFFF {
        format!("{plugin}:{object_id:06X}")
    } else {
        format!("{plugin}:{object_id:08X}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CASES: &str = include_str!("../../../../../py_creation_lib/python/creation_lib/resources/catalog_schemas/plugin_catalog_cases.json");

    #[test]
    fn form_key_matches_shared_vectors() {
        let cases: serde_json::Value = serde_json::from_str(CASES).unwrap();
        let formats = cases["form_key_format"].as_array().unwrap();
        assert!(!formats.is_empty());
        for case in formats {
            let id = u32::try_from(case["id"].as_u64().unwrap()).unwrap();
            assert_eq!(form_key(case["plugin"].as_str().unwrap(), id), case["text"].as_str().unwrap());
        }
    }

    #[test]
    fn catalog_path_is_feature_folder_plus_plugin_file() {
        let path = catalog_path("Fishing", "SeventySix.esm");
        assert_eq!(
            path.to_string_lossy().replace('\\', "/"),
            "F4SE/Plugins/B21_TalesFromAppalachia/Fishing/SeventySix.esm.json"
        );
    }
}
