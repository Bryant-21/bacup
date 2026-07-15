use std::io::Cursor;
use std::path::{Path, PathBuf};

use esp_authoring_core::plugin_runtime::{
    plugin_handle_read_authoring_record_by_editor_id_and_signature_json,
    plugin_handle_read_authoring_record_value_json,
};
use materials_native::bgsm;

use super::ba2_resolver::Ba2Resolver;
use super::manifest::TextureBundle;

// ─────────────────────────────────────────────────────────────────────────────
// Public helpers (also tested below)
// ─────────────────────────────────────────────────────────────────────────────

/// Keep `[A-Za-z0-9_]` characters; drop spaces; replace all other chars with `_`.
pub(crate) fn safe_name(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else if ch == ' ' {
            // drop spaces
        } else {
            out.push('_');
        }
    }
    out
}

/// Join `root` and `name` with a single `/`, trimming extra slashes.
pub(crate) fn join_manifest_path(root: &str, name: &str) -> String {
    let root = root.trim_matches('/');
    let name = name.trim_matches('/');
    if root.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", root, name)
    }
}

/// Strip a leading `textures/` prefix from `prefix` and append `.bgsm`.
pub(crate) fn material_path_for_output_prefix(prefix: &str) -> String {
    let stripped = prefix.strip_prefix("textures/").unwrap_or(prefix);
    format!("{}.bgsm", stripped)
}

/// Strip ASCII whitespace and null bytes from both ends of a path string.
fn trim_path(path: &str) -> &str {
    path.trim_matches(|c: char| c.is_ascii_whitespace() || c == '\0')
}

/// Normalize a texture path: forward-slash + ensure `textures/` prefix.
/// Does NOT lowercase (matching the deleted Python behaviour).
pub(crate) fn normalize_texture_path(path: &str) -> String {
    let forward = trim_path(path).replace('\\', "/");
    let lower = forward.to_ascii_lowercase();
    if lower.starts_with("textures/") {
        forward
    } else {
        format!("textures/{}", forward)
    }
}

/// Normalize a material path: forward-slash + ensure `materials/` prefix.
pub(crate) fn normalize_material_path(path: &str) -> String {
    let forward = trim_path(path).replace('\\', "/");
    let lower = forward.to_ascii_lowercase();
    if lower.starts_with("materials/") {
        forward
    } else {
        format!("materials/{}", forward)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Field-access helpers for authoring-dict JSON
// ─────────────────────────────────────────────────────────────────────────────

/// Walk the `fields` array and return the value for the first entry whose
/// single key matches `key` (case-sensitive).
pub(crate) fn field_value<'a>(
    fields: &'a [serde_json::Value],
    key: &str,
) -> Option<&'a serde_json::Value> {
    for entry in fields {
        if let Some(obj) = entry.as_object() {
            if let Some(v) = obj.get(key) {
                return Some(v);
            }
        }
    }
    None
}

pub(crate) fn field_str<'a>(fields: &'a [serde_json::Value], key: &str) -> Option<&'a str> {
    field_value(fields, key)?.as_str()
}

/// Read a record-reference field from the authoring-dict `fields` array.
/// References render as `{"reference": {"plugin": "...", "object_id": "..."}}`;
/// returns a `"Plugin:ObjectId"` form_key string ready for plugin_handle lookups.
pub(crate) fn field_reference_form_key(fields: &[serde_json::Value], key: &str) -> Option<String> {
    let v = field_value(fields, key)?;
    let r = v.get("reference")?;
    let plugin = r.get("plugin").and_then(|x| x.as_str())?;
    let object_id = r.get("object_id").and_then(|x| x.as_str())?;
    let plugin = plugin.trim();
    let object_id = object_id.trim();
    if plugin.is_empty() || object_id.is_empty() {
        return None;
    }
    Some(format!("{plugin}:{object_id}"))
}

/// Same as `field_reference_form_key` but returns all matches in order.
pub(crate) fn field_all_reference_form_keys(
    fields: &[serde_json::Value],
    key: &str,
) -> Vec<String> {
    field_all(fields, key)
        .into_iter()
        .filter_map(|v| {
            let r = v.get("reference")?;
            let plugin = r.get("plugin").and_then(|x| x.as_str()).map(str::trim)?;
            let object_id = r.get("object_id").and_then(|x| x.as_str()).map(str::trim)?;
            if plugin.is_empty() || object_id.is_empty() {
                return None;
            }
            Some(format!("{plugin}:{object_id}"))
        })
        .collect()
}

