//! The on-disk PerkCards document (`PerkCards/<Plugin>.json`, docs/2026-10-12-plugin-catalogs-plan.md).
//! The phase builds the catalog with `HEX:Plugin` keys and `{plugin, object_id}` bindings; this module
//! rewrites every form reference to `Plugin.ext:HEX` and splits the catalog into the envelope's
//! settings and sections. Tales' `PerkCardsDocument.h` reverses it.

use serde_json::{Map, Value, json};

use crate::plugin_catalog;

/// Sections merged per key across plugins; everything else is a `settings` field.
const SECTIONS: &[&str] = &["cards", "excluded", "effect_records", "target_forms", "weight_items"];

fn is_plugin_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !name.contains(':') && !name.contains(char::is_whitespace)
        && (lower.ends_with(".esm") || lower.ends_with(".esp") || lower.ends_with(".esl"))
}

/// `3D295E:SeventySix.esm` -> `SeventySix.esm:3D295E`.
fn form_key_from_legacy(text: &str) -> Option<String> {
    let (hex, plugin) = text.split_once(':')?;
    if !(6..=8).contains(&hex.len()) || !hex.chars().all(|c| c.is_ascii_hexdigit()) || !is_plugin_file(plugin) {
        return None;
    }
    Some(plugin_catalog::form_key(plugin, u32::from_str_radix(hex, 16).ok()?))
}

fn convert(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(form_key_from_legacy(&text).unwrap_or(text)),
        Value::Array(items) => Value::Array(items.into_iter().map(convert).collect()),
        Value::Object(map) => {
            let binding = match (map.get("plugin"), map.get("object_id")) {
                (Some(Value::String(plugin)), Some(Value::String(id))) if is_plugin_file(plugin) => {
                    u32::from_str_radix(id, 16).ok().map(|id| plugin_catalog::form_key(plugin, id))
                }
                _ => None,
            };
            let mut out = Map::new();
            let bound = binding.is_some();
            if let Some(form) = binding {
                out.insert("form".into(), Value::String(form));
            }
            for (key, item) in map {
                if bound && (key == "plugin" || key == "object_id") {
                    continue;
                }
                let key = form_key_from_legacy(&key).unwrap_or(key);
                out.insert(key, convert(item));
            }
            Value::Object(out)
        }
        other => other,
    }
}

fn rename_key_to_form(section: &mut Value) {
    for entry in section.as_array_mut().into_iter().flatten() {
        if let Some(object) = entry.as_object_mut() {
            if let Some(key) = object.remove("key") {
                object.insert("form".into(), key);
            }
        }
    }
}

/// `legacy` is the object `PerkCardsPhase` builds; the result is the envelope.
pub(crate) fn to_document(legacy: Value) -> Value {
    let Value::Object(map) = convert(legacy) else {
        return json!({"version": 1, "settings": {}});
    };
    let mut settings = Map::new();
    let mut document = Map::new();
    document.insert("version".into(), json!(1));
    for (name, value) in map {
        if name == "schema_version" {
            continue;
        }
        if SECTIONS.contains(&name.as_str()) {
            document.insert(name, value);
        } else {
            settings.insert(name, value);
        }
    }
    for name in ["cards", "excluded"] {
        if let Some(section) = document.get_mut(name) {
            rename_key_to_form(section);
        }
    }
    document.insert("settings".into(), Value::Object(settings));
    Value::Object(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_become_plugin_first_and_bindings_become_form_objects() {
        let document = to_document(json!({
            "schema_version": 1, "source_plugin": "SeventySix.esm", "output_plugin": "SeventySix.esm",
            "starting_deck": ["005272:SeventySix.esm"],
            "cards": [{"key": "005272:SeventySix.esm", "name": "Number Cruncher", "sound": "1A7633:SeventySix.esm",
                "ranks": [{"source_perk": "04A0C5:SeventySix.esm", "description": "10:Not.esm text",
                    "perk": {"plugin": "SeventySix.esm", "object_id": "04A0C5", "signature": "PERK"}},
                    {"source_perk": "04A0C6:SeventySix.esm", "perk": null}]}],
            "excluded": [{"key": "797E00:SeventySix.esm", "reason": "ghoul-only"}],
            "effect_records": {"00000F:SeventySix.esm": {"references": ["000010:SeventySix.esm"], "fields": []}},
            "target_forms": {"00000F:SeventySix.esm": {"plugin": "Fallout4.esm", "object_id": "00000F", "signature": "MISC"},
                "000010:SeventySix.esm": null},
            "weight_items": [{"source_item": "844A74:SeventySix.esm", "keywords": ["3896DF:SeventySix.esm"],
                "targets": [{"plugin": "SeventySix.esm", "object_id": "844A74", "provenance": "final-mapper"}]}],
            "pack": {"source_key": "2DB879:SeventySix.esm", "rarities": [{"eligible_cards": ["005272:SeventySix.esm"]}]},
            "roots": {"PerkScrapCurrency": "5AD146:SeventySix.esm"},
            "economy": {"allocation_cap": 56},
        }));
        assert_eq!(document["version"], 1);
        assert!(document.get("schema_version").is_none());
        let settings = &document["settings"];
        assert_eq!(settings["starting_deck"], json!(["SeventySix.esm:005272"]));
        assert_eq!(settings["pack"]["source_key"], "SeventySix.esm:2DB879");
        assert_eq!(settings["pack"]["rarities"][0]["eligible_cards"], json!(["SeventySix.esm:005272"]));
        assert_eq!(settings["roots"]["PerkScrapCurrency"], "SeventySix.esm:5AD146");
        assert_eq!(settings["economy"]["allocation_cap"], 56);
        let card = &document["cards"][0];
        assert_eq!(card["form"], "SeventySix.esm:005272");
        assert!(card.get("key").is_none());
        assert_eq!(card["sound"], "SeventySix.esm:1A7633");
        assert_eq!(card["ranks"][0]["source_perk"], "SeventySix.esm:04A0C5");
        assert_eq!(card["ranks"][0]["description"], "10:Not.esm text");
        assert_eq!(card["ranks"][0]["perk"], json!({"form": "SeventySix.esm:04A0C5", "signature": "PERK"}));
        assert!(card["ranks"][1]["perk"].is_null());
        assert_eq!(document["excluded"][0]["form"], "SeventySix.esm:797E00");
        assert_eq!(document["effect_records"]["SeventySix.esm:00000F"]["references"][0], "SeventySix.esm:000010");
        assert_eq!(document["target_forms"]["SeventySix.esm:00000F"], json!({"form": "Fallout4.esm:00000F", "signature": "MISC"}));
        assert!(document["target_forms"]["SeventySix.esm:000010"].is_null());
        assert_eq!(document["weight_items"][0]["source_item"], "SeventySix.esm:844A74");
        assert_eq!(document["weight_items"][0]["targets"][0], json!({"form": "SeventySix.esm:844A74", "provenance": "final-mapper"}));
    }
}
