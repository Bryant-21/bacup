//! Job 3: per-grass NIF asset graph conversion.
//!
//! For each grass entry: extract the source NIF, convert it to FO4,
//! walk material/texture refs found in the NIF, extract + convert each.
//! Populates `GrassEntry.assets` so the manifest records what was written.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::phase::textures::{build_request, game_texture_suffixes, group_textures};
use crate::terrain_textures::ba2_resolver::Ba2Resolver;
use crate::terrain_textures::manifest::{GrassAsset, GrassEntry};
use crate::terrain_textures::nif_refs::{inline_texture_refs, material_refs};
use materials_native::convert::{Game, downgrade_bgem, downgrade_bgsm};
use materials_native::texture_convert::{
    TextureConversionParamsPayload, convert_texture_set_paths,
};
use materials_native::{bgem, bgsm};
use nif_core_native::convert_file::{ConvertFileOptions, convert_nif_file};

#[derive(Default)]
pub struct GrassAssetCache {
    by_source_nif: HashMap<String, Vec<GrassAsset>>,
}

pub fn populate_assets_cached(
    entry: &mut GrassEntry,
    cache: &mut GrassAssetCache,
    resolver: &Ba2Resolver,
    extraction_root: &Path,
    mod_path: &Path,
    source_game: &str,
) {
    populate_with_cache(entry, cache, source_game, |entry| {
        populate_assets(entry, resolver, extraction_root, mod_path, source_game)
    });
}

pub fn collect_asset_refs_cached(
    entry: &mut GrassEntry,
    cache: &mut GrassAssetCache,
    resolver: &Ba2Resolver,
    extraction_root: &Path,
    source_game: &str,
) {
    populate_with_cache(entry, cache, source_game, |entry| {
        collect_asset_refs(entry, resolver, extraction_root, source_game)
    });
}

fn populate_with_cache<F>(
    entry: &mut GrassEntry,
    cache: &mut GrassAssetCache,
    source_game: &str,
    populate: F,
) where
    F: FnOnce(&mut GrassEntry),
{
    let Some(key) = grass_asset_cache_key(entry, source_game) else {
        populate(entry);
        return;
    };
    if let Some(assets) = cache.by_source_nif.get(&key) {
        entry.assets.extend(assets.clone());
        return;
    }

    let asset_start = entry.assets.len();
    populate(entry);
    cache
        .by_source_nif
        .insert(key, entry.assets[asset_start..].to_vec());
}

fn grass_asset_cache_key(entry: &GrassEntry, source_game: &str) -> Option<String> {
    if entry.model_file_name.is_empty() {
        return None;
    }
    Some(source_nif_archive_path(&entry.model_file_name, source_game).to_ascii_lowercase())
}

