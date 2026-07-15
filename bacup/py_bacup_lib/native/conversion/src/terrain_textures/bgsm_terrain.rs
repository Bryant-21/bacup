//! Job 2: write converted FO4 BGSM files for source LTEXes that referenced
//! BGSM materials. Reads the extracted source BGSM, downgrades to FO4 v2,
//! rewrites texture-slot paths to the converted DDS prefix, writes to
//! `mod_path/data/materials/<output_material_path>`.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::terrain_textures::manifest::TextureManifest;
use materials_native::bgsm;
use materials_native::convert::{Game, downgrade_bgsm};

/// Returns the count of BGSM files written.
pub fn write_converted_materials(
    manifest: &TextureManifest,
    mod_path: &Path,
    source_game: &str,
) -> Result<u32, String> {
    let materials_root = mod_path.join("data").join("materials");
    let mut written = 0u32;
    let mut written_paths = HashSet::new();
    for bundle in &manifest.textures {
        if bundle.source_material_file.is_empty() {
            continue;
        }
        let Some(output_material_path) = bundle.output_material_path.as_deref() else {
            continue;
        };
        if output_material_path.is_empty() {
            continue;
        }
        if !written_paths.insert(output_material_path.to_ascii_lowercase()) {
            continue;
        }
        if bundle.output_prefix.is_empty() {
            return Err(format!(
                "bundle for {} has source_material_file but empty output_prefix",
                bundle.source_ltex_editor_id
            ));
        }

        let bytes = fs::read(&bundle.source_material_file)
            .map_err(|e| format!("read {}: {e}", bundle.source_material_file))?;
        let mut bgsm_data = bgsm::parse(&bytes)
            .map_err(|e| format!("parse {}: {e}", bundle.source_material_file))?;
        bgsm_data = downgrade_bgsm(
            bgsm_data,
            &bundle.source_material_path,
            material_game(source_game)?,
            Game::Fo4,
        );

        // Rewrite BGSM texture slots relative to the textures root.
        let prefix = texture_prefix_for_bgsm(&bundle.output_prefix);
        let glow_slot = if has_glow(&bgsm_data) {
            Some(bgsm_slot_path(&format!("{prefix}_g.dds")))
        } else {
            Some(String::new())
        };
        bgsm_data.DiffuseTexture = bgsm_slot_path(&format!("{prefix}_d.dds"));
        bgsm_data.NormalTexture = bgsm_slot_path(&format!("{prefix}_n.dds"));
        bgsm_data.SmoothSpecTexture = bgsm_slot_path(&format!("{prefix}_s.dds"));
        bgsm_data.GlowTexture = glow_slot;

        // Write to mod_path/data/materials/<output_material_path>.
        let target = materials_root.join(
            output_material_path
                .split('/')
                .collect::<std::path::PathBuf>(),
        );
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        let out_bytes = bgsm::write(&bgsm_data);
        fs::write(&target, out_bytes).map_err(|e| format!("write {}: {e}", target.display()))?;
        written += 1;
    }
    Ok(written)
}

fn material_game(game: &str) -> Result<Game, String> {
    Game::from_str(game).ok_or_else(|| format!("unknown material conversion game: {game}"))
}

fn texture_prefix_for_bgsm(output_prefix: &str) -> String {
    let s = output_prefix.replace('\\', "/");
    let s = s.trim_matches('/').to_owned();
    let without_root = if s.to_ascii_lowercase().starts_with("textures/") {
        strip_texture_root(&s)
    } else {
        &s
    };
    strip_known_asset_prefix(without_root)
}

fn strip_texture_root(path: &str) -> &str {
    let mut parts = path.splitn(2, '/');
    let first = parts.next().unwrap_or_default();
    if first.eq_ignore_ascii_case("textures") {
        parts.next().unwrap_or_default()
    } else {
        path
    }
}

fn strip_known_asset_prefix(path: &str) -> String {
    let mut parts = path.splitn(2, '/');
    let Some(first) = parts.next() else {
        return path.to_owned();
    };
    if !is_known_asset_prefix(first) {
        return path.to_owned();
    }
    parts.next().unwrap_or_default().to_owned()
}

fn is_known_asset_prefix(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "fo4" | "fo76" | "fnv" | "fo3" | "skyrim" | "skyrimse" | "starfield" | "oblivion"
    )
}

// BGSM texture-slot paths use forward slashes in vanilla FO4 (>99% of vanilla
// BGSMs); backslashes only appear in some Creation Club content. Keep the
// vanilla convention so FO4's BGSM loader recognises the slots.
fn bgsm_slot_path(value: &str) -> String {
    value.replace('\\', "/")
}

fn has_glow(bgsm: &bgsm::BgsmData) -> bool {
    bgsm.EmitEnabled
        || bgsm.Glowmap
        || bgsm
            .GlowTexture
            .as_deref()
            .map(|s| !s.replace('\0', "").trim().is_empty())
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_prefix_omits_textures_root() {
        assert_eq!(
            texture_prefix_for_bgsm("textures/fnv/terrain/appalachia/Foo"),
            "terrain/appalachia/Foo"
        );
        assert_eq!(
            texture_prefix_for_bgsm("/textures/terrain/Foo/"),
            "terrain/Foo"
        );
        assert_eq!(texture_prefix_for_bgsm("terrain/Foo"), "terrain/Foo");
    }

    #[test]
    fn bgsm_slot_path_uses_forward_slashes() {
        assert_eq!(bgsm_slot_path("terrain\\x.dds"), "terrain/x.dds");
        assert_eq!(bgsm_slot_path("terrain/x.dds"), "terrain/x.dds");
    }
}
