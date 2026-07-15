//! Shared FO76 LOD-by-convention asset-closure builder.
//!
//! Single source for "base MODL → existing `_lod[_N].nif` + its material/texture
//! closure", used by BOTH the `walk` phase (graph/bounded flows) and the regen
//! whole-plugin asset collection (`conversion_run_collect_lod_closures`, consumed by
//! `unified.py::_collect_assets_native`). The path rule itself lives in
//! `lod_paths`; this module adds the existence check + nif/material/texture
//! expansion so the asset phases convert+ship the LOD closure.
//!
//! Consistency with `synthesize_object_lod`: both gate on the SAME `lod_paths`
//! derivation + the SAME FO76-source existence check over the SAME LOD-capable
//! base records, so every synthesized `MNAM` slot has its mesh shipped (the
//! `MNAM` string and the shipped `Meshes\…\_lod.nif` come from one `LodCandidate`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use esp_authoring_core::plugin_runtime::{
    ParsedItem, ensure_core_section, plugin_handle_store_ref, record_index_entry_by_form_key,
};
use materials_native::{bgem, bgsm};
use nif_core_native::model::NifFile;

use crate::phase::lod_paths::derive_lod_candidates;
use crate::translator::Game;

/// Base record signatures whose `_lod.nif` meshes are useful to the LOD asset
/// closure. This is intentionally broader than [`FO4_DIRECT_MNAM_BASE_SIGS`]:
/// finding and shipping a source LOD mesh does not prove that FO4 permits the
/// 1040-byte Distant-LOD `MNAM` subrecord on that base-record schema.
pub const LOD_BASE_SIGS: [&str; 6] = ["STAT", "SCOL", "MSTT", "TREE", "FLOR", "ACTI"];

/// FO4 base-record signatures with proven support for the 4 x 260-byte object
/// LOD `MNAM` layout. Other LOD-capable source bases need a real STAT proxy;
/// until one exists they must not receive a direct `MNAM`.
pub const FO4_DIRECT_MNAM_BASE_SIGS: [&str; 1] = ["STAT"];

const MNAM_SLOT: usize = 260;

/// One discovered LOD-closure asset. `kind` ∈ {"nif","material","texture"};
/// `source_path` is the Data-relative path (`Meshes\…` / `Materials\…` / `Textures\…`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LodAsset {
    pub kind: String,
    pub source_path: String,
}

/// Dedup key shared across collectors so the walk + regen paths agree.
pub fn dedup_key(kind: &str, source_path: &str) -> String {
    format!(
        "{kind}|{}",
        source_path.to_ascii_lowercase().replace('\\', "/")
    )
}

/// Absolute path of a `meshes`-relative (lowercase, `/`-sep) LOD source path.
pub fn lod_abs_path(source_dir: &Path, source_rel: &str) -> PathBuf {
    let mut p = source_dir.join("meshes");
    for c in source_rel.split('/') {
        if !c.is_empty() {
            p.push(c);
        }
    }
    p
}

/// Append the existing `_lod[_N].nif` closures (nif + material + texture) for a
/// single base MODL. Existence-gated under `source_dir`, deduped via `seen`.
pub fn append_lod_closures_for_modl(
    modl: &str,
    source_dir: &Path,
    seen: &mut HashSet<String>,
    out: &mut Vec<LodAsset>,
) {
    append_lod_closures_for_modl_for_game(modl, source_dir, Game::Fo76, seen, out);
}

pub fn append_lod_closures_for_modl_for_game(
    modl: &str,
    source_dir: &Path,
    source_game: Game,
    seen: &mut HashSet<String>,
    out: &mut Vec<LodAsset>,
) {
    if should_skip_object_lod_model(modl) {
        return;
    }
    for cand in derive_lod_candidates(source_game, modl) {
        let abs = lod_abs_path(source_dir, &cand.source_rel);
        if !abs.is_file() {
            continue;
        }
        let nif_src = format!("Meshes\\{}", cand.source_rel.replace('/', "\\"));
        if seen.insert(dedup_key("nif", &nif_src)) {
            out.push(LodAsset {
                kind: "nif".to_string(),
                source_path: nif_src,
            });
        }
        expand_lod_closure(&abs, source_dir, seen, out);
    }
}

