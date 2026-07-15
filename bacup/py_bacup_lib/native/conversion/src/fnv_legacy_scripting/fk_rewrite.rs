//! FormKey rewriting for FNV legacy scripting payloads.
//!
//! # What this does
//! Walks a `serde_json::Value` payload produced by one of the FNV synthesizers
//! (`quest::translate_qust_record`, `dialogue::translate_info_record`,
//! `scene::translate_scen_record`, or DIAL re-emission) and rewrites every
//! embedded FormKey reference using the active `FormKeyMapper`.
//!
//! Mirrors Python `FormKeyMapper.rewrite_formkeys` exactly:
//! - Bare `"OBJID:Plugin.esm"` strings matching the `_FK_PATTERN` regex are
//!   looked up; if mapped, the string is replaced with the rendered target FK.
//! - Canonical-ref dicts (`{"reference": {"plugin": ..., "object_id": ...}}`)
//!   are looked up the same way; the `reference` inner dict is replaced
//!   in-place while sibling keys are preserved.
//! - Unmapped references pass through unchanged.
//!
//! Plus the Python `_drop_unmapped_scene_parent` special-case: after generic
//! rewrite, walk SCEN payload `fields` and drop any `PNAM` entry whose value
//! is a source-side FormKey string that did not survive remap into a
//! valid target plugin.
//!
//! # Why pre-mutation
//! `insert_authoring_record_value` resolves cross-plugin FormKey
//! refs against the target plugin's master table. That handles refs into
//! *master plugins* (`FNV.esm`) but NOT refs into *other records translated
//! during the same conversion run* (which live in the output plugin and have
//! freshly-allocated local IDs that don't exist on any master). The mapper
//! tracks those allocations; pre-mutating the payload threads them through.

use serde_json::{Map, Value};

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::FormKey;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Recursively rewrite every FormKey reference in `payload` via the mapper.
/// Mutates `payload` in place. Returns the number of refs rewritten.
///
/// Bare-string refs (`"OBJID:Plugin.esm"`) are recognized by a simple
/// hex-prefix + plugin-extension check rather than a heavy regex — matches
/// the Python `_FK_PATTERN` `^[0-9A-Fa-f]{2,6}:.+\.(esm|esp|esl)$` in
/// practice on the payload shapes produced by the FNV synthesizers.
pub fn rewrite_payload_formkeys(payload: &mut Value, mapper: &FormKeyMapper) -> u32 {
    let mut counter = 0u32;
    rewrite_value(payload, mapper, &mut counter);
    counter
}

/// SCEN-specific cleanup: drop any `PNAM` (parent-scene) reference that
/// could not be remapped to a target plugin after generic FK rewrite.
/// Mirrors Python `_drop_unmapped_scene_parent`. No-op when `payload` is
/// not a dict with a `fields` array, or when no PNAM entries are present.
pub fn drop_unmapped_scene_parent(payload: &mut Value, mapper: &FormKeyMapper) -> u32 {
    let fields = match payload
        .as_object_mut()
        .and_then(|m| m.get_mut("fields"))
        .and_then(|v| v.as_array_mut())
    {
        Some(arr) => arr,
        None => return 0,
    };

    let target_plugin_syms = collect_target_plugin_syms(mapper);
    let mut dropped: u32 = 0;
    fields.retain(|entry| {
        let obj = match entry.as_object() {
            Some(o) if o.len() == 1 => o,
            _ => return true,
        };
        let key = obj.keys().next().map(String::as_str).unwrap_or("");
        if key != "PNAM" {
            return true;
        }
        let value = match obj.get("PNAM").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return true,
        };
        if let Some(target_plugin) = plugin_from_fk_str(value) {
            if target_plugin_syms.iter().any(|p| p == target_plugin) {
                return true;
            }
        }
        dropped += 1;
        false
    });
    dropped
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

fn rewrite_value(value: &mut Value, mapper: &FormKeyMapper, counter: &mut u32) {
    match value {
        Value::String(s) => {
            if let Some(new_s) = remap_fk_string(s, mapper) {
                *s = new_s;
                *counter += 1;
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_value(item, mapper, counter);
            }
        }
        Value::Object(map) => {
            // Canonical-ref shape — replace `reference` inner if remapped.
            if let Some(new_inner) = remap_canonical_ref(map, mapper) {
                if let Some(slot) = map.get_mut("reference") {
                    *slot = Value::Object(new_inner);
                    *counter += 1;
                }
                // Still recurse into sibling keys; canonical-ref refs typically
                // have only `reference`, but Python preserves siblings.
                for (k, v) in map.iter_mut() {
                    if k == "reference" {
                        continue;
                    }
                    rewrite_value(v, mapper, counter);
                }
                return;
            }
            for v in map.values_mut() {
                rewrite_value(v, mapper, counter);
            }
        }
        _ => {}
    }
}

