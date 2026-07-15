use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value as JsonValue;

static MATERIAL_SOURCE_OVERRIDES: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Material source-path override table: normalized material key -> data-relative
/// replacement source path. Consumed by the materials converter
/// (`ConvertMaterialsRequest::source_path_overrides`), which applies it to every
/// entry — including the ones `convert_all` enumerates internally.
pub fn material_source_overrides() -> &'static HashMap<String, String> {
    MATERIAL_SOURCE_OVERRIDES
        .get_or_init(|| parse_material_source_overrides(crate::embedded::MATERIAL_SOURCE_OVERRIDES))
}

fn parse_material_source_overrides(text: &str) -> HashMap<String, String> {
    let Ok(value): Result<JsonValue, _> = serde_saphyr::from_str(text) else {
        return HashMap::new();
    };
    let Some(obj) = value.as_object() else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for (key, value) in obj {
        let Some(value) = value.as_str() else {
            continue;
        };
        let Some(key) = normalize_material_key(key) else {
            continue;
        };
        let Some(value) = normalize_material_override(value) else {
            continue;
        };
        out.insert(key, value);
    }
    out
}

fn normalize_material_key(value: &str) -> Option<String> {
    let rel = normalize_data_relative(value)?;
    let rel = if rel.starts_with("materials/") {
        rel
    } else {
        format!("materials/{rel}")
    };
    (rel.ends_with(".bgsm") || rel.ends_with(".bgem")).then_some(rel)
}

fn normalize_material_override(value: &str) -> Option<String> {
    normalize_material_key(value)
}

fn normalize_data_relative(value: &str) -> Option<String> {
    let raw = value.trim().trim_matches('\0').replace('\\', "/");
    if raw.is_empty() || raw.starts_with("//") {
        return None;
    }
    let mut path = raw.trim_start_matches('/').to_ascii_lowercase();
    if let Some((_, rest)) = path.split_once("/data/") {
        path = rest.to_string();
    } else if let Some(rest) = path.strip_prefix("data/") {
        path = rest.to_string();
    } else if is_windows_absolute(&path) || path.contains(':') {
        return None;
    }
    let parts: Vec<&str> = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.is_empty() || parts.iter().any(|part| *part == "..") {
        return None;
    }
    Some(parts.join("/"))
}

fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_temp_ground_override_is_loaded() {
        let overrides = material_source_overrides();
        assert_eq!(
            overrides.get("materials/landscape/ground/temp_groundtexture01.bgsm"),
            Some(&"materials/landscape/ground/forestrocks01.bgsm".to_string())
        );
    }

    #[test]
    fn absolute_data_prefixed_key_normalizes_to_material_key() {
        assert_eq!(
            normalize_material_key(
                "C:\\Projects\\76\\Build\\PC\\Data\\Materials\\Landscape\\Ground\\TEMP_GroundTexture01.bgsm"
            ),
            Some("materials/landscape/ground/temp_groundtexture01.bgsm".to_string())
        );
        assert_eq!(
            normalize_material_key("Landscape/Ground/Foo.bgsm"),
            Some("materials/landscape/ground/foo.bgsm".to_string())
        );
    }
}