/// Populate `entry.assets` for one grass entry. Best-effort: on per-asset
/// failure, logs to stderr and continues. NIF extract-or-convert failure
/// short-circuits this entry's asset walk (downstream texture/material refs
/// inside that NIF can't be discovered without it).
pub fn populate_assets(
    entry: &mut GrassEntry,
    resolver: &Ba2Resolver,
    extraction_root: &Path,
    mod_path: &Path,
    source_game: &str,
) {
    if entry.model_file_name.is_empty() {
        return;
    }
    let mut seen: HashSet<String> = HashSet::new(); // dedupe by lowercased archive-relative path

    let target_game = "fo4";
    let asset_prefix = source_game;

    let nif_rel = source_nif_archive_path(&entry.model_file_name, asset_prefix);
    if !seen.insert(nif_rel.to_ascii_lowercase()) {
        return;
    }

    let nif_extracted = match resolver.extract_to(&nif_rel, extraction_root) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[grass_assets] extract NIF {nif_rel}: {e}");
            return;
        }
    };

    let nif_target_rel = target_nif_archive_path(&entry.model_file_name, asset_prefix);
    let nif_target = mod_path.join("data").join(rel_to_pathbuf(&nif_target_rel));
    if let Some(parent) = nif_target.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = convert_nif_file(
        &nif_extracted,
        &nif_target,
        source_game,
        target_game,
        None,
        &ConvertFileOptions {
            asset_prefix: None,
            ..ConvertFileOptions::default()
        },
    ) {
        eprintln!("[grass_assets] convert NIF {nif_rel}: {e:?}");
        return;
    }
    entry.assets.push(GrassAsset {
        asset_type: "nif".to_owned(),
        source_path: nif_rel.clone(),
        resolved_path: nif_extracted.to_string_lossy().into_owned(),
    });

    // Walk material refs on the extracted source NIF for fidelity. Materials
    // (BGSM/BGEM) are converted inline; their referenced textures are pooled
    // into `tex_refs` along with inline-NIF texture refs and converted via
    // the shared texture pipeline so source-game roles are mapped by the
    // normal texture conversion rules instead of copied with stale names.
    let mut tex_refs: Vec<String> = Vec::new();

    for mat_rel_raw in material_refs(&nif_extracted) {
        let mat_rel = normalize_material_archive_path(&mat_rel_raw, asset_prefix);
        if !seen.insert(format!("material:{}", mat_rel.to_ascii_lowercase())) {
            continue;
        }
        match convert_one_material(
            &mat_rel,
            resolver,
            extraction_root,
            mod_path,
            source_game,
            target_game,
            asset_prefix,
        ) {
            Ok(Some(asset)) => entry.assets.push(asset),
            Ok(None) => {}
            Err(e) => eprintln!("[grass_assets] material {mat_rel}: {e}"),
        }
        if let Ok(material_texture_paths) = collect_material_texture_refs(resolver, &mat_rel) {
            for tex_rel_raw in material_texture_paths {
                let tex_rel = normalize_texture_archive_path(&tex_rel_raw, asset_prefix);
                if seen.insert(format!("texture:{}", tex_rel.to_ascii_lowercase())) {
                    tex_refs.push(tex_rel);
                }
            }
        }
    }

    for tex_rel_raw in inline_texture_refs(&nif_extracted) {
        let tex_rel = normalize_texture_archive_path(&tex_rel_raw, asset_prefix);
        if seen.insert(format!("texture:{}", tex_rel.to_ascii_lowercase())) {
            tex_refs.push(tex_rel);
        }
    }

    convert_texture_refs(
        &tex_refs,
        resolver,
        extraction_root,
        mod_path,
        &mut entry.assets,
        source_game,
        target_game,
        asset_prefix,
    );
}

/// Populate `entry.assets` with the NIF/material/texture graph only.
///
/// This is used when the shared Python asset phases own conversion work. It
/// still resolves source files so downstream phases receive concrete paths, but
/// it does not write converted assets under the mod's data folder.
pub fn collect_asset_refs(
    entry: &mut GrassEntry,
    resolver: &Ba2Resolver,
    extraction_root: &Path,
    source_game: &str,
) {
    if entry.model_file_name.is_empty() {
        return;
    }
    let mut seen: HashSet<String> = HashSet::new();
    let asset_prefix = source_game;

    let nif_rel = source_nif_archive_path(&entry.model_file_name, asset_prefix);
    if !seen.insert(nif_rel.to_ascii_lowercase()) {
        return;
    }

    let nif_extracted = match resolver.extract_to(&nif_rel, extraction_root) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[grass_assets] extract NIF {nif_rel}: {e}");
            return;
        }
    };
    entry.assets.push(GrassAsset {
        asset_type: "nif".to_owned(),
        source_path: nif_rel.clone(),
        resolved_path: nif_extracted.to_string_lossy().into_owned(),
    });

    let mut tex_refs: Vec<String> = Vec::new();
    for mat_rel_raw in material_refs(&nif_extracted) {
        let mat_rel = normalize_material_archive_path(&mat_rel_raw, asset_prefix);
        if !seen.insert(format!("material:{}", mat_rel.to_ascii_lowercase())) {
            continue;
        }
        if let Ok(material_path) = resolver.extract_to(&mat_rel, extraction_root) {
            entry.assets.push(GrassAsset {
                asset_type: "material".to_owned(),
                source_path: mat_rel.clone(),
                resolved_path: material_path.to_string_lossy().into_owned(),
            });
        }
        if let Ok(material_texture_paths) = collect_material_texture_refs(resolver, &mat_rel) {
            for tex_rel_raw in material_texture_paths {
                let tex_rel = normalize_texture_archive_path(&tex_rel_raw, asset_prefix);
                if seen.insert(format!("texture:{}", tex_rel.to_ascii_lowercase())) {
                    tex_refs.push(tex_rel);
                }
            }
        }
    }

    for tex_rel_raw in inline_texture_refs(&nif_extracted) {
        let tex_rel = normalize_texture_archive_path(&tex_rel_raw, asset_prefix);
        if seen.insert(format!("texture:{}", tex_rel.to_ascii_lowercase())) {
            tex_refs.push(tex_rel);
        }
    }

    collect_texture_asset_refs(&tex_refs, resolver, extraction_root, &mut entry.assets);
}

