// Params shape (JSON passed via `run_phase`):
// {
//   "textures":               ["Textures/Weapons/Foo_d.dds", ...],  // relative, from dep-graph
//   "source_extracted":       "/abs/path/to/extracted",             // source game BA2-extracted root
//   "target_format_overrides": { "Foo_d.dds": "BC7_UNORM" },       // optional per-file format
//   "ao_multiplier":          0.5,                                  // optional remix params
//   "specular_multiplier":    1.0,
//   "gloss_multiplier":       1.0,
//   "spec_offset":            0.8,
//   "gpu_min_pixels":         262144,                               // optional BC7 GPU cutoff
//   "conversion_workers":     16
// }
//
// Phase output: DDS files written to `mod_path/data/Textures/...`.
// PhaseReport.assets_written = total files written.
// PhaseReport.records_dropped = target-game-owned outputs skipped.
// PhaseReport.warnings       = total conversions skipped or failed.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use materials_native::texture_convert::{
    TextureConversionParamsPayload, TexturePathInput, TexturePathOutput, TextureSetPathRequest,
};

// ---------------------------------------------------------------------------
// Texture role + suffix tables (mirrors Python naming.py / compression.py)
// ---------------------------------------------------------------------------
//
// Each game's suffix list is ordered longest-first so that a longer match
// ("_sk" / "_normal") is found before its shorter prefix ("_s" / "_n").

pub fn game_texture_suffixes(game: &str) -> &'static [(&'static str, &'static str)] {
    match game {
        "fo76" => &[
            ("diffuse", "_d"),
            ("normal", "_n"),
            ("glow", "_g"),
            ("lighting", "_l"),
            ("reflectivity", "_r"),
        ],
        "skyrimse" | "skyrim" => &[
            ("subsurface", "_sk"), // longer than "_s", must be first
            ("diffuse", "_d"),
            ("normal", "_n"),
            ("glow", "_g"),
            ("specular", "_s"),
        ],
        "starfield" => &[
            ("normal", "_normal"),
            ("roughness", "_rough"),
            ("metallic", "_metal"),
            ("diffuse", "_color"),
            ("ao", "_ao"),
            ("glow", "_emissive"),
        ],
        "fo3" | "fnv" => &[("diffuse", "_d"), ("normal", "_n")],
        // fo4 and all other games
        _ => &[
            ("subsurface", "_sk"),
            ("diffuse", "_d"),
            ("normal", "_n"),
            ("glow", "_g"),
            ("specular", "_s"),
        ],
    }
}

/// Detect the semantic role of a texture from its stem (filename without extension).
/// Returns the role string or None if no suffix matches.
fn detect_role(
    stem: &str,
    suffixes: &'static [(&'static str, &'static str)],
) -> Option<&'static str> {
    let lower = stem.to_ascii_lowercase();
    for (role, sfx) in suffixes {
        if lower.ends_with(*sfx) {
            return Some(role);
        }
    }
    None
}

fn detect_role_for_source(
    stem: &str,
    suffixes: &'static [(&'static str, &'static str)],
    source_game: &str,
) -> Option<&'static str> {
    detect_role(stem, suffixes).or_else(|| {
        if source_game.eq_ignore_ascii_case("fo76") {
            Some("diffuse")
        } else {
            None
        }
    })
}

fn role_stem_base(
    stem: &str,
    role: &str,
    suffixes: &'static [(&'static str, &'static str)],
    source_game: &str,
) -> String {
    if source_game.eq_ignore_ascii_case("fo76") && role == "diffuse" {
        if !stem
            .to_ascii_lowercase()
            .ends_with(suffix_for_role(suffixes, "diffuse").unwrap_or("_d"))
        {
            return stem.to_ascii_lowercase();
        }
    }
    let sfx = suffixes
        .iter()
        .find(|(r, _)| *r == role)
        .map(|(_, s)| *s)
        .unwrap_or("");
    let lower = stem.to_ascii_lowercase();
    let sfx_lower = sfx.to_ascii_lowercase();
    let idx = lower.rfind(&sfx_lower).unwrap_or(lower.len());
    stem[..idx].to_ascii_lowercase()
}

fn texture_group_base_alias(path: &Path, base: &str, source_game: &str) -> String {
    // The FO76 eye lash bundle (abbreviated eyebro_* aux maps + the
    // eyebrown_d diffuse whose alpha carries the lash strands the FO76 lash
    // geometry UVs sample) must group apart from the bare vanilla-named eye
    // diffuses: its outputs are FO76-only names that need to ship, while a
    // shared group's bare eyebrown.dds diffuse output collides with the FO4
    // base texture and marks the whole group base-owned/skipped.
    if source_game.eq_ignore_ascii_case("fo76") && is_fo76_character_eye_lash_texture(path) {
        return "eyebrown_lashes".to_owned();
    }
    base.to_owned()
}

fn is_fo76_character_eye_lash_texture(path: &Path) -> bool {
    let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let filename_lower = filename.to_ascii_lowercase();
    let is_lash_member = filename_lower.ends_with(".dds")
        && (filename_lower.starts_with("eyebro_") || filename_lower == "eyebrown_d.dds");
    if !is_lash_member {
        return false;
    }

    let rel = texture_relative_to_textures_root(&path.to_string_lossy(), Path::new(""));
    rel.to_ascii_lowercase() == format!("actors/character/eyes/{filename_lower}")
}

fn compression_for_role(role: &str) -> &'static str {
    match role {
        "normal" | "specular" => "BC5_UNORM",
        _ => "BC7_UNORM",
    }
}

/// Cross-game role fallback table (mirrors Python naming._ROLE_FALLBACKS).
fn fallback_role(role: &str) -> Option<&'static str> {
    match role {
        "lighting" => Some("glow"),
        "reflectivity" => Some("specular"),
        "roughness" | "metallic" => Some("specular"),
        "specular" => Some("reflectivity"),
        _ => None,
    }
}

/// Map a source role to an output role in the target game.
/// Returns None if the target game has no appropriate slot.
fn output_role(
    src_role: &str,
    source_game: &str,
    target_game: &str,
    target_suffixes: &'static [(&'static str, &'static str)],
) -> Option<&'static str> {
    // fo76 → fo4: bundle path handles reflectivity/lighting merging; individual
    // fallback for partial sets.
    if source_game == "fo76" && target_game == "fo4" {
        if src_role == "reflectivity" || src_role == "lighting" {
            return Some("specular");
        }
    }

    // Direct match.
    if let Some((role, _)) = target_suffixes.iter().find(|(r, _)| *r == src_role) {
        return Some(role);
    }

    // Fallback mapping.
    let alt = fallback_role(src_role)?;
    target_suffixes
        .iter()
        .find(|(r, _)| *r == alt)
        .map(|(r, _)| *r)
}

/// Convert a texture filename from source-game suffix to target-game suffix.
/// Returns the filename unchanged if no source suffix is recognised.
fn convert_filename(
    filename: &str,
    source_suffixes: &'static [(&'static str, &'static str)],
    target_suffixes: &'static [(&'static str, &'static str)],
) -> String {
    let path = Path::new(filename);
    let stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return filename.to_owned(),
    };
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let lower = stem.to_ascii_lowercase();

    let Some((role, src_sfx)) = source_suffixes.iter().find(|(_, s)| lower.ends_with(*s)) else {
        return filename.to_owned();
    };

    // Find target suffix (direct or fallback).
    let target_sfx = target_suffixes
        .iter()
        .find(|(r, _)| *r == *role)
        .map(|(_, s)| *s)
        .or_else(|| {
            fallback_role(role).and_then(|alt| {
                target_suffixes
                    .iter()
                    .find(|(r, _)| *r == alt)
                    .map(|(_, s)| *s)
            })
        });

    let Some(tsuffix) = target_sfx else {
        return filename.to_owned();
    };

    let sfx_lower = src_sfx.to_ascii_lowercase();
    let idx = lower.rfind(&sfx_lower).unwrap_or(lower.len());
    format!("{}{}{}", &stem[..idx], tsuffix, ext)
}