pub fn should_skip_object_lod_model(modl: &str) -> bool {
    let model = normalize_model_path(modl);
    if model.starts_with("sky/") {
        return true;
    }
    if !model.starts_with("effects/") {
        return false;
    }
    let file = model.rsplit('/').next().unwrap_or(model.as_str());
    ["cloud", "fog", "smoke", "storm", "sandstorm", "weather"]
        .iter()
        .any(|needle| file.contains(needle))
}

fn normalize_model_path(modl: &str) -> String {
    let mut model = modl
        .trim()
        .trim_end_matches('\0')
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('\\', "/")
        .to_ascii_lowercase();
    if let Some(stripped) = model.strip_prefix("meshes/") {
        model = stripped.to_string();
    }
    model
}

/// Append explicit FO4/FO76 DistantLOD (MNAM) slots from a source base record.
/// These override the convention path for FO76 trees whose LOD filenames are
/// abbreviated differently from MODL.
fn append_lod_closures_for_mnam(
    mnam: &[u8],
    source_dir: &Path,
    seen: &mut HashSet<String>,
    out: &mut Vec<LodAsset>,
) {
    let mut rels = Vec::new();
    for level in 0..4 {
        let off = level * MNAM_SLOT;
        if off >= mnam.len() {
            break;
        }
        let end = (off + MNAM_SLOT).min(mnam.len());
        let rel = normalize_mnam_slot_path(&mnam[off..end]);
        if !rel.is_empty() {
            rels.push(rel);
        }
    }
    rels.extend(scan_mnam_paths(mnam));
    let mut unique_rels = HashSet::new();
    for rel in rels {
        if !unique_rels.insert(rel.to_ascii_lowercase()) {
            continue;
        }
        let source_path = format!("Meshes\\{}", rel.to_ascii_lowercase());
        let abs = abs_for_source_path(source_dir, &source_path);
        let abs_path = Path::new(&abs);
        if !abs_path.is_file() {
            continue;
        }
        if seen.insert(dedup_key("nif", &source_path)) {
            out.push(LodAsset {
                kind: "nif".to_string(),
                source_path,
            });
        }
        expand_lod_closure(abs_path, source_dir, seen, out);
    }
}

fn scan_mnam_paths(data: &[u8]) -> Vec<String> {
    let mut paths = Vec::new();
    for start in 0..data.len() {
        if !is_lod_path_start(data, start) {
            continue;
        }
        let Some(end) = find_nif_path_end(data, start) else {
            continue;
        };
        paths.push(String::from_utf8_lossy(&data[start..end]).replace('/', "\\"));
    }
    paths
}

fn is_lod_path_start(data: &[u8], start: usize) -> bool {
    [
        b"LOD\\".as_slice(),
        b"LOD/".as_slice(),
        b"DLC01\\LOD\\".as_slice(),
        b"DLC01/LOD/".as_slice(),
        b"DLC02\\LOD\\".as_slice(),
        b"DLC02/LOD/".as_slice(),
        b"DLC03\\LOD\\".as_slice(),
        b"DLC03/LOD/".as_slice(),
        b"DLC04\\LOD\\".as_slice(),
        b"DLC04/LOD/".as_slice(),
        b"BYOH\\LOD\\".as_slice(),
        b"BYOH/LOD/".as_slice(),
        b"_BYOH\\LOD\\".as_slice(),
        b"_BYOH/LOD/".as_slice(),
    ]
    .iter()
    .any(|prefix| starts_with_ascii_ci(data, start, prefix))
}

fn starts_with_ascii_ci(data: &[u8], start: usize, needle: &[u8]) -> bool {
    data.get(start..start + needle.len())
        .is_some_and(|haystack| haystack.eq_ignore_ascii_case(needle))
}

fn find_nif_path_end(data: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    while index + 4 <= data.len() {
        if data[index..index + 4].eq_ignore_ascii_case(b".nif") {
            return Some(index + 4);
        }
        let byte = data[index];
        if !(byte == b'\\'
            || byte == b'/'
            || byte == b'_'
            || byte == b'-'
            || byte == b'.'
            || byte.is_ascii_alphanumeric())
        {
            return None;
        }
        index += 1;
    }
    None
}

fn normalize_mnam_slot_path(data: &[u8]) -> String {
    let mut path = read_zstring(data).trim().replace('/', "\\");
    if path.len() >= 7 && path[..7].eq_ignore_ascii_case("meshes\\") {
        path = path[7..].to_string();
    }
    path
}