fn collect_texture_asset_refs(
    tex_refs: &[String],
    resolver: &Ba2Resolver,
    extraction_root: &Path,
    out_assets: &mut Vec<GrassAsset>,
) {
    for tex_rel in tex_refs {
        if let Ok(extracted_path) = resolver.extract_to(tex_rel, extraction_root) {
            out_assets.push(GrassAsset {
                asset_type: "texture".to_owned(),
                source_path: tex_rel.clone(),
                resolved_path: extracted_path.to_string_lossy().into_owned(),
            });
        }
    }
}

/// Extract every referenced texture, then route through the shared
/// `convert_texture_set_paths` pipeline so source texture roles are remapped
/// the same way the `convert_textures` phase handles non-grass assets.
fn convert_texture_refs(
    tex_refs: &[String],
    resolver: &Ba2Resolver,
    extraction_root: &Path,
    mod_path: &Path,
    out_assets: &mut Vec<GrassAsset>,
    source_game: &str,
    target_game: &str,
    asset_prefix: &str,
) {
    if tex_refs.is_empty() {
        return;
    }

    // Resolve source textures and bucket by archive-relative parent directory so
    // each bucket maps to one output subdir under `mod_path/data/...`. Textures
    // missing from both extracted assets and BA2 fallback are silently skipped;
    // those may be static target-game references injected by material downgrade.
    let mut by_subdir: BTreeMap<String, Vec<(String, PathBuf)>> = BTreeMap::new();
    for tex_rel in tex_refs {
        match resolver.extract_to(tex_rel, extraction_root) {
            Ok(extracted_path) => {
                let parent = Path::new(tex_rel)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("")
                    .to_owned();
                by_subdir
                    .entry(parent)
                    .or_default()
                    .push((tex_rel.clone(), extracted_path));
            }
            Err(_) => continue,
        }
    }

    let source_suffixes = game_texture_suffixes(source_game);
    let target_suffixes = game_texture_suffixes(target_game);
    let format_overrides: HashMap<String, String> = HashMap::new();
    let params = TextureConversionParamsPayload::default();

    for (subdir, items) in by_subdir {
        let output_subdir = output_archive_path(&subdir, "textures", asset_prefix);
        let output_dir = mod_path.join("data").join(rel_to_pathbuf(&output_subdir));
        if let Err(e) = fs::create_dir_all(&output_dir) {
            eprintln!("[grass_assets] mkdir {}: {e}", output_dir.display());
            continue;
        }

        let source_paths: Vec<String> = items
            .iter()
            .map(|(_, path)| path.to_string_lossy().into_owned())
            .collect();
        let groups = group_textures(&source_paths, extraction_root, source_suffixes, source_game);

        for group in groups {
            let Some(request) = build_request(
                &group,
                &output_dir,
                source_game,
                target_game,
                source_suffixes,
                target_suffixes,
                &format_overrides,
                params,
                true,
                0,
            ) else {
                continue;
            };

            // Capture input source paths before the request moves into the
            // converter. Outputs may be merged (e.g. _r + _l → _s), so per-
            // output 1:1 attribution is intentionally lossy here.
            let source_paths: Vec<String> = request
                .inputs
                .iter()
                .filter_map(|i| i.path.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .collect();
            let source_label = source_paths.join(",");

            match convert_texture_set_paths(request) {
                Ok(result) => {
                    for item in result.converted {
                        out_assets.push(GrassAsset {
                            asset_type: "texture".to_owned(),
                            source_path: source_label.clone(),
                            resolved_path: item.path,
                        });
                    }
                }
                Err(e) => eprintln!(
                    "[grass_assets] texture set conv ({}): {e}",
                    output_dir.display()
                ),
            }
        }
    }
}

/// Returns Ok(Some(asset)) on conversion, Ok(None) if absent from BA2, Err on parse/write error.
fn convert_one_material(
    mat_rel: &str,
    resolver: &Ba2Resolver,
    extraction_root: &Path,
    mod_path: &Path,
    source_game: &str,
    target_game: &str,
    asset_prefix: &str,
) -> Result<Option<GrassAsset>, String> {
    let bytes = match resolver.find(mat_rel) {
        Some(b) => b,
        None => return Ok(None),
    };
    // Stash the extracted source for reproducibility (mirrors texture path).
    let _ = resolver.extract_to(mat_rel, extraction_root);

    let target_rel = output_archive_path(mat_rel, "materials", asset_prefix);
    let target = mod_path.join("data").join(rel_to_pathbuf(&target_rel));
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }

    let lower = mat_rel.to_ascii_lowercase();
    let out_bytes = if lower.ends_with(".bgsm") {
        let parsed = bgsm::parse(&bytes).map_err(|e| format!("parse {mat_rel}: {e:?}"))?;
        let mut downgraded = downgrade_bgsm(
            parsed,
            mat_rel,
            material_game(source_game)?,
            material_game(target_game)?,
        );
        normalize_bgsm_texture_paths(&mut downgraded, asset_prefix);
        bgsm::write(&downgraded)
    } else if lower.ends_with(".bgem") {
        let parsed = bgem::parse(&bytes).map_err(|e| format!("parse {mat_rel}: {e:?}"))?;
        let mut downgraded = downgrade_bgem(
            parsed,
            mat_rel,
            material_game(source_game)?,
            material_game(target_game)?,
        );
        normalize_bgem_texture_paths(&mut downgraded, asset_prefix);
        bgem::write(&downgraded)
    } else {
        // Unknown extension — copy as-is.
        bytes.clone()
    };
    fs::write(&target, &out_bytes).map_err(|e| format!("write {}: {e}", target.display()))?;
    Ok(Some(GrassAsset {
        asset_type: "material".to_owned(),
        source_path: mat_rel.to_owned(),
        resolved_path: target.to_string_lossy().into_owned(),
    }))
}