// ---------------------------------------------------------------------------
// Group textures by base name
// ---------------------------------------------------------------------------

pub struct TextureGroup {
    pub files: Vec<(PathBuf, String)>, // (abs_source_path, role)
}

#[derive(Clone)]
pub(crate) struct TextureEntry {
    pub(crate) source_path: String,
    pub(crate) output_subpath: Option<String>,
}

pub fn group_textures(
    texture_paths: &[String],
    source_dir: &Path,
    source_suffixes: &'static [(&'static str, &'static str)],
    source_game: &str,
) -> Vec<TextureGroup> {
    let mut map: HashMap<String, Vec<(PathBuf, String)>> = HashMap::new();

    for rel in texture_paths {
        // Accept both absolute paths (e.g. from resolved_path) and
        // relative paths that are joined against source_dir.
        let abs = {
            let p = Path::new(rel.as_str());
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                source_dir.join(p)
            }
        };
        let filename = abs
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(rel.as_str());
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(filename);

        let role = detect_role_for_source(stem, source_suffixes, source_game)
            .unwrap_or("unknown")
            .to_owned();

        let base = if role != "unknown" {
            role_stem_base(stem, &role, source_suffixes, source_game)
        } else {
            stem.to_ascii_lowercase()
        };
        let base = texture_group_base_alias(&abs, &base, source_game);

        map.entry(base).or_default().push((abs, role));
    }

    add_implicit_fo76_bundle_siblings(&mut map, source_suffixes, source_game);
    add_implicit_fo76_glow_siblings(&mut map, source_suffixes);

    map.into_values()
        .map(|files| TextureGroup { files })
        .collect()
}

fn suffix_for_role(
    suffixes: &'static [(&'static str, &'static str)],
    role: &str,
) -> Option<&'static str> {
    suffixes
        .iter()
        .find(|(candidate, _)| *candidate == role)
        .map(|(_, suffix)| *suffix)
}

fn sibling_path_with_suffix(path: &Path, from_suffix: &str, to_suffix: &str) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_str()?;
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    let lower = stem.to_ascii_lowercase();
    let suffix_lower = from_suffix.to_ascii_lowercase();
    let idx = lower.rfind(&suffix_lower)?;
    let sibling_name = format!("{}{}{}", &stem[..idx], to_suffix, ext);
    Some(path.with_file_name(sibling_name))
}

fn path_with_role_suffix(path: &Path, from_suffix: &str, to_suffix: &str) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_str()?;
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    if from_suffix.is_empty() {
        return Some(path.with_file_name(format!("{stem}{to_suffix}{ext}")));
    }
    let lower = stem.to_ascii_lowercase();
    let suffix_lower = from_suffix.to_ascii_lowercase();
    let idx = lower.rfind(&suffix_lower)?;
    Some(path.with_file_name(format!("{}{}{}", &stem[..idx], to_suffix, ext)))
}

fn actual_role_suffix(
    path: &Path,
    role: &str,
    source_suffixes: &'static [(&'static str, &'static str)],
    source_game: &str,
) -> Option<&'static str> {
    let configured = suffix_for_role(source_suffixes, role)?;
    if source_game.eq_ignore_ascii_case("fo76") && role == "diffuse" {
        let stem = path.file_stem()?.to_str()?.to_ascii_lowercase();
        if !stem.ends_with(configured) {
            return Some("");
        }
    }
    Some(configured)
}

fn bundle_sibling_candidates(
    source_path: &Path,
    source_role: &str,
    target_role: &str,
    source_suffixes: &'static [(&'static str, &'static str)],
    source_game: &str,
) -> Vec<PathBuf> {
    let Some(from_suffix) =
        actual_role_suffix(source_path, source_role, source_suffixes, source_game)
    else {
        return Vec::new();
    };
    let Some(target_suffix) = suffix_for_role(source_suffixes, target_role) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    if let Some(path) = path_with_role_suffix(source_path, from_suffix, target_suffix) {
        candidates.push(path);
    }
    if source_game.eq_ignore_ascii_case("fo76") && target_role == "diffuse" {
        if let Some(path) = path_with_role_suffix(source_path, from_suffix, "") {
            if !candidates.iter().any(|candidate| candidate == &path) {
                candidates.push(path);
            }
        }
    }
    candidates
}

fn add_implicit_fo76_glow_siblings(
    groups: &mut HashMap<String, Vec<(PathBuf, String)>>,
    source_suffixes: &'static [(&'static str, &'static str)],
) {
    let Some(lighting_suffix) = suffix_for_role(source_suffixes, "lighting") else {
        return;
    };
    let Some(glow_suffix) = suffix_for_role(source_suffixes, "glow") else {
        return;
    };

    for files in groups.values_mut() {
        if files.iter().any(|(_, role)| role == "glow") {
            continue;
        }
        let Some(lighting_path) = files
            .iter()
            .find(|(_, role)| role == "lighting")
            .map(|(path, _)| path.clone())
        else {
            continue;
        };
        let Some(glow_path) =
            sibling_path_with_suffix(&lighting_path, lighting_suffix, glow_suffix)
        else {
            continue;
        };
        if glow_path.is_file() {
            files.push((glow_path, "glow".to_owned()));
        }
    }
}

fn add_implicit_fo76_bundle_siblings(
    groups: &mut HashMap<String, Vec<(PathBuf, String)>>,
    source_suffixes: &'static [(&'static str, &'static str)],
    source_game: &str,
) {
    let Some(diffuse_suffix) = suffix_for_role(source_suffixes, "diffuse") else {
        return;
    };
    let Some(reflectivity_suffix) = suffix_for_role(source_suffixes, "reflectivity") else {
        return;
    };
    let Some(lighting_suffix) = suffix_for_role(source_suffixes, "lighting") else {
        return;
    };
    let bundle_roles = [
        ("diffuse", diffuse_suffix),
        ("reflectivity", reflectivity_suffix),
        ("lighting", lighting_suffix),
    ];

    for files in groups.values_mut() {
        if !files.iter().any(|(_, role)| {
            bundle_roles
                .iter()
                .any(|(bundle_role, _)| role == bundle_role)
        }) {
            continue;
        }

        let mut seen_paths: HashSet<String> = files
            .iter()
            .map(|(path, _)| {
                path.to_string_lossy()
                    .replace('\\', "/")
                    .to_ascii_lowercase()
            })
            .collect();

        for (source_path, source_role) in files.clone() {
            if !bundle_roles.iter().any(|(role, _)| source_role == *role) {
                continue;
            }

            for (target_role, _) in bundle_roles {
                if files.iter().any(|(_, role)| role == target_role) {
                    continue;
                }
                for sibling_path in bundle_sibling_candidates(
                    &source_path,
                    &source_role,
                    target_role,
                    source_suffixes,
                    source_game,
                ) {
                    if !sibling_path.is_file() {
                        continue;
                    }
                    let sibling_key = sibling_path
                        .to_string_lossy()
                        .replace('\\', "/")
                        .to_ascii_lowercase();
                    if seen_paths.insert(sibling_key) {
                        files.push((sibling_path, target_role.to_owned()));
                        break;
                    }
                }
            }
        }
    }
}

pub(crate) fn bucket_textures_by_output_subdir(
    texture_entries: &[TextureEntry],
    source_dir: &Path,
) -> BTreeMap<String, Vec<String>> {
    let mut buckets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for texture in texture_entries {
        buckets
            .entry(texture_output_subdir_for_entry(texture, source_dir))
            .or_default()
            .push(texture.source_path.clone());
    }
    buckets
}

