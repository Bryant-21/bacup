use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use esp_authoring_core::plugin_runtime::plugin_handle_store_ref;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use rayon::prelude::*;
use unicode_casefold::UnicodeCaseFold;

use crate::run::OwnedPluginHandle;

type RecordRow = (String, String, String);

const ASSET_PREFIXES: &[&str] = &["meshes/", "textures/", "materials/", "sound/"];

fn casefold(value: &str) -> String {
    value.case_fold().collect()
}

#[pyfunction(name = "conversion_setup_official_target_master_names")]
fn official_target_master_names(
    target_game: &str,
    configured_names: Vec<String>,
    base_name: &str,
) -> Vec<String> {
    if target_game.to_lowercase() == "fo4" {
        return [
            "Fallout4.esm",
            "DLCRobot.esm",
            "DLCworkshop01.esm",
            "DLCCoast.esm",
            "DLCworkshop02.esm",
            "DLCworkshop03.esm",
            "DLCNukaWorld.esm",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
    }
    if !configured_names.is_empty() {
        return configured_names;
    }
    if base_name.is_empty() {
        Vec::new()
    } else {
        vec![base_name.to_owned()]
    }
}

#[pyfunction(name = "conversion_setup_required_masters")]
fn required_masters(
    source_game: &str,
    target_game: &str,
    translate_records: bool,
) -> Vec<(String, String)> {
    if source_game == "fo76" && target_game == "fo4" && translate_records {
        vec![("XDI.esm".into(), "FO76→FO4 record conversion requires XDI.esm as a master; install Extended Dialogue Interface in Fallout 4 Data or another mod in the selected mod manager's mods directory".into())]
    } else {
        Vec::new()
    }
}

#[pyfunction(name = "conversion_setup_relocation_roots")]
fn relocation_roots(source_game: &str, target_game: &str, configured: Vec<String>) -> Vec<String> {
    let roots: Vec<_> = configured
        .into_iter()
        .map(|value| {
            value
                .replace('\\', "/")
                .trim()
                .trim_matches('/')
                .to_lowercase()
        })
        .filter(|value| !value.is_empty())
        .collect();
    if !roots.is_empty() {
        return roots;
    }
    let defaults = match (
        source_game.to_lowercase().as_str(),
        target_game.to_lowercase().as_str(),
    ) {
        ("fo76", "fo4") => crate::relocation::FO76_FO4_DEFAULT_RELOCATION_MESH_ROOTS,
        ("skyrimse", "fo4") => crate::relocation::SKYRIMSE_FO4_DEFAULT_RELOCATION_MESH_ROOTS,
        _ => &[],
    };
    defaults.iter().map(|root| (*root).to_owned()).collect()
}

#[pyfunction(name = "conversion_setup_relocation_namespace")]
fn relocation_namespace(source_game: &str, target_game: &str, configured: &str) -> String {
    let namespace = configured.trim().trim_matches(['/', '\\']);
    if !namespace.is_empty() {
        return namespace.to_owned();
    }
    match (
        source_game.to_lowercase().as_str(),
        target_game.to_lowercase().as_str(),
    ) {
        ("fo76", "fo4") => "FO76",
        ("skyrimse", "fo4") => "Skyrim",
        _ => "",
    }
    .to_owned()
}

fn skip_chars(value: &str, count: usize) -> String {
    value.chars().skip(count).collect()
}

fn normalized_asset_key(path: &str, root: Option<&str>) -> String {
    let mut value = path.replace('\\', "/").trim().to_owned();
    if let Some(root) = root {
        let root = root.replace('\\', "/").trim_end_matches('/').to_owned();
        if casefold(&value).starts_with(&format!("{}/", casefold(&root))) {
            value = skip_chars(&value, root.chars().count() + 1);
        }
    }
    let lowered = casefold(&value);
    if let Some(index) = lowered.rfind("/data/") {
        value = skip_chars(&value, lowered[..index].chars().count() + 6);
    } else if lowered.starts_with("data/") {
        value = skip_chars(&value, 5);
    } else {
        for prefix in ASSET_PREFIXES {
            if let Some(index) = lowered.rfind(&format!("/{prefix}")) {
                value = skip_chars(&value, lowered[..index].chars().count() + 1);
                break;
            }
        }
    }
    casefold(value.trim_start_matches('/'))
}

pub(crate) fn asset_candidates(path: &str, asset_type: &str) -> Vec<String> {
    if path.is_empty() {
        return Vec::new();
    }
    let key = normalized_asset_key(path, None);
    let prefix = match casefold(asset_type).as_str() {
        "nif" | "animation" | "animation_dir" | "behavior" => Some("meshes/"),
        "texture" => Some("textures/"),
        "material" => Some("materials/"),
        "sound" | "audio" => Some("sound/"),
        _ => None,
    };
    let alternate = prefix
        .filter(|_| !ASSET_PREFIXES.iter().any(|prefix| key.starts_with(prefix)))
        .map(|prefix| format!("{prefix}{key}"));
    std::iter::once(key).chain(alternate).collect()
}

#[pyclass(name = "TargetAssetIndex", frozen)]
#[derive(Default)]
struct AssetIndex {
    #[pyo3(get)]
    keys: HashSet<String>,
    #[pyo3(get)]
    owners: HashMap<String, String>,
    #[pyo3(get)]
    warnings: Vec<String>,
    #[pyo3(get)]
    files_scanned: usize,
    #[pyo3(get)]
    workers: usize,
}

#[pymethods]
impl AssetIndex {
    fn has_asset_for_type(&self, path: &str, asset_type: &str) -> bool {
        asset_candidates(path, asset_type)
            .iter()
            .any(|key| self.keys.contains(key))
    }

    #[getter]
    fn asset_count(&self) -> usize {
        self.keys.len()
    }
}

fn scan_asset_files(base: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut stack = vec![base.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                paths.push(entry.path());
            }
        }
    }
    paths
}