/// Read a source BGSM/BGEM from BA2 and return its inline texture slot strings.
fn collect_material_texture_refs(
    resolver: &Ba2Resolver,
    mat_rel: &str,
) -> Result<Vec<String>, String> {
    let Some(bytes) = resolver.find(mat_rel) else {
        return Ok(Vec::new());
    };
    let lower = mat_rel.to_ascii_lowercase();
    let mut texs: Vec<String> = Vec::new();

    let clean = |s: &str| s.replace('\0', "").trim().to_owned();
    let push = |out: &mut Vec<String>, v: &str| {
        let c = clean(v);
        if !c.is_empty() {
            out.push(c);
        }
    };

    if lower.ends_with(".bgsm") {
        let m = bgsm::parse(&bytes).map_err(|e| format!("parse {mat_rel}: {e:?}"))?;
        push(&mut texs, &m.DiffuseTexture);
        push(&mut texs, &m.NormalTexture);
        push(&mut texs, &m.SmoothSpecTexture);
        push(&mut texs, &m.GreyscaleTexture);
        for opt in [
            &m.EnvmapTexture,
            &m.GlowTexture,
            &m.InnerLayerTexture,
            &m.WrinklesTexture,
            &m.DisplacementTexture,
            &m.SpecularTexture,
            &m.LightingTexture,
            &m.FlowTexture,
            &m.DistanceFieldAlphaTexture,
        ] {
            if let Some(s) = opt.as_deref() {
                push(&mut texs, s);
            }
        }
    } else if lower.ends_with(".bgem") {
        let m = bgem::parse(&bytes).map_err(|e| format!("parse {mat_rel}: {e:?}"))?;
        push(&mut texs, &m.BaseTexture);
        push(&mut texs, &m.GrayscaleTexture);
        push(&mut texs, &m.EnvmapTexture);
        push(&mut texs, &m.NormalTexture);
        push(&mut texs, &m.EnvmapMaskTexture);
        for opt in [
            &m.SpecularTexture,
            &m.LightingTexture,
            &m.GlowTexture,
            &m.GlassRoughnessScratch,
            &m.GlassDirtOverlay,
        ] {
            if let Some(s) = opt.as_deref() {
                push(&mut texs, s);
            }
        }
    }
    Ok(texs)
}

