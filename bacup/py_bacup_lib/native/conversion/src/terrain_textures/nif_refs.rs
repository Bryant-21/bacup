use std::path::Path;

use nif_core_native::model::{NifFile, NifValue};

const INLINE_TEXTURE_FIELDS: &[&str] = &[
    "Source Texture",
    "Greyscale Texture",
    "Env Map Texture",
    "Normal Texture",
    "Env Mask Texture",
    "Reflectance Texture",
    "Lighting Texture",
    "Emit Gradient Texture",
];

fn is_shader_block(type_name: &str) -> bool {
    type_name == "BSLightingShaderProperty" || type_name == "BSEffectShaderProperty"
}

fn normalize(s: &str) -> String {
    s.trim().replace('\\', "/")
}

fn has_material_ext(lower: &str) -> bool {
    lower.ends_with(".bgsm") || lower.ends_with(".bgem")
}

/// Read .bgsm/.bgem material file references off BSLightingShaderProperty /
/// BSEffectShaderProperty blocks. Returns deduped, forward-slashed paths.
/// Returns empty vec on any read error (best-effort enrichment).
pub fn material_refs(nif_path: &Path) -> Vec<String> {
    let nif = match NifFile::load(nif_path) {
        Ok(n) => n,
        Err(_) => return Vec::new(),
    };

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();

    for block in nif.blocks.iter() {
        if !is_shader_block(&block.type_name) {
            continue;
        }
        let s = match block.get_field("Name") {
            Some(NifValue::String(s)) => s,
            _ => continue,
        };
        let normalized = normalize(s);
        if normalized.is_empty() {
            continue;
        }
        let lower = normalized.to_ascii_lowercase();
        if !has_material_ext(&lower) {
            continue;
        }
        if seen.insert(lower) {
            out.push(normalized);
        }
    }

    out
}

/// Read inline shader texture strings (Source Texture, Greyscale Texture, etc.)
/// off shader blocks that do NOT have a .bgsm/.bgem material name. Reads from
/// either direct fields on the block or a nested "Shader Property Data" struct.
/// Returns deduped, forward-slashed paths. Empty vec on any read error.
pub fn inline_texture_refs(nif_path: &Path) -> Vec<String> {
    let nif = match NifFile::load(nif_path) {
        Ok(n) => n,
        Err(_) => return Vec::new(),
    };

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();

    for block in nif.blocks.iter() {
        if !is_shader_block(&block.type_name) {
            continue;
        }

        // Skip blocks that have a material file — inline fields are inert.
        if let Some(NifValue::String(s)) = block.get_field("Name") {
            if has_material_ext(&s.to_ascii_lowercase()) {
                continue;
            }
        }

        // Path A: direct fields on the block.
        for &field_name in INLINE_TEXTURE_FIELDS {
            if let Some(NifValue::String(s)) = block.get_field(field_name) {
                let normalized = normalize(s);
                if !normalized.is_empty() {
                    let lower = normalized.to_ascii_lowercase();
                    if seen.insert(lower) {
                        out.push(normalized);
                    }
                }
            }
        }

        // Path B: nested "Shader Property Data" struct (FO76+).
        if let Some(NifValue::Struct(map)) = block.get_field("Shader Property Data") {
            for &field_name in INLINE_TEXTURE_FIELDS {
                if let Some(NifValue::String(s)) = map.get(field_name) {
                    let normalized = normalize(s);
                    if !normalized.is_empty() {
                        let lower = normalized.to_ascii_lowercase();
                        if seen.insert(lower) {
                            out.push(normalized);
                        }
                    }
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn empty_on_missing_file() {
        assert!(material_refs(Path::new("/definitely/not/a/file.nif")).is_empty());
        assert!(inline_texture_refs(Path::new("/definitely/not/a/file.nif")).is_empty());
    }

    #[test]
    fn empty_on_non_nif_file() {
        // pass a path that exists but isn't a NIF (e.g., Cargo.toml of this crate)
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(material_refs(&p).is_empty());
        assert!(inline_texture_refs(&p).is_empty());
    }
}