fn texture_output_subdir_for_entry(entry: &TextureEntry, source_dir: &Path) -> String {
    if let Some(output_subpath) = entry.output_subpath.as_deref() {
        return texture_output_subdir_from_data_subpath(output_subpath);
    }
    texture_output_subdir(&entry.source_path, source_dir)
}

fn texture_output_subdir_from_data_subpath(output_subpath: &str) -> String {
    let rel = output_subpath
        .trim()
        .replace('\\', "/")
        .trim_matches('/')
        .to_owned();
    let lower = rel.to_ascii_lowercase();
    let without_root = if lower.starts_with("data/textures/") {
        &rel[14..]
    } else if lower.starts_with("textures/") {
        &rel[9..]
    } else if lower.starts_with("data/") {
        &rel[5..]
    } else {
        rel.as_str()
    };
    without_root
        .rsplit_once('/')
        .map(|(parent, _)| parent.trim_matches('/').to_owned())
        .unwrap_or_default()
}

fn texture_output_subdir(texture_path: &str, source_dir: &Path) -> String {
    let rel = texture_relative_to_textures_root(texture_path, source_dir);
    rel.rsplit_once('/')
        .map(|(parent, _)| parent.trim_matches('/').to_owned())
        .unwrap_or_default()
}

fn texture_relative_to_textures_root(texture_path: &str, source_dir: &Path) -> String {
    let clean = texture_path
        .trim()
        .replace('\\', "/")
        .trim_matches('/')
        .to_owned();
    if clean.is_empty() {
        return clean;
    }

    let source_root = source_dir
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_owned();
    if !source_root.is_empty() {
        let source_prefix = format!("{source_root}/");
        if let Some(rest) = strip_prefix_ignore_ascii_case(&clean, &source_prefix) {
            return strip_texture_root_and_profile(rest);
        }
    }

    if let Some(rest) = after_path_segment_ignore_ascii_case(&clean, "textures") {
        return strip_known_profile_component(rest);
    }

    if is_absolute_path_like(&clean) {
        return clean
            .rsplit('/')
            .next()
            .unwrap_or(clean.as_str())
            .to_owned();
    }

    strip_texture_root_and_profile(&clean)
}

fn strip_texture_root_and_profile(path: &str) -> String {
    let rel = path.trim_matches('/');
    let lower = rel.to_ascii_lowercase();
    let without_root = if lower.starts_with("data/textures/") {
        &rel[14..]
    } else if lower.starts_with("textures/") {
        &rel[9..]
    } else if lower.starts_with("data/") {
        &rel[5..]
    } else {
        rel
    };
    strip_known_profile_component(without_root)
}

fn strip_known_profile_component(path: &str) -> String {
    let rel = path.trim_matches('/');
    let Some((first, rest)) = rel.split_once('/') else {
        return rel.to_owned();
    };
    if is_known_asset_prefix(first) {
        rest.to_owned()
    } else {
        rel.to_owned()
    }
}

fn strip_prefix_ignore_ascii_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    if value.len() >= prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&value[prefix.len()..])
    } else {
        None
    }
}

fn after_path_segment_ignore_ascii_case<'a>(path: &'a str, segment: &str) -> Option<&'a str> {
    let mut offset = 0usize;
    for part in path.split('/') {
        let next_offset = offset + part.len() + 1;
        if part.eq_ignore_ascii_case(segment) {
            return Some(path.get(next_offset..).unwrap_or_default());
        }
        offset = next_offset;
    }
    None
}

fn is_absolute_path_like(path: &str) -> bool {
    path.starts_with('/')
        || path.starts_with("//")
        || path.as_bytes().get(1).is_some_and(|value| *value == b':')
}

fn is_known_asset_prefix(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "fo4" | "fo76" | "fnv" | "fo3" | "skyrim" | "skyrimse" | "starfield" | "oblivion"
    )
}

fn rel_to_pathbuf(rel: &str) -> PathBuf {
    rel.split('/')
        .filter(|component| !component.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Build one TextureSetPathRequest for a texture group
// ---------------------------------------------------------------------------

const ROCKCLIFF76_GLOSS_MULTIPLIER: f32 = 0.33;

fn texture_group_rel_key(path: &Path) -> String {
    texture_relative_to_textures_root(&path.to_string_lossy(), Path::new(""))
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn conversion_params_for_group(
    group: &TextureGroup,
    source_game: &str,
    target_game: &str,
    params: TextureConversionParamsPayload,
) -> TextureConversionParamsPayload {
    if !source_game.eq_ignore_ascii_case("fo76") || !target_game.eq_ignore_ascii_case("fo4") {
        return params;
    }

    let has_rockcliff76_lighting = group.files.iter().any(|(path, role)| {
        role == "lighting" && texture_group_rel_key(path) == "landscape/rocks/rockcliff76_l.dds"
    });
    if !has_rockcliff76_lighting {
        return params;
    }

    let mut adjusted = params;
    adjusted.gloss_multiplier = adjusted.gloss_multiplier.min(ROCKCLIFF76_GLOSS_MULTIPLIER);
    adjusted
}

#[allow(clippy::too_many_arguments)]
pub fn build_request(
    group: &TextureGroup,
    output_dir: &Path,
    source_game: &str,
    target_game: &str,
    source_suffixes: &'static [(&'static str, &'static str)],
    target_suffixes: &'static [(&'static str, &'static str)],
    format_overrides: &HashMap<String, String>,
    params: TextureConversionParamsPayload,
    use_gpu: bool,
    gpu_min_pixels: u32,
) -> Option<TextureSetPathRequest> {
    let params = conversion_params_for_group(group, source_game, target_game, params);
    let roles: HashSet<&str> = group.files.iter().map(|(_, r)| r.as_str()).collect();

    let is_fo76_fo4_spec_pair = source_game == "fo76"
        && target_game == "fo4"
        && roles.contains("reflectivity")
        && roles.contains("lighting");
    let is_fo76_fo4_bundle = is_fo76_fo4_spec_pair && roles.contains("diffuse");

    let mut inputs: Vec<TexturePathInput> = Vec::new();
    let mut outputs: Vec<TexturePathOutput> = Vec::new();
    let mut seen_out: HashSet<(String, String)> = HashSet::new();

    for (abs_path, role) in &group.files {
        if role == "unknown" || !abs_path.is_file() {
            continue;
        }

        inputs.push(TexturePathInput {
            role: role.clone(),
            path: abs_path.clone(),
        });

        // FO76 reflectivity+lighting pairs merge into one FO4 spec/gloss map.
        // Some utility/effect sets ship only _r/_l, with no _d sibling.
        if is_fo76_fo4_spec_pair && (role == "reflectivity" || role == "lighting") {
            continue;
        }

        let out_role = match output_role(role, source_game, target_game, target_suffixes) {
            Some(r) => r,
            None => continue,
        };

        let filename = abs_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(role.as_str());
        let out_name = convert_filename(filename, source_suffixes, target_suffixes);

        let key = (out_role.to_owned(), out_name.clone());
        if seen_out.contains(&key) {
            continue;
        }
        seen_out.insert(key);

        let fmt = format_overrides
            .get(filename)
            .map(String::as_str)
            .unwrap_or_else(|| compression_for_role(out_role))
            .to_owned();

        outputs.push(TexturePathOutput {
            role: out_role.to_owned(),
            path: output_dir.join(&out_name),
            format: fmt,
        });
    }

    // For fo76→fo4, add the merged specular output whenever _r + _l exist,
    // even if the set has no diffuse texture.
    if is_fo76_fo4_spec_pair {
        // specular from reflectivity filename.
        if let Some((refl_path, _)) = group.files.iter().find(|(_, r)| r == "reflectivity") {
            if let Some(filename) = refl_path.file_name().and_then(|n| n.to_str()) {
                let spec_name = convert_filename(filename, source_suffixes, target_suffixes);
                let key = ("specular".to_owned(), spec_name.clone());
                if !seen_out.contains(&key) {
                    seen_out.insert(key);
                    let fmt = format_overrides
                        .get(filename)
                        .map(String::as_str)
                        .unwrap_or_else(|| compression_for_role("specular"))
                        .to_owned();
                    outputs.push(TexturePathOutput {
                        role: "specular".to_owned(),
                        path: output_dir.join(&spec_name),
                        format: fmt,
                    });
                }
            }
        }

        // Full diffuse bundles also synthesize glow from lighting. _r/_l-only
        // utility pairs only need the spec/gloss _s output.
        if is_fo76_fo4_bundle
            && let Some((light_path, _)) = group.files.iter().find(|(_, r)| r == "lighting")
        {
            if let Some(filename) = light_path.file_name().and_then(|n| n.to_str()) {
                let glow_name = {
                    // lighting _l → glow _g in target.
                    let glow_sfx = target_suffixes
                        .iter()
                        .find(|(r, _)| *r == "glow")
                        .map(|(_, s)| *s)
                        .unwrap_or("_g");
                    let src_sfx = source_suffixes
                        .iter()
                        .find(|(r, _)| *r == "lighting")
                        .map(|(_, s)| *s)
                        .unwrap_or("_l");
                    let stem = Path::new(filename)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(filename);
                    let ext = Path::new(filename)
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| format!(".{e}"))
                        .unwrap_or_default();
                    let lower = stem.to_ascii_lowercase();
                    let sfx_lower = src_sfx.to_ascii_lowercase();
                    let idx = lower.rfind(&sfx_lower).unwrap_or(lower.len());
                    format!("{}{}{}", &stem[..idx], glow_sfx, ext)
                };
                let key = ("glow".to_owned(), glow_name.clone());
                if !seen_out.contains(&key) {
                    seen_out.insert(key);
                    let fmt = format_overrides
                        .get(filename)
                        .map(String::as_str)
                        .unwrap_or_else(|| compression_for_role("glow"))
                        .to_owned();
                    outputs.push(TexturePathOutput {
                        role: "glow".to_owned(),
                        path: output_dir.join(&glow_name),
                        format: fmt,
                    });
                }
            }
        }
    }

    if inputs.is_empty() || outputs.is_empty() {
        return None;
    }

    Some(TextureSetPathRequest {
        source_game: source_game.to_owned(),
        target_game: target_game.to_owned(),
        inputs,
        outputs,
        params,
        use_gpu,
        gpu_min_pixels,
        // The phase already runs one rayon worker per texture group, so let each
        // image compress single-threaded — otherwise DirectXTex's per-image thread
        // fan nests inside the worker pool and oversubscribes the CPU
        // (conversion_workers x hardware_concurrency). Caps threads at the pool size.
        parallel_compression: false,
    })
}

pub(crate) struct TextureWorkItem {
    pub(crate) group: TextureGroup,
    pub(crate) output_dir: PathBuf,
}

pub(crate) fn enumerate_source_textures(source_extracted: &Path) -> Vec<String> {
    let mut out = Vec::new();
    collect_dds_files(&source_extracted.join("Textures"), &mut out);
    out.sort();
    out
}

fn collect_dds_files(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_dds_files(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("dds"))
            .unwrap_or(false)
        {
            out.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
}

pub(crate) fn parse_texture_entries(value: Option<&serde_json::Value>) -> Vec<TextureEntry> {
    let Some(arr) = value.and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            if let Some(path) = item.as_str() {
                return Some(TextureEntry {
                    source_path: path.to_owned(),
                    output_subpath: None,
                });
            }
            let obj = item.as_object()?;
            let source_path = obj
                .get("source_path")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())?
                .to_owned();
            let output_subpath = obj
                .get("output_subpath")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            Some(TextureEntry {
                source_path,
                output_subpath,
            })
        })
        .collect()
}

