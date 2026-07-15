use std::collections::HashSet;

use esp_authoring_core::plugin_runtime::plugin_handle_read_authoring_record_value_json;
use serde_json::Value as JsonValue;

use crate::terrain_textures::ltex_walk::{
    field_all_reference_form_keys, field_reference_form_key, field_str, field_value,
    normalize_esp_form_key,
};
use crate::terrain_textures::manifest::{GrassEntry, ObjectBounds};

const FO4_GRASS_FLAGS: &[&str] = &["VertexLighting", "UniformScaling", "FitToSlope"];
const FO76_MIN_POSITION_RANGE: f32 = 12.0;
const FO76_GCVR_VEGETATION_MIN_POSITION_RANGE: f32 = 32.0;
const FO76_GCVR_CLUTTER_MIN_POSITION_RANGE: f32 = 48.0;
const FO76_GCVR_DEFAULT_COVER_SCALAR: f32 = 0.286;
const FO76_GCVR_DEFAULT_ENTRY_WEIGHT: u16 = 100;
const FO76_GCVR_SENTINEL_ENTRY_WEIGHT: u16 = 65535;
const FO76_GCVR_PRIMARY_GRASS_DENSITY_MULTIPLIER: f32 = 1.5;
const FO76_GCVR_SECONDARY_VEGETATION_DENSITY_MULTIPLIER: f32 = 0.5;
const FO76_GCVR_MAX_PRIMARY_GRASS_DENSITY: u8 = 56;
const FO76_GCVR_MAX_SECONDARY_VEGETATION_DENSITY: u8 = 8;
const FO76_GCVR_MAX_CLUTTER_DENSITY: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
struct GcvrGrassRef {
    form_key: String,
    weight: Option<u16>,
}

#[derive(Debug, Clone, Copy)]
struct GcvrDensityPolicy {
    cover_scalar: f32,
    entry_weight_fraction: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GcvrGrassKind {
    PrimaryGrass,
    SecondaryVegetation,
    Clutter,
}

/// Build `Vec<GrassEntry>` for one LTEX (assets stay empty; the grass-asset pass fills them).
pub fn grass_entries_for_ltex(
    handle_id: u64,
    ltex_form_key: &str,
    source_game: &str,
) -> Result<Vec<GrassEntry>, String> {
    let ltex_fk_norm = normalize_esp_form_key(ltex_form_key);
    let ltex = plugin_handle_read_authoring_record_value_json(handle_id, &ltex_fk_norm)
        .map_err(|e| format!("plugin lookup failed for {ltex_form_key}: {e}"))?
        .ok_or_else(|| format!("LTEX not found: {ltex_form_key}"))?;
    let ltex_fields = ltex
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    let direct_grass_form_keys = direct_grass_form_keys(ltex_fields);
    if !direct_grass_form_keys.is_empty() {
        return grass_entries_for_form_keys(handle_id, direct_grass_form_keys, source_game);
    }

    let Some(gcvr_form_key) = field_reference_form_key(ltex_fields, "GroundCover")
        .or_else(|| field_reference_form_key(ltex_fields, "ONAM"))
    else {
        return Ok(Vec::new());
    };

    grass_entries_for_gcvr(handle_id, &gcvr_form_key, source_game)
}

pub fn grass_entries_for_gcvr(
    handle_id: u64,
    gcvr_form_key: &str,
    source_game: &str,
) -> Result<Vec<GrassEntry>, String> {
    let gcvr_fk_norm = normalize_esp_form_key(gcvr_form_key);
    let gcvr = plugin_handle_read_authoring_record_value_json(handle_id, &gcvr_fk_norm)
        .map_err(|e| format!("plugin lookup failed for GCVR {gcvr_form_key}: {e}"))?
        .ok_or_else(|| format!("GCVR not found: {gcvr_form_key}"))?;
    let gcvr_fields = gcvr
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    grass_entries_for_gcvr_refs(
        handle_id,
        gcvr_grass_refs(gcvr_fields),
        source_game,
        gcvr_cover_scalar(gcvr_fields),
    )
}

pub fn gcvr_land_texture_form_keys(
    handle_id: u64,
    gcvr_form_key: &str,
) -> Result<Vec<String>, String> {
    let gcvr_fk_norm = normalize_esp_form_key(gcvr_form_key);
    let gcvr = plugin_handle_read_authoring_record_value_json(handle_id, &gcvr_fk_norm)
        .map_err(|e| format!("plugin lookup failed for GCVR {gcvr_form_key}: {e}"))?
        .ok_or_else(|| format!("GCVR not found: {gcvr_form_key}"))?;
    let gcvr_fields = gcvr
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);
    Ok(gcvr_land_texture_form_keys_from_fields(gcvr_fields))
}