/// Load the LOD nif and append its referenced materials + textures, plus each
/// material's textures (BGSM/BGEM). Best-effort: failures are skipped (the nif
/// asset itself is still queued).
fn expand_lod_closure(
    nif_abs: &Path,
    source_dir: &Path,
    seen: &mut HashSet<String>,
    out: &mut Vec<LodAsset>,
) {
    let Ok(nif) = NifFile::load(nif_abs) else {
        return;
    };
    let refs = nif.referenced_asset_paths();
    for mat in &refs.materials {
        let mat_q = qualify("material", mat);
        if seen.insert(dedup_key("material", &mat_q)) {
            out.push(LodAsset {
                kind: "material".to_string(),
                source_path: mat_q,
            });
        }
        for tex in material_textures(source_dir, mat) {
            let tex_q = qualify("texture", &tex);
            if seen.insert(dedup_key("texture", &tex_q)) {
                out.push(LodAsset {
                    kind: "texture".to_string(),
                    source_path: tex_q,
                });
            }
        }
    }
    for tex in &refs.textures {
        let tex_q = qualify("texture", tex);
        if seen.insert(dedup_key("texture", &tex_q)) {
            out.push(LodAsset {
                kind: "texture".to_string(),
                source_path: tex_q,
            });
        }
    }
}

/// Ensure a material/texture path carries its Data root (`Materials\`/`Textures\`).
fn qualify(kind: &str, path: &str) -> String {
    let root = if kind == "material" {
        "Materials"
    } else {
        "Textures"
    };
    let norm = path
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\");
    let prefix = format!("{}\\", root.to_ascii_lowercase());
    if norm.to_ascii_lowercase().starts_with(&prefix) {
        norm
    } else {
        format!("{root}\\{norm}")
    }
}

fn push_tex(out: &mut Vec<String>, s: &str) {
    let t = s.trim().trim_end_matches('\0').trim();
    if !t.is_empty() {
        out.push(t.to_string());
    }
}

/// Read a material's textures (BGSM/BGEM) from the FO76 source dir. Empty on any
/// failure (the material asset itself is still queued for conversion).
fn material_textures(source_dir: &Path, mat: &str) -> Vec<String> {
    let rel = mat
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('\\', "/");
    let rel = if rel.to_ascii_lowercase().starts_with("materials/") {
        rel
    } else {
        format!("materials/{rel}")
    };
    let mut abs = source_dir.to_path_buf();
    for c in rel.split('/') {
        if !c.is_empty() {
            abs.push(c);
        }
    }
    let Ok(bytes) = std::fs::read(&abs) else {
        return Vec::new();
    };
    let lower = mat.to_ascii_lowercase();
    let mut out = Vec::new();
    if lower.ends_with(".bgsm") {
        if let Ok(m) = bgsm::parse(&bytes) {
            push_tex(&mut out, &m.DiffuseTexture);
            push_tex(&mut out, &m.NormalTexture);
            push_tex(&mut out, &m.SmoothSpecTexture);
            push_tex(&mut out, &m.GreyscaleTexture);
            for opt in [
                &m.EnvmapTexture,
                &m.GlowTexture,
                &m.SpecularTexture,
                &m.LightingTexture,
            ] {
                if let Some(t) = opt {
                    push_tex(&mut out, t);
                }
            }
        }
    } else if lower.ends_with(".bgem") {
        if let Ok(m) = bgem::parse(&bytes) {
            push_tex(&mut out, &m.BaseTexture);
            push_tex(&mut out, &m.NormalTexture);
            push_tex(&mut out, &m.GrayscaleTexture);
            push_tex(&mut out, &m.EnvmapTexture);
            push_tex(&mut out, &m.EnvmapMaskTexture);
            for opt in [&m.SpecularTexture, &m.LightingTexture, &m.GlowTexture] {
                if let Some(t) = opt {
                    push_tex(&mut out, t);
                }
            }
        }
    }
    out
}

fn read_zstring(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).into_owned()
}