pub(crate) fn build_texture_work_items(
    buckets: &BTreeMap<String, Vec<String>>,
    source_dir: &Path,
    source_suffixes: &'static [(&'static str, &'static str)],
    source_game: &str,
    base_output_dir: &Path,
) -> Vec<TextureWorkItem> {
    let mut items = Vec::new();
    for (subdir, bucket) in buckets {
        let bucket_output_dir = if subdir.is_empty() {
            base_output_dir.to_path_buf()
        } else {
            base_output_dir.join(rel_to_pathbuf(subdir))
        };
        for group in group_textures(bucket, source_dir, source_suffixes, source_game) {
            items.push(TextureWorkItem {
                group,
                output_dir: bucket_output_dir.clone(),
            });
        }
    }
    items
}

pub(crate) fn output_exists_in_target(
    output_path: &Path,
    data_root: &Path,
    target_dirs: &[PathBuf],
) -> bool {
    if target_dirs.is_empty() {
        return false;
    }
    let Ok(rel) = output_path.strip_prefix(data_root) else {
        return false;
    };
    target_dirs.iter().any(|t| t.join(rel).is_file())
}

/// A texture group is "base-owned" when the target game already ships it, so
/// converting it would overwrite base-game textures. Keyed on the diffuse
/// output: if the diffuse already exists in the target, the whole set is treated
/// as base-owned and skipped — including any synthesized output (e.g. a `_g`
/// glow derived from the FO76 `_l` alpha) that the base game lacks. A coarser
/// per-group `all()` would convert the entire set whenever one synthesized
/// output was new, clobbering the colliding diffuse/normal/specular. Groups with
/// no diffuse fall back to "every output already exists in the target".
pub(crate) fn group_is_base_owned(
    outputs: &[TexturePathOutput],
    data_root: &Path,
    target_dirs: &[PathBuf],
    target_assets: Option<&crate::target_assets::TargetAssetStore>,
) -> bool {
    if target_dirs.is_empty() && target_assets.is_none() {
        return false;
    }
    let exists = |output: &Path| {
        if let Some(store) = target_assets
            && let Ok(relative) = output.strip_prefix(data_root)
            && store.has_asset(&relative.to_string_lossy())
        {
            return true;
        }
        output_exists_in_target(output, data_root, target_dirs)
    };
    let diffuse_in_target = outputs
        .iter()
        .find(|output| output.role == "diffuse")
        .is_some_and(|output| exists(&output.path));
    let all_in_target = outputs.iter().all(|output| exists(&output.path));
    diffuse_in_target || all_in_target
}

pub(crate) fn extract_f32(params: &serde_json::Value, key: &str, default: f32) -> f32 {
    params
        .get(key)
        .and_then(|v| v.as_f64())
        .map(|f| f as f32)
        .unwrap_or(default)
}