fn scan_asset_base(base: &Path, workers: usize) -> Result<Vec<PathBuf>, String> {
    let Ok(entries) = fs::read_dir(base) else {
        return Ok(scan_asset_files(base));
    };
    let mut paths = Vec::new();
    let mut directories = Vec::new();
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            directories.push(entry.path());
        } else if kind.is_file() {
            paths.push(entry.path());
        }
    }
    if workers <= 1 || directories.len() <= 1 {
        for directory in directories {
            paths.extend(scan_asset_files(&directory));
        }
    } else {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers.min(directories.len()))
            .build()
            .map_err(|error| error.to_string())?;
        let subtrees: Vec<_> = pool.install(|| {
            directories
                .par_iter()
                .map(|path| scan_asset_files(path))
                .collect()
        });
        paths.extend(subtrees.into_iter().flatten());
    }
    Ok(paths)
}

fn build_asset_index(roots: &[String], workers: usize) -> Result<AssetIndex, String> {
    let mut index = AssetIndex {
        workers: workers.max(1),
        ..AssetIndex::default()
    };
    let mut seen = HashSet::new();
    let roots: Vec<_> = roots
        .iter()
        .filter(|root| !root.is_empty())
        .map(PathBuf::from)
        .filter(|root| {
            let resolved = fs::canonicalize(root)
                .or_else(|_| std::path::absolute(root))
                .unwrap_or_else(|_| root.clone());
            seen.insert(casefold(&resolved.to_string_lossy()))
        })
        .collect();
    if roots.is_empty() {
        index
            .warnings
            .push("target asset preflight: target asset roots are not configured".into());
        return Ok(index);
    }
    for root in roots {
        if !root.is_dir() {
            index.warnings.push(format!(
                "target asset preflight: missing directory {}",
                root.display()
            ));
            continue;
        }
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) => {
                index.warnings.push(format!(
                    "target asset preflight: cannot read directory {}: {error}",
                    root.display()
                ));
                continue;
            }
        };
        let directories: Vec<_> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        for name in ["Meshes", "Textures", "Materials", "Sound"] {
            let matches: Vec<_> = directories
                .iter()
                .filter(|path| {
                    path.file_name()
                        .is_some_and(|part| part.to_string_lossy().eq_ignore_ascii_case(name))
                })
                .collect();
            let Some(base) = matches.first() else {
                continue;
            };
            for duplicate in &matches[1..] {
                index.warnings.push(format!("target asset preflight: duplicate top-level asset directory {}: kept {}, skipped {}",
                    base.file_name().unwrap_or_default().to_string_lossy(), base.display(), duplicate.display()));
            }
            for path in scan_asset_base(base, index.workers)? {
                index.files_scanned += 1;
                let key =
                    normalized_asset_key(&path.to_string_lossy(), Some(&root.to_string_lossy()));
                if let Some(previous) = index.owners.get(&key) {
                    index.warnings.push(format!("target asset preflight: duplicate normalized asset {key}: kept {previous}, skipped {}", path.display()));
                    continue;
                }
                index.keys.insert(key.clone());
                index
                    .owners
                    .insert(key, path.to_string_lossy().into_owned());
            }
        }
    }
    Ok(index)
}