pub fn record_editor_id(handle_id: u64, form_key: &str) -> Result<String, String> {
    let normalized = normalize_esp_form_key(form_key);
    let record = plugin_handle_read_authoring_record_value_json(handle_id, &normalized)
        .map_err(|e| format!("plugin lookup failed for {form_key}: {e}"))?
        .ok_or_else(|| format!("record not found: {form_key}"))?;
    Ok(record
        .get("eid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned())
}

fn direct_grass_form_keys(fields: &[JsonValue]) -> Vec<String> {
    let mut keys = field_all_reference_form_keys(fields, "Grass");
    keys.extend(field_all_reference_form_keys(fields, "GNAM"));
    keys
}

#[cfg(test)]
fn gcvr_grass_form_keys(fields: &[JsonValue]) -> Vec<String> {
    gcvr_grass_refs(fields)
        .into_iter()
        .map(|grass_ref| grass_ref.form_key)
        .collect()
}

fn gcvr_grass_refs(fields: &[JsonValue]) -> Vec<GcvrGrassRef> {
    let mut refs = Vec::new();
    for entry in fields {
        let Some(obj) = entry.as_object() else {
            continue;
        };
        if let Some(value) = obj.get("GrassTexture").or_else(|| obj.get("GNAM")) {
            if let Some(form_key) = reference_form_key_from_value(value) {
                refs.push(GcvrGrassRef {
                    form_key,
                    weight: None,
                });
            }
        } else if let Some(value) = obj.get("UnknownInt").or_else(|| obj.get("DNAM")) {
            if let Some(last) = refs
                .last_mut()
                .filter(|grass_ref| grass_ref.weight.is_none())
            {
                last.weight = u16_value(value);
            }
        }
    }
    refs
}

fn gcvr_land_texture_form_keys_from_fields(fields: &[JsonValue]) -> Vec<String> {
    let mut keys = field_all_reference_form_keys(fields, "LandscapeTexture");
    keys.extend(field_all_reference_form_keys(fields, "LNAM"));
    keys
}

fn grass_entries_for_form_keys(
    handle_id: u64,
    form_keys: Vec<String>,
    source_game: &str,
) -> Result<Vec<GrassEntry>, String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut entries: Vec<GrassEntry> = Vec::new();
    for form_key in form_keys {
        if !seen.insert(form_key.to_ascii_lowercase()) {
            continue;
        }
        let grass = plugin_handle_read_authoring_record_value_json(handle_id, &form_key)
            .map_err(|e| format!("plugin lookup failed for GRAS {form_key}: {e}"))?
            .ok_or_else(|| format!("GRAS not found: {form_key}"))?;
        entries.push(entry_from_grass(&grass, &form_key, source_game));
    }
    Ok(entries)
}

fn grass_entries_for_gcvr_refs(
    handle_id: u64,
    grass_refs: Vec<GcvrGrassRef>,
    source_game: &str,
    cover_scalar: f32,
) -> Result<Vec<GrassEntry>, String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut entries: Vec<GrassEntry> = Vec::new();
    let is_fo76 = is_fo76_source_game(source_game);
    let total_weight = gcvr_total_weight(&grass_refs);
    for grass_ref in &grass_refs {
        if !seen.insert(grass_ref.form_key.to_ascii_lowercase()) {
            continue;
        }
        let grass = plugin_handle_read_authoring_record_value_json(handle_id, &grass_ref.form_key)
            .map_err(|e| format!("plugin lookup failed for GRAS {}: {e}", grass_ref.form_key))?
            .ok_or_else(|| format!("GRAS not found: {}", grass_ref.form_key))?;
        let gcvr_policy = if is_fo76 {
            Some(GcvrDensityPolicy {
                cover_scalar,
                entry_weight_fraction: gcvr_entry_weight_fraction(
                    grass_ref,
                    total_weight,
                    grass_refs.len(),
                ),
            })
        } else {
            None
        };
        entries.push(entry_from_grass_with_policy(
            &grass,
            &grass_ref.form_key,
            source_game,
            gcvr_policy,
        ));
    }
    Ok(entries)
}