/// Walk the `fields` array and return **all** values whose single key matches
/// `key` (case-sensitive). Used for repeated subrecords like `GrassTexture`.
pub(crate) fn field_all<'a>(
    fields: &'a [serde_json::Value],
    key: &str,
) -> Vec<&'a serde_json::Value> {
    let mut out = Vec::new();
    for entry in fields {
        if let Some(obj) = entry.as_object() {
            if let Some(v) = obj.get(key) {
                out.push(v);
            }
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Main entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Normalize a form key from the terrain crate's `"ObjectId:Plugin"` format to the
/// ESP index's expected `"Plugin:ObjectId"` format. If the left side is a hex
/// object ID and the right side is a plugin name, they are swapped. Otherwise the
/// input is returned unchanged.
pub(crate) fn normalize_esp_form_key(fk: &str) -> std::borrow::Cow<'_, str> {
    if let Some((left, right)) = fk.split_once(':') {
        let left = left.trim();
        let right = right.trim();
        // If left side is all hex and right side contains a dot (plugin extension),
        // assume "ObjectId:PluginName" — swap to "PluginName:ObjectId".
        let left_is_hex = !left.is_empty() && left.chars().all(|c| c.is_ascii_hexdigit());
        let right_is_plugin = right.contains('.');
        if left_is_hex && right_is_plugin {
            return std::borrow::Cow::Owned(format!("{right}:{left}"));
        }
    }
    std::borrow::Cow::Borrowed(fk)
}

/// Build a `TextureBundle` for a single LTEX record (grass is left empty —
/// filled in later by the grass walk).
pub fn build_bundle(
    handle_id: u64,
    ltex_form_key: &str,
    resolver: &Ba2Resolver,
    extraction_root: &Path,
    output_prefix_root: &str,
) -> Result<TextureBundle, String> {
    // Load the Rust-owned LTEX authoring value.
    // Terrain crate emits form keys as "ObjectId:Plugin"; ESP index expects "Plugin:ObjectId".
    let ltex_form_key_normalized = normalize_esp_form_key(ltex_form_key);
    let ltex_json =
        plugin_handle_read_authoring_record_value_json(handle_id, &ltex_form_key_normalized)
            .map_err(|e| format!("plugin lookup failed for {ltex_form_key}: {e}"))?
            .ok_or_else(|| format!("LTEX record not found: {ltex_form_key}"))?;

    let ltex_eid = ltex_json
        .get("eid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let fields = ltex_json
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    // ── 2. Texture field (BNAM, authoring key "Texture") ─────────────────────
    let texture_field = field_str(fields, "Texture")
        .or_else(|| field_str(fields, "BNAM"))
        .unwrap_or("")
        .to_string();

    // ── 3. Havok data (HNAM → "HavokData" → { "Friction", "Restitution" }) ───
    let (havok_friction, havok_restitution) = read_havok_data(fields);

    // ── 4. Material type object id (MNAM → "MaterialType") ───────────────────
    let material_type_object_id = read_material_type_object_id(fields);

    // ── 5. Dispatch: BGSM branch or TXST branch ──────────────────────────────
    let texture_lower = texture_field.to_ascii_lowercase();
    let is_bgsm = texture_lower.ends_with(".bgsm") || texture_lower.ends_with(".bgem");

    let source_txst_form_key = field_reference_form_key(fields, "TNAM")
        .or_else(|| field_reference_form_key(fields, "TextureSet"));

    let (
        diffuse_path,
        normal_path,
        reflectivity_path,
        lighting_path,
        source_txst_form_key,
        source_txst_editor_id,
        output_material_path,
        source_material_path,
        source_material_file,
    ) = if is_bgsm {
        bgsm_branch(
            &texture_field,
            resolver,
            extraction_root,
            output_prefix_root,
            &ltex_eid,
        )?
    } else if let Some(txst_form_key) = source_txst_form_key.as_deref() {
        let (a, b, c, d, e, f, g) = txst_branch_by_form_key(
            handle_id,
            txst_form_key,
            resolver,
            extraction_root,
            output_prefix_root,
        )
        .map_err(|e| format!("{e} (from LTEX {ltex_form_key})"))?;
        (a, b, c, d, e, f, g, String::new(), String::new())
    } else {
        let (a, b, c, d, e, f, g) = txst_branch_by_editor_id(
            handle_id,
            &ltex_eid,
            &texture_field,
            resolver,
            extraction_root,
            output_prefix_root,
        )
        .map_err(|e| format!("{e} (from LTEX {ltex_form_key})"))?;
        (a, b, c, d, e, f, g, String::new(), String::new())
    };

    // ── 6. Compute output_prefix ──────────────────────────────────────────────
    let label = if !ltex_eid.is_empty() {
        ltex_eid.clone()
    } else if !source_txst_editor_id.is_empty() {
        source_txst_editor_id.clone()
    } else {
        "Texture".to_string()
    };
    let output_prefix = join_manifest_path(output_prefix_root, &safe_name(&label));

    Ok(TextureBundle {
        source_ltex_form_key: ltex_form_key.to_string(),
        source_ltex_editor_id: ltex_eid,
        source_gcvr_form_key: None,
        source_gcvr_editor_id: None,
        source_txst_form_key,
        source_txst_editor_id,
        diffuse_path,
        normal_path,
        reflectivity_path,
        lighting_path,
        output_prefix,
        output_material_path,
        source_material_path,
        source_material_file,
        material_type_object_id,
        havok_friction,
        havok_restitution,
        grass: Vec::new(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// TXST branch
// ─────────────────────────────────────────────────────────────────────────────

type TxstBranchResult = (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
);

fn txst_branch_by_form_key(
    handle_id: u64,
    txst_form_key: &str,
    resolver: &Ba2Resolver,
    extraction_root: &Path,
    output_prefix_root: &str,
) -> Result<TxstBranchResult, String> {
    let txst_form_key_normalized = normalize_esp_form_key(txst_form_key);
    let txst_json =
        plugin_handle_read_authoring_record_value_json(handle_id, &txst_form_key_normalized)
            .map_err(|e| format!("plugin lookup failed for TXST {txst_form_key}: {e}"))?
            .ok_or_else(|| format!("TXST record not found: {txst_form_key}"))?;
    let txst_eid = txst_json
        .get("eid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    txst_branch_from_json(
        txst_form_key_normalized.into_owned(),
        txst_eid,
        txst_json,
        resolver,
        extraction_root,
        output_prefix_root,
    )
}

fn txst_branch_by_editor_id(
    handle_id: u64,
    ltex_eid: &str,
    texture_field: &str,
    resolver: &Ba2Resolver,
    extraction_root: &Path,
    output_prefix_root: &str,
) -> Result<TxstBranchResult, String> {
    // Derive the TXST editor-id: strip "Landscape" prefix if present, then
    // reconstruct as "Landscape<stem>".
    let stem = if !texture_field.is_empty() {
        // Strip extension + any leading path, use filename stem.
        let forward = texture_field.replace('\\', "/");
        let filename = forward.rsplit('/').next().unwrap_or(&forward);
        // Strip extension.
        let stem = filename.rsplit('.').skip(1).next().unwrap_or(filename);
        stem.to_string()
    } else {
        // Derive from LTEX eid.
        let stripped = ltex_eid.strip_prefix("Landscape").unwrap_or(ltex_eid);
        stripped.to_string()
    };

    let txst_eid = format!("Landscape{stem}");

    let (txst_form_key, txst_json) =
        plugin_handle_read_authoring_record_by_editor_id_and_signature_json(
            handle_id, &txst_eid, "TXST",
        )
        .map_err(|e| format!("plugin lookup failed for TXST {txst_eid}: {e}"))?
        .ok_or_else(|| format!("TXST record not found for editor id: {txst_eid}"))?;
    txst_branch_from_json(
        txst_form_key,
        txst_eid,
        txst_json,
        resolver,
        extraction_root,
        output_prefix_root,
    )
}

fn txst_branch_from_json(
    txst_form_key: String,
    txst_eid: String,
    txst_json: serde_json::Value,
    resolver: &Ba2Resolver,
    extraction_root: &Path,
    output_prefix_root: &str,
) -> Result<TxstBranchResult, String> {
    let txst_fields = txst_json
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    let diffuse_rel = txst_texture_field(txst_fields, &["Diffuse"])
        .ok_or_else(|| format!("TXST {txst_eid}: missing Diffuse field"))?
        .to_string();
    let normal_rel = txst_texture_field(txst_fields, &["NormalGloss"])
        .ok_or_else(|| format!("TXST {txst_eid}: missing NormalGloss field"))?
        .to_string();
    let spec_rel = txst_texture_field(txst_fields, &["Specular", "SmoothSpec"])
        .ok_or_else(|| format!("TXST {txst_eid}: missing Specular/SmoothSpec field"))?
        .to_string();
    let lighting_rel = txst_texture_field(txst_fields, &["Lighting", "Glow"])
        .ok_or_else(|| format!("TXST {txst_eid}: missing Lighting/Glow field"))?
        .to_string();

    let diffuse_norm = normalize_texture_path(&diffuse_rel);
    let normal_norm = normalize_texture_path(&normal_rel);
    let spec_norm = normalize_texture_path(&spec_rel);
    let lighting_norm = normalize_texture_path(&lighting_rel);

    let diffuse_out = resolver
        .extract_to(&diffuse_norm, extraction_root)
        .map_err(|e| format!("failed to extract diffuse for {txst_eid}: {e}"))?;
    let normal_out = resolver
        .extract_to(&normal_norm, extraction_root)
        .map_err(|e| format!("failed to extract normal for {txst_eid}: {e}"))?;
    let spec_out = resolver
        .extract_to(&spec_norm, extraction_root)
        .map_err(|e| format!("failed to extract specular for {txst_eid}: {e}"))?;
    let lighting_out = resolver
        .extract_to(&lighting_norm, extraction_root)
        .map_err(|e| format!("failed to extract lighting for {txst_eid}: {e}"))?;

    let _ = output_prefix_root; // output_prefix computed in caller

    Ok((
        path_to_string(diffuse_out),
        path_to_string(normal_out),
        path_to_string(spec_out),
        path_to_string(lighting_out),
        txst_form_key,
        txst_eid,
        None, // no material file
    ))
}

fn txst_texture_field<'a>(fields: &'a [serde_json::Value], keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(value) = field_str(fields, key).filter(|s| !s.is_empty()) {
            return Some(value);
        }
    }
    let texture_sets = field_value(fields, "TexturesRgbAs")?.as_array()?;
    for texture_set in texture_sets {
        let Some(texture_set) = texture_set.as_object() else {
            continue;
        };
        for key in keys {
            if let Some(value) = texture_set
                .get(*key)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                return Some(value);
            }
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// BGSM branch
// ─────────────────────────────────────────────────────────────────────────────

fn bgsm_branch(
    texture_field: &str,
    resolver: &Ba2Resolver,
    extraction_root: &Path,
    output_prefix_root: &str,
    ltex_eid: &str,
) -> Result<
    (
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        String,
    ),
    String,
> {
    let mat_rel = normalize_material_path(texture_field);
    let mat_bytes = resolver
        .find(&mat_rel)
        .ok_or_else(|| format!("material file not found in FO76 archives: {mat_rel}"))?;

    let bgsm_data = bgsm::parse(&mat_bytes).map_err(|e| {
        let filename = mat_rel.rsplit('/').next().unwrap_or(&mat_rel);
        format!("failed to parse BGSM {filename}: {e}")
    })?;

    let diffuse_rel = normalize_texture_path(&bgsm_data.DiffuseTexture);
    let normal_rel = normalize_texture_path(&bgsm_data.NormalTexture);

    // SpecularTexture (Option) first, then fall back to SmoothSpecTexture.
    let spec_src = bgsm_data
        .SpecularTexture
        .as_deref()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            if bgsm_data.SmoothSpecTexture.is_empty() {
                None
            } else {
                Some(bgsm_data.SmoothSpecTexture.as_str())
            }
        })
        .ok_or_else(|| {
            let filename = mat_rel.rsplit('/').next().unwrap_or(&mat_rel);
            format!("BGSM {filename}: missing specular texture (SpecularTexture and SmoothSpecTexture both empty)")
        })?;
    let spec_rel = normalize_texture_path(spec_src);

    let lighting_src = bgsm_data
        .LightingTexture
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            let filename = mat_rel.rsplit('/').next().unwrap_or(&mat_rel);
            format!("BGSM {filename}: missing LightingTexture")
        })?;
    let lighting_rel = normalize_texture_path(lighting_src);

    // Extract the material file itself.
    let mat_out = resolver
        .extract_to(&mat_rel, extraction_root)
        .map_err(|e| format!("failed to extract material {mat_rel}: {e}"))?;
    let source_material_file = path_to_string(mat_out);

    let diffuse_out = resolver
        .extract_to(&diffuse_rel, extraction_root)
        .map_err(|e| format!("failed to extract diffuse for bgsm {mat_rel}: {e}"))?;
    let normal_out = resolver
        .extract_to(&normal_rel, extraction_root)
        .map_err(|e| format!("failed to extract normal for bgsm {mat_rel}: {e}"))?;
    let spec_out = resolver
        .extract_to(&spec_rel, extraction_root)
        .map_err(|e| format!("failed to extract specular for bgsm {mat_rel}: {e}"))?;
    let lighting_out = resolver
        .extract_to(&lighting_rel, extraction_root)
        .map_err(|e| format!("failed to extract lighting for bgsm {mat_rel}: {e}"))?;

    // Compute output_material_path from the output_prefix_root + ltex label.
    let label = if !ltex_eid.is_empty() {
        ltex_eid
    } else {
        mat_rel.rsplit('/').next().unwrap_or("Texture")
    };
    let prefix = join_manifest_path(output_prefix_root, &safe_name(label));
    let output_material_path = Some(material_path_for_output_prefix(&prefix));

    Ok((
        path_to_string(diffuse_out),
        path_to_string(normal_out),
        path_to_string(spec_out),
        path_to_string(lighting_out),
        String::new(), // no TXST form_key in BGSM path
        String::new(), // no TXST editor_id in BGSM path
        output_material_path,
        texture_field.to_owned(), // source_material_path (original LTEX reference)
        source_material_file,     // absolute path to extracted BGSM on disk
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// Havok / material-type helpers
// ─────────────────────────────────────────────────────────────────────────────

fn read_havok_data(fields: &[serde_json::Value]) -> (Option<u8>, Option<u8>) {
    let Some(hd) = field_value(fields, "HavokData") else {
        return (None, None);
    };
    // HavokData value is a struct-dict: { "Friction": 30, "Restitution": 30 }.
    // Python parity: accept ints AND numeric strings; clamp to 0..=255; default 30.
    let friction = Some(havok_field(hd, "Friction", 30));
    let restitution = Some(havok_field(hd, "Restitution", 30));
    (friction, restitution)
}

/// Read a u8 field from a `HavokData` JSON object, accepting either a JSON
/// integer or a numeric string (matches Python's `int(value)`). Clamps to
/// `0..=255`. Returns `default` if the key is missing or unparseable.
pub(crate) fn havok_field(havok: &serde_json::Value, key: &str, default: u8) -> u8 {
    let Some(v) = havok.get(key) else {
        return default;
    };
    let parsed = v
        .as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()));
    parsed.map(|n| n.min(255) as u8).unwrap_or(default)
}

fn read_material_type_object_id(fields: &[serde_json::Value]) -> Option<String> {
    let form_key_str = field_reference_form_key(fields, "MaterialType")?;
    // form_key now in "Plugin:ObjectId" shape; pull the object_id.
    let (_, object_id_text) = form_key_str.rsplit_once(':')?;
    let object_id = u32::from_str_radix(object_id_text.trim(), 16).ok()?;
    Some(format!("{:06X}", object_id & 0x00FF_FFFF))
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_name_keeps_alnum_drops_punct() {
        assert_eq!(safe_name("Forest Grass/01"), "ForestGrass_01");
        assert_eq!(safe_name("a.b-c"), "a_b_c");
    }

    #[test]
    fn join_manifest_path_handles_blanks() {
        assert_eq!(
            join_manifest_path("textures/terrain/x", "Foo"),
            "textures/terrain/x/Foo"
        );
        assert_eq!(join_manifest_path("", "Foo"), "Foo");
        assert_eq!(join_manifest_path("textures/", "/Foo/"), "textures/Foo");
    }

    #[test]
    fn material_path_for_output_prefix_strips_textures_prefix_and_appends_bgsm() {
        assert_eq!(
            material_path_for_output_prefix("textures/terrain/appalachia/Foo"),
            "terrain/appalachia/Foo.bgsm"
        );
    }

    #[test]
    fn normalize_texture_path_lowercase_forward_slash_prefix() {
        assert_eq!(
            normalize_texture_path("Foo\\Bar.dds"),
            "textures/Foo/Bar.dds"
        );
        assert_eq!(normalize_texture_path("textures/x.dds"), "textures/x.dds");
    }

    #[test]
    fn havok_value_parses_string_or_int() {
        use serde_json::json;
        assert_eq!(havok_field(&json!({"Friction": 42}), "Friction", 30), 42);
        assert_eq!(havok_field(&json!({"Friction": "42"}), "Friction", 30), 42);
        assert_eq!(havok_field(&json!({"Friction": "abc"}), "Friction", 30), 30);
        assert_eq!(havok_field(&json!({}), "Friction", 30), 30);
        assert_eq!(havok_field(&json!({"Friction": 300}), "Friction", 30), 255);
    }

    #[test]
    fn field_all_zero_matches() {
        use serde_json::json;
        let fields = vec![json!({"Other": "a"}), json!({"Another": "b"})];
        assert!(field_all(&fields, "GrassTexture").is_empty());
    }

    #[test]
    fn field_all_single_match() {
        use serde_json::json;
        let fields = vec![
            json!({"GrassTexture": "001:Foo.esm"}),
            json!({"Other": "x"}),
        ];
        let result = field_all(&fields, "GrassTexture");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].as_str().unwrap(), "001:Foo.esm");
    }

    #[test]
    fn field_all_multiple_matches() {
        use serde_json::json;
        let fields = vec![
            json!({"GrassTexture": "001:Foo.esm"}),
            json!({"Other": "x"}),
            json!({"GrassTexture": "002:Foo.esm"}),
            json!({"GrassTexture": "003:Foo.esm"}),
        ];
        let result = field_all(&fields, "GrassTexture");
        assert_eq!(result.len(), 3);
        assert_eq!(result[2].as_str().unwrap(), "003:Foo.esm");
    }

    #[test]
    fn field_reference_form_key_parses_reference_shape() {
        use serde_json::json;
        let fields = vec![
            json!({ "GroundCover": { "reference": { "plugin": "Seventy.esm", "object_id": "011C67" }}}),
            json!({ "Other": "x" }),
        ];
        assert_eq!(
            field_reference_form_key(&fields, "GroundCover").as_deref(),
            Some("Seventy.esm:011C67")
        );
        assert!(field_reference_form_key(&fields, "Missing").is_none());
    }

    #[test]
    fn field_all_reference_form_keys_returns_all_in_order() {
        use serde_json::json;
        let fields = vec![
            json!({ "GrassTexture": { "reference": { "plugin": "P.esm", "object_id": "001" }}}),
            json!({ "UnknownInt": 65535 }),
            json!({ "GrassTexture": { "reference": { "plugin": "P.esm", "object_id": "002" }}}),
        ];
        let keys = field_all_reference_form_keys(&fields, "GrassTexture");
        assert_eq!(keys, vec!["P.esm:001".to_string(), "P.esm:002".to_string()]);
    }

    #[test]
    fn txst_texture_field_reads_direct_fields() {
        use serde_json::json;
        let fields = vec![
            json!({ "Diffuse": "terrain/appalachia/foo_d.dds" }),
            json!({ "Specular": "terrain/appalachia/foo_s.dds" }),
        ];
        assert_eq!(
            txst_texture_field(&fields, &["Diffuse"]),
            Some("terrain/appalachia/foo_d.dds")
        );
        assert_eq!(
            txst_texture_field(&fields, &["Specular", "SmoothSpec"]),
            Some("terrain/appalachia/foo_s.dds")
        );
    }

    #[test]
    fn txst_texture_field_reads_textures_rgbas_entries() {
        use serde_json::json;
        let fields = vec![json!({
            "TexturesRgbAs": [{
                "Diffuse": "terrain\\appalachia\\foo_d.dds",
                "NormalGloss": "terrain\\appalachia\\foo_n.dds",
                "SmoothSpec": "terrain\\appalachia\\foo_s.dds",
                "Glow": "terrain\\appalachia\\foo_g.dds"
            }]
        })];
        assert_eq!(
            txst_texture_field(&fields, &["Diffuse"]),
            Some("terrain\\appalachia\\foo_d.dds")
        );
        assert_eq!(
            txst_texture_field(&fields, &["Specular", "SmoothSpec"]),
            Some("terrain\\appalachia\\foo_s.dds")
        );
        assert_eq!(
            txst_texture_field(&fields, &["Lighting", "Glow"]),
            Some("terrain\\appalachia\\foo_g.dds")
        );
    }
}