#[pyfunction(name = "conversion_target_asset_index")]
fn asset_index_py(py: Python<'_>, roots: Vec<String>, workers: usize) -> PyResult<AssetIndex> {
    py.detach(|| build_asset_index(&roots, workers))
        .map_err(PyRuntimeError::new_err)
}

const FO4_BUILTIN_ROWS: &[(&str, &str, &str)] = &[
    ("AnimationMult", "AVIF", "0002D2"),
    ("WeapReloadSpeedMult", "AVIF", "0002D3"),
    ("ActionPointsRate", "AVIF", "0002D8"),
    ("RadsRate", "AVIF", "0002DB"),
    ("MeleeDamage", "AVIF", "0002DE"),
    ("weaponSpeedMult", "AVIF", "000312"),
    ("PowerGenerated", "AVIF", "00032E"),
    ("Food", "AVIF", "000331"),
    ("Water", "AVIF", "000332"),
    ("Fatigue", "AVIF", "00034F"),
    ("ActionPointsRateMult", "AVIF", "000359"),
    ("ConditionRateMult", "AVIF", "00035A"),
    ("PowerArmorBattery", "AVIF", "00035C"),
    ("ReflectDamage", "AVIF", "00035F"),
    ("Sneak", "AVIF", "00037F"),
    ("PlayCredits", "GLOB", "000063"),
];

#[derive(Default)]
struct RecordPreflight {
    rows: Vec<RecordRow>,
    seen: HashMap<(String, String), usize>,
    warnings: Vec<String>,
}

impl RecordPreflight {
    fn insert(&mut self, editor_id: String, signature: String, form_key: String, warn: bool) {
        if editor_id.is_empty() || signature.len() != 4 || form_key.is_empty() {
            return;
        }
        let signature = signature.to_uppercase();
        let key = (editor_id.case_fold().collect(), signature.clone());
        if let Some(index) = self.seen.get(&key) {
            let kept = &self.rows[*index].2;
            if warn && kept != &form_key {
                self.warnings.push(format!(
                    "duplicate target EditorID/signature {editor_id}/{signature}: kept {kept}, skipped {form_key}"
                ));
            }
            return;
        }
        self.seen.insert(key, self.rows.len());
        self.rows.push((editor_id, signature, form_key));
    }

    fn add_builtins(&mut self, target_game: &str, master_names: &[String]) {
        if target_game.eq_ignore_ascii_case("fo4")
            && master_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case("Fallout4.esm"))
        {
            for &(editor_id, signature, object_id) in FO4_BUILTIN_ROWS {
                self.insert(
                    editor_id.to_owned(),
                    signature.to_owned(),
                    format!("{object_id}:Fallout4.esm"),
                    false,
                );
            }
        }
    }
}

fn build_record_preflight(
    target_game: &str,
    master_paths: &[String],
) -> Result<RecordPreflight, String> {
    let mut result = RecordPreflight::default();
    let mut master_names = Vec::new();
    for path in master_paths {
        let path = Path::new(path);
        master_names.push(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        );
        let handle =
            OwnedPluginHandle::load_index(path, target_game).map_err(|error| error.to_string())?;
        let mut entries = {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            store
                .get_mut(&handle.id())
                .unwrap()
                .record_index_entries_with_editor_ids()
        };
        // The previous public record-index API orders each master's rows by rendered FormKey.
        entries.sort_by_cached_key(|entry| entry.form_key.render());
        for entry in entries {
            let form_key = if entry.form_key.plugin.is_empty() {
                entry.form_key.render()
            } else {
                format!(
                    "{:06X}:{}",
                    entry.form_key.object_id & 0x00FF_FFFF,
                    entry.form_key.plugin
                )
            };
            result.insert(entry.eid, entry.signature.to_string(), form_key, true);
        }
    }
    result.add_builtins(target_game, &master_names);
    Ok(result)
}