// ---- path helpers ----

fn source_nif_archive_path(model_file_name: &str, asset_prefix: &str) -> String {
    let s = model_file_name
        .replace('\\', "/")
        .trim_matches(|c: char| c.is_ascii_whitespace() || c == '\0' || c == '/')
        .to_owned();
    let without_meshes = strip_archive_root(&s, "meshes");
    let without_prefix = strip_asset_prefix(without_meshes, asset_prefix);
    format!("meshes/{without_prefix}")
}

fn target_nif_archive_path(model_file_name: &str, asset_prefix: &str) -> String {
    let s = model_file_name
        .replace('\\', "/")
        .trim_matches(|c: char| c.is_ascii_whitespace() || c == '\0' || c == '/')
        .to_owned();
    output_archive_path(&s, "meshes", asset_prefix)
}

fn normalize_texture_archive_path(p: &str, asset_prefix: &str) -> String {
    let s = p
        .replace('\\', "/")
        .trim_matches(|c: char| c.is_ascii_whitespace() || c == '\0' || c == '/')
        .to_owned();
    let without_root = strip_archive_root(&s, "textures");
    let without_prefix = strip_asset_prefix(without_root, asset_prefix);
    format!("textures/{without_prefix}")
}

fn normalize_material_archive_path(p: &str, asset_prefix: &str) -> String {
    let s = p
        .replace('\\', "/")
        .trim_matches(|c: char| c.is_ascii_whitespace() || c == '\0' || c == '/')
        .to_owned();
    let without_root = strip_archive_root(&s, "materials");
    let without_prefix = strip_asset_prefix(without_root, asset_prefix);
    format!("materials/{without_prefix}")
}

fn output_archive_path(path: &str, root: &str, asset_prefix: &str) -> String {
    let s = path
        .replace('\\', "/")
        .trim_matches(|c: char| c.is_ascii_whitespace() || c == '\0' || c == '/')
        .to_owned();
    if s.is_empty() {
        return root.to_owned();
    }
    let without_root = strip_archive_root(&s, root);
    let without_prefix = strip_asset_prefix(without_root, asset_prefix);
    format!("{root}/{without_prefix}")
}

fn strip_archive_root<'a>(path: &'a str, root: &str) -> &'a str {
    path.strip_prefix(&format!("{root}/"))
        .or_else(|| path.strip_prefix(&format!("{}/", root.to_ascii_uppercase())))
        .filter(|_| !root.is_empty())
        .unwrap_or_else(|| {
            let mut parts = path.splitn(2, '/');
            let first = parts.next().unwrap_or_default();
            if first.eq_ignore_ascii_case(root) {
                parts.next().unwrap_or_default()
            } else {
                path
            }
        })
}