fn entry_from_grass(grass: &JsonValue, form_key: &str, asset_prefix: &str) -> GrassEntry {
    entry_from_grass_with_policy(grass, form_key, asset_prefix, None)
}

fn entry_from_grass_with_policy(
    grass: &JsonValue,
    form_key: &str,
    asset_prefix: &str,
    gcvr_policy: Option<GcvrDensityPolicy>,
) -> GrassEntry {
    let editor_id = grass
        .get("eid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let fields = grass
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    let object_bounds = field_value(fields, "ObjectBounds")
        .and_then(object_bounds_from_value)
        .unwrap_or_default();

    let data_field = field_value(fields, "DATA").or_else(|| field_value(fields, "DNAM"));
    let data_obj = data_field.and_then(|v| v.as_object());
    let source_model_file_name = field_str(fields, "ModelFileName").unwrap_or("");
    let grass_kind = gcvr_grass_kind(&editor_id, source_model_file_name);
    let source_unknown = data_obj
        .and_then(|o| f32_field(o, "Unknown"))
        .unwrap_or(0.0);

    let source_position_range = data_obj
        .and_then(|o| f32_field(o, "PositionRange"))
        .unwrap_or(0.0);
    let min_position_range = position_range_floor(asset_prefix, gcvr_policy, grass_kind);
    let (position_range, position_range_normalized) =
        normalized_position_range(source_position_range, min_position_range);
    let source_density = data_obj.and_then(|o| u8_field(o, "Density")).unwrap_or(0);
    let (density_scalar, density_cap) = density_policy(source_unknown, gcvr_policy, grass_kind);

    GrassEntry {
        source_form_key: form_key.to_owned(),
        source_editor_id: editor_id,
        object_bounds,
        model_file_name: output_model_file_name(source_model_file_name, asset_prefix),
        model_information: field_str(fields, "ModelInformation")
            .unwrap_or("")
            .to_owned(),
        density: normalized_density(source_density, density_scalar, density_cap),
        max_slope: data_obj.and_then(|o| u8_field(o, "MaxSlope")).unwrap_or(0),
        position_range,
        height_range: data_obj
            .and_then(|o| f32_field(o, "HeightRange"))
            .unwrap_or(0.0),
        color_range: data_obj
            .and_then(|o| f32_field(o, "ColorRange"))
            .unwrap_or(0.0),
        wave_period: data_obj
            .and_then(|o| f32_field(o, "WavePeriod"))
            .unwrap_or(0.0),
        flags: data_obj
            .and_then(|o| o.get("Flags"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str())
                    .filter(|s| FO4_GRASS_FLAGS.contains(s))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        position_range_normalized,
        assets: Vec::new(),
    }
}

fn is_fo76_source_game(source_game: &str) -> bool {
    source_game.trim().to_ascii_lowercase().replace('-', "") == "fo76"
}

fn min_position_range_for_source_game(source_game: &str) -> f32 {
    if is_fo76_source_game(source_game) {
        FO76_MIN_POSITION_RANGE
    } else {
        0.0
    }
}

fn normalized_position_range(value: f32, min_position_range: f32) -> (f32, bool) {
    if min_position_range > 0.0 && value < min_position_range {
        (min_position_range, true)
    } else {
        (value, false)
    }
}

fn position_range_floor(
    source_game: &str,
    gcvr_policy: Option<GcvrDensityPolicy>,
    grass_kind: GcvrGrassKind,
) -> f32 {
    if gcvr_policy.is_some() && is_fo76_source_game(source_game) {
        return match grass_kind {
            GcvrGrassKind::PrimaryGrass | GcvrGrassKind::SecondaryVegetation => {
                FO76_GCVR_VEGETATION_MIN_POSITION_RANGE
            }
            GcvrGrassKind::Clutter => FO76_GCVR_CLUTTER_MIN_POSITION_RANGE,
        };
    }
    min_position_range_for_source_game(source_game)
}

fn density_policy(
    source_unknown: f32,
    gcvr_policy: Option<GcvrDensityPolicy>,
    grass_kind: GcvrGrassKind,
) -> (Option<f32>, u8) {
    let Some(policy) = gcvr_policy else {
        return (None, u8::MAX);
    };
    let dominance = if source_unknown > 0.0 {
        (source_unknown / 100.0).clamp(0.0, 1.0)
    } else {
        policy.entry_weight_fraction.clamp(0.0, 1.0)
    };
    let (kind_multiplier, cap) = match grass_kind {
        GcvrGrassKind::PrimaryGrass => (
            FO76_GCVR_PRIMARY_GRASS_DENSITY_MULTIPLIER,
            FO76_GCVR_MAX_PRIMARY_GRASS_DENSITY,
        ),
        GcvrGrassKind::SecondaryVegetation => (
            FO76_GCVR_SECONDARY_VEGETATION_DENSITY_MULTIPLIER,
            FO76_GCVR_MAX_SECONDARY_VEGETATION_DENSITY,
        ),
        GcvrGrassKind::Clutter => (1.0, FO76_GCVR_MAX_CLUTTER_DENSITY),
    };
    (Some(policy.cover_scalar * dominance * kind_multiplier), cap)
}

fn gcvr_grass_kind(editor_id: &str, model_file_name: &str) -> GcvrGrassKind {
    let editor = editor_id.to_ascii_lowercase();
    let model = model_file_name.to_ascii_lowercase();
    if ["rock", "branch"]
        .iter()
        .any(|token| editor.contains(token) || model.contains(token))
    {
        return GcvrGrassKind::Clutter;
    }
    if ["weed", "hemlock"]
        .iter()
        .any(|token| editor.contains(token) || model.contains(token))
    {
        return GcvrGrassKind::SecondaryVegetation;
    }
    GcvrGrassKind::PrimaryGrass
}

fn normalized_density(value: u8, scalar: Option<f32>, cap: u8) -> u8 {
    let Some(scalar) = scalar else {
        return value;
    };
    if value == 0 || scalar <= 0.0 {
        return 0;
    }
    ((value as f32 * scalar).round() as i32).clamp(1, i32::from(cap)) as u8
}

fn gcvr_cover_scalar(fields: &[JsonValue]) -> f32 {
    field_value(fields, "YNAM")
        .and_then(f32_value)
        .unwrap_or(FO76_GCVR_DEFAULT_COVER_SCALAR)
        .clamp(0.0, 1.0)
}

fn gcvr_total_weight(grass_refs: &[GcvrGrassRef]) -> u32 {
    grass_refs
        .iter()
        .map(|grass_ref| u32::from(gcvr_entry_weight(grass_ref.weight)))
        .sum()
}

fn gcvr_entry_weight_fraction(
    grass_ref: &GcvrGrassRef,
    total_weight: u32,
    grass_ref_count: usize,
) -> f32 {
    if total_weight == 0 {
        return if grass_ref_count > 0 {
            1.0 / grass_ref_count as f32
        } else {
            0.0
        };
    }
    f32::from(gcvr_entry_weight(grass_ref.weight)) / total_weight as f32
}

fn gcvr_entry_weight(value: Option<u16>) -> u16 {
    match value {
        None | Some(FO76_GCVR_SENTINEL_ENTRY_WEIGHT) => FO76_GCVR_DEFAULT_ENTRY_WEIGHT,
        Some(value) => value.min(FO76_GCVR_DEFAULT_ENTRY_WEIGHT),
    }
}

fn output_model_file_name(model_file_name: &str, asset_prefix: &str) -> String {
    let normalized = model_file_name
        .replace('\\', "/")
        .trim_matches(|c: char| c.is_ascii_whitespace() || c == '\0' || c == '/')
        .to_owned();
    if normalized.is_empty() {
        return normalized;
    }

    let without_meshes = normalized
        .strip_prefix("meshes/")
        .or_else(|| normalized.strip_prefix("Meshes/"))
        .unwrap_or(&normalized);
    let mut parts: Vec<&str> = without_meshes.split('/').collect();
    if parts
        .first()
        .is_some_and(|part| part.eq_ignore_ascii_case(asset_prefix))
    {
        parts.remove(0);
    }
    parts.join("/")
}

fn object_bounds_from_value(value: &JsonValue) -> Option<ObjectBounds> {
    let obj = value.as_object()?;
    Some(ObjectBounds {
        object_bounds_x1: i16_field(obj, "ObjectBoundsX1").unwrap_or(0),
        object_bounds_y1: i16_field(obj, "ObjectBoundsY1").unwrap_or(0),
        object_bounds_z1: i16_field(obj, "ObjectBoundsZ1").unwrap_or(0),
        object_bounds_x2: i16_field(obj, "ObjectBoundsX2").unwrap_or(0),
        object_bounds_y2: i16_field(obj, "ObjectBoundsY2").unwrap_or(0),
        object_bounds_z2: i16_field(obj, "ObjectBoundsZ2").unwrap_or(0),
    })
}

// Helpers accept int OR numeric string in the JSON (Python int()/float() parity).
fn u8_field(obj: &serde_json::Map<String, JsonValue>, key: &str) -> Option<u8> {
    let v = obj.get(key)?;
    let parsed = v
        .as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()));
    parsed.map(|n| n.min(255) as u8)
}