#[pyfunction(name = "conversion_target_record_preflight")]
fn record_preflight_py(
    py: Python<'_>,
    target_game: &str,
    master_paths: Vec<String>,
) -> PyResult<(Vec<RecordRow>, Vec<String>)> {
    let result = py
        .detach(|| build_record_preflight(target_game, &master_paths))
        .map_err(PyRuntimeError::new_err)?;
    Ok((result.rows, result.warnings))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(official_target_master_names, m)?)?;
    m.add_function(wrap_pyfunction!(required_masters, m)?)?;
    m.add_function(wrap_pyfunction!(relocation_roots, m)?)?;
    m.add_function(wrap_pyfunction!(relocation_namespace, m)?)?;
    m.add_class::<AssetIndex>()?;
    m.add_function(wrap_pyfunction!(asset_index_py, m)?)?;
    m.add_function(wrap_pyfunction!(record_preflight_py, m)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_target_masters_preserve_override_order_and_metadata_fallbacks() {
        for game in ["fo4", "FO4"] {
            assert_eq!(
                official_target_master_names(game, vec!["Unordered.esm".into()], "Wrong.esm"),
                [
                    "Fallout4.esm",
                    "DLCRobot.esm",
                    "DLCworkshop01.esm",
                    "DLCCoast.esm",
                    "DLCworkshop02.esm",
                    "DLCworkshop03.esm",
                    "DLCNukaWorld.esm",
                ]
            );
        }
        for game in [
            "fo3",
            "fnv",
            "fo76",
            "skyrim",
            "skyrimse",
            "starfield",
            "unknown",
        ] {
            let configured = vec!["Second.esm".into(), "First.esm".into()];
            assert_eq!(
                official_target_master_names(game, configured.clone(), "Base.esm"),
                configured
            );
            assert_eq!(
                official_target_master_names(game, vec![], "Base.esm"),
                ["Base.esm"]
            );
            assert!(official_target_master_names(game, vec![], "").is_empty());
        }
    }

    #[test]
    #[ignore = "requires a real-master directory and a captured preflight baseline"]
    fn real_master_record_preflight_matches_snapshot() {
        let master_dir = PathBuf::from(std::env::var("BACUP_PREFLIGHT_MASTER_DIR").unwrap());
        let baseline_path = std::env::var("BACUP_PREFLIGHT_BASELINE").unwrap();
        let expected: serde_json::Value =
            serde_json::from_slice(&fs::read(baseline_path).unwrap()).unwrap();
        let paths: Vec<_> = expected["master_names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| {
                master_dir
                    .join(name.as_str().unwrap())
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        let result = build_record_preflight("fo4", &paths).unwrap();
        assert_eq!(
            serde_json::to_value(&result.rows).unwrap(),
            expected["rows"]
        );
        assert_eq!(
            serde_json::to_value(&result.warnings).unwrap(),
            expected["warnings"]
        );
        println!(
            "preflight parity: {} rows, {} warnings, {} masters",
            result.rows.len(),
            result.warnings.len(),
            paths.len()
        );
    }

    #[test]
    fn setup_defaults_preserve_pair_rules_and_explicit_options() {
        assert_eq!(required_masters("fo76", "fo4", true)[0].0, "XDI.esm");
        assert!(required_masters("fo76", "fo4", false).is_empty());
        assert!(required_masters("skyrimse", "fo4", true).is_empty());
        for (source, namespace) in [("fo76", "FO76"), ("skyrimse", "Skyrim")] {
            assert_eq!(
                relocation_roots(source, "fo4", vec![]),
                ["meshes/landscape"]
            );
            assert_eq!(relocation_namespace(source, "fo4", ""), namespace);
        }
        assert!(relocation_roots("fnv", "fo4", vec![]).is_empty());
        assert_eq!(
            relocation_roots(
                "fo76",
                "fo4",
                vec![" Meshes\\SetDressing/ ".into(), "".into()]
            ),
            ["meshes/setdressing"]
        );
        assert_eq!(relocation_namespace("fo76", "fo4", " /Custom\\ "), "Custom");
    }

    #[test]
    fn asset_candidates_normalize_roots_separators_and_kind_prefixes() {
        for (path, kind, expected) in [
            (
                " C:\\Game\\Data\\Textures\\Test.DDS ",
                "texture",
                vec!["textures/test.dds"],
            ),
            (
                "D:/extracted/fo4/Meshes/Actors/Test.NIF",
                "nif",
                vec!["meshes/actors/test.nif"],
            ),
            (
                "Actors/Test.hkx",
                "behavior",
                vec!["actors/test.hkx", "meshes/actors/test.hkx"],
            ),
            ("Sound/test.wav", "nif", vec!["sound/test.wav"]),
            ("unknown.bin", "unknown", vec!["unknown.bin"]),
            ("", "nif", vec![]),
        ] {
            assert_eq!(asset_candidates(path, kind), expected, "{path}");
        }
        assert_eq!(
            normalized_asset_key("D:/ROOT/Textures/Straße.DDS", Some("d:/root")),
            "textures/strasse.dds"
        );
    }

    #[test]
    fn asset_inventory_preserves_root_precedence_and_missing_diagnostics() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        for root in [&first, &second] {
            fs::create_dir_all(root.join("Meshes/Actors")).unwrap();
            fs::create_dir_all(root.join("Textures/Other")).unwrap();
            fs::write(root.join("Meshes/Actors/Test.nif"), b"mesh").unwrap();
        }
        fs::write(second.join("Textures/Other/a.dds"), b"texture").unwrap();
        let roots = vec![
            first.to_string_lossy().into_owned(),
            first.to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
            temp.path().join("missing").to_string_lossy().into_owned(),
        ];
        for workers in [1, 4] {
            let index = build_asset_index(&roots, workers).unwrap();
            assert_eq!(index.files_scanned, 3);
            assert_eq!(index.keys.len(), 2);
            assert_eq!(
                PathBuf::from(&index.owners["meshes/actors/test.nif"]),
                first.join("Meshes/Actors/Test.nif")
            );
            assert!(index.has_asset_for_type("actors/test.nif", "nif"));
            assert!(index.warnings[0].starts_with(
                "target asset preflight: duplicate normalized asset meshes/actors/test.nif:"
            ));
            assert!(index.warnings[1].starts_with("target asset preflight: missing directory "));
        }
        assert_eq!(
            build_asset_index(&[], 1).unwrap().warnings,
            ["target asset preflight: target asset roots are not configured"]
        );
    }

    #[test]
    fn earlier_master_wins_per_casefolded_editor_id_and_signature() {
        let mut result = RecordPreflight::default();
        for (eid, sig, fk) in [
            ("Ammo10mm", "AMMO", "01F276:Fallout4.esm"),
            ("ammo10mm", "ammo", "01F276:Fallout4.esm"),
            ("ammo10mm", "AMMO", "099999:DLCRobot.esm"),
            ("Ammo10mm", "MISC", "000100:DLCRobot.esm"),
            ("Straße", "MISC", "000101:DLCRobot.esm"),
            ("STRASSE", "MISC", "000102:DLCRobot.esm"),
            ("", "MISC", "000103:DLCRobot.esm"),
        ] {
            result.insert(eid.into(), sig.into(), fk.into(), true);
        }
        assert_eq!(result.rows.len(), 3);
        assert_eq!(result.rows[0].2, "01F276:Fallout4.esm");
        assert_eq!(
            result.warnings,
            [
                "duplicate target EditorID/signature ammo10mm/AMMO: kept 01F276:Fallout4.esm, skipped 099999:DLCRobot.esm",
                "duplicate target EditorID/signature STRASSE/MISC: kept 000101:DLCRobot.esm, skipped 000102:DLCRobot.esm",
            ]
        );
    }

    #[test]
    fn builtins_require_fo4_base_master_and_never_replace_physical_records() {
        let mut result = RecordPreflight::default();
        result.add_builtins("fo4", &["DLCRobot.esm".into()]);
        result.add_builtins("skyrimse", &["Fallout4.esm".into()]);
        assert!(result.rows.is_empty());
        result.insert(
            "PowerGenerated".into(),
            "AVIF".into(),
            "123456:Fallout4.esm".into(),
            true,
        );
        result.add_builtins("FO4", &["fallout4.esm".into()]);
        assert_eq!(result.rows.len(), 16);
        assert_eq!(result.rows[0].2, "123456:Fallout4.esm");
        assert!(result.rows.contains(&(
            "PlayCredits".into(),
            "GLOB".into(),
            "000063:Fallout4.esm".into()
        )));
        assert!(result.warnings.is_empty());
    }
}