fn strip_asset_prefix<'a>(path: &'a str, asset_prefix: &str) -> &'a str {
    let mut parts = path.splitn(2, '/');
    let first = parts.next().unwrap_or_default();
    if first.eq_ignore_ascii_case(asset_prefix) {
        parts.next().unwrap_or_default()
    } else {
        path
    }
}

fn normalize_bgsm_texture_paths(bgsm: &mut bgsm::BgsmData, asset_prefix: &str) {
    bgsm.DiffuseTexture = material_texture_output_path(&bgsm.DiffuseTexture, asset_prefix);
    bgsm.NormalTexture = material_texture_output_path(&bgsm.NormalTexture, asset_prefix);
    bgsm.SmoothSpecTexture = material_texture_output_path(&bgsm.SmoothSpecTexture, asset_prefix);
    bgsm.GreyscaleTexture = material_texture_output_path(&bgsm.GreyscaleTexture, asset_prefix);
    if let Some(value) = bgsm.GlowTexture.take() {
        bgsm.GlowTexture = Some(material_texture_output_path(&value, asset_prefix));
    }
    if let Some(value) = bgsm.EnvmapTexture.take() {
        bgsm.EnvmapTexture = Some(material_texture_output_path(&value, asset_prefix));
    }
    if let Some(value) = bgsm.InnerLayerTexture.take() {
        bgsm.InnerLayerTexture = Some(material_texture_output_path(&value, asset_prefix));
    }
    if let Some(value) = bgsm.WrinklesTexture.take() {
        bgsm.WrinklesTexture = Some(material_texture_output_path(&value, asset_prefix));
    }
    if let Some(value) = bgsm.DisplacementTexture.take() {
        bgsm.DisplacementTexture = Some(material_texture_output_path(&value, asset_prefix));
    }
    if let Some(value) = bgsm.SpecularTexture.take() {
        bgsm.SpecularTexture = Some(material_texture_output_path(&value, asset_prefix));
    }
    if let Some(value) = bgsm.LightingTexture.take() {
        bgsm.LightingTexture = Some(material_texture_output_path(&value, asset_prefix));
    }
    if let Some(value) = bgsm.FlowTexture.take() {
        bgsm.FlowTexture = Some(material_texture_output_path(&value, asset_prefix));
    }
    if let Some(value) = bgsm.DistanceFieldAlphaTexture.take() {
        bgsm.DistanceFieldAlphaTexture = Some(material_texture_output_path(&value, asset_prefix));
    }
}

fn normalize_bgem_texture_paths(bgem: &mut bgem::BgemData, asset_prefix: &str) {
    bgem.BaseTexture = material_texture_output_path(&bgem.BaseTexture, asset_prefix);
    bgem.GrayscaleTexture = material_texture_output_path(&bgem.GrayscaleTexture, asset_prefix);
    bgem.EnvmapTexture = material_texture_output_path(&bgem.EnvmapTexture, asset_prefix);
    bgem.NormalTexture = material_texture_output_path(&bgem.NormalTexture, asset_prefix);
    bgem.EnvmapMaskTexture = material_texture_output_path(&bgem.EnvmapMaskTexture, asset_prefix);
    if let Some(value) = bgem.SpecularTexture.take() {
        bgem.SpecularTexture = Some(material_texture_output_path(&value, asset_prefix));
    }
    if let Some(value) = bgem.LightingTexture.take() {
        bgem.LightingTexture = Some(material_texture_output_path(&value, asset_prefix));
    }
    if let Some(value) = bgem.GlowTexture.take() {
        bgem.GlowTexture = Some(material_texture_output_path(&value, asset_prefix));
    }
}