/// If `s` matches the FK shape and has a mapper entry, return the remapped
/// "OBJID:Plugin.esm" rendering.
fn remap_fk_string(s: &str, mapper: &FormKeyMapper) -> Option<String> {
    if !looks_like_fk(s) {
        return None;
    }
    let source_fk = parse_colon_fk(s, mapper)?;
    let mapped = mapper.lookup(source_fk)?;
    if mapped == source_fk {
        return None; // identity mapping — no change.
    }
    Some(render_colon_fk(mapped, mapper))
}

/// If `map` is a canonical-ref `{"reference": {"plugin", "object_id"}}` and
/// the FK is in the mapper, return the new inner `Map` to install. Returns
/// `None` when not a canonical ref or no mapping exists.
fn remap_canonical_ref(
    map: &Map<String, Value>,
    mapper: &FormKeyMapper,
) -> Option<Map<String, Value>> {
    let inner = map.get("reference")?.as_object()?;
    let plugin = inner.get("plugin")?.as_str()?;
    let object_id_raw = inner.get("object_id")?;
    let object_id = match object_id_raw {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    if is_null_object_id(&object_id) {
        return None;
    }
    let source_fk = parse_obj_plugin(&object_id, plugin, mapper)?;
    let mapped = mapper.lookup(source_fk)?;
    if mapped == source_fk {
        return None;
    }
    let target_plugin = mapper.interner.resolve(mapped.plugin)?.to_string();
    let mut new_inner = Map::new();
    new_inner.insert("plugin".to_string(), Value::String(target_plugin));
    new_inner.insert(
        "object_id".to_string(),
        Value::String(format!("{:06X}", mapped.local)),
    );
    Some(new_inner)
}

/// Lightweight pre-check: does `s` look like "OBJID:Plugin.esm"?
/// Matches Python `_FK_PATTERN` `^[0-9A-Fa-f]{2,6}:.+\.(esm|esp|esl)$` for the
/// shapes that appear in our payloads. We accept hex up to 8 chars (the
/// authoring layer sometimes renders full 8-char IDs); this is a superset of
/// Python's 2-6 range, which is harmless — a non-FK-prefixed string never
/// reaches `mapper.lookup`.
fn looks_like_fk(s: &str) -> bool {
    let (hex, plugin) = match s.split_once(':') {
        Some(pair) => pair,
        None => return false,
    };
    if hex.is_empty() || hex.len() > 8 {
        return false;
    }
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    let lc = plugin.to_ascii_lowercase();
    lc.ends_with(".esm") || lc.ends_with(".esp") || lc.ends_with(".esl")
}

/// Parse `"OBJID:Plugin.esm"` into a FormKey using the mapper's interner.
/// Returns `None` for unparseable hex or empty plugin.
fn parse_colon_fk(s: &str, mapper: &FormKeyMapper) -> Option<FormKey> {
    let (hex, plugin) = s.split_once(':')?;
    parse_obj_plugin(hex, plugin, mapper)
}

fn parse_obj_plugin(hex: &str, plugin: &str, mapper: &FormKeyMapper) -> Option<FormKey> {
    let trimmed_plugin = plugin.trim();
    if trimmed_plugin.is_empty() {
        return None;
    }
    let local = u32::from_str_radix(hex.trim(), 16).ok()?;
    // Interner reads from the `FormKeyMapper`'s shared interner; we use the
    // immutable `resolve` API to find an existing Sym for the plugin name. If
    // the plugin hasn't been interned yet, this FK can't possibly be in the
    // mapper's source_to_target table — return None to short-circuit.
    let plugin_sym = mapper.interner.get(trimmed_plugin)?;
    Some(FormKey {
        local,
        plugin: plugin_sym,
    })
}

fn render_colon_fk(fk: FormKey, mapper: &FormKeyMapper) -> String {
    let plugin = mapper.interner.resolve(fk.plugin).unwrap_or("<unknown>");
    format!("{:06X}:{}", fk.local, plugin)
}

/// Match Python `from_ref`'s null-FK skip: all-zero object IDs are sentinels.
fn is_null_object_id(s: &str) -> bool {
    s.trim().trim_start_matches('0').is_empty()
}

/// Extract the plugin part of a `"OBJID:Plugin"` FormKey string, returning
/// `None` when malformed.
fn plugin_from_fk_str(s: &str) -> Option<&str> {
    let (_, plugin) = s.split_once(':')?;
    let trimmed = plugin.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Collect every target-side plugin name observed in the mapper's
/// source_to_target table. Used by `drop_unmapped_scene_parent` to decide
/// whether a PNAM ref survived remap.
fn collect_target_plugin_syms(mapper: &FormKeyMapper) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (_src, tgt) in mapper.source_to_target_iter() {
        if let Some(name) = mapper.interner.resolve(tgt.plugin) {
            let owned = name.to_string();
            if !out.contains(&owned) {
                out.push(owned);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::{FormKey, SigCode};
    use crate::sym::{StringInterner, Sym};
    use serde_json::json;

    fn mapper_with_mapping<'a>(
        interner: &'a mut StringInterner,
        source: (u32, &str),
        target: (u32, &str),
    ) -> FormKeyMapper<'a> {
        let src_plugin = interner.intern(source.1);
        let tgt_plugin = interner.intern(target.1);
        let src_fk = FormKey {
            local: source.0,
            plugin: src_plugin,
        };
        let tgt_fk = FormKey {
            local: target.0,
            plugin: tgt_plugin,
        };
        let mut mapper = FormKeyMapper::new(
            std::iter::empty::<(Sym, FormKey, SigCode)>(),
            MapperOptions::default(),
            interner,
        );
        mapper.add_mapping(src_fk, tgt_fk);
        mapper
    }

    // -----------------------------------------------------------------------
    // looks_like_fk
    // -----------------------------------------------------------------------

    #[test]
    fn looks_like_fk_accepts_canonical_shape() {
        assert!(looks_like_fk("001234:FNV.esm"));
        assert!(looks_like_fk("AB:Plugin.esp"));
        assert!(looks_like_fk("0001:My.esl"));
    }

    #[test]
    fn looks_like_fk_rejects_non_fk_strings() {
        assert!(!looks_like_fk(""));
        assert!(!looks_like_fk("not-an-fk"));
        assert!(!looks_like_fk("123:NoExtension"));
        assert!(!looks_like_fk("XYZ:Plugin.esm")); // non-hex hex part
        assert!(!looks_like_fk("123456789:Plugin.esm")); // too long
    }

    // -----------------------------------------------------------------------
    // rewrite_payload_formkeys — string shape
    // -----------------------------------------------------------------------

    #[test]
    fn rewrite_string_fk_replaces_when_mapped() {
        let mut interner = StringInterner::new();
        let mapper = mapper_with_mapping(&mut interner, (0x100, "FNV.esm"), (0x800, "Output.esp"));

        let mut payload = json!({
            "form_id": "000100",
            "fields": [
                { "QNAM": "000100:FNV.esm" },
            ]
        });
        let n = rewrite_payload_formkeys(&mut payload, &mapper);
        assert_eq!(n, 1);
        assert_eq!(
            payload["fields"][0]["QNAM"].as_str().unwrap(),
            "000800:Output.esp"
        );
    }

    #[test]
    fn rewrite_string_fk_leaves_unmapped_alone() {
        let mut interner = StringInterner::new();
        let _src_plugin_intern = interner.intern("FNV.esm");
        let mapper = FormKeyMapper::new(
            std::iter::empty::<(Sym, FormKey, SigCode)>(),
            MapperOptions::default(),
            &mut interner,
        );

        let mut payload = json!({ "QNAM": "000100:FNV.esm" });
        let n = rewrite_payload_formkeys(&mut payload, &mapper);
        assert_eq!(n, 0);
        assert_eq!(payload["QNAM"].as_str().unwrap(), "000100:FNV.esm");
    }

    #[test]
    fn rewrite_string_fk_ignores_non_fk_strings() {
        let mut interner = StringInterner::new();
        let mapper = FormKeyMapper::new(
            std::iter::empty::<(Sym, FormKey, SigCode)>(),
            MapperOptions::default(),
            &mut interner,
        );
        let mut payload = json!({ "eid": "MyEditorID", "note": "no FK here" });
        let n = rewrite_payload_formkeys(&mut payload, &mapper);
        assert_eq!(n, 0);
        assert_eq!(payload["eid"].as_str().unwrap(), "MyEditorID");
    }

    // -----------------------------------------------------------------------
    // rewrite_payload_formkeys — canonical-ref shape
    // -----------------------------------------------------------------------

    #[test]
    fn rewrite_canonical_ref_replaces_inner_keys() {
        let mut interner = StringInterner::new();
        let mapper = mapper_with_mapping(&mut interner, (0x100, "FNV.esm"), (0x800, "Output.esp"));

        let mut payload = json!({
            "fields": [
                {
                    "QNAM": {
                        "reference": {
                            "plugin": "FNV.esm",
                            "object_id": "000100"
                        }
                    }
                }
            ]
        });
        let n = rewrite_payload_formkeys(&mut payload, &mapper);
        assert_eq!(n, 1);
        let ref_inner = &payload["fields"][0]["QNAM"]["reference"];
        assert_eq!(ref_inner["plugin"].as_str().unwrap(), "Output.esp");
        assert_eq!(ref_inner["object_id"].as_str().unwrap(), "000800");
    }

    #[test]
    fn rewrite_canonical_ref_preserves_sibling_keys() {
        let mut interner = StringInterner::new();
        let mapper = mapper_with_mapping(&mut interner, (0x100, "FNV.esm"), (0x800, "Output.esp"));

        let mut payload = json!({
            "QNAM": {
                "reference": {"plugin": "FNV.esm", "object_id": "000100"},
                "_comment": "preserved"
            }
        });
        rewrite_payload_formkeys(&mut payload, &mapper);
        assert_eq!(payload["QNAM"]["_comment"].as_str().unwrap(), "preserved");
        assert_eq!(
            payload["QNAM"]["reference"]["plugin"].as_str().unwrap(),
            "Output.esp"
        );
    }

    #[test]
    fn rewrite_canonical_ref_with_null_object_id_skipped() {
        let mut interner = StringInterner::new();
        let mapper = mapper_with_mapping(&mut interner, (0x100, "FNV.esm"), (0x800, "Output.esp"));

        let mut payload = json!({
            "QNAM": {"reference": {"plugin": "FNV.esm", "object_id": "000000"}}
        });
        let n = rewrite_payload_formkeys(&mut payload, &mapper);
        assert_eq!(n, 0); // null FK never matches
    }

    // -----------------------------------------------------------------------
    // rewrite_payload_formkeys — deep traversal
    // -----------------------------------------------------------------------

    #[test]
    fn rewrite_descends_into_nested_arrays_and_dicts() {
        let mut interner = StringInterner::new();
        let mapper = mapper_with_mapping(&mut interner, (0x100, "FNV.esm"), (0x800, "Output.esp"));

        let mut payload = json!({
            "fields": [
                { "Conditions": [
                    { "CTDA": { "form_id": "000100:FNV.esm", "value": 1.0 } },
                    { "CTDA": "000100:FNV.esm" },
                ]},
            ]
        });
        let n = rewrite_payload_formkeys(&mut payload, &mapper);
        assert_eq!(n, 2);
        assert_eq!(
            payload["fields"][0]["Conditions"][0]["CTDA"]["form_id"]
                .as_str()
                .unwrap(),
            "000800:Output.esp"
        );
        assert_eq!(
            payload["fields"][0]["Conditions"][1]["CTDA"]
                .as_str()
                .unwrap(),
            "000800:Output.esp"
        );
    }

    // -----------------------------------------------------------------------
    // drop_unmapped_scene_parent
    // -----------------------------------------------------------------------

    #[test]
    fn drop_pnam_keeps_target_plugin_refs() {
        let mut interner = StringInterner::new();
        let mapper = mapper_with_mapping(&mut interner, (0x100, "FNV.esm"), (0x800, "Output.esp"));

        let mut payload = json!({
            "fields": [
                { "EDID": "SceneA" },
                { "PNAM": "000800:Output.esp" },  // already on the target plugin
            ]
        });
        let dropped = drop_unmapped_scene_parent(&mut payload, &mapper);
        assert_eq!(dropped, 0);
        let pnam = payload["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f.as_object().unwrap().contains_key("PNAM"));
        assert!(pnam.is_some(), "PNAM should be preserved");
    }

    #[test]
    fn drop_pnam_removes_source_plugin_refs() {
        let mut interner = StringInterner::new();
        let mapper = mapper_with_mapping(&mut interner, (0x100, "FNV.esm"), (0x800, "Output.esp"));

        let mut payload = json!({
            "fields": [
                { "EDID": "SceneA" },
                { "PNAM": "000100:FNV.esm" },  // source plugin — should drop
            ]
        });
        let dropped = drop_unmapped_scene_parent(&mut payload, &mapper);
        assert_eq!(dropped, 1);
        let pnam_left = payload["fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f.as_object().unwrap().contains_key("PNAM"));
        assert!(!pnam_left, "PNAM should have been dropped");
    }

    #[test]
    fn drop_pnam_noop_when_no_fields_array() {
        let mut interner = StringInterner::new();
        let mapper = FormKeyMapper::new(
            std::iter::empty::<(Sym, FormKey, SigCode)>(),
            MapperOptions::default(),
            &mut interner,
        );
        let mut payload = json!({ "form_id": "000100" });
        let dropped = drop_unmapped_scene_parent(&mut payload, &mapper);
        assert_eq!(dropped, 0);
    }
}