fn i16_field(obj: &serde_json::Map<String, JsonValue>, key: &str) -> Option<i16> {
    let v = obj.get(key)?;
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
        .map(|n| n.clamp(i16::MIN as i64, i16::MAX as i64) as i16)
}

fn f32_field(obj: &serde_json::Map<String, JsonValue>, key: &str) -> Option<f32> {
    let v = obj.get(key)?;
    f32_value(v)
}

fn u16_value(v: &JsonValue) -> Option<u16> {
    let parsed = v
        .as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()));
    parsed.map(|n| n.min(u64::from(u16::MAX)) as u16)
}

fn f32_value(v: &JsonValue) -> Option<f32> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
        .map(|n| n as f32)
}

fn reference_form_key_from_value(value: &JsonValue) -> Option<String> {
    let r = value.get("reference")?;
    let plugin = r.get("plugin").and_then(|x| x.as_str()).map(str::trim)?;
    let object_id = r.get("object_id").and_then(|x| x.as_str()).map(str::trim)?;
    if plugin.is_empty() || object_id.is_empty() {
        return None;
    }
    Some(format!("{plugin}:{object_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_bounds_from_value_parses_keys() {
        let v = json!({
            "ObjectBoundsX1": -8, "ObjectBoundsY1": -30, "ObjectBoundsZ1": -20,
            "ObjectBoundsX2": 7,  "ObjectBoundsY2": 30,  "ObjectBoundsZ2": 20,
        });
        let b = object_bounds_from_value(&v).unwrap();
        assert_eq!(b.object_bounds_x1, -8);
        assert_eq!(b.object_bounds_z2, 20);
    }

    #[test]
    fn entry_from_grass_pulls_flags_filtered_to_fo4_subset() {
        let v = json!({
            "eid": "TestGrass",
            "fields": [
                { "ModelFileName": "meshes/test.nif" },
                { "ModelInformation": "" },
                { "DATA": {
                    "Density": 5, "MaxSlope": 30,
                    "PositionRange": 1.5, "HeightRange": 0.2,
                    "ColorRange": 0.1, "WavePeriod": 8.0,
                    "Flags": ["VertexLighting", "SomeUnknownFlag", "UniformScaling"],
                }},
            ],
        });
        let e = entry_from_grass(&v, "000900:Test.esm", "fnv");
        assert_eq!(e.source_form_key, "000900:Test.esm");
        assert_eq!(e.model_file_name, "test.nif");
        assert_eq!(e.density, 5);
        assert_eq!(e.position_range, 1.5);
        assert_eq!(
            e.flags,
            vec!["VertexLighting".to_string(), "UniformScaling".to_string()]
        );
        assert!(e.assets.is_empty());
    }

    #[test]
    fn entry_from_grass_accepts_numeric_string_values() {
        let v = json!({
            "eid": "G",
            "fields": [{ "DATA": { "Density": "7", "PositionRange": "2.5" }}]
        });
        let e = entry_from_grass(&v, "000900:Test.esm", "fnv");
        assert_eq!(e.density, 7);
        assert_eq!(e.position_range, 2.5);
        assert!(!e.position_range_normalized);
    }

    #[test]
    fn entry_from_grass_clamps_fo76_position_range_for_fo4_spacing() {
        let v = json!({
            "eid": "G",
            "fields": [{ "DATA": { "PositionRange": 1.0 }}]
        });

        let e = entry_from_grass(&v, "000900:SeventySix.esm", "fo76");

        assert_eq!(e.position_range, 12.0);
        assert!(e.position_range_normalized);
    }

    #[test]
    fn entry_from_grass_preserves_non_fo76_position_range() {
        let v = json!({
            "eid": "G",
            "fields": [{ "DATA": { "PositionRange": 1.0 }}]
        });

        let e = entry_from_grass(&v, "000900:Test.esm", "fo4");

        assert_eq!(e.position_range, 1.0);
        assert!(!e.position_range_normalized);
    }

    #[test]
    fn gcvr_grass_refs_keep_per_entry_weights() {
        let fields = vec![
            json!({
                "GrassTexture": {
                    "reference": { "plugin": "SeventySix.esm", "object_id": "3900A5" }
                }
            }),
            json!({ "UnknownInt": 50 }),
            json!({
                "GrassTexture": {
                    "reference": { "plugin": "SeventySix.esm", "object_id": "3B396F" }
                }
            }),
            json!({ "UnknownInt": 65535 }),
        ];

        assert_eq!(
            gcvr_grass_refs(&fields),
            vec![
                GcvrGrassRef {
                    form_key: "SeventySix.esm:3900A5".to_string(),
                    weight: Some(50),
                },
                GcvrGrassRef {
                    form_key: "SeventySix.esm:3B396F".to_string(),
                    weight: Some(65535),
                },
            ]
        );
    }

    #[test]
    fn fo76_gcvr_policy_uses_gras_unknown_as_vegetation_dominance() {
        let v = json!({
            "eid": "Forest76GrassObj03A",
            "fields": [{ "DATA": { "Unknown": 97.0, "Density": 94, "PositionRange": 0.1 }}]
        });

        let e = entry_from_grass_with_policy(
            &v,
            "3900A5:SeventySix.esm",
            "fo76",
            Some(GcvrDensityPolicy {
                cover_scalar: 0.4,
                entry_weight_fraction: 0.2,
            }),
        );

        assert_eq!(e.density, 55);
        assert_eq!(e.position_range, 32.0);
        assert!(e.position_range_normalized);
    }

    #[test]
    fn fo76_gcvr_policy_keeps_secondary_weeds_below_main_grass() {
        let v = json!({
            "eid": "Forest76WeedObj01",
            "fields": [{ "DATA": { "Unknown": 32.0, "Density": 98, "PositionRange": 0.1 }}]
        });

        let e = entry_from_grass_with_policy(
            &v,
            "3B396F:SeventySix.esm",
            "fo76",
            Some(GcvrDensityPolicy {
                cover_scalar: 0.4,
                entry_weight_fraction: 0.2,
            }),
        );

        assert_eq!(e.density, 6);
        assert_eq!(e.position_range, 32.0);
    }

    #[test]
    fn fo76_gcvr_default_cover_scalar_tames_rock_density() {
        let v = json!({
            "eid": "Forest76Rocks_OBJ01",
            "fields": [{ "DATA": { "Unknown": 68.0, "Density": 55, "PositionRange": 1.0 }}]
        });
        let grass_ref = GcvrGrassRef {
            form_key: "SeventySix.esm:00DAFA".to_string(),
            weight: Some(65535),
        };
        let total_weight = gcvr_total_weight(&[
            grass_ref.clone(),
            GcvrGrassRef {
                form_key: "SeventySix.esm:00DAFB".to_string(),
                weight: Some(65535),
            },
        ]);

        let e = entry_from_grass_with_policy(
            &v,
            &grass_ref.form_key,
            "fo76",
            Some(GcvrDensityPolicy {
                cover_scalar: FO76_GCVR_DEFAULT_COVER_SCALAR,
                entry_weight_fraction: gcvr_entry_weight_fraction(&grass_ref, total_weight, 2),
            }),
        );

        assert_eq!(e.density, 3);
        assert_eq!(e.position_range, 48.0);
    }

    #[test]
    fn fo76_gcvr_leaf_twig_grass_model_uses_primary_grass_policy() {
        let v = json!({
            "eid": "ForestLeavesTwigsObj01",
            "fields": [
                { "ModelFileName": "Landscape\\Grass\\ForestGrassObj01.nif" },
                { "DATA": { "Unknown": 62.0, "Density": 60, "PositionRange": 0.25 }}
            ]
        });

        let e = entry_from_grass_with_policy(
            &v,
            "0878FF:SeventySix.esm",
            "fo76",
            Some(GcvrDensityPolicy {
                cover_scalar: 0.29,
                entry_weight_fraction: 0.5,
            }),
        );

        assert_eq!(e.density, 16);
        assert_eq!(e.position_range, 32.0);
    }

    #[test]
    fn non_fo76_policy_preserves_density_and_position_range() {
        let v = json!({
            "eid": "ForestGrassObj01",
            "fields": [{ "DATA": { "Density": 9, "PositionRange": 32.0 }}]
        });

        let e = entry_from_grass(&v, "0878FF:Fallout4.esm", "fo4");

        assert_eq!(e.density, 9);
        assert_eq!(e.position_range, 32.0);
        assert!(!e.position_range_normalized);
    }

    #[test]
    fn direct_grass_form_keys_reads_raw_gnam_refs() {
        let fields = vec![
            json!({ "GNAM": { "reference": { "plugin": "SeventySix.esm", "object_id": "8E0B62" }}}),
            json!({ "GNAM": { "reference": { "plugin": "SeventySix.esm", "object_id": "8E0B63" }}}),
        ];

        assert_eq!(
            direct_grass_form_keys(&fields),
            vec![
                "SeventySix.esm:8E0B62".to_string(),
                "SeventySix.esm:8E0B63".to_string(),
            ]
        );
    }

    #[test]
    fn gcvr_grass_form_keys_keeps_legacy_grass_texture_refs() {
        let fields = vec![json!({
            "GrassTexture": { "reference": { "plugin": "SeventySix.esm", "object_id": "011C68" }}
        })];

        assert_eq!(
            gcvr_grass_form_keys(&fields),
            vec!["SeventySix.esm:011C68".to_string()]
        );
    }

    #[test]
    fn gcvr_land_texture_form_keys_reads_named_and_raw_lnam_refs() {
        let fields = vec![
            json!({
                "LandscapeTexture": {
                    "reference": { "plugin": "SeventySix.esm", "object_id": "00D677" }
                }
            }),
            json!({
                "LNAM": {
                    "reference": { "plugin": "SeventySix.esm", "object_id": "00E559" }
                }
            }),
        ];

        assert_eq!(
            gcvr_land_texture_form_keys_from_fields(&fields),
            vec![
                "SeventySix.esm:00D677".to_string(),
                "SeventySix.esm:00E559".to_string(),
            ]
        );
    }

    #[test]
    fn output_model_file_name_strips_source_prefix() {
        assert_eq!(
            output_model_file_name("Landscape\\Grass\\Foo.nif", "fnv"),
            "Landscape/Grass/Foo.nif"
        );
        assert_eq!(
            output_model_file_name("Meshes/fnv/Landscape/Grass/Foo.nif", "fnv"),
            "Landscape/Grass/Foo.nif"
        );
    }
}