fn material_texture_output_path(path: &str, asset_prefix: &str) -> String {
    let clean = path.trim_end_matches('\0').trim();
    if clean.is_empty() {
        return clean.to_owned();
    }
    let mut normalized = clean.replace('\\', "/");
    if let Some(stripped) = normalized.strip_prefix("data/") {
        normalized = stripped.to_owned();
    } else if let Some(stripped) = normalized.strip_prefix("Data/") {
        normalized = stripped.to_owned();
    }
    if !normalized.to_ascii_lowercase().starts_with("textures/") {
        normalized = format!("textures/{normalized}");
    }

    // BGSM texture slot paths use forward slashes in vanilla FO4 (8787/8848
    // of vanilla diffuse slots). Backslashes only appear in some Creation
    // Club content. FO4's BGSM loader treats them differently from the NIF
    // texture-set loader; sticking with the vanilla convention avoids pink.
    let without_root = strip_archive_root(&normalized, "textures");
    let without_prefix = strip_asset_prefix(without_root, asset_prefix);
    without_prefix.to_owned()
}

fn material_game(game: &str) -> Result<Game, String> {
    Game::from_str(game).ok_or_else(|| format!("unknown material conversion game: {game}"))
}

fn rel_to_pathbuf(rel: &str) -> PathBuf {
    rel.split('/').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_nif_archive_path_strips_output_prefix() {
        assert_eq!(
            source_nif_archive_path("fnv\\landscape\\grass\\foo.nif", "fnv"),
            "meshes/landscape/grass/foo.nif"
        );
        assert_eq!(
            source_nif_archive_path("Meshes/fnv/x.nif", "fnv"),
            "meshes/x.nif"
        );
    }

    #[test]
    fn target_nif_archive_path_omits_output_prefix() {
        assert_eq!(
            target_nif_archive_path("landscape\\grass\\foo.nif", "fnv"),
            "meshes/landscape/grass/foo.nif"
        );
        assert_eq!(
            target_nif_archive_path("Meshes/fnv/x.nif", "fnv"),
            "meshes/x.nif"
        );
    }

    #[test]
    fn material_texture_output_path_uses_forward_slashes() {
        assert_eq!(
            material_texture_output_path("Textures/Landscape/Grass/foo_d.dds", "fnv"),
            "Landscape/Grass/foo_d.dds"
        );
        assert_eq!(
            material_texture_output_path("Textures/Shared/Cubemaps/base.dds", "fnv"),
            "Shared/Cubemaps/base.dds"
        );
        assert_eq!(
            material_texture_output_path("fo76/Landscape/Plants/Bramble01_n.dds", "fo76"),
            "Landscape/Plants/Bramble01_n.dds"
        );
    }

    #[test]
    fn grass_asset_cache_reuses_normalized_model_assets() {
        let mut cache = GrassAssetCache::default();
        let mut calls = 0u32;
        let mut first = GrassEntry {
            model_file_name: "Landscape\\Grass\\Foo.NIF".to_owned(),
            ..GrassEntry::default()
        };
        populate_with_cache(&mut first, &mut cache, "fo76", |entry| {
            calls += 1;
            entry.assets.push(GrassAsset {
                asset_type: "nif".to_owned(),
                source_path: "meshes/landscape/grass/foo.nif".to_owned(),
                resolved_path: "X:/extracted/fo76/meshes/landscape/grass/foo.nif"
                    .to_owned(),
            });
        });

        let mut second = GrassEntry {
            model_file_name: "Meshes/FO76/Landscape/Grass/Foo.nif".to_owned(),
            ..GrassEntry::default()
        };
        populate_with_cache(&mut second, &mut cache, "fo76", |entry| {
            calls += 1;
            entry.assets.push(GrassAsset {
                asset_type: "nif".to_owned(),
                source_path: "unexpected".to_owned(),
                resolved_path: "unexpected".to_owned(),
            });
        });

        assert_eq!(calls, 1);
        assert_eq!(second.assets.len(), 1);
        assert_eq!(
            second.assets[0].source_path,
            "meshes/landscape/grass/foo.nif"
        );
    }
}