pub(crate) fn parse_conversion_workers(
    params: &serde_json::Value,
    fallback: Option<usize>,
) -> Option<usize> {
    params
        .get("conversion_workers")
        .and_then(|v| v.as_u64())
        .and_then(|workers| usize::try_from(workers).ok())
        .filter(|workers| *workers > 0)
        .or_else(|| fallback.filter(|workers| *workers > 0))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::Phase;

    #[test]
    fn detect_fo4_diffuse_role() {
        let sfx = game_texture_suffixes("fo4");
        assert_eq!(detect_role("armor_d", sfx), Some("diffuse"));
    }

    #[test]
    fn detect_fo4_normal_role() {
        let sfx = game_texture_suffixes("fo4");
        assert_eq!(detect_role("armor_n", sfx), Some("normal"));
    }

    #[test]
    fn detect_fo76_reflectivity_role() {
        let sfx = game_texture_suffixes("fo76");
        assert_eq!(detect_role("armor_r", sfx), Some("reflectivity"));
    }

    #[test]
    fn detect_fo76_lighting_role() {
        let sfx = game_texture_suffixes("fo76");
        assert_eq!(detect_role("armor_l", sfx), Some("lighting"));
    }

    #[test]
    fn detect_fo76_bare_texture_as_diffuse_for_source() {
        let sfx = game_texture_suffixes("fo76");
        assert_eq!(
            detect_role_for_source("woodcratedynamite", sfx, "fo76"),
            Some("diffuse")
        );
        assert_eq!(
            detect_role_for_source("woodcratedynamite", sfx, "fo4"),
            None
        );
    }

    #[test]
    fn parse_conversion_workers_prefers_phase_param() {
        let p = serde_json::json!({ "conversion_workers": 3 });

        assert_eq!(parse_conversion_workers(&p, Some(7)), Some(3));
    }

    #[test]
    fn parse_conversion_workers_uses_run_config_fallback() {
        let p = serde_json::json!({});

        assert_eq!(parse_conversion_workers(&p, Some(7)), Some(7));
    }

    #[test]
    fn parse_conversion_workers_ignores_zero_values() {
        let p = serde_json::json!({ "conversion_workers": 0 });

        assert_eq!(parse_conversion_workers(&p, Some(0)), None);
    }

    #[test]
    fn subsurface_longer_suffix_wins_over_specular() {
        let sfx = game_texture_suffixes("fo4");
        // "_sk" must match before "_s".
        assert_eq!(detect_role("armor_sk", sfx), Some("subsurface"));
    }

    #[test]
    fn convert_fo76_diffuse_name_same_suffix() {
        let src = game_texture_suffixes("fo76");
        let tgt = game_texture_suffixes("fo4");
        assert_eq!(convert_filename("armor_d.dds", src, tgt), "armor_d.dds");
    }

    #[test]
    fn convert_fo76_reflectivity_to_fo4_specular() {
        let src = game_texture_suffixes("fo76");
        let tgt = game_texture_suffixes("fo4");
        // reflectivity _r → specular _s via fallback.
        assert_eq!(convert_filename("armor_r.dds", src, tgt), "armor_s.dds");
    }

    #[test]
    fn convert_fo76_lighting_to_fo4_glow() {
        let src = game_texture_suffixes("fo76");
        let tgt = game_texture_suffixes("fo4");
        // lighting _l → glow _g via fallback("lighting") = "glow".
        assert_eq!(convert_filename("armor_l.dds", src, tgt), "armor_g.dds");
    }

    #[test]
    fn compression_normal_is_bc5() {
        assert_eq!(compression_for_role("normal"), "BC5_UNORM");
    }

    #[test]
    fn compression_diffuse_is_bc7() {
        assert_eq!(compression_for_role("diffuse"), "BC7_UNORM");
    }

    #[test]
    fn group_textures_produces_single_group_for_fo76_set() {
        let suffixes = game_texture_suffixes("fo76");
        let groups = group_textures(
            &[
                "Textures/armor_d.dds".to_owned(),
                "Textures/armor_r.dds".to_owned(),
                "Textures/armor_l.dds".to_owned(),
            ],
            Path::new("/nonexistent"),
            suffixes,
            "fo76",
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 3);
    }

    #[test]
    fn group_textures_adds_existing_fo76_glow_sibling_for_lighting() {
        let tmp = std::env::temp_dir().join("group_textures_fo76_implicit_glow");
        let textures_dir = tmp.join("Textures");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&textures_dir).unwrap();
        for name in ["armor_d.dds", "armor_r.dds", "armor_l.dds", "armor_g.dds"] {
            std::fs::write(textures_dir.join(name), b"dds").unwrap();
        }

        let suffixes = game_texture_suffixes("fo76");
        let groups = group_textures(
            &[
                textures_dir
                    .join("armor_d.dds")
                    .to_string_lossy()
                    .to_string(),
                textures_dir
                    .join("armor_r.dds")
                    .to_string_lossy()
                    .to_string(),
                textures_dir
                    .join("armor_l.dds")
                    .to_string_lossy()
                    .to_string(),
            ],
            &tmp,
            suffixes,
            "fo76",
        );

        assert_eq!(groups.len(), 1);
        let roles: HashSet<&str> = groups[0]
            .files
            .iter()
            .map(|(_, role)| role.as_str())
            .collect();
        assert_eq!(
            roles,
            HashSet::from(["diffuse", "reflectivity", "lighting", "glow"])
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn group_textures_adds_existing_fo76_bundle_siblings_for_reflectivity() {
        let tmp = std::env::temp_dir().join("group_textures_fo76_implicit_bundle");
        let textures_dir = tmp.join("Textures");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&textures_dir).unwrap();
        for name in ["rock_d.dds", "rock_r.dds", "rock_l.dds"] {
            std::fs::write(textures_dir.join(name), b"dds").unwrap();
        }

        let suffixes = game_texture_suffixes("fo76");
        let groups = group_textures(
            &[textures_dir
                .join("rock_r.dds")
                .to_string_lossy()
                .to_string()],
            &tmp,
            suffixes,
            "fo76",
        );

        assert_eq!(groups.len(), 1);
        let roles: HashSet<&str> = groups[0]
            .files
            .iter()
            .map(|(_, role)| role.as_str())
            .collect();
        assert_eq!(
            roles,
            HashSet::from(["diffuse", "reflectivity", "lighting"])
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn group_textures_adds_bare_fo76_diffuse_sibling_for_reflectivity() {
        let tmp = std::env::temp_dir().join("group_textures_fo76_bare_diffuse_bundle");
        let textures_dir = tmp.join("Textures");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&textures_dir).unwrap();
        for name in ["woodcratedynamite.dds", "woodcratedynamite_r.dds"] {
            std::fs::write(textures_dir.join(name), b"dds").unwrap();
        }

        let suffixes = game_texture_suffixes("fo76");
        let groups = group_textures(
            &[textures_dir
                .join("woodcratedynamite_r.dds")
                .to_string_lossy()
                .to_string()],
            &tmp,
            suffixes,
            "fo76",
        );

        assert_eq!(groups.len(), 1);
        assert!(groups[0].files.iter().any(|(path, role)| {
            role == "diffuse"
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case("woodcratedynamite.dds"))
        }));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn group_textures_prefers_fo76_d_suffix_over_bare_diffuse_sibling() {
        let tmp = std::env::temp_dir().join("group_textures_fo76_diffuse_preference");
        let textures_dir = tmp.join("Textures");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&textures_dir).unwrap();
        for name in ["crate.dds", "crate_d.dds", "crate_r.dds"] {
            std::fs::write(textures_dir.join(name), b"dds").unwrap();
        }

        let suffixes = game_texture_suffixes("fo76");
        let groups = group_textures(
            &[textures_dir
                .join("crate_r.dds")
                .to_string_lossy()
                .to_string()],
            &tmp,
            suffixes,
            "fo76",
        );

        let diffuse_names: Vec<String> = groups[0]
            .files
            .iter()
            .filter(|(_, role)| role == "diffuse")
            .filter_map(|(path, _)| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
            })
            .collect();
        assert_eq!(diffuse_names, vec!["crate_d.dds".to_owned()]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn group_textures_aliases_fo76_eyebro_lashes_to_eyebrown_bundle() {
        let tmp = std::env::temp_dir().join("group_textures_fo76_eyebro_lashes");
        let textures_dir = tmp
            .join("Textures")
            .join("Actors")
            .join("Character")
            .join("Eyes");
        let output = tmp.join("out");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&textures_dir).unwrap();
        for name in [
            "eyebro_l.dds",
            "eyebro_n.dds",
            "eyebro_r.dds",
            "eyebrown_d.dds",
        ] {
            std::fs::write(textures_dir.join(name), b"dds").unwrap();
        }

        let source_suffixes = game_texture_suffixes("fo76");
        let target_suffixes = game_texture_suffixes("fo4");
        let groups = group_textures(
            &[
                textures_dir
                    .join("eyebro_l.dds")
                    .to_string_lossy()
                    .to_string(),
                textures_dir
                    .join("eyebro_n.dds")
                    .to_string_lossy()
                    .to_string(),
                textures_dir
                    .join("eyebro_r.dds")
                    .to_string_lossy()
                    .to_string(),
                textures_dir
                    .join("eyebrown_d.dds")
                    .to_string_lossy()
                    .to_string(),
            ],
            &tmp,
            source_suffixes,
            "fo76",
        );

        assert_eq!(groups.len(), 1);
        let roles: HashSet<&str> = groups[0]
            .files
            .iter()
            .map(|(_, role)| role.as_str())
            .collect();
        assert_eq!(
            roles,
            HashSet::from(["diffuse", "normal", "reflectivity", "lighting"])
        );

        let request = build_request(
            &groups[0],
            &output,
            "fo76",
            "fo4",
            source_suffixes,
            target_suffixes,
            &HashMap::new(),
            TextureConversionParamsPayload::default(),
            false,
            0,
        )
        .expect("aliased lash bundle should build a conversion request");

        let output_names: HashSet<String> = request
            .outputs
            .iter()
            .filter_map(|output| output.path.file_name().and_then(|name| name.to_str()))
            .map(|name| name.to_ascii_lowercase())
            .collect();
        assert!(output_names.contains("eyebrown_d.dds"));
        assert!(output_names.contains("eyebro_n.dds"));
        assert!(output_names.contains("eyebro_s.dds"));
        assert!(output_names.contains("eyebro_g.dds"));
        assert!(!output_names.contains("eyebrown_s.dds"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn group_textures_splits_fo76_lash_bundle_from_bare_eye_diffuse() {
        // Live repro: convert_all enumerates BOTH the bare eyebrown.dds iris
        // diffuse (FO4-colliding output → base-owned) and the eyebrown_d.dds
        // lash diffuse. Grouped together, the bare diffuse marked the whole
        // group base-owned and the lash texture never shipped (empty
        // Textures/Actors/Character/eyes/ in the regen output).
        let tmp = std::env::temp_dir().join("group_textures_fo76_lash_split");
        let textures_dir = tmp
            .join("Textures")
            .join("Actors")
            .join("Character")
            .join("Eyes");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&textures_dir).unwrap();
        for name in [
            "eyebrown.dds",
            "eyebrown_d.dds",
            "eyebro_n.dds",
            "eyebro_r.dds",
            "eyebro_l.dds",
        ] {
            std::fs::write(textures_dir.join(name), b"dds").unwrap();
        }

        let source_suffixes = game_texture_suffixes("fo76");
        let paths: Vec<String> = [
            "eyebrown.dds",
            "eyebrown_d.dds",
            "eyebro_n.dds",
            "eyebro_r.dds",
            "eyebro_l.dds",
        ]
        .iter()
        .map(|name| textures_dir.join(name).to_string_lossy().to_string())
        .collect();
        let groups = group_textures(&paths, &tmp, source_suffixes, "fo76");

        assert_eq!(groups.len(), 2);
        let lash_group = groups
            .iter()
            .find(|group| {
                group.files.iter().any(|(path, _)| {
                    path.file_name().and_then(|n| n.to_str()) == Some("eyebrown_d.dds")
                })
            })
            .expect("lash bundle group");
        let lash_names: HashSet<String> = lash_group
            .files
            .iter()
            .filter_map(|(path, _)| path.file_name().and_then(|n| n.to_str()))
            .map(|name| name.to_ascii_lowercase())
            .collect();
        assert!(lash_names.contains("eyebro_n.dds"));
        assert!(!lash_names.contains("eyebrown.dds"));

        let bare_group = groups
            .iter()
            .find(|group| {
                group.files.iter().any(|(path, _)| {
                    path.file_name().and_then(|n| n.to_str()) == Some("eyebrown.dds")
                })
            })
            .expect("bare iris group");
        assert!(!bare_group.files.iter().any(|(path, _)| {
            path.file_name().and_then(|n| n.to_str()) == Some("eyebrown_d.dds")
        }));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn build_request_converts_fo76_bare_diffuse_texture() {
        let tmp = std::env::temp_dir().join("build_request_fo76_bare_diffuse");
        let textures_dir = tmp
            .join("Textures")
            .join("SetDressing")
            .join("WoodCrateDynamite");
        let output = tmp.join("out");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&textures_dir).unwrap();
        std::fs::write(textures_dir.join("woodcratedynamite.dds"), b"dds").unwrap();

        let source_suffixes = game_texture_suffixes("fo76");
        let target_suffixes = game_texture_suffixes("fo4");
        let groups = group_textures(
            &[textures_dir
                .join("woodcratedynamite.dds")
                .to_string_lossy()
                .to_string()],
            &tmp,
            source_suffixes,
            "fo76",
        );
        let request = build_request(
            &groups[0],
            &output,
            "fo76",
            "fo4",
            source_suffixes,
            target_suffixes,
            &HashMap::new(),
            TextureConversionParamsPayload::default(),
            false,
            0,
        )
        .expect("bare diffuse should build a conversion request");

        assert_eq!(request.inputs[0].role, "diffuse");
        assert_eq!(request.outputs[0].role, "diffuse");
        assert!(request.outputs[0].path.ends_with("woodcratedynamite.dds"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn build_request_converts_fo76_reflectivity_lighting_without_diffuse_to_specular() {
        let tmp = std::env::temp_dir().join("build_request_fo76_rl_no_diffuse");
        let textures_dir = tmp.join("Textures").join("Effects");
        let output = tmp.join("out");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&textures_dir).unwrap();
        std::fs::write(textures_dir.join("1_generic_lighting_dark_r.dds"), b"dds").unwrap();
        std::fs::write(textures_dir.join("1_generic_lighting_dark_l.dds"), b"dds").unwrap();

        let source_suffixes = game_texture_suffixes("fo76");
        let target_suffixes = game_texture_suffixes("fo4");
        let groups = group_textures(
            &[
                textures_dir
                    .join("1_generic_lighting_dark_r.dds")
                    .to_string_lossy()
                    .to_string(),
                textures_dir
                    .join("1_generic_lighting_dark_l.dds")
                    .to_string_lossy()
                    .to_string(),
            ],
            &tmp,
            source_suffixes,
            "fo76",
        );
        let request = build_request(
            &groups[0],
            &output,
            "fo76",
            "fo4",
            source_suffixes,
            target_suffixes,
            &HashMap::new(),
            TextureConversionParamsPayload::default(),
            false,
            0,
        )
        .expect("_r/_l pair without _d should build a conversion request");

        let specular_outputs: Vec<&TexturePathOutput> = request
            .outputs
            .iter()
            .filter(|output| output.role == "specular")
            .collect();
        assert_eq!(specular_outputs.len(), 1);
        assert!(
            specular_outputs[0]
                .path
                .ends_with("1_generic_lighting_dark_s.dds")
        );
        assert!(
            !request
                .outputs
                .iter()
                .any(|output| output.role == "specular"
                    && output.path.ends_with("1_generic_lighting_dark_g.dds"))
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn build_request_caps_rockcliff76_gloss_multiplier() {
        let tmp = std::env::temp_dir().join("build_request_rockcliff76_gloss");
        let textures_dir = tmp.join("Textures").join("Landscape").join("Rocks");
        let output = tmp.join("out");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&textures_dir).unwrap();
        std::fs::write(textures_dir.join("RockCliff76_d.dds"), b"dds").unwrap();
        std::fs::write(textures_dir.join("RockCliff76_r.dds"), b"dds").unwrap();
        std::fs::write(textures_dir.join("RockCliff76_l.dds"), b"dds").unwrap();

        let source_suffixes = game_texture_suffixes("fo76");
        let target_suffixes = game_texture_suffixes("fo4");
        let groups = group_textures(
            &[textures_dir
                .join("RockCliff76_d.dds")
                .to_string_lossy()
                .to_string()],
            &tmp,
            source_suffixes,
            "fo76",
        );
        let request = build_request(
            &groups[0],
            &output,
            "fo76",
            "fo4",
            source_suffixes,
            target_suffixes,
            &HashMap::new(),
            TextureConversionParamsPayload::default(),
            false,
            0,
        )
        .expect("RockCliff76 bundle should build a conversion request");

        assert!(
            (request.params.gloss_multiplier - ROCKCLIFF76_GLOSS_MULTIPLIER).abs() < f32::EPSILON
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn group_textures_separates_different_base_names() {
        let suffixes = game_texture_suffixes("fo4");
        let groups = group_textures(
            &[
                "Textures/weapon_d.dds".to_owned(),
                "Textures/armor_d.dds".to_owned(),
            ],
            Path::new("/nonexistent"),
            suffixes,
            "fo4",
        );
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn build_texture_work_items_keeps_same_base_in_distinct_subdirs_separate() {
        let mut buckets: BTreeMap<String, Vec<String>> = BTreeMap::new();
        buckets.insert(
            "Architecture".to_owned(),
            vec!["Textures/Architecture/foo_d.dds".to_owned()],
        );
        buckets.insert(
            "Clutter".to_owned(),
            vec!["Textures/Clutter/foo_d.dds".to_owned()],
        );
        let suffixes = game_texture_suffixes("fo4");
        let base = Path::new("/mod/data/Textures");
        let items = build_texture_work_items(&buckets, Path::new(""), suffixes, "fo4", base);
        assert_eq!(items.len(), 2);
        let dirs: HashSet<PathBuf> = items.iter().map(|i| i.output_dir.clone()).collect();
        assert!(dirs.contains(&base.join("Architecture")));
        assert!(dirs.contains(&base.join("Clutter")));
    }

    #[test]
    fn texture_output_subdir_preserves_asset_tree_without_game_prefix() {
        assert_eq!(
            texture_output_subdir(
                "X:/extracted/fo76/Textures/fo76/Landscape/Rocks/foo_d.dds",
                Path::new("X:/extracted/fo76"),
            ),
            "Landscape/Rocks"
        );
        assert_eq!(
            texture_output_subdir("Textures/fo76/Landscape/Rocks/foo_d.dds", Path::new("")),
            "Landscape/Rocks"
        );
        assert_eq!(
            texture_output_subdir("Landscape/Rocks/foo_d.dds", Path::new("")),
            "Landscape/Rocks"
        );
    }

    #[test]
    fn convert_textures_skip_existing_counts_existing_output() {
        use crate::phase::{PhaseCtx, PhaseReport};
        use crate::run::{
            ConversionRun, RunConfig, RunError, RunParams, create_run, drop_run, with_run,
        };
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let tmp =
            std::env::temp_dir().join("convert_textures_skip_existing_counts_existing_output");
        let source = tmp.join("source");
        let output = tmp.join("mod");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(source.join("Textures")).unwrap();
        std::fs::create_dir_all(output.join("data").join("Textures")).unwrap();
        std::fs::write(source.join("Textures").join("foo_d.dds"), b"not a dds").unwrap();
        std::fs::write(
            output.join("data").join("Textures").join("foo_d.dds"),
            b"existing",
        )
        .unwrap();

        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "textures": [source.join("Textures").join("foo_d.dds").to_string_lossy()],
                "source_extracted": source.to_string_lossy(),
                "skip_existing": true
            });
            let mut ctx = PhaseCtx {
                run,
                mod_path: &output,
                source_extracted_dir: &source,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            crate::phase::textures_v2::ConvertTexturesV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 1);
        assert_eq!(report.warnings, 0);
        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn texture_phase_converts_relocation_member_absent_from_params() {
        use crate::phase::{PhaseCtx, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        fn any_dds_under(dir: &std::path::Path) -> bool {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return false;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if any_dds_under(&p) {
                        return true;
                    }
                } else if p
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("dds"))
                    .unwrap_or(false)
                {
                    return true;
                }
            }
            false
        }

        let tmp = std::env::temp_dir().join("texture_phase_relocation_member_absent");
        let source = tmp.join("source");
        let output = tmp.join("mod");
        let tex_dir = source.join("Textures").join("Landscape").join("Rocks");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tex_dir).unwrap();
        directxtex_native::write_dds_rgba_image(
            &tex_dir.join("rock_d.dds"),
            8,
            8,
            &vec![128u8; 8 * 8 * 4],
            "R8G8B8A8_UNORM",
            false,
        )
        .unwrap();

        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                base_asset_namespace: "FO76".into(),
                ..Default::default()
            },
        })
        .unwrap();

        // Inject a texture member directly (bypassing the compare) to isolate the phase.
        with_run(id, |run| -> Result<(), RunError> {
            run.relocation_members
                .insert("textures/landscape/rocks/rock_d.dds".to_string());
            Ok(())
        })
        .unwrap();

        let _report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            // params.textures intentionally EMPTY — the member must still convert.
            let params = serde_json::json!({
                "textures": [],
                "source_extracted": source.to_string_lossy(),
                "skip_existing": false,
                "use_gpu": false
            });
            let mut ctx = PhaseCtx {
                run,
                mod_path: &output,
                source_extracted_dir: &source,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            crate::phase::textures_v2::ConvertTexturesV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        let fo76_dir = output.join("data").join("Textures").join("FO76");
        assert!(
            any_dds_under(&fo76_dir),
            "expected a relocated texture under {}",
            fo76_dir.display()
        );

        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn scoped_convert_textures_skips_target_owned_output() {
        use crate::phase::{PhaseCtx, PhaseEvent, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let tmp = std::env::temp_dir().join("scoped_convert_textures_skips_target_owned_output");
        let source = tmp.join("source");
        let output = tmp.join("mod");
        let target = tmp.join("target");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(source.join("Textures")).unwrap();
        std::fs::create_dir_all(target.join("Textures")).unwrap();
        std::fs::write(source.join("Textures").join("foo_d.dds"), b"not a dds").unwrap();
        std::fs::write(target.join("Textures").join("foo_d.dds"), b"target").unwrap();

        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            let params = serde_json::json!({
                "textures": [source.join("Textures").join("foo_d.dds").to_string_lossy()],
                "source_extracted": source.to_string_lossy(),
                "skip_existing": false
            });
            let mut ctx = PhaseCtx {
                run,
                mod_path: &output,
                source_extracted_dir: &source,
                target_extracted_dir: Some(&target),
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            crate::phase::textures_v2::ConvertTexturesV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.records_dropped, 1);
        assert_eq!(report.warnings, 0);

        let v2_summary_seen = with_run(id, |run| -> Result<bool, RunError> {
            let mut seen = false;
            while let Ok(ev) = run.event_rx.try_recv() {
                if let PhaseEvent::Log { message, .. } = ev {
                    if message.contains("base_owned_groups=1") {
                        seen = true;
                    }
                }
            }
            Ok(seen)
        })
        .unwrap();
        assert!(v2_summary_seen);
        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn scoped_namespaced_fo76_reflectivity_uses_bundle_conversion() {
        use crate::phase::{PhaseCtx, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let tmp =
            std::env::temp_dir().join("scoped_namespaced_fo76_reflectivity_bundle_conversion");
        let source = tmp.join("source");
        let output = tmp.join("mod");
        let textures_dir = source.join("Textures").join("Landscape").join("Rocks");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&textures_dir).unwrap();

        directxtex_native::write_dds_rgba_image(
            &textures_dir.join("rock_d.dds"),
            8,
            8,
            &vec![128u8; 8 * 8 * 4],
            "R8G8B8A8_UNORM",
            false,
        )
        .unwrap();
        directxtex_native::write_dds_rgba_image(
            &textures_dir.join("rock_r.dds"),
            4,
            4,
            &vec![0u8; 4 * 4 * 4],
            "BC4_UNORM",
            false,
        )
        .unwrap();
        directxtex_native::write_dds_rgba_image(
            &textures_dir.join("rock_l.dds"),
            8,
            8,
            &vec![128u8; 8 * 8 * 4],
            "R8G8B8A8_UNORM",
            false,
        )
        .unwrap();

        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            let params = serde_json::json!({
                "textures": [{
                    "source_path": textures_dir.join("rock_r.dds").to_string_lossy(),
                    "output_subpath": "Textures/FO76/Landscape/Rocks/rock_r.dds"
                }],
                "source_extracted": source.to_string_lossy(),
                "skip_existing": false,
                "use_gpu": false
            });
            let mut ctx = PhaseCtx {
                run,
                mod_path: &output,
                source_extracted_dir: &source,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            crate::phase::textures_v2::ConvertTexturesV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.warnings, 0);
        assert_eq!(report.assets_written, 3);
        let spec = output
            .join("data")
            .join("Textures")
            .join("FO76")
            .join("Landscape")
            .join("Rocks")
            .join("rock_s.dds");
        let image = directxtex_native::read_dds_float_rgba_image(&spec).unwrap();
        assert_eq!(image.width, 8);
        assert_eq!(image.height, 8);
        assert_eq!(image.dxgi_format, 83);

        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn enumerate_source_textures_walks_dds_recursively() {
        let tmp = std::env::temp_dir().join("enumerate_source_textures_walks_dds");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("Textures").join("Sub")).unwrap();
        fs::write(tmp.join("Textures").join("a_d.dds"), b"x").unwrap();
        fs::write(tmp.join("Textures").join("Sub").join("b_n.dds"), b"x").unwrap();
        fs::write(tmp.join("Textures").join("Sub").join("note.txt"), b"x").unwrap();

        let found = enumerate_source_textures(&tmp);

        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|p| p.ends_with("a_d.dds")));
        assert!(found.iter().any(|p| p.ends_with("Sub/b_n.dds")));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn output_exists_in_target_checks_relative_path() {
        let tmp = std::env::temp_dir().join("output_exists_in_target_checks_relative_path");
        let _ = fs::remove_dir_all(&tmp);
        let data_root = tmp.join("mod").join("data");
        let target = tmp.join("target");
        fs::create_dir_all(target.join("Textures").join("Arch")).unwrap();
        fs::write(target.join("Textures").join("Arch").join("foo_d.dds"), b"x").unwrap();

        let present = data_root.join("Textures").join("Arch").join("foo_d.dds");
        let absent = data_root.join("Textures").join("Arch").join("bar_d.dds");
        let targets = vec![target.clone()];

        assert!(output_exists_in_target(&present, &data_root, &targets));
        assert!(!output_exists_in_target(&absent, &data_root, &targets));
        assert!(!output_exists_in_target(&present, &data_root, &[]));
        let _ = fs::remove_dir_all(&tmp);
    }

    fn texture_output(role: &str, path: PathBuf) -> TexturePathOutput {
        TexturePathOutput {
            role: role.to_owned(),
            path,
            format: "BC7_UNORM".to_owned(),
        }
    }

    #[test]
    fn group_is_base_owned_when_diffuse_in_target_even_if_glow_is_new() {
        // Regression: FO76 BaseMaleBody collides with FO4 base _d/_n/_s but its
        // _l alpha synthesizes a _g glow the base game lacks. The group must
        // still be skipped (base-owned) so the diffuse never overwrites base.
        let tmp = std::env::temp_dir().join("group_is_base_owned_diffuse_keyed");
        let _ = fs::remove_dir_all(&tmp);
        let data_root = tmp.join("mod").join("data");
        let target = tmp.join("target");
        fs::create_dir_all(target.join("Textures").join("Actors")).unwrap();
        // FO4 base ships the diffuse but NOT the synthesized glow.
        fs::write(
            target.join("Textures").join("Actors").join("body_d.dds"),
            b"x",
        )
        .unwrap();

        let outputs = vec![
            texture_output(
                "diffuse",
                data_root.join("Textures").join("Actors").join("body_d.dds"),
            ),
            texture_output(
                "glow",
                data_root.join("Textures").join("Actors").join("body_g.dds"),
            ),
        ];
        let targets = vec![target.clone()];

        assert!(group_is_base_owned(&outputs, &data_root, &targets, None));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn group_is_base_owned_false_when_diffuse_is_fo76_unique() {
        // The diffuse is FO76-unique (absent from base); only an unrelated glow
        // path collides. The set must convert, not be skipped.
        let tmp = std::env::temp_dir().join("group_is_base_owned_new_diffuse");
        let _ = fs::remove_dir_all(&tmp);
        let data_root = tmp.join("mod").join("data");
        let target = tmp.join("target");
        fs::create_dir_all(target.join("Textures")).unwrap();
        fs::write(target.join("Textures").join("body_g.dds"), b"x").unwrap();

        let outputs = vec![
            texture_output("diffuse", data_root.join("Textures").join("body_d.dds")),
            texture_output("glow", data_root.join("Textures").join("body_g.dds")),
        ];
        let targets = vec![target.clone()];

        assert!(!group_is_base_owned(&outputs, &data_root, &targets, None));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn group_is_base_owned_false_without_target_dirs() {
        let data_root = Path::new("/mod/data");
        let outputs = vec![texture_output(
            "diffuse",
            PathBuf::from("/mod/data/Textures/body_d.dds"),
        )];
        assert!(!group_is_base_owned(&outputs, data_root, &[], None));
    }

    #[test]
    fn params_json_extracts_correctly() {
        let params = serde_json::json!({
            "textures": ["Textures/foo_d.dds"],
            "source_extracted": "/some/path",
            "ao_multiplier": 0.3,
            "specular_multiplier": 1.2
        });
        let textures: Vec<String> = params
            .get("textures")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(textures, vec!["Textures/foo_d.dds"]);

        let ao = extract_f32(&params, "ao_multiplier", 0.5);
        assert!((ao - 0.3).abs() < 1e-5);

        let spec = extract_f32(&params, "specular_multiplier", 1.0);
        assert!((spec - 1.2).abs() < 1e-5);

        let gloss = extract_f32(&params, "gloss_multiplier", 1.0);
        assert!((gloss - 1.0).abs() < 1e-5);
    }

    #[test]
    fn convert_all_emits_progress_events() {
        use crate::phase::{PhaseCtx, PhaseEvent, PhaseReport};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let tmp = std::env::temp_dir().join("convert_all_emits_progress_events");
        let source = tmp.join("source");
        let output = tmp.join("mod");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(source.join("Textures")).unwrap();
        std::fs::create_dir_all(output.join("data").join("Textures")).unwrap();
        // Two distinct base names -> two groups so progress can step.
        for name in ["a_d.dds", "b_d.dds"] {
            // minimal real DDS so the group builds a request; reuse the directxtex writer.
            directxtex_native::write_dds_rgba_image(
                &source.join("Textures").join(name),
                4,
                4,
                &vec![255u8; 4 * 4 * 4],
                "R8G8B8A8_UNORM",
                false,
            )
            .unwrap();
        }

        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let _report: PhaseReport = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            let params = serde_json::json!({
                "convert_all": true,
                "source_extracted": source.to_string_lossy(),
                "skip_existing": false
            });
            let mut ctx = PhaseCtx {
                run,
                mod_path: &output,
                source_extracted_dir: &source,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            crate::phase::textures_v2::ConvertTexturesV2Phase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        let progress_seen = with_run(id, |run| -> Result<bool, RunError> {
            let mut seen = false;
            while let Ok(ev) = run.event_rx.try_recv() {
                if matches!(
                    ev,
                    PhaseEvent::Progress {
                        phase: "convert_textures_v2",
                        ..
                    }
                ) {
                    seen = true;
                }
            }
            Ok(seen)
        })
        .unwrap();
        assert!(
            progress_seen,
            "texture phase must emit at least one Progress event"
        );

        drop_run(id).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