/// Enumerate every LOD-capable base record in `handle_id` and return the
/// `(asset_type, source_path, resolved_path)` LOD-closure assets that exist under
/// `source_dir`. `resolved_path` is the absolute on-disk path. This is the regen
/// whole-plugin entry (the source plugin handle is read; the FO76 `_lod` meshes
/// live on the filesystem under `source_dir`).
pub fn enumerate_lod_closures(
    handle_id: u64,
    source_dir: &Path,
    root_form_keys: &[String],
) -> Result<Vec<(String, String, String)>, String> {
    let mut store = plugin_handle_store_ref()
        .lock()
        .map_err(|e| format!("plugin handle store poisoned: {e}"))?;
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| format!("unknown plugin handle {handle_id}"))?;
    let wanted_form_ids = if root_form_keys.is_empty() {
        None
    } else {
        let core = ensure_core_section(slot);
        Some(
            root_form_keys
                .iter()
                .filter_map(|form_key| record_index_entry_by_form_key(&core, form_key))
                .map(|entry| entry.raw_form_id)
                .collect::<HashSet<_>>(),
        )
    };
    let slot = store
        .get(&handle_id)
        .ok_or_else(|| format!("unknown plugin handle {handle_id}"))?;
    let source_game = slot
        .parsed
        .game
        .as_deref()
        .and_then(Game::from_str)
        .unwrap_or(Game::Fo76);
    let mut seen: HashSet<String> = HashSet::new();
    let mut assets: Vec<LodAsset> = Vec::new();
    enumerate_in_items(
        &slot.parsed.root_items,
        source_dir,
        source_game,
        wanted_form_ids.as_ref(),
        &mut seen,
        &mut assets,
    );
    Ok(assets
        .into_iter()
        .map(|a| {
            let resolved = abs_for_source_path(source_dir, &a.source_path);
            (a.kind, a.source_path, resolved)
        })
        .collect())
}

fn enumerate_in_items(
    items: &[ParsedItem],
    source_dir: &Path,
    source_game: Game,
    wanted_form_ids: Option<&HashSet<u32>>,
    seen: &mut HashSet<String>,
    out: &mut Vec<LodAsset>,
) {
    for item in items {
        match item {
            ParsedItem::Record(record)
                if LOD_BASE_SIGS.contains(&record.signature.as_str())
                    && wanted_form_ids
                        .map(|wanted| wanted.contains(&record.form_id))
                        .unwrap_or(true) =>
            {
                let modl = record_modl(record);
                if modl.as_deref().is_some_and(should_skip_object_lod_model) {
                    continue;
                }
                for mnam in record
                    .subrecords
                    .iter()
                    .filter(|s| s.signature.as_str() == "MNAM")
                {
                    append_lod_closures_for_mnam(&mnam.data, source_dir, seen, out);
                }
                if let Some(modl) = modl {
                    append_lod_closures_for_modl_for_game(
                        &modl,
                        source_dir,
                        source_game,
                        seen,
                        out,
                    );
                }
            }
            ParsedItem::Group(group) => {
                enumerate_in_items(
                    &group.children,
                    source_dir,
                    source_game,
                    wanted_form_ids,
                    seen,
                    out,
                );
            }
            _ => {}
        }
    }
}

fn record_modl(record: &esp_authoring_core::plugin_runtime::ParsedRecord) -> Option<String> {
    record
        .subrecords
        .iter()
        .find(|s| s.signature.as_str() == "MODL")
        .map(|s| read_zstring(&s.data))
        .filter(|m| !m.is_empty())
}

/// Absolute path of a Data-relative source path (`Meshes\…`/`Materials\…`/…)
/// under `source_dir`.
fn abs_for_source_path(source_dir: &Path, source_path: &str) -> String {
    let rel = source_path.replace('\\', "/");
    let mut p = source_dir.to_path_buf();
    for c in rel.split('/') {
        if !c.is_empty() {
            p.push(c);
        }
    }
    p.to_string_lossy().into_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedRecord, ParsedSubrecord, insert_parsed_record_in_slot, plugin_handle_close_native,
        plugin_handle_new_native, plugin_handle_store_ref,
    };
    use smol_str::SmolStr;

    fn write_lod(root: &Path, rel: &str) {
        let mut p = root.join("meshes");
        for c in rel.split('/') {
            p.push(c);
        }
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"nif").unwrap();
    }

    #[test]
    fn append_closures_adds_existing_lod_nif() {
        let tmp = tempfile::tempdir().unwrap();
        write_lod(tmp.path(), "lod/architecture/foo/bar01_lod.nif");

        let mut seen = HashSet::new();
        let mut out = Vec::new();
        append_lod_closures_for_modl(
            "Architecture\\Foo\\Bar01.nif",
            tmp.path(),
            &mut seen,
            &mut out,
        );
        assert!(
            out.iter().any(|a| a.kind == "nif"
                && a.source_path
                    .to_ascii_lowercase()
                    .contains("lod\\architecture\\foo\\bar01_lod.nif")),
            "expected the LOD nif closure asset, got {out:?}"
        );
    }

    #[test]
    fn append_closures_uses_skyrim_sibling_lod_convention() {
        let tmp = tempfile::tempdir().unwrap();
        write_lod(tmp.path(), "architecture/farmhouse/farmhouse01_lod.nif");

        let mut seen = HashSet::new();
        let mut out = Vec::new();
        append_lod_closures_for_modl_for_game(
            r"Architecture\Farmhouse\Farmhouse01.nif",
            tmp.path(),
            Game::SkyrimSe,
            &mut seen,
            &mut out,
        );

        assert!(out.iter().any(|asset| {
            asset.kind == "nif"
                && asset
                    .source_path
                    .eq_ignore_ascii_case(r"Meshes\architecture\farmhouse\farmhouse01_lod.nif")
        }));
    }

    #[test]
    fn explicit_skyrim_mnam_scanner_keeps_minus_one_boundary_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let hlod = r"LOD\Farmhouse\Farmhouse01_HLOD.nif";
        let lod = r"LOD\Farmhouse\Farmhouse01_LOD.nif";
        write_lod(tmp.path(), "lod/farmhouse/farmhouse01_hlod.nif");
        write_lod(tmp.path(), "lod/farmhouse/farmhouse01_lod.nif");

        let mut mnam = vec![0u8; MNAM_SLOT * 4];
        for (offset, path) in [
            (0, hlod),
            (MNAM_SLOT, hlod),
            (MNAM_SLOT * 2 - 1, hlod),
            (MNAM_SLOT * 3 - 1, lod),
        ] {
            mnam[offset..offset + path.len()].copy_from_slice(path.as_bytes());
        }

        let mut seen = HashSet::new();
        let mut out = Vec::new();
        append_lod_closures_for_mnam(&mnam, tmp.path(), &mut seen, &mut out);

        assert!(out.iter().any(|asset| {
            asset
                .source_path
                .eq_ignore_ascii_case(r"Meshes\lod\farmhouse\farmhouse01_hlod.nif")
        }));
        assert!(out.iter().any(|asset| {
            asset
                .source_path
                .eq_ignore_ascii_case(r"Meshes\lod\farmhouse\farmhouse01_lod.nif")
        }));
    }

    #[test]
    fn explicit_skyrim_dlc_mnam_scanner_keeps_prefixed_boundary_path() {
        let tmp = tempfile::tempdir().unwrap();
        let lod = r"DLC01\LOD\Castle\CastleWall_LOD.nif";
        write_lod(tmp.path(), "dlc01/lod/castle/castlewall_lod.nif");
        let mut mnam = vec![0u8; MNAM_SLOT * 4];
        let offset = MNAM_SLOT - 1;
        mnam[offset..offset + lod.len()].copy_from_slice(lod.as_bytes());

        let mut seen = HashSet::new();
        let mut out = Vec::new();
        append_lod_closures_for_mnam(&mnam, tmp.path(), &mut seen, &mut out);

        assert!(out.iter().any(|asset| {
            asset
                .source_path
                .eq_ignore_ascii_case(r"Meshes\dlc01\lod\castle\castlewall_lod.nif")
        }));
    }

    #[test]
    fn append_closures_skips_absent_lod() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path()).unwrap();
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        append_lod_closures_for_modl(
            "Architecture\\Foo\\NoLod01.nif",
            tmp.path(),
            &mut seen,
            &mut out,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn append_closures_skips_runtime_sky_and_weather_effect_models() {
        let tmp = tempfile::tempdir().unwrap();
        write_lod(tmp.path(), "lod/sky/clouddistant01_lod.nif");
        write_lod(tmp.path(), "lod/effects/radstormdistantcloud_lod.nif");
        let mut seen = HashSet::new();
        let mut out = Vec::new();

        append_lod_closures_for_modl(r"Sky\CloudDistant01.nif", tmp.path(), &mut seen, &mut out);
        append_lod_closures_for_modl(
            r"Effects\RadStormDistantCloud.nif",
            tmp.path(),
            &mut seen,
            &mut out,
        );

        assert!(out.is_empty());
    }

    #[test]
    fn enumerate_over_handle_returns_lod_nif_with_resolved_path() {
        let tmp = tempfile::tempdir().unwrap();
        write_lod(tmp.path(), "dlc03/lod/architecture/barn/barn01_lod.nif");

        let handle_id = plugin_handle_new_native("LodEnum.esm", Some("fo4")).expect("new handle");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle_id).unwrap();
            let mut modl = b"DLC03\\Architecture\\Barn\\Barn01.nif".to_vec();
            modl.push(0);
            insert_parsed_record_in_slot(
                slot,
                ParsedRecord {
                    signature: SmolStr::new("STAT"),
                    form_id: 0x0000_0901,
                    flags: 0,
                    version_control: 0,
                    form_version: None,
                    version2: None,
                    subrecords: vec![ParsedSubrecord {
                        signature: SmolStr::new("MODL"),
                        data: Bytes::from(modl),
                        semantic_type: None,
                    }],
                    raw_payload: None,
                    parse_error: None,
                },
            );
        }

        let rows = enumerate_lod_closures(handle_id, tmp.path(), &[]).expect("enumerate");
        let nif = rows
            .iter()
            .find(|(kind, sp, _)| {
                kind == "nif" && sp.to_ascii_lowercase().contains("barn01_lod.nif")
            })
            .expect("LOD nif row present");
        assert!(
            Path::new(&nif.2).is_file(),
            "resolved_path must point at the on-disk LOD mesh: {nif:?}"
        );
        assert!(
            enumerate_lod_closures(handle_id, tmp.path(), &["LodEnum.esm:000902".to_string()],)
                .unwrap()
                .is_empty()
        );
        assert!(
            !enumerate_lod_closures(handle_id, tmp.path(), &["LodEnum.esm:000901".to_string()],)
                .unwrap()
                .is_empty()
        );

        plugin_handle_close_native(handle_id);
    }

    #[test]
    fn enumerate_over_handle_uses_explicit_mnam_lod_slots() {
        let tmp = tempfile::tempdir().unwrap();
        write_lod(
            tmp.path(),
            "lod/landscape/trees/chargen/treemaplepw01or_lod_1.nif",
        );
        write_lod(
            tmp.path(),
            "lod/landscape/trees/chargen/treemaplepw01or_lod_3.nif",
        );

        let handle_id =
            plugin_handle_new_native("ExplicitLod.esm", Some("fo76")).expect("new handle");
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle_id).unwrap();
            let mut mnam = vec![0u8; MNAM_SLOT * 4];
            for (level, path) in [
                (0, r"LOD\Landscape\Trees\Chargen\TreeMaplePW01Or_LOD_1.nif"),
                (3, r"LOD\Landscape\Trees\Chargen\TreeMaplePW01Or_LOD_3.nif"),
            ] {
                let off = level * MNAM_SLOT;
                mnam[off..off + path.len()].copy_from_slice(path.as_bytes());
            }
            insert_parsed_record_in_slot(
                slot,
                ParsedRecord {
                    signature: SmolStr::new("STAT"),
                    form_id: 0x0000_0910,
                    flags: 0,
                    version_control: 0,
                    form_version: None,
                    version2: None,
                    subrecords: vec![ParsedSubrecord {
                        signature: SmolStr::new("MNAM"),
                        data: Bytes::from(mnam),
                        semantic_type: None,
                    }],
                    raw_payload: None,
                    parse_error: None,
                },
            );
        }

        let rows = enumerate_lod_closures(handle_id, tmp.path(), &[]).expect("enumerate");
        assert!(
            rows.iter().any(|(kind, sp, _)| {
                kind == "nif"
                    && sp
                        .to_ascii_lowercase()
                        .contains("treemaplepw01or_lod_1.nif")
            }),
            "explicit level 0/1 LOD should be collected: {rows:?}"
        );
        assert!(
            rows.iter().any(|(kind, sp, _)| {
                kind == "nif"
                    && sp
                        .to_ascii_lowercase()
                        .contains("treemaplepw01or_lod_3.nif")
            }),
            "explicit level 3 LOD should be collected: {rows:?}"
        );

        plugin_handle_close_native(handle_id);
    }
}
